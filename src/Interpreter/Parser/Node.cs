using Un.Util;

namespace Un;

public sealed class Node(int start, int length, NodeKind kind, TokenType op = default, params Node[] children) : ISpan
{
    public int Start { get; } = start;
    public int Length { get; } = length;
    public NodeKind Kind { get; } = kind;
    public TokenType Operator { get; } = op;
    public IReadOnlyList<Node> Children { get; } = children;
    public object? Value { get; init; }
    public IReadOnlyList<Node>? Annotations { get; init; }

    public Node WithChildren(IReadOnlyList<Node> newChildren) => new(Start, Length, Kind, Operator, children: [.. newChildren])
    {
        Annotations = Annotations,
        Value = Value
    };

    public static Node Const(object value, int start, int length)
    {
        return value switch
        {
            int i => new Node(start, length, NodeKind.Integer) { Value = i },
            long l => new Node(start, length, NodeKind.Integer) { Value = l },
            double d => new Node(start, length, NodeKind.Float) { Value = d },
            float f => new Node(start, length, NodeKind.Float) { Value = f },
            string s => new Node(start, length, NodeKind.String) { Value = s },
            bool b => new Node(start, length, NodeKind.Boolean) { Value = b },

            null => new Node(start, length, NodeKind.None),

            _ => throw new Panic($"unsupported const type: {value.GetType()}")
        };
    }


}