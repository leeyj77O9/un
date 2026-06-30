using Un.Util;

namespace Un;

public struct Token(int start, int length, TokenType type) : ISpan
{
    public int Start = start;
    public int Length = length;
    public TokenType Type = type; 
    public string Message = string.Empty;

    readonly int ISpan.Start => Start;
    readonly int ISpan.Length => Length;

    public readonly string ToString(string code) => $"Token: {code.Substring(Start, Length)}, Type: {Type}";
}