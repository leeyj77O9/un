using Un.Object;
using Un.Object.Primitive;
using Un.Object.Collections;

namespace Un;

public static class Convert
{
    public static List ToList(Node node, Context context)
    {
        List list = [];

        foreach (var value in node.Children.Split(TokenType.Comma).Select(x => Operator.On(x, context)))
        {
            if (value.As<Spreads>(out var spread))
                foreach (var item in spread)
                    list.Append(item);
            else list.Append(value);
        }

        return list;
    }

    public static Tup ToTuple(Node node, Context context)
    {
        var list = new List();
        var names = new List<string>();
        var splited = node.Children.Split(TokenType.Comma).Select(x => x.Split(TokenType.Assign)).ToList();

        foreach (var pair in splited)
        {
            var name = "";
            var value = Obj.None;

            if (pair.Count == 1)
            {
                value = Operator.On(pair[0], context);
            }
            else if (pair.Count == 2)
            {
                name = pair[0].Split(TokenType.Colon)[0][0].Value;
                value = Operator.On(pair[1], context);
            }

            names.Add(name);
            if (value.As<Spreads>(out var spread))
                foreach (var item in spread)
                    list.Append(item);
            else list.Append(value);
        }

        return new Tup([.. list], [.. names]);
    }

    public static Tup ToIndex(Node node, Context context) => new([.. node.Children.Split(TokenType.Colon).Select(x => Operator.On(x, context))], []);

    public static Tup ToPair(Node node, Context context)
    {
        var temp = node.Children.Split(TokenType.Colon).ToList();
        return new([new Str(temp[0][0].Value), Operator.On(temp[1], context)], []);
    }

    public static Dict ToDict(Node node, Context context) => new(node.Children.Split(TokenType.Comma).Select(x => ToPair(new("pair", TokenType.Pair) { Children = x }, context)).Select(y => (y[0], y[1])).ToDictionary());

    public static Set ToSet(Node node, Context context) => new([.. ToTuple(node, context).Value]);

    public static Obj ToFStr(Node node, Context context)
    {
        var src = node.Value;
        int len = src.Length, depth = 0, i = 0, j = 0;
        var tokenizer = new Tokenizer();
        var lexer = new Lexer();

        List<string> raw = [""];
        List values = [];

        while (i < len)
        {
            char c = src[i];

            if (c == '{')
            {
                var expr = "";
                depth++;
                while (i < len)
                {
                    c = src[++i];
                    depth = c switch
                    {
                        '{' => depth + 1,
                        '}' => depth - 1,
                        _ => depth
                    };

                    if (depth == 0)
                        break;

                    expr += c;
                }
                values.Append(Operator.On(lexer.Lex(tokenizer.Tokenize(new("", [expr]))), context));
                raw.Add("");
                j++;
            }
            else raw[j] += c;

            i++;
        }

        return node.Children.Count == 0 ? Mix(raw, values)
            : context.Scope[$"{node.Children[0].Value}"].Call(new Tup([new Tup([.. raw.Select(x => new Str(x))], [""]), .. values.Value[..values.Count]], ["", .. new string(' ', values.Count).Split()]));
    }

    public static Obj Auto(Node node, Context context)
    {
        var (value, type, _) = node;

        if (type == TokenType.Integer)
            return Int.From(System.Convert.ToInt64(value));
        else if (type == TokenType.None)
            return Obj.None;
        else if (type == TokenType.Float)
            return new Float(System.Convert.ToDouble(value));
        else if (type == TokenType.Boolean && bool.TryParse(value, out var boolValue))
            return Bool.From(boolValue);
        else if (type == TokenType.String)
            return new Str(value);
        else if (DateTime.TryParse(value, out var dateValue))
            return new Date(dateValue);
        else if (type == TokenType.List)
            return ToList(node, context);
        else if (type == TokenType.Tuple)
            return ToTuple(node, context);
        else if (type == TokenType.Dict)
            return ToDict(node, context);
        else if (type == TokenType.Set)
            return ToSet(node, context);
        else if (type == TokenType.FString)
            return ToFStr(node, context);
        else
            return new Err($"conversion for {node.Type} is not implemented.");

    }

    private static Str Mix(List<string> raw, List values)
    {
        if (raw.Count == 1) return new Str(raw[0]);

        string mixed = "";

        for (int i = 0; i < raw.Count - 1; i++)
        {
            mixed += raw[i];
            mixed += values[i].ToStr().As<Str>().Value;
        }

        return new Str(mixed + raw[^1]);
    }

    public static List<List<Node>> Split(this List<Node> nodes, TokenType type)
    {
        List<List<Node>> splited = [];
        int start = 0, end = 0;

        while (end < nodes.Count)
        {
            if (nodes[end].Type == type)
            {
                splited.Add(nodes[start..end]);
                start = end + 1;
            }
            end++;
        }

        if (start != end)
            splited.Add(nodes[start..end]);

        return splited;
    }

    public static List<List<Node>> Split(this List<Node> nodes, params TokenType[] types)
    {
        List<List<Node>> splited = [];
        int start = 0, end = 0;

        while (end < nodes.Count)
        {
            if (types.Contains(nodes[end].Type))
            {
                splited.Add(nodes[start..end]);
                start = end + 1;
            }
            end++;
        }

        if (start != end)
            splited.Add(nodes[start..end]);

        return splited;
    }

    public static string ToCode(this List<Node> nodes)
    {
        List<char> code = [];

        foreach (var node in nodes)
        {
            if (node.Type == TokenType.String)
            {
                code.Add('"');
                code.AddRange(node.Value.ToCharArray());
                code.Add('"');
            }
            else if (node.Type == TokenType.Property)
            {
                code.Add('.');
                code.AddRange(node.Value.ToCharArray());
            }
            else if (node.Type == TokenType.NullableProperty)
            {
                code.Add('?');
                code.Add('.');
                code.AddRange(node.Value.ToCharArray());
            }
            else
                code.AddRange(node.Value.ToCharArray());
        }

        return string.Join("", code);
    }
}