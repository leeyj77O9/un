namespace Un;

public class Error(string message, int start, int length, Source source, string header = "error", Exception? inner = null) : Exception(message, inner)
{
    public string Header { get; } = header;
    public int Start { get; } = start;
    public int Length { get; } = length;
    public Source File { get; } = source;

    public Error(string message, Node node, Source source, string header = "error", Exception? inner = null) :
        this(message, node.Start, node.Length, source, header, inner)
    { }

    public override string ToString()
    {
        var lineText = File.GetLineText(Start);

        int lineStart = File.GetLineStart(Start);
        int column = Start - lineStart;

        int indent = 0;
        while (indent < lineText.Length && char.IsWhiteSpace(lineText[indent]))
            indent++;

        var display = lineText[indent..];
        column = Math.Max(0, column - indent);

        return
    $"""

<{File.Name}>, line [{File.GetLine(Start)}], column [{File.GetColumn(Start)}]
    {display}
    {new string(' ', column)}{new string('^', Math.Max(1, Length))}
{Header}: {Message}
""";
    }
}