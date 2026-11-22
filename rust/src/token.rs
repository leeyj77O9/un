use std::fmt;

/// Generates `TokenType` and its diagnostic `name()` from one ordered list.
/// `stringify!` supplies the default spelling; the optional `=> "literal"`
/// override is used only where the stderr contract differs from the variant
/// name (`Eof`, `NoneKw`, `NullKw`). Never change those literals.
macro_rules! define_token_types {
    ($(
        $(#[$meta:meta])*
        $variant:ident $(=> $name:literal)?
    ),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum TokenType {
            $(
                $(#[$meta])*
                $variant,
            )*
        }
        impl TokenType {
            pub fn name(&self) -> &'static str {
                match self {
                    $(TokenType::$variant => token_name!($variant $(=> $name)?),)*
                }
            }
        }
    };
}
/// Default spelling for a token name, overridable for contract-locked names.
macro_rules! token_name {
    ($variant:ident) => { stringify!($variant) };
    ($variant:ident => $name:literal) => { $name };
}

define_token_types! {
    Eof => "EOF",
    Error,
    NoneKw => "None",
    NullKw => "Null",
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
    #[doc = r#" A `b"..."` / `b'...'` byte-string literal. The lexer keeps the whole"#]
    #[doc = r#" token (prefix + quotes) so the parser can decode escapes into bytes."#]
    Bytes,
}
impl TokenType {
    /// Assignment operators accepted by the parser, including plain `=` and
    /// `:=`. The set is exactly the parser's historical one: `?=` is absent.
    pub fn is_assignment_operator(&self) -> bool {
        matches!(
            self,
            TokenType::Assign
                | TokenType::ColonAssign
                | TokenType::PlusAssign
                | TokenType::MinusAssign
                | TokenType::AsteriskAssign
                | TokenType::SlashAssign
                | TokenType::DoubleSlashAssign
                | TokenType::DoubleAsteriskAssign
                | TokenType::PercentAssign
                | TokenType::BAndAssign
                | TokenType::BOrAssign
                | TokenType::BXorAssign
                | TokenType::LeftShiftAssign
                | TokenType::RightShiftAssign
                | TokenType::DoubleQuestionAssign
        )
    }

    /// Binary operator produced when desugaring a compound assignment.
    /// Plain assignments and `?=` have no binary counterpart.
    pub fn as_compound_binary(&self) -> Option<TokenType> {
        match self {
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
        }
    }
}
impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub start: usize,
    pub length: usize,
    pub typ: TokenType,
}
impl Token {
    pub fn new(start: usize, length: usize, typ: TokenType) -> Self {
        Self { start, length, typ }
    }
    pub fn end(&self) -> usize {
        self.start + self.length
    }
}
