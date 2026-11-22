use crate::token::TokenType;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Identifier,
    Integer,
    Float,
    String,
    FString,
    Boolean,
    None,

    Null,
    Tuple,
    List,
    Dict,
    Set,
    Unary,
    Binary,
    Ternary,
    Call,
    Property,
    Index,
    Slice,
    Assign,
    Function,
    Extension,
    Accessor,
    If,
    ElIf,
    Else,
    Match,
    For,
    While,
    Return,
    Break,
    Skip,
    Use,
    Using,
    Try,
    Catch,
    Finally,
    Defer,
    Block,
    Pair,
    Typed,
    Class,
    Enum,
    Parameter,
    Annotation,
    Lambda,
    Wildcard,
    Case,
    ClassBases,
    Spread,
    KwSpread,
    IfCase,
    Branch,
    Tagged,
    /// Type parameter list `[T, U]` on a function declaration. Always appended
    /// as the last child so positional readers (`children[0..2]`) stay valid.
    TypeParams,
    /// `type Name = int | str`. Children: `[name, type-expression]`; the type
    /// expression keeps the same `Const::Str` text representation used by
    /// ordinary annotations so existing type readers can consume it as-is.
    TypeAlias,
    /// A guarded match arm: `case <pattern> if <condition>`. Children:
    /// `[pattern, condition]`. `Case.children[0]` is a `Guard` when guarded,
    /// so `Case.children[1]` stays the arm body.
    Guard,
    /// An or-pattern: `A | B`. Children are the alternatives, left to right.
    OrPattern,
    /// A `b"..."` byte-string literal. Children are `Integer` nodes holding
    /// one byte (0..=255) each, preserving raw/non-ASCII bytes losslessly.
    Bytes,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
}
#[derive(Debug, Clone)]
pub struct Node {
    pub start: usize,
    pub length: usize,
    pub kind: NodeKind,
    pub op: Option<TokenType>,
    pub children: Vec<Node>,
    pub value: Option<Const>,
    pub annotations: Option<Vec<Node>>,
}
impl Node {
    pub fn new(start: usize, length: usize, kind: NodeKind) -> Self {
        Self {
            start,
            length,
            kind,
            op: None,
            children: Vec::new(),
            value: None,
            annotations: None,
        }
    }
    pub fn with_op(start: usize, length: usize, kind: NodeKind, op: TokenType, children: Vec<Node>) -> Self {
        Self {
            start,
            length,
            kind,
            op: Some(op),
            children,
            value: None,
            annotations: None,
        }
    }
    pub fn with_children(start: usize, length: usize, kind: NodeKind, op: Option<TokenType>, children: Vec<Node>) -> Self {
        Self {
            start,
            length,
            kind,
            op,
            children,
            value: None,
            annotations: None,
        }
    }
    pub fn with_value(mut self, value: Const) -> Self {
        self.value = Some(value);
        self
    }

    pub fn with_new_children(&self, new_children: Vec<Node>) -> Self {
        Self {
            start: self.start,
            length: self.length,
            kind: self.kind,
            op: self.op,
            children: new_children,
            value: self.value.clone(),
            annotations: self.annotations.clone(),
        }
    }

    pub fn konst(value: Const, start: usize, length: usize) -> Self {
        match value {
            Const::Int(_) => Node::new(start, length, NodeKind::Integer).with_value(value),
            Const::Float(_) => Node::new(start, length, NodeKind::Float).with_value(value),
            Const::Str(_) => Node::new(start, length, NodeKind::String).with_value(value),
            Const::Bool(_) => Node::new(start, length, NodeKind::Boolean).with_value(value),

            Const::Null => Node::new(start, length, NodeKind::None),
        }
    }
    pub fn with_annotations(mut self, annotations: Vec<Node>) -> Self {
        self.annotations = Some(annotations);
        self
    }
}
