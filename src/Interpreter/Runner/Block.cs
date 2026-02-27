namespace Un;

public class Block(string type, string code, int line, Scope scope)
{
    public string Type { get; set; } = type;
    public string Code { get; set; } = code;
    public int Line { get; set; } = line;
    public Scope scope { get; } = scope;
}