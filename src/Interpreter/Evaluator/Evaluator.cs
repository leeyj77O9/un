using System.Text;
using Un.Object;
using Un.Object.Collections;
using Un.Object.Function;
using Un.Object.Iter;
using Un.Object.Primitive;
using Un.Object.Type;

namespace Un;

public sealed class Evaluator(Context context)
{
    private readonly Context context = context;

    public Obj Eval(Node node)
    {
        return node.Kind switch
        {
            NodeKind.Identifier => EvalIdentifier(node),

            NodeKind.Integer => Int.From((long)(node.Value ?? throw new Error("invalid int object", node, context.Source))),
            NodeKind.Boolean => Bool.From((bool)(node.Value ?? throw new Error("invalid bool object", node, context.Source))),
            NodeKind.Float => new Float((double)(node.Value ?? throw new Error("invalid float object", node, context.Source))),
            NodeKind.String => Str.From((string)(node.Value ?? throw new Error("invalid str object", node, context.Source))),
            NodeKind.FString => EvalFString(node),
            NodeKind.None => Obj.None,

            NodeKind.Assign => EvalAssign(node),

            NodeKind.Binary => EvalBinary(node),

            NodeKind.Call => EvalCall(node),
            NodeKind.Property => EvalProperty(node),
            NodeKind.Index => EvalIndex(node),
            NodeKind.Unary => EvalUnary(node),

            NodeKind.Function => EvalFunction(node),

            NodeKind.Lambda => EvalLambda(node),
            NodeKind.Match => EvalMatch(node),

            NodeKind.Go => EvalGo(node),

            NodeKind.Tuple => EvalTuple(node),
            NodeKind.List => EvalList(node),
            NodeKind.Dict => EvalDict(node),

            NodeKind.Block => EvalBlock(node),

            NodeKind.Use => EvalUse(node),
            NodeKind.Using => EvalUsing(node),

            NodeKind.While => EvalWhile(node),
            NodeKind.For => EvalFor(node),

            NodeKind.If => EvalIf(node),

            NodeKind.Return => throw new ReturnFlow(Eval(node.Children[0]), node.Start, node.Length),
            NodeKind.Skip => throw new SkipFlow(node.Start, node.Length),
            NodeKind.Break => throw new BreakFlow(node.Start, node.Length),

            NodeKind.Class => EvalClass(node),
            NodeKind.Enum => EvalEnum(node),

            _ => throw new Error($"unsupported node kind {node.Kind}", node, context.Source)
        };
    }

    private Obj EvalIdentifier(Node node)
    {
        var name = GetText(node);

        if (context.Scope.Get(name, out var value))
            return value;

        if (Global.TryGetGlobalVariable(name, out value))
            return value;

        if (Global.IsClass(name))
        {
            return new TObj(UnType.Create(name))
            {
                Annotations = Global.GetClass(name).Annotations
            };
        }

        throw new Error($"undefined variable '{name}'", node, context.Source);
    }

    private void Assign(Node target, Obj value)
    {
        switch (target.Kind)
        {
            case NodeKind.Identifier:
                {
                    var name = GetText(target);
                    context.Scope.Set(name, value);
                    return;
                }

            case NodeKind.Typed:
                {
                    Assign(target.Children[0], value);
                    return;
                }

            case NodeKind.Tuple:
                {
                    AssignTuple(target, value);
                    return;
                }
        }

        throw new Error("invalid assignment target", target, context.Source);
    }

    private void AssignTuple(Node target, Obj value)
    {
        if (value is not Tup tuple)
            throw new Error("cannot unpack non-tuple", target, context.Source);

        if (tuple.Count != target.Children.Count)
            throw new Error($"expected {target.Children.Count} values, got {tuple.Count}", target, context.Source);

        for (var i = 0; i < target.Children.Count; i++)
            Assign(target.Children[i], tuple[i]);
    }

    private Obj EvalAssign(Node node)
    {
        var pattern = node.Children[0];
        var value = Eval(node.Children[1]);

        BindPattern(pattern, value);

        return value;
    }

    private Obj EvalUnary(Node node)
    {
        if (node.Operator == TokenType.Go)
            return EvalGo(node);

        if (node.Operator == TokenType.Wait)
            return EvalWait(node);

        var value = Eval(node.Children[0]);

        var result = node.Operator switch
        {
            TokenType.Plus => value.Pos(),
            TokenType.Minus => value.Neg(),
            TokenType.Not => value.Not(),
            TokenType.BNot => value.BNot(),
            TokenType.Asterisk => value.Spread(),
            _ => throw new Error($"invalid unary operator {node.Operator}", node, context.Source)
        };
     
        return Unwrap(result, node);
    }

    private Obj EvalBinary(Node node)
    {
        var left = Eval(node.Children[0]);

        if (node.Operator == TokenType.And)
        {
            var cond = Unwrap(left.ToBool(), node).As<Bool>().Value;
            return cond ? Eval(node.Children[1]) : left;
        }

        if (node.Operator == TokenType.Or)
        {
            var cond = Unwrap(left.ToBool(), node).As<Bool>().Value;

            return cond ? left : Eval(node.Children[1]);
        }

        var right = Eval(node.Children[1]);

        var result = node.Operator switch
        {
            TokenType.Plus => left.Add(right),
            TokenType.Minus => left.Sub(right),
            TokenType.Asterisk => left.Mul(right),
            TokenType.Slash => left.Div(right),
            TokenType.DoubleSlash => left.IDiv(right),
            TokenType.Percent => left.Mod(right),
            TokenType.DoubleAsterisk => left.Pow(right),
            TokenType.BAnd => left.BAnd(right),
            TokenType.BOr => left.BOr(right),
            TokenType.BXor => left.BXor(right),
            TokenType.LeftShift => left.LShift(right),
            TokenType.RightShift => left.RShift(right),
            TokenType.Equal => left.Eq(right),
            TokenType.Unequal => left.NEq(right),
            TokenType.LessThan => left.Lt(right),
            TokenType.GreaterThan => left.Gt(right),
            TokenType.LessOrEqual => left.LtOrEq(right),
            TokenType.GreaterOrEqual => left.GtOrEq(right),
            TokenType.Xor => left.Xor(right),
            TokenType.In => right.In(left),
            TokenType.Is => right is TObj || left is TObj ? right.Is(left) : left.Is(right),
            _ => throw new Error($"invalid binary operator {node.Operator}", node, context.Source)
        };

        return Unwrap(result, node);
    }

    private Obj EvalProperty(Node node)
    {
        var target = Eval(node.Children[0]);
        var name = GetText(node.Children[1]);
        var value = target is TObj t ? Global.GetClass(t).GetAttr(name) : target.GetAttr(name);

        return Unwrap(value, node);
    }

    private Obj EvalIndex(Node node)
    {
        var target = Eval(node.Children[0]);
        var index = Eval(node.Children[1]);

        return Unwrap(target.GetItem(index), node);
    }

    private Tup EvalArguments(Node tupleNode)
    {
        var args = new List<(string Name, Obj Value)>();

        foreach (var arg in tupleNode.Children)
        {
            switch (arg.Kind)
            {
                case NodeKind.Spread:
                    var unpack = Unpack(Eval(arg.Children[0]));

                    if (unpack[0] is Err err)
                        throw new Error(err.Message, arg, context.Source, header: err.Header);

                    foreach (var value in unpack)
                        args.Add(("", value));
                    break;

                case NodeKind.KwSpread:
                    {
                        var value = Eval(arg.Children[0]);

                        if (value is not Dict dict)
                            throw new Error("** argument must be dict", arg, context.Source);

                        foreach (var (k, v) in dict.Value)
                            args.Add((k.As<Str>().Value, v));

                        break;
                    }

                case NodeKind.Pair:
                    args.Add((GetText(arg.Children[0]), Eval(arg.Children[1])));
                    break;

                default:
                    args.Add(("", Eval(arg)));
                    break;
            }
        }

        return new Tup([.. args.Select(x => x.Value)], [.. args.Select(x => x.Name)]);
    }

    private Obj EvalCall(Node node)
    {
        var callable = Eval(node.Children[0]);
        var args = EvalArguments(node.Children[1]);
        Obj result;

        context.PushFrame(new(GetText(node), context.Source, node.Start, node.Length));

        if (callable is TObj t)
            result = Global.GetClass(t).Clone().Init(args);
        else
        {
            if (!callable.Clone().As<Fn>(out var fn))
                return new Err($"unsupported operand type(s) for (): '{callable.Type}'");

            result = fn.Call(args);
        }

        result = Unwrap(result, node);

        context.PopFrame();

        return result;
    }

    private Fn EvalFunction(Node node)
    {
        var signature = node.Children[0];
        var name = GetText(signature);
        var parameters = node.Children[1];
        var body = node.Children[2];

        Fn fn = new LFn([..body.Children], context)
        {
            Name = name,
            Closure = context,
            Args = Fn.GetArgs(parameters, context),
        };

        fn = EvalAnnotations(fn, node.Annotations!).As<Fn>();

        context.Scope.Set(name, fn);

        return fn;
    }

    private Future EvalGo(Node node)
    {
        var expr = node.Children[0];

        return new Future(Task.Run(() =>
        {
            var eval = new Evaluator(context);
            return eval.Eval(expr);
        }));
    }

    private Obj EvalWait(Node node)
    {
        var value = Eval(node.Children[0]);

        if (value is not Future future)
            throw new Error("'wait' requires Future", node, context.Source);

        return future.Wait();
    }

    private bool MatchPattern(Node pattern, Obj value, Scope scope)
    {
        switch (pattern.Kind)
        {
            case NodeKind.Wildcard:
                return true;
            case NodeKind.Identifier:
                {
                    var name = GetText(pattern);
                    scope.Set(name, value);
                    return true;
                }
            case NodeKind.Typed:
                {
                    var name = pattern.Children[0];
                    var type = pattern.Children[1];

                    var nameText = GetText(name);

                    scope.Set(nameText, value);

                    return true;
                }
            case NodeKind.Tuple:
                {
                    var items = pattern.Children;

                    var values = Unpack(value);

                    if (values[0] is Err err)
                        throw new Error(err.Message, pattern, context.Source, header: err.Header);

                    if (values is null || values.Length != items.Count)
                        return false;

                    for (int i = 0; i < items.Count; i++)
                    {
                        if (!MatchPattern(items[i], values[i], scope))
                            return false;
                    }

                    return true;
                }
            case NodeKind.Integer:
            case NodeKind.Float:
            case NodeKind.String:
            case NodeKind.Boolean:
            case NodeKind.None:
                {
                    var lit = Eval(pattern);
                    return lit.Eq(value).ToBool().As<Bool>().Value;
                }
            default:
                throw new Error($"invalid pattern {pattern.Kind}", pattern, context.Source);
        }
    }

    private Obj EvalMatch(Node node)
    {
        var value = Eval(node.Children[0]);

        for (int i = 1; i < node.Children.Count; i++)
        {
            var caseNode = node.Children[i];

            var pattern = caseNode.Children[0];
            var body = caseNode.Children[1];

            var scope = new Scope(context.Scope);

            if (MatchPattern(pattern, value, scope))
            {
                context.Scope = scope;
                return Eval(body);
            }
        }

        throw new Error("no match case", node, context.Source);
    }

    private Tup EvalTuple(Node node) => new([.. node.Children.Select(Eval)]);

    private List EvalList(Node node) => new([.. node.Children.Select(Eval)]);

    private Dict EvalDict(Node node)
    {
        var dict = new Dict();

        foreach (var pair in node.Children)
        {
            var key = Eval(pair.Children[0]);
            var value = Eval(pair.Children[1]);

            dict.Value[key] = value;
        }

        return dict;
    }

    private Obj EvalBlock(Node node)
    {
        Obj result = Obj.None;

        foreach (var child in node.Children)
            result = Unwrap(Eval(child), child);

        return result;
    }

    private Obj EvalUse(Node node)
    {
        var path = GetText(node.Children[0]);
        var splited = path.Split('.');
        var lastPath = splited[^1];
        var alias = lastPath;

        if (node.Children.Count == 2)        
            alias = GetText(node.Children[1]);

        if (!Global.IsClass(lastPath))
            Global.Import(alias == "*" ? splited[..^1] : splited, alias, []);
        else
            Global.Include(path);

        return Obj.None;        
    }

    private Obj EvalUsing(Node node)
    {
        var result = EvalAssign(node.Children[0]);
        Unwrap(result.Entry(), node);

        context.Usings.Push(result);

        return result;
    }

    private Enu EvalEnum(Node node)
    {
        var enumName = GetText(node.Children[0]);
        var constants = new Map();
        var i = 0L;

        for (int j = 1; j < node.Children.Count; j++)
        {
            var member = node.Children[j];

            if (member.Kind == NodeKind.Identifier)
            {
                constants.Add(GetText(member), Int.From(i++));
            }
            else if (member.Kind == NodeKind.Assign)
            {
                var value = (long)member.Children[1].Value!;

                constants.Add(GetText(member.Children[0]), Int.From(value));

                i = value + 1;
            }
            else throw new Error("invalid enum", node, context.Source);
        }

        var enu = new Enu(UnType.Create(enumName), 0)
        {
            Members = constants
        };

        Global.SetClass(enumName, enu);

        return enu;
    }

    private Obj EvalClass(Node node)
    {
        int index = 0;

        var nameNode = node.Children[index++];
        var className = GetText(nameNode);

        Node? parameters = null;
        Node? bases = null;

        if (index < node.Children.Count && node.Children[index].Kind == NodeKind.Tuple)
            parameters = node.Children[index++];

        if (index < node.Children.Count && node.Children[index].Kind == NodeKind.ClassBases)
            bases = node.Children[index++];

        var body = node.Children[index];

        Obj type;

        if (parameters is not null)
        {
            var fields = new List<string>();

            foreach (var param in parameters.Children)
            {
                string name;
                var value = param.Children[0];

                if (value.Kind == NodeKind.Identifier)
                    name = GetText(value);
                else if (value.Kind == NodeKind.Typed)
                    name = GetText(value.Children[0]);
                else
                    throw new Error("invalid initializer", node, context.Source);
                
                fields.Add(name);
            }

            type = new Stru(UnType.Create(className), [.. fields]);
        }
        else
        {
            type = new Obj(UnType.Create(className));
        }

        type = EvalAnnotations(type, node.Annotations!);

        Global.SetClass(className, type);

        if (bases is not null)
        {
            var first = bases.Children[0];
            var firstName = GetText(first);

            if (!Global.TryGetClass(firstName, out var baseType))
                throw new Error($"unknown base class '{firstName}'", node, context.Source);

            type.Super = baseType;
            type.Types = UnionType.Create(type.Types, baseType.Types);

            foreach (var baseNode in bases.Children.Skip(1))
            {
                var baseName = GetText(baseNode);

                if (!Global.TryGetClass(baseName, out baseType))
                    throw new Error($"unknown base class '{baseName}'", node, context.Source);

                type.Types = UnionType.Create(type.Types, baseType.Types);
            }
        }

        foreach (var member in body.Children)
        {
            if (member.Kind == NodeKind.Function)
            {
                var fn = EvalFunction(member);
                type.SetAttr(GetText(member.Children[0]), fn);
            }
            else if (member.Kind == NodeKind.Assign)
            {
                var value = Eval(member.Children[1]);
                type.SetAttr(GetText(member.Children[0]), value);
            }
        }

        return type;
    }

    private Obj EvalFor(Node node)
    {
        var target = node.Children[0];
        var iterable = node.Children[1];
        var body = node.Children[2];

        var iter = Eval(iterable).Iter().As<Iters>();

        foreach (var item in iter.Value)
        {
            BindPattern(target, item);

            try
            {
                foreach (var child in body.Children)
                    Eval(child);
            }
            catch (SkipFlow)
            {
                continue;
            }
            catch (BreakFlow)
            {
                break;
            }
            catch (ReturnFlow rf)
            {
                return rf.Value;
            }
        }

        DeletePattern(target);

        return Obj.None;
    }
    
    private Obj EvalWhile(Node node)
    {
        var condition = node.Children[0];
        var block = node.Children[1];

        while (true)
        {
            var cond = Eval(condition);

            if (!cond.As<Bool>().Value)
                break;

            try
            {
                Eval(block);
            }
            catch (SkipFlow)
            {
                continue;
            }
            catch (BreakFlow)
            {
                break;
            }
            catch (ReturnFlow rf)
            {
                return rf.Value;
            }           
        }

        return Obj.None;    
    }

    private Obj EvalIf(Node node)
    {
        foreach (var child in node.Children)
        {
            if (child.Kind == NodeKind.IfCase)
            {
                var condition = child.Children[0];
                var body = child.Children[1];

                var condVal = Eval(condition).ToBool().As<Bool>().Value;

                if (condVal)
                    return Eval(body);
            }
            else if (child.Kind == NodeKind.Else)
            {
                return Eval(child.Children[0]);
            }
        }

        return Obj.None;
    }

    private LFn EvalLambda(Node node)
    {
        var parameters = node.Children[0];
        var body = node.Children[1];

        var fn = new LFn([.. body.Children], context)
        {
            Name = "lambda",
            Closure = context,
            Args = Fn.GetArgs(parameters, context),
        };

        return fn;
    }

    private Str EvalFString(Node node)
    {
        var sb = new StringBuilder();

        foreach (var child in node.Children)
        {
            if (child.Kind == NodeKind.String)
                sb.Append(child.Value);
            else
                sb.Append(Eval(child).ToStr().As<Str>().Value);
        }

        return Str.From(sb.ToString());
    }

    private Obj EvalAnnotations(Obj obj, IReadOnlyList<Node> annotations)
    {
        foreach (var annotation in annotations.AsEnumerable().Reverse())
        {
            var decorator = Eval(annotation.Children[0]);

            if (!decorator.As<Fn>(out var fn))
                throw new Error("annotation must evaluate to function", annotation, context.Source);

            obj = fn.Call(new([obj]));
        }

        return obj;
    }

    private void DeletePattern(Node pattern)
    {
        switch (pattern.Kind)
        {
            case NodeKind.Identifier:
                {
                    var name = GetText(pattern);
                    context.Scope.Remove(name);
                    break;
                }
            case NodeKind.Wildcard:
                break;
            case NodeKind.Typed:
                {
                    var nameNode = pattern.Children[0];
                    var typeNode = pattern.Children[1];

                    var name = GetText(nameNode);

                    context.Scope.Remove(name);

                    break;
                }
            case NodeKind.Tuple:
                {
                    var items = pattern.Children;

                    for (int i = 0; i < items.Count; i++)
                        DeletePattern(items[i]);
                    break;
                }
            default:
                throw new Error($"invalid pattern {pattern.Kind}", pattern, context.Source);
        }
    }

    private void BindPattern(Node pattern, Obj value)
    {
        switch (pattern.Kind)
        {
            case NodeKind.Identifier:
                {
                    var name = GetText(pattern);
                    context.Scope.Set(name, value);
                    break;
                }
            case NodeKind.Wildcard:
                break;
            case NodeKind.Typed:
                {
                    var nameNode = pattern.Children[0];
                    var typeNode = pattern.Children[1];

                    var name = GetText(nameNode);

                    context.Scope.Set(name, value);

                    break;
                }
            case NodeKind.Tuple:
                {
                    var items = pattern.Children;
                    var values = Unpack(value);

                    if (values[0] is Err err)
                        throw new Error(err.Message, pattern, context.Source, header: err.Header);

                    if (items.Count != values.Length)
                        throw new Error("tuple unpack mismatch", pattern, context.Source);

                    for (int i = 0; i < items.Count; i++)
                        BindPattern(items[i], values[i]);
                    break;
                }
            case NodeKind.Property:
                {
                    var target = Eval(pattern.Children[0]);
                    var name = GetText(pattern.Children[1]);

                    Unwrap(target.SetAttr(name, value), pattern);
                    break;
                }
            case NodeKind.Index:
                {
                    var target = Eval(pattern.Children[0]);
                    var index = Eval(pattern.Children[1]);

                    Unwrap(target.SetItem(index, value), pattern);
                    break;
                }
            default:
                throw new Error($"invalid pattern {pattern.Kind}", pattern, context.Source);
        }
    }

    private string GetText(Node node) => (string)(node.Value ?? context.Source.Code.Substring(node.Start, node.Length));

    private Obj[] Unpack(Obj obj)
    {
        return obj switch
        {
            Tup t => t.Value,
            List l => l.Value,
            _ => [new Err($"cannot unpack {obj.GetType().Name}")]
        };
    }

    private Obj Unwrap(Obj obj, Node node)
    {
        if (obj is Err err)
            throw new Error(err.Message, node, context.Source);
        return obj;
    }
}