namespace Un;

public class Node(string value, TokenType type)
{
    public string Value { get; set; } = value;
    public int Index { get; set; }
    public TokenType Type { get; set; } = type;
    public List<Node> Children { get; set; } = [];

    public void Deconstruct(out string value, out TokenType type, out List<Node> children)
    {
        value = Value;
        type = Type;
        children = Children;
    }

    public string ToString(int depth) 
    {
        var buffer = $"Node: {Value}, Type: {Type}";
        if (Children.Count > 0)
        {
            buffer += "\n";
            for (var i = 0; i < Children.Count - 1; i++)
                buffer += new string(' ', depth) + $"├{Children[i].ToString(depth + 1)}\n";
            buffer += new string(' ', depth) + $"└{Children[^1].ToString(depth + 1)}";
        }
        return buffer;
    }

    public override string ToString() 
    {
        var buffer = $"Node: {Value}, Type: {Type}";
        if (Children.Count > 0)
        {
            buffer += "\n";
            for (var i = 0; i < Children.Count - 1; i++)            
                buffer += $" ├{Children[i].ToString(1)}\n";
            buffer += $" └{Children[^1].ToString(1)}";
        }
        return buffer;
    }
}