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

    /// absent child node (the reference implementation uses `null` children for
    /// omitted slice bounds, e.g. `a[..2]`); the VM treats it as `None`
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
    PlusAssign,
    MinusAssign,
    AsteriskAssign,
    DoubleAsteriskAssign,
    SlashAssign,
    DoubleSlashAssign,
    PercentAssign,
    BAndAssign,
    BOrAssign,
    BXorAssign,
    LeftShiftAssign,
    RightShiftAssign,
    QuestionAssign,
    DoubleQuestionAssign,

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

    Go,

    Block,
    Pair,

    Typed,

    Class,
    Enum,
    Parameter,
    Argument,
    Alias,
    Annotation,
    Lambda,
    Target,
    Wildcard,
    Case,
    ClassBases,
    Spread,
    KwSpread,
    IfCase,
    Branch,
    Tagged,
}

/// literal value stored on a node (`object? Value` in the reference implementation)
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

    /// mirrors `Node.WithChildren` (start/length/kind/op/value/annotations are kept).
    /// named `with_new_children` so the associated function form stays unambiguous
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

    /// mirrors `Node.Const(value, start, length)`
    pub fn konst(value: Const, start: usize, length: usize) -> Self {
        match value {
            Const::Int(_) => Node::new(start, length, NodeKind::Integer).with_value(value),
            Const::Float(_) => Node::new(start, length, NodeKind::Float).with_value(value),
            Const::Str(_) => Node::new(start, length, NodeKind::String).with_value(value),
            Const::Bool(_) => Node::new(start, length, NodeKind::Boolean).with_value(value),
            // `null` becomes a None node and carries no value
            Const::Null => Node::new(start, length, NodeKind::None),
        }
    }

    pub fn with_annotations(mut self, annotations: Vec<Node>) -> Self {
        self.annotations = Some(annotations);
        self
    }

    pub fn end(&self) -> usize {
        self.start + self.length
    }
}
