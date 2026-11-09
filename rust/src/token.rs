use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Eof,
    Error,
    NoneKw,
    NullKw,

    LParen,
    RParen,
    LBrace,
    RBrace,
    LBrack,
    RBrack,

    DoubleQuestion,
    Dot,
    DotDot,
    Question,

    Assign,
    ColonAssign,
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

    Or,
    And,
    Xor,
    Not,

    In,
    Is,
    IsNot,
    As,

    Equal,
    Unequal,
    LessOrEqual,
    GreaterOrEqual,
    LessThan,
    GreaterThan,

    BOr,
    BXor,
    BAnd,

    LeftShift,
    RightShift,

    Plus,
    Minus,
    Positive,
    Negative,

    Asterisk,
    DoubleAsterisk,
    Spread,
    DictSpread,

    Slash,
    DoubleSlash,
    Percent,
    At,

    BNot,
    Bang,

    Indexer,
    Slicer,
    Property,
    NullableProperty,

    Comma,
    Colon,
    QuestionDot,

    Func,
    Class,
    Enum,
    True,
    False,
    If,
    ElIf,
    Else,
    Match,
    For,
    While,
    Break,
    Skip,
    Return,
    Use,
    Using,
    Comment,

    Identifier,

    Integer,
    Float,
    String,
    FString,
    Boolean,
    List,
    Dict,
    Set,
    Tuple,
    Iter,

    Pair,
    Annotation,

    Call,

    Go,
    Wait,

    Try,
    Catch,
    Defer,

    NewLine,
    Indent,
    Dedent,
    Finally,
    Wildcard,
    Underscore,
}

impl TokenType {
    pub fn text(&self) -> &'static str {
        match self {
            TokenType::Eof => "eof",
            TokenType::Error => "error",
            TokenType::NoneKw => "none",
            TokenType::NullKw => "null",
            TokenType::LParen => "(",
            TokenType::RParen => ")",
            TokenType::LBrace => "{",
            TokenType::RBrace => "}",
            TokenType::LBrack => "[",
            TokenType::RBrack => "]",
            TokenType::DoubleQuestion => "??",
            TokenType::Dot => ".",
            TokenType::DotDot => "..",
            TokenType::Question => "?",
            TokenType::Assign => "=",
            TokenType::ColonAssign => ":=",
            TokenType::PlusAssign => "+=",
            TokenType::MinusAssign => "-=",
            TokenType::AsteriskAssign => "*=",
            TokenType::DoubleAsteriskAssign => "**=",
            TokenType::SlashAssign => "/=",
            TokenType::DoubleSlashAssign => "//=",
            TokenType::PercentAssign => "%=",
            TokenType::BAndAssign => "&=",
            TokenType::BOrAssign => "|=",
            TokenType::BXorAssign => "^=",
            TokenType::LeftShiftAssign => "<<=",
            TokenType::RightShiftAssign => ">>=",
            TokenType::QuestionAssign => "?=",
            TokenType::DoubleQuestionAssign => "??=",
            TokenType::Or => "or",
            TokenType::And => "and",
            TokenType::Xor => "xor",
            TokenType::Not => "not",
            TokenType::In => "in",
            TokenType::Is => "is",
            TokenType::IsNot => "is not",
            TokenType::As => "as",
            TokenType::Equal => "==",
            TokenType::Unequal => "!=",
            TokenType::LessOrEqual => "<=",
            TokenType::GreaterOrEqual => ">=",
            TokenType::LessThan => "<",
            TokenType::GreaterThan => ">",
            TokenType::BOr => "|",
            TokenType::BXor => "^",
            TokenType::BAnd => "&",
            TokenType::LeftShift => "<<",
            TokenType::RightShift => ">>",
            TokenType::Plus => "+",
            TokenType::Minus => "-",
            TokenType::Positive => "+",
            TokenType::Negative => "-",
            TokenType::Asterisk => "*",
            TokenType::DoubleAsterisk => "**",
            TokenType::Spread => "...",
            TokenType::DictSpread => "**",
            TokenType::Slash => "/",
            TokenType::DoubleSlash => "//",
            TokenType::Percent => "%",
            TokenType::At => "@",
            TokenType::BNot => "~",
            TokenType::Bang => "!",
            TokenType::Indexer => "indexer",
            TokenType::Slicer => "slicer",
            TokenType::Property => "property",
            TokenType::NullableProperty => "nullable property",
            TokenType::Comma => ",",
            TokenType::Colon => ":",
            TokenType::QuestionDot => "?.",
            TokenType::Func => "fn",
            TokenType::Class => "class",
            TokenType::Enum => "enum",
            TokenType::True => "true",
            TokenType::False => "false",
            TokenType::If => "if",
            TokenType::ElIf => "elif",
            TokenType::Else => "else",
            TokenType::Match => "match",
            TokenType::For => "for",
            TokenType::While => "while",
            TokenType::Break => "break",
            TokenType::Skip => "skip",
            TokenType::Return => "return",
            TokenType::Use => "use",
            TokenType::Using => "using",
            TokenType::Comment => "comment",
            TokenType::Identifier => "identifier",
            TokenType::Integer => "integer",
            TokenType::Float => "float",
            TokenType::String => "string",
            TokenType::FString => "fstring",
            TokenType::Boolean => "boolean",
            TokenType::List => "list",
            TokenType::Dict => "dict",
            TokenType::Set => "set",
            TokenType::Tuple => "tuple",
            TokenType::Iter => "iter",
            TokenType::Pair => "pair",
            TokenType::Annotation => "annotation",
            TokenType::Call => "call",
            TokenType::Go => "go",
            TokenType::Wait => "wait",
            TokenType::Try => "try",
            TokenType::Catch => "catch",
            TokenType::Defer => "defer",
            TokenType::NewLine => "newline",
            TokenType::Indent => "indent",
            TokenType::Dedent => "dedent",
            TokenType::Finally => "finally",
            TokenType::Wildcard => "wildcard",
            TokenType::Underscore => "_",
        }
    }
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub start: usize,
    pub length: usize,
    pub typ: TokenType,
    pub message: Option<String>,
}

impl Token {
    pub fn new(start: usize, length: usize, typ: TokenType) -> Self {
        Self {
            start,
            length,
            typ,
            message: None,
        }
    }

    pub fn error(start: usize, length: usize, message: impl Into<String>) -> Self {
        Self {
            start,
            length,
            typ: TokenType::Error,
            message: Some(message.into()),
        }
    }

    pub fn end(&self) -> usize {
        self.start + self.length
    }
}
