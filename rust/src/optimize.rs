use crate::ast::{Const, Node, NodeKind};
use crate::error::Panic;
use crate::token::TokenType;
pub const MAX_PASSES: usize = 64;
pub struct OptimizationResult {
    pub root: Node,
    pub pass_count: usize,
}
pub fn optimize(root: &Node) -> Result<OptimizationResult, Panic> {
    let mut root = root.clone();
    for pass in 1..=MAX_PASSES {
        let (next, changed) = optimize_pass(&root)?;
        root = next;

        if !changed {
            return Ok(OptimizationResult {
                root,
                pass_count: pass,
            });
        }
    }
    Err(Panic::new(format!(
        "optimizer did not reach a fixed point within {MAX_PASSES} passes"
    )))
}
fn optimize_pass(node: &Node) -> Result<(Node, bool), Panic> {
    let (node, changed) = optimize_children(node)?;
    match node.kind {
        NodeKind::Binary => {
            let (node, local) = optimize_binary(&node)?;
            Ok((node, changed || local))
        }
        NodeKind::Ternary => {
            let (node, local) = optimize_ternary(&node);
            Ok((node, changed || local))
        }
        NodeKind::If => {
            let (node, local) = optimize_if(&node)?;
            Ok((node, changed || local))
        }
        NodeKind::While => {
            let (node, local) = optimize_while(&node);
            Ok((node, changed || local))
        }
        _ => Ok((node, changed)),
    }
}
fn optimize_children(node: &Node) -> Result<(Node, bool), Panic> {
    if node.children.is_empty() {
        return Ok((node.clone(), false));
    }
    let mut changed = false;
    let mut new_children = Vec::with_capacity(node.children.len());
    for child in &node.children {
        let (opt, child_changed) = optimize_pass(child)?;
        new_children.push(opt);
        changed |= child_changed;
    }
    if changed {
        Ok((node.with_new_children(new_children), true))
    } else {
        Ok((node.clone(), false))
    }
}
fn optimize_binary(node: &Node) -> Result<(Node, bool), Panic> {
    let left = &node.children[0];
    let right = &node.children[1];
    let (a, b) = match (is_const(left), is_const(right)) {
        (Some(a), Some(b)) => (a, b),
        _ => return Ok((node.clone(), false)),
    };

    let op = match node.op {
        Some(op) => op,
        None => return Ok((node.clone(), false)),
    };
    if op == TokenType::And || op == TokenType::Or || op == TokenType::Xor {
        return Ok((node.clone(), false));
    }
    match eval(op, a, b) {
        Ok(value) => Ok((Node::konst(value, node.start, node.length), true)),
        Err(ConstError::Caught) => Ok((node.clone(), false)),

        Err(ConstError::Uncaught(panic)) => Err(panic),
    }
}
fn optimize_ternary(node: &Node) -> (Node, bool) {
    let cond = &node.children[0];
    if let Some(Const::Bool(b)) = is_const(cond) {
        let branch = node.children[if b { 1 } else { 2 }].clone();
        return (branch, true);
    }
    (node.clone(), false)
}
fn optimize_if(node: &Node) -> Result<(Node, bool), Panic> {
    let mut changed = false;
    let mut children: Vec<Node> = Vec::with_capacity(node.children.len());
    for child in &node.children {
        if child.kind == NodeKind::IfCase {
            let (condition, cond_changed) = optimize_pass(&child.children[0])?;
            let (body, body_changed) = optimize_pass(&child.children[1])?;
            if let Some(Const::Bool(b)) = is_const(&condition) {
                changed = true;
                if b {
                    return Ok((body, true));
                }
                continue;
            }
            if cond_changed || body_changed {
                changed = true;

                children.push(Node::with_children(
                    child.start,
                    child.length,
                    NodeKind::IfCase,
                    None,
                    vec![condition, body],
                ));
            } else {
                children.push(child.clone());
            }
        } else {
            let (block, block_changed) = optimize_pass(&child.children[0])?;
            if block_changed {
                changed = true;
                children.push(Node::with_children(
                    child.start,
                    child.length,
                    NodeKind::Else,
                    None,
                    vec![block],
                ));
            } else {
                children.push(child.clone());
            }
        }
    }
    if children.is_empty() {
        return Ok((Node::new(node.start, 0, NodeKind::Block), true));
    }
    if children.len() == 1 && children[0].kind == NodeKind::Else {
        return Ok((children[0].children[0].clone(), true));
    }
    if !changed {
        return Ok((node.clone(), false));
    }
    Ok((node.with_new_children(children), true))
}
fn optimize_while(node: &Node) -> (Node, bool) {
    let cond = &node.children[0];
    if let Some(Const::Bool(false)) = is_const(cond) {
        return (Node::new(0, 0, NodeKind::Block), true);
    }
    (node.clone(), false)
}
fn is_const(node: &Node) -> Option<Const> {
    match node.kind {
        NodeKind::Integer | NodeKind::Float | NodeKind::String | NodeKind::Boolean => {
            node.value.clone()
        }
        _ => None,
    }
}
enum ConstError {

    Caught,

    Uncaught(Panic),
}
fn eval(op: TokenType, a: Const, b: Const) -> Result<Const, ConstError> {
    match (a, b) {
        (Const::Int(x), Const::Int(y)) => eval_long(op, x, y),
        (Const::Float(x), Const::Float(y)) => eval_double(op, x, y),
        (Const::Int(x), Const::Float(y)) => eval_double(op, x as f64, y),
        (Const::Float(x), Const::Int(y)) => eval_double(op, x, y as f64),
        (Const::Str(x), Const::Str(y)) => {
            if op == TokenType::Plus {
                Ok(Const::Str(x + &y))
            } else {
                Err(ConstError::Caught)
            }
        }

        (a, b) => {
            let _ = (a, b);
            Err(ConstError::Caught)
        }
    }
}
fn overflow() -> ConstError {
    ConstError::Uncaught(Panic::named(
        "System.OverflowException",
        "Arithmetic operation resulted in an overflow.",
    ))
}
fn eval_long(op: TokenType, a: i64, b: i64) -> Result<Const, ConstError> {
    match op {
        TokenType::Plus => a.checked_add(b).map(Const::Int).ok_or_else(overflow),
        TokenType::Minus => a.checked_sub(b).map(Const::Int).ok_or_else(overflow),
        TokenType::Asterisk => a.checked_mul(b).map(Const::Int).ok_or_else(overflow),

        TokenType::Slash => {
            if b == 0 {
                return Err(ConstError::Caught);
            }
            Ok(Const::Float(a as f64 / b as f64))
        }
        TokenType::DoubleSlash => {
            if b == 0 {
                return Err(ConstError::Caught);
            }
            Ok(Const::Int(a / b))
        }
        TokenType::Percent => {
            if b == 0 {
                return Err(ConstError::Caught);
            }
            Ok(Const::Int(a % b))
        }
        TokenType::DoubleAsterisk => eval_pow_long(a, b),
        TokenType::Equal => Ok(Const::Bool(a == b)),
        TokenType::Unequal => Ok(Const::Bool(a != b)),
        TokenType::GreaterThan => Ok(Const::Bool(a > b)),
        TokenType::LessThan => Ok(Const::Bool(a < b)),
        TokenType::GreaterOrEqual => Ok(Const::Bool(a >= b)),
        TokenType::LessOrEqual => Ok(Const::Bool(a <= b)),
        _ => Err(ConstError::Caught),
    }
}
fn eval_pow_long(a: i64, b: i64) -> Result<Const, ConstError> {
    if b < 0 {
        return Err(ConstError::Caught);
    }
    let d = (a as f64).powf(b as f64);
    if d.is_infinite() || d.is_nan() || d > i64::MAX as f64 || d < i64::MIN as f64 {
        return Err(ConstError::Caught);
    }
    Ok(Const::Int(d as i64))
}
fn eval_double(op: TokenType, a: f64, b: f64) -> Result<Const, ConstError> {
    match op {
        TokenType::Plus => Ok(Const::Float(a + b)),
        TokenType::Minus => Ok(Const::Float(a - b)),
        TokenType::Asterisk => Ok(Const::Float(a * b)),

        TokenType::Slash => Ok(Const::Float(a / b)),
        TokenType::DoubleSlash => {
            if b == 0.0 {
                return Err(ConstError::Caught);
            }
            Ok(Const::Int((a / b) as i64))
        }
        TokenType::Percent => {
            if b == 0.0 {
                return Err(ConstError::Caught);
            }
            Ok(Const::Float(a % b))
        }
        TokenType::DoubleAsterisk => {
            let d = a.powf(b);
            if d.is_infinite() || d.is_nan() {
                return Err(ConstError::Caught);
            }
            Ok(Const::Float(d))
        }
        TokenType::Equal => Ok(Const::Bool(a == b)),
        TokenType::Unequal => Ok(Const::Bool(a != b)),
        TokenType::GreaterThan => Ok(Const::Bool(a > b)),
        TokenType::LessThan => Ok(Const::Bool(a < b)),
        TokenType::GreaterOrEqual => Ok(Const::Bool(a >= b)),
        TokenType::LessOrEqual => Ok(Const::Bool(a <= b)),
        _ => Err(ConstError::Caught),
    }
}
