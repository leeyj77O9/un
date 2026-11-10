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

    pub fn with_annotations(mut self, annotations: Vec<Node>) -> Self {
        self.annotations = Some(annotations);
        self
    }

    pub fn end(&self) -> usize {
        self.start + self.length
    }
}
