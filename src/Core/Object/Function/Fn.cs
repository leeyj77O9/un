using System.Linq;
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

    public Property? Property { get; set; }

    protected Obj Bind(Scope scope, Tup args)
    {
        scope.SetLocal("self", Self);
        scope.SetLocal("super", Super);

        if (Property is not null)
            scope.SetLocal("field", new FieldRef(Self, Property));

        if (TryBindSimplePositional(scope, args, out var simpleResult))
            return simpleResult;

        args = UnpackArgs(args);

        var positional = new List<Obj>();
        var keyword = new Map();

        for (int i = 0; i < args.Count; i++)
        {
            var name = args.Name[i];
            var value = args.Value[i];

            if (string.IsNullOrEmpty(name))
            {
                positional.Add(value);
            }
            else
            {
                if (!keyword.TryAdd(name, value))
                    return new Err($"multiple values for argument '{name}'");
            }
        }

        int posIndex = 0;
        Arg starArg = Arg.Null;
        Arg kwArg = Arg.Null;
        bool keywordOnly = false;

        foreach (var arg in Args)
        {
            if (arg.Name == "self" || arg.Name == "super")
                continue;

            if (arg.IsPositional)
            {
                starArg = arg;
                keywordOnly = true;
                continue;
            }

            if (arg.IsKeyword)
            {
                kwArg = arg;
                continue;
            }

            Obj? value = null;

            if (!keywordOnly && posIndex < positional.Count)
                value = positional[posIndex++];
            else if (keyword.Remove(arg.Name, out var kwValue))
                value = kwValue;
            else if (!arg.IsEssential)
                value = arg.DefaultValue!;
            else
                return new Err($"missing required argument '{arg.Name}'");

            scope.SetLocal(arg.Name, value);
        }

        if (!starArg.IsNull())
        {
            var rest = positional.Skip(posIndex).ToArray();
            scope.SetLocal(starArg.Name, new Tup(rest));
        }
        else if (posIndex < positional.Count)
            return new Err("too many positional arguments");

        if (!kwArg.IsNull())
        {
            var dict = new Dict();

            foreach (var (k, v) in keyword)
                dict.Value[Str.From(k)] = v;

            scope.SetLocal(kwArg.Name, dict);
        }
        else if (keyword.Count > 0)
            return new Err($"unexpected keyword argument '{keyword.Keys.First()}'");

        return None;
    }

    // 바인딩 불변조건 (두 경로 모두 동일하게 적용해야 한다):
    //   1. self/super는 바인딩하지 않는다. 2. 위치 인자는 선언 순서대로 채운다.
    //   3. '*args'/'**kwargs'가 있거나 기본값이 없는 선택 인자를 건너뛰어야 하면 일반 경로가 필요하다.
    //   4. 누락/중복/알 수 없는 인자 오류 메시지는 일반 경로와 동일한 규칙을 따라야 한다.
    // TryBindSimplePositional은 이를 만족하는 좁은 경우에만 성공을 반환하는 "보수적 사전 검사"이며,
    // 판정이 애매하면 반드시 false를 반환해 Bind(일반 경로)에 넘겨야 한다.
    // 두 경로 중 한쪽만 수정하면 조용히 어긋나므로 규칙 변경 시 반드시 양쪽을 함께 수정할 것.
    private bool TryBindSimplePositional(Scope scope, Tup args, out Obj result)
    {
        for (var i = 0; i < args.Count; i++)
        {
            if (!string.IsNullOrEmpty(args.Name[i]) || args[i] is Spreads)
            {
                result = None;
                return false;
            }
        }

        var bindableCount = 0;

        for (var i = 0; i < Args.Count; i++)
        {
            var arg = Args[i];

            if (arg.Name == "self" || arg.Name == "super")
                continue;

            if (arg.IsPositional || arg.IsKeyword)
            {
                result = None;
                return false;
            }

            if (bindableCount < args.Count)
            {
                scope.SetLocal(arg.Name, args[bindableCount]);
                bindableCount++;
                continue;
            }

            if (!arg.IsEssential)
            {
                scope.SetLocal(arg.Name, arg.DefaultValue!);
                bindableCount++;
                continue;
            }

            result = new Err($"missing required argument '{arg.Name}'");
            return true;
        }

        if (args.Count > bindableCount)
        {
            result = new Err("too many positional arguments");
            return true;
        }

        result = None;
        return true;
    }

    public override Str Repr() => Str.From($"fn {Name}({string.Join(", ", Args.Select(x => $"{x.Name}: {x.Type}"))}) -> {ReturnType}");

    public override Str ToStr() => Str.From($"fn({string.Join(", ", Args.Select(x => x.Type))}) -> {ReturnType}");

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
                        defaultValue = new Tup(Array.Empty<Obj>());
                        break;
                    }

                case NodeKind.Unary when node.Operator == TokenType.DoubleAsterisk:
                    {
                        keyword = true;
                        name = GetName(node.Children[0]);
                        defaultValue = new Dict();
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