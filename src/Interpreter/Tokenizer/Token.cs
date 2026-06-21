namespace Un;

public struct Token(string value, TokenType type)
{
    public readonly static Token Error = new("error", TokenType.Error);
    public readonly static Token None = new("none", TokenType.None);

    public string Value = value;
    public TokenType Type = type; 

    public override readonly string ToString() => $"Token: {Value}, Type: {Type}";
}