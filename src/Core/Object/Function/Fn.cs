using Un.Object.Primitive;
using Un.Object.Collections;
using Un.Object.Type;

namespace Un.Object.Function;

public class Fn(Context closure) : Obj(UnType.Func)
{
    public string? Name { get; set; }
    public List<Arg> Args { get; set; } = [];
    public BaseType ReturnType { get; set; } = UnType.Any;
    public Context Closure { get; set; } = closure;

    protected Obj Bind(Scope scope, Tup args)
    {
        scope["self"] = Self;
        scope["super"] = Super;

        args = UnpackArgs(args);

        var unnamed = new List<Obj>();
        var extraNamed = new Map();

        for (int i = 0; i < args.Count; i++)
        {
            var name = args.Name[i];
            var val = args.Value[i];

            if (string.IsNullOrEmpty(name))
            {
                unnamed.Add(val);
            }
            else
            {
                if (scope.ContainsKeyInTop(name))
                    return new Err($"argument '{name}' provided multiple times.");
                extraNamed[name] = val;
            }
        }

        int unnamedIndex = 0;
        var argNames = new HashSet<string>();
        Arg positionalArg = Arg.Null, keywordArg = Arg.Null;

        bool positionalReached = false;

        foreach (var arg in Args)
        {
            argNames.Add(arg.Name);

            if (arg.IsPositional)
            {
                positionalArg = arg;
                positionalReached = true;
                continue;
            }

            if (arg.IsKeyword)
            {
                keywordArg = arg;
                continue;
            }

            if (arg.IsEssential)
            {
                if (unnamedIndex < unnamed.Count)
                {
                    scope[arg.Name] = unnamed[unnamedIndex++];
                }
                else if (extraNamed.TryGetValue(arg.Name, out var val))
                {
                    scope[arg.Name] = val;
                    extraNamed.Remove(arg.Name);
                }
                else
                {
                    return new Err($"missing required argument: '{arg.Name}'");
                }
            }
            else if (arg.IsOptional && !positionalReached)
            {
                if (unnamedIndex < unnamed.Count)
                {
                    scope[arg.Name] = unnamed[unnamedIndex++];
                }
                else if (extraNamed.TryGetValue(arg.Name, out var val))
                {
                    scope[arg.Name] = val;
                    extraNamed.Remove(arg.Name);
                }
                else
                {
                    scope[arg.Name] = arg.DefaultValue!;
                }
            }
            else if (extraNamed.TryGetValue(arg.Name, out var val))
            {
                scope[arg.Name] = val;
                extraNamed.Remove(arg.Name);
            }
            else
            {
                scope[arg.Name] = arg.DefaultValue!;
            }
        }

        if (unnamedIndex < unnamed.Count)
        {
            if (!positionalArg.IsNull())
            {
                var rest = unnamed.Skip(unnamedIndex);
                scope[positionalArg.Name] = new Tup([.. rest], new string[rest.Count()]);
            }
            else
            {
                return new Err("function does not accept positional arguments.");
            }
        }

        if (!positionalArg.IsNull() && !scope.ContainsKeyInTop(positionalArg.Name))
            scope[positionalArg.Name] = new Tup([], []);

        if (extraNamed.Count > 0)
        {
            if (!keywordArg.IsNull())
            {
                var dict = new Dict();
                foreach (var (k, v) in extraNamed)
                    dict.Value.Add(Str.From(k), v);
                scope[keywordArg.Name] = dict;
            }
            else
            {
                var unexpected = extraNamed.Keys.First();
                return new Err($"unexpected keyword argument: '{unexpected}'");
            }
        }

        return None;
    }

    public override Obj Repr() => Str.From($"fn({string.Join(", ", Args.Select(x => x.Type))}) -> {ReturnType}");

    public override int GetHashCode() => Name?.GetHashCode() ?? Type.GetHashCode();

    public override bool Equals(object? obj)
    {
        if (obj is not Fn other)
            return false;

        return ReferenceEquals(this, other);
    }

    public static Tup UnpackArgs(Tup rawArgs)
    {
        var objs = new List<Obj>();
        var names = new List<string>();

        for (var i = 0; i < rawArgs.Count; i++)
        {
            if (rawArgs[i] is Spreads spread)
            {
                foreach (var v in spread)
                {
                    objs.Add(v);
                    names.Add(rawArgs.Name[i]);
                }
            }
            else
            {
                objs.Add(rawArgs[i]);
                names.Add(rawArgs.Name[i]);
            }
        }

        return new([.. objs], [.. names]);
    }

    public static List<Arg> GetArgs(Node tuple, Context context)
    {
        var result = new List<Arg>();
        var eval = new Evaluator(context);

        foreach (var parameter in tuple.Children)
        {
            if (parameter.Kind != NodeKind.Parameter)
                throw new Error("invalid parameter", tuple, context.Source);

            var node = parameter.Children[0];

            string name;
            BaseType type = UnType.Any;
            bool optional = false;
            bool positional = false;
            bool keyword = false;
            Obj defaultValue = Null;

            switch (node.Kind)
            {
                case NodeKind.Identifier:
                    {
                        name = GetName(node);
                        break;
                    }

                case NodeKind.Typed:
                    {
                        name = GetName(node.Children[0]);
                        type = GetType(node.Children[1]);
                        break;
                    }

                case NodeKind.Assign:
                    {
                        optional = true;

                        var target = node.Children[0];

                        if (target.Kind == NodeKind.Typed)
                        {
                            name = GetName(target.Children[0]);
                            type = GetType(target.Children[1]);
                        }
                        else
                        {
                            name = GetName(target);
                        }

                        defaultValue = eval.Eval(node.Children[1]);
                        break;
                    }

                case NodeKind.Unary when node.Operator == TokenType.Asterisk:
                    {
                        positional = true;
                        name = GetName(node.Children[0]);
                        break;
                    }

                case NodeKind.Unary when node.Operator == TokenType.DoubleAsterisk:
                    {
                        keyword = true;
                        name = GetName(node.Children[0]);
                        break;
                    }

                default:
                    throw new Error("invalid function parameter", node, context.Source);
            }

            result.Add(new Arg(name)
            {
                Type = type,
                IsEssential = !optional && !positional && !keyword,
                IsOptional = optional,
                IsPositional = positional,
                IsKeyword = keyword,
                DefaultValue = defaultValue
            });
        }

        return result;

        string GetName(Node node) => (string)(node.Value ?? throw new Error("invalid argument name", node, context.Source));

        BaseType GetType(Node node) => UnType.Create(context.Source.Code.Substring(node.Start, node.Length));
    }
}