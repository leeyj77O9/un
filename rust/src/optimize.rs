use crate::ast::{Const, Node, NodeKind};
use crate::error::Panic;
use crate::token::TokenType;
static TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
/// A fresh synthetic identifier for desugaring temps. The leading control
/// character can never appear in a source identifier (the lexer only accepts
/// ASCII alphanumerics and `_`), so it cannot shadow a user variable. It
/// carries its name in `value` because it has no source span to read.
fn fresh_temp(node: &Node) -> Node {
    let n = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Node::new(node.start, 0, NodeKind::Identifier)
        .with_value(crate::ast::Const::Str(format!("\u{1}tmp{n}")))
}
/// A single bottom-up pass reaches the fixpoint: children are optimized
/// before their parents, so every fold/elimination is applied to an
/// already-final subtree. The counter only exists for diagnostics.
pub struct OptimizationResult {
    pub root: Node,
    pub pass_count: usize,
}
pub fn optimize(root: &Node) -> Result<OptimizationResult, Panic> {
    let (root, _changed) = optimize_pass(root)?;
    Ok(OptimizationResult {
        root,
        pass_count: 1,
    })
}
/// Node-local desugaring applied to a node whose children are already
/// desugared and optimized. Rewrites applied here: compound assignment
/// wraps the value in a `Binary(op)`, and `Branch`/`ElIf` arms of an `If`
/// become `IfCase` arms. The rewritten node is returned to `optimize_pass`,
/// which folds it in the same step.
fn desugar_local(node: &Node) -> Option<Node> {
    match node.kind {
        NodeKind::Assign => desugar_assign(node),
        NodeKind::If => Some(desugar_if(node)),
        _ => None,
    }
}
fn desugar_assign(node: &Node) -> Option<Node> {
    let op = node.op?;
    let op2 = match op {
        TokenType::PlusAssign => Some(TokenType::Plus),
        TokenType::MinusAssign => Some(TokenType::Minus),
        TokenType::AsteriskAssign => Some(TokenType::Asterisk),
        TokenType::SlashAssign => Some(TokenType::Slash),
        TokenType::PercentAssign => Some(TokenType::Percent),
        TokenType::DoubleAsteriskAssign => Some(TokenType::DoubleAsterisk),
        TokenType::DoubleSlashAssign => Some(TokenType::DoubleSlash),
        TokenType::BAndAssign => Some(TokenType::BAnd),
        TokenType::BOrAssign => Some(TokenType::BOr),
        TokenType::BXorAssign => Some(TokenType::BXor),
        TokenType::LeftShiftAssign => Some(TokenType::LeftShift),
        TokenType::RightShiftAssign => Some(TokenType::RightShift),
        TokenType::DoubleQuestionAssign => Some(TokenType::DoubleQuestion),
        _ => None,
    }?;
    if node.children.len() != 2 {
        return None;
    }
    // A typed compound assignment (`x: int += 1`) carries a `Typed` node on
    // the LHS. Unwrap it for the *load* side and re-wrap the *store* target;
    // otherwise the annotation reaches code generation as a value expression
    // (unsupported node kind Typed) or breaks the stack discipline.
    let typed_ann: Option<Node> = if node.children[0].kind == NodeKind::Typed {
        Some(node.children[0].children[1].clone())
    } else {
        None
    };
    let lhs = if node.children[0].kind == NodeKind::Typed {
        node.children[0].children[0].clone()
    } else {
        node.children[0].clone()
    };
    let rhs = node.children[1].clone();
    let wrap_typed = |target: Node| -> Node {
        match &typed_ann {
            Some(ann) => Node::with_children(
                target.start,
                ann.start + ann.length - target.start,
                NodeKind::Typed,
                None,
                vec![target, ann.clone()],
            ),
            None => target,
        }
    };
    // A plain-identifier target re-reads the slot — free and side-effect
    // free, so inline the binary directly.
    if lhs.kind == NodeKind::Identifier {
        let binary = Node::with_op(node.start, node.length, NodeKind::Binary, op2, vec![lhs.clone(), rhs]);
        return Some(Node::with_children(node.start, node.length, NodeKind::Assign, None, vec![wrap_typed(lhs), binary]));
    }
    // Property/Index/Slice targets may have side effects in the receiver or
    // subscript expressions (`make().v += 10`). Python evaluates the target
    // once; we do the same by binding the receiver (and each subscript) to a
    // synthetic temp, then writing through the temp.
    let span = (node.start, node.length);
    let start = node.start;
    let mk_assign = |t: Node, v: Node| {
        Node::with_children(span.0, span.1, NodeKind::Assign, None, vec![t, v])
    };
    let mk_binary = |l: Node| {
        Node::with_op(span.0, span.1, NodeKind::Binary, op2, vec![l, rhs.clone()])
    };
    match lhs.kind {
        NodeKind::Property if lhs.children.len() == 2 => {
            let name = lhs.children[1].clone();
            let tmp = fresh_temp(node);
            let load = Node::with_children(start, 0, NodeKind::Property, None, vec![tmp.clone(), name.clone()]);
            let store = wrap_typed(Node::with_children(start, 0, NodeKind::Property, None, vec![tmp.clone(), name]));
            Some(Node::with_children(
                start,
                span.1,
                NodeKind::Block,
                None,
                vec![
                    mk_assign(tmp, lhs.children[0].clone()),
                    mk_assign(store, mk_binary(load)),
                ],
            ))
        }
        NodeKind::Index if lhs.children.len() == 2 => {
            let obj_tmp = fresh_temp(node);
            let key_tmp = fresh_temp(node);
            let load = Node::with_children(start, 0, NodeKind::Index, None, vec![obj_tmp.clone(), key_tmp.clone()]);
            let store = wrap_typed(Node::with_children(start, 0, NodeKind::Index, None, vec![obj_tmp.clone(), key_tmp.clone()]));
            Some(Node::with_children(
                start,
                span.1,
                NodeKind::Block,
                None,
                vec![
                    mk_assign(obj_tmp, lhs.children[0].clone()),
                    mk_assign(key_tmp, lhs.children[1].clone()),
                    mk_assign(store, mk_binary(load)),
                ],
            ))
        }
        NodeKind::Slice if lhs.children.len() > 1 && lhs.children.len() <= 4 => {
            let obj_tmp = fresh_temp(node);
            // A `Null` bound means "omitted" — keep the literal null child
            // (the codegen skips null bounds) instead of routing it through
            // a temp, which would turn it into a runtime value.
            let bound_tmps: Vec<(bool, Node)> = lhs.children[1..]
                .iter()
                .map(|bound| {
                    if bound.kind == NodeKind::Null {
                        (false, bound.clone())
                    } else {
                        (true, fresh_temp(node))
                    }
                })
                .collect();
            let mut load_children = vec![obj_tmp.clone()];
            let mut store_children = vec![obj_tmp.clone()];
            load_children.extend(bound_tmps.iter().map(|(_, t)| t.clone()));
            store_children.extend(bound_tmps.iter().map(|(_, t)| t.clone()));
            let load = Node::with_children(start, 0, NodeKind::Slice, None, load_children);
            let store = wrap_typed(Node::with_children(start, 0, NodeKind::Slice, None, store_children));
            let mut statements: Vec<Node> = Vec::with_capacity(lhs.children.len() + 1);
            statements.push(mk_assign(obj_tmp, lhs.children[0].clone()));
            for ((is_temp, tmp), bound) in bound_tmps.iter().zip(lhs.children[1..].iter()) {
                if *is_temp {
                    statements.push(mk_assign(tmp.clone(), bound.clone()));
                }
            }
            statements.push(mk_assign(store, mk_binary(load)));
            Some(Node::with_children(start, span.1, NodeKind::Block, None, statements))
        }
        _ => {
            // Unsupported target shape: keep the previous expansion.
            let binary = Node::with_op(node.start, node.length, NodeKind::Binary, op2, vec![lhs.clone(), rhs]);
            Some(Node::with_children(node.start, node.length, NodeKind::Assign, None, vec![wrap_typed(lhs), binary]))
        }
    }
}
/// Rewrite `Branch`/`ElIf` arms into `IfCase` inside an `If` node. The
/// `Else` arm is preserved; non-matching arms (absent here) follow through.
/// The `Else` arm is kept last so codegen can identify it as the else-branch.
fn desugar_if(node: &Node) -> Node {
    let mut cases: Vec<Node> = Vec::new();
    let mut else_node: Option<Node> = None;
    for child in &node.children {
        match child.kind {
            NodeKind::Branch | NodeKind::ElIf => {
                cases.push(Node::with_children(
                    child.start,
                    child.length,
                    NodeKind::IfCase,
                    None,
                    child.children.clone(),
                ));
            }
            // Already-desugared IfCase arms (nested `if` children are
            // desugared bottom-up): keep them as-is instead of dropping.
            NodeKind::IfCase => cases.push(child.clone()),
            NodeKind::Else => {
                else_node = Some(child.clone());
            }
            _ => {}
        }
    }
    let mut children = cases;
    if let Some(else_node) = else_node {
        children.push(else_node);
    }
    Node::with_children(node.start, node.length, NodeKind::If, None, children)
}

fn optimize_pass(node: &Node) -> Result<(Node, bool), Panic> {
    let (node, mut changed) = optimize_children(node)?;
    // `desugar_local` may substitute the node tree (it always builds fresh
    // nodes), which changes the subtree's identity for every parent above;
    // the parents must be rebuilt so the new shape is not silently dropped.
    let node = match desugar_local(&node) {
        Some(desugared) => {
            changed = true;
            desugared
        }
        None => node,
    };
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
fn eval_long(op: TokenType, a: i64, b: i64) -> Result<Const, ConstError> {
    match op {
        TokenType::Plus => Ok(Const::Int(a.wrapping_add(b))),
        TokenType::Minus => Ok(Const::Int(a.wrapping_sub(b))),
        TokenType::Asterisk => Ok(Const::Int(a.wrapping_mul(b))),

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
            Ok(Const::Int(a.wrapping_div(b)))
        }
        TokenType::Percent => {
            if b == 0 {
                return Err(ConstError::Caught);
            }
            Ok(Const::Int(a.wrapping_rem(b)))
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
            Ok(Const::Int(crate::runtime::prims::cast_long(a / b)))
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
