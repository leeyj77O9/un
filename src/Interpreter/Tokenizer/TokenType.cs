namespace Un;

public enum TokenType
{
    Error,
    None,
    Null,

    LParen, RParen,
    LBrace, RBrace,
    LBrack, RBrack,

    DoubleQuestion,
    Dot, Question,

    Assign, PlusAssign, MinusAssign,
    AsteriskAssign, DoubleAsteriskAssign,
    SlashAssign, DoubleSlashAssign,
    PercentAssign,
    BAndAssign, BOrAssign, BXorAssign,
    LeftShiftAssign, RightShiftAssign,
    QuestionAssign, DoubleQuestionAssign,

    Or, And, Xor, Not,

    In, Is, As,

    Equal, Unequal,
    LessOrEqual, GreaterOrEqual,
    LessThan, GreaterThan,

    BOr, BXor, BAnd,

    LeftShift, RightShift,

    Plus, Minus,
    Positive, Negative,

    Asterisk, DoubleAsterisk,
    Spread, DictSpread,

    Slash, DoubleSlash,
    Percent, At,

    BNot, Bang,

    Indexer, Slicer,
    Property, NullableProperty,

    Comma, Colon, QuestionDot,

    Func, Class, Enum,
    True, False,
    If, ElIf, Else, Match,
    For, While,
    Break, Skip, Return,
    Use, Using,
    Comment,

    Identifier,

    Integer, Float, String, FString, Boolean,
    List, Dict, Set, Tuple, Iter,

    Pair, Annotation,

    Call,

    Go, Wait,

    Try, Defer,
}