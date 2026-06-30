namespace Un;

public sealed class Desugarer
{
    public Node Desugar(Node root) => Visit(root);

    private Node Visit(Node node)
    {
        return node.Kind switch
        {
            NodeKind.Block => DesugarBlock(node),
            NodeKind.If => DesugarIf(node),
            NodeKind.Assign => DesugarAssign(node),
            NodeKind.While => DesugarWhile(node),
            NodeKind.Binary => DesugarBinary(node),
            _ => DesugarChildren(node)
        };
    }

    private Node DesugarChildren(Node node)
    {
        if (node.Children == null || node.Children.Count == 0)
            return node;

        var changed = false;
        var newChildren = new Node[node.Children.Count];

        for (int i = 0; i < node.Children.Count; i++)
        {
            var c = Visit(node.Children[i]);
            newChildren[i] = c;

            if (!ReferenceEquals(c, node.Children[i]))
                changed = true;
        }

        return changed ? node.WithChildren(newChildren) : node;
    }

    private Node DesugarBlock(Node node) => DesugarChildren(node);

    private Node DesugarAssign(Node node)
    {
        var lhs = node.Children[0];
        var rhs = Visit(node.Children[1]);

        // +=, -= ...
        if (node.Operator != default)
        {
            var op = node.Operator switch
            {
                TokenType.PlusAssign => TokenType.Plus,
                TokenType.MinusAssign => TokenType.Minus,
                TokenType.AsteriskAssign => TokenType.Asterisk,
                TokenType.SlashAssign => TokenType.Slash,
                TokenType.PercentAssign => TokenType.Percent,
                TokenType.DoubleAsteriskAssign => TokenType.DoubleAsterisk,
                TokenType.DoubleSlashAssign => TokenType.DoubleSlash,
                TokenType.BAndAssign => TokenType.BAnd,
                TokenType.BOrAssign => TokenType.BOr,
                TokenType.BXorAssign => TokenType.BXor,
                _ => default
            };

            if (op != default)
            {
                rhs = new Node(node.Start, node.Length, NodeKind.Binary, op, lhs, rhs);
            }
        }

        return new Node(node.Start, node.Length, NodeKind.Assign, default, lhs, rhs);
    }

    private Node DesugarIf(Node node)
    {
        var cases = new List<Node>();
        Node? elseNode = null;

        foreach (var childRaw in node.Children)
        {
            var child = Visit(childRaw);

            if (child.Kind == NodeKind.Branch)
            {
                var cond = child.Children[0];
                var body = child.Children[1];

                cases.Add(new Node(child.Start, child.Length, NodeKind.IfCase, default, cond, body));
            }
            else if (child.Kind == NodeKind.ElIf)
            {
                var cond = child.Children[0];
                var body = child.Children[1];

                cases.Add(new Node(child.Start, child.Length, NodeKind.IfCase, default, cond, body));
            }
            else if (child.Kind == NodeKind.Else)
            {
                elseNode = new Node(child.Start, child.Length, NodeKind.Else, default, child.Children[0]);
            }
        }

        var children = new List<Node>();
        children.AddRange(cases);

        if (elseNode != null)
            children.Add(elseNode);

        return new Node(node.Start, node.Length, NodeKind.If, default, [.. children]);
    }

    private Node DesugarWhile(Node node)
    {
        var cond = Visit(node.Children[0]);
        var body = Visit(node.Children[1]);

        return new Node(node.Start, node.Length, NodeKind.While, default, cond, body);
    }

    private Node DesugarBinary(Node node)
    {
        var left = Visit(node.Children[0]);
        var right = Visit(node.Children[1]);

        return new Node(node.Start, node.Length, NodeKind.Binary, node.Operator, left, right);
    }
}