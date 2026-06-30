namespace Un;

public static class Optimizer
{
    public static Node Optimize(Node root)
    {
        while (true)
        {
            var before = root;

            root = OptimizePass(root);

            if (ReferenceEquals(before, root))
                break;
        }

        return root;
    }

    private static Node OptimizePass(Node node)
    {
        if (node == null) return null!;

        node = OptimizeChildren(node);

        return node.Kind switch
        {
            NodeKind.Binary => OptimizeBinary(node),
            NodeKind.If => OptimizeIf(node),
            NodeKind.While => OptimizeWhile(node),
            _ => node
        };
    }

    private static Node OptimizeChildren(Node node)
    {
        if (node.Children == null || node.Children.Count == 0)
            return node;

        var changed = false;
        var newChildren = new Node[node.Children.Count];

        for (int i = 0; i < node.Children.Count; i++)
        {
            var opt = OptimizePass(node.Children[i]);
            newChildren[i] = opt;

            if (!ReferenceEquals(opt, node.Children[i]))
                changed = true;
        }

        return changed ? node.WithChildren(newChildren) : node;
    }

    private static Node OptimizeBinary(Node node)
    {
        var left = node.Children[0];
        var right = node.Children[1];

        left = OptimizePass(left);
        right = OptimizePass(right);

        if (IsConst(left, out var a) && IsConst(right, out var b))
            return Node.Const(Eval(node.Operator, a, b), node.Start, node.Length);

        switch (node.Operator)
        {
            case TokenType.Plus:
                if (IsZero(left)) return right;
                if (IsZero(right)) return left;
                break;

            case TokenType.Minus:
                if (IsZero(right)) return left;
                break;

            case TokenType.Asterisk:
                if (IsZero(left) || IsZero(right))
                    return Node.Const(0, node.Start, node.Length);
                if (IsOne(left)) return right;
                if (IsOne(right)) return left;
                break;

            case TokenType.Slash:
                if (IsOne(right)) return left;
                break;
        }

        if (node.Operator == TokenType.Asterisk)
            if (IsConst(right, out var v) && v is long l && l == 2)
                return new Node(node.Start, node.Length, NodeKind.Binary, TokenType.Plus, left, left);

        return node;
    }

    private static Node OptimizeIf(Node node)
    {
        var changed = false;
        var children = new List<Node>(node.Children.Count);

        foreach (var child in node.Children)
        {
            if (child.Kind == NodeKind.IfCase)
            {
                var condition = OptimizePass(child.Children[0]);
                var body = OptimizePass(child.Children[1]);

                if (IsConst(condition, out var v) && v is bool b)
                {
                    changed = true;

                    if (b)
                        return body;

                    continue;
                }

                if (!ReferenceEquals(condition, child.Children[0]) || !ReferenceEquals(body, child.Children[1]))
                {
                    changed = true;

                    children.Add(new Node(child.Start, child.Length, NodeKind.IfCase, default, condition, body));
                }
                else
                {
                    children.Add(child);
                }
            }
            else 
            {
                var body = OptimizePass(child.Children[0]);

                if (!ReferenceEquals(body, child.Children[0]))
                {
                    changed = true;

                    children.Add(new Node(child.Start, child.Length, NodeKind.Else, default, body));
                }
                else
                {
                    children.Add(child);
                }
            }
        }

        if (children.Count == 0)
            return new Node(node.Start, 0, NodeKind.Block);

        if (children.Count == 1 && children[0].Kind == NodeKind.Else)
            return children[0].Children[0];

        if (!changed)
            return node;

        return node.WithChildren(children);
    }
    private static Node OptimizeWhile(Node node)
    {
        var cond = node.Children[0];

        if (IsConst(cond, out var v) && v is bool b && !b)
            return new Node(0, 0, NodeKind.Block);

        return node;
    }

    private static bool IsConst(Node n, out object v)
    {
        v = n.Value!;
        return n.Kind is NodeKind.Integer or NodeKind.Float or NodeKind.String or NodeKind.Boolean;
    }

    private static bool IsZero(Node n) => IsConst(n, out var v) && v is long l && l == 0;

    private static bool IsOne(Node n) => IsConst(n, out var v) && v is long l && l == 1;

    private static object Eval(TokenType op, object a, object b)
    {
        return (a, b) switch
        {
            (long x, long y) => EvalLong(op, x, y),

            (double x, double y) => EvalDouble(op, x, y),
            (long x, double y) => EvalDouble(op, x, y),
            (double x, long y) => EvalDouble(op, x, y),

            _ => throw new Panic($"unsupported optimize types: {a.GetType().Name}, {b.GetType().Name}")
        };
    }

    private static object EvalLong(TokenType op, long a, long b)
    {
        return op switch
        {
            TokenType.Plus => a + b,
            TokenType.Minus => a - b,
            TokenType.Asterisk => a * b,
            TokenType.Slash => a / b,
            TokenType.Percent => a % b,

            TokenType.Equal => a == b,
            TokenType.Unequal => a != b,

            TokenType.GreaterThan => a > b,
            TokenType.LessThan => a < b,
            TokenType.GreaterOrEqual => a >= b,
            TokenType.LessOrEqual => a <= b,

            _ => throw new Panic($"invalid optimize operator '{op}' for long")
        };
    }

    private static object EvalDouble(TokenType op, double a, double b)
    {
        return op switch
        {
            TokenType.Plus => a + b,
            TokenType.Minus => a - b,
            TokenType.Asterisk => a * b,
            TokenType.Slash => a / b,
            TokenType.Percent => a % b,

            TokenType.Equal => a == b,
            TokenType.Unequal => a != b,

            TokenType.GreaterThan => a > b,
            TokenType.LessThan => a < b,
            TokenType.GreaterOrEqual => a >= b,
            TokenType.LessOrEqual => a <= b,

            _ => throw new Panic($"invalid optimize operator '{op}' for double")
        };
    }

    private static string Hash(Node node)
    {
        if (node.Children == null || node.Children.Count == 0)
            return $"{node.Kind}:{node.Value}";

        return $"{node.Kind}:{node.Operator}:{Hash(node.Children[0])}:{Hash(node.Children[1])}";
    }

    private class Env
    {
        public Dictionary<string, object> Const = [];
    }
}