namespace Un;

public class Error(string message, int start, int length, Source source, string header = "Error", Exception? inner = null) : Exception(message, inner)
{
    public string Header { get; } = header;
    public int Start { get; } = start;
    public int Lenght { get; } = length;
    public Source File { get; } = source; 

    public Error(string message, Node node, Source source, string header = "Error", Exception? inner = null) : 
        this(message, node.Start, node.Length, source, header, inner) { }

    public override string ToString() =>
$"""

<{File.Name}>, line [{File.GetLine(Start)}], column [{File.GetColumn(Start)}]
    {File.GetLineText(Start)}
    {new string(' ', Start - File.GetLineStart(Start))}{new string('^', Lenght)}
{Header}: {Message}
""";
}