namespace Un;

public enum NodeKind
{
    Identifier,
    Integer,
    Float,
    String,
    FString,
    Boolean,
    None,   

    Tuple,
    List,
    Dict,
    Set,

    Unary,
    Binary,

    Call,
    Property,
    Index,
    Slice,

    Assign, PlusAssign, MinusAssign,
    AsteriskAssign, DoubleAsteriskAssign,
    SlashAssign, DoubleSlashAssign,
    PercentAssign,
    BAndAssign, BOrAssign, BXorAssign,
    LeftShiftAssign, RightShiftAssign,
    QuestionAssign, DoubleQuestionAssign,

    Function,

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
    Branch
}