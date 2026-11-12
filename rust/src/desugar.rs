use crate::ast::{Node, NodeKind};
use crate::token::TokenType;

/// mirrors `Desugarer` of the reference implementation
/// (`src/Interpreter/Desugarer/Desugarer.cs`).
pub fn desugar(root: &Node) -> Node {
    visit(root)
}

fn visit(node: &Node) -> Node {
    match node.kind {
        NodeKind::Block => desugar_children(node),
        NodeKind::If => desugar_if(node),
        NodeKind::Assign => desugar_assign(node),
        NodeKind::While => desugar_while(node),
        NodeKind::Binary => desugar_binary(node),
        _ => desugar_children(node),
    }
}

fn desugar_children(node: &Node) -> Node {
    if node.children.is_empty() {
        return node.clone();
    }

    let new_children = node.children.iter().map(visit).collect();
    node.with_new_children(new_children)
}

/// `+=` and friends expand to `Assign(lhs, Binary(op, lhs, rhs))`.
/// the reference does **not** visit the left-hand side, and clears `Operator`.
fn desugar_assign(node: &Node) -> Node {
    let lhs = node.children[0].clone();
    let rhs = visit(&node.children[1]);

    let rhs = match node.op {
        Some(op) => match compound_op(op) {
            Some(op) => Node::with_op(node.start, node.length, NodeKind::Binary, op, vec![lhs.clone(), rhs]),
            None => rhs,
        },
        None => rhs,
    };

    Node::with_children(node.start, node.length, NodeKind::Assign, None, vec![lhs, rhs])
}

fn compound_op(op: TokenType) -> Option<TokenType> {
    match op {
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
        // `=` and `:=` (`QuestionAssign` too) are not expanded
        _ => None,
    }
}

/// `Branch`/`ElIf` are normalized to `IfCase`, `Else` is kept, and the
/// cases are emitted before the `Else` node.
fn desugar_if(node: &Node) -> Node {
    let mut cases: Vec<Node> = Vec::new();
    let mut else_node: Option<Node> = None;

    for child_raw in &node.children {
        let child = visit(child_raw);

        match child.kind {
            NodeKind::Branch | NodeKind::ElIf => {
                let cond = child.children[0].clone();
                let body = child.children[1].clone();
                cases.push(Node::with_children(
                    child.start,
                    child.length,
                    NodeKind::IfCase,
                    None,
                    vec![cond, body],
                ));
            }
            NodeKind::Else => {
                let body = child.children[0].clone();
                else_node = Some(Node::with_children(
                    child.start,
                    child.length,
                    NodeKind::Else,
                    None,
                    vec![body],
                ));
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

fn desugar_while(node: &Node) -> Node {
    let cond = visit(&node.children[0]);
    let body = visit(&node.children[1]);

    Node::with_children(node.start, node.length, NodeKind::While, None, vec![cond, body])
}

fn desugar_binary(node: &Node) -> Node {
    let left = visit(&node.children[0]);
    let right = visit(&node.children[1]);

    Node::with_children(
        node.start,
        node.length,
        NodeKind::Binary,
        node.op,
        vec![left, right],
    )
}
