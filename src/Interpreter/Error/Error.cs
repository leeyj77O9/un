namespace Un;

public class Error(string message, Context context, string header = "Error", Exception inner = null!) : Exception(message, inner)
{
    public string Header { get; } = header;
    private string code = context.File.Code.Count > context.File.Line ? context.File.Code[context.File.Line].code.Trim() : "";
    private string fileName = context.File.Name;
    private int line = context.File.Line + 1;

    public override string ToString() =>
$"""
    <{fileName}>, line [{line}] 
        {code}
        {new string('^', code.Length)}
{Header} : {Message}
""";
}
