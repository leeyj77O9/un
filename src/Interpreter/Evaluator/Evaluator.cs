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
            NodeKind.Tagged => EvalTaggedTemplate(node),
            NodeKind.None => Obj.None,

            NodeKind.Assign => EvalAssign(node),

            NodeKind.Binary => EvalBinary(node),

            NodeKind.Call => EvalCall(node),
            NodeKind.Property => EvalProperty(node),
            NodeKind.Index => EvalIndex(node),
            NodeKind.Slice => EvalSlice(node),
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

            NodeKind.Try => EvalTry(node),
            NodeKind.Defer => EvalDefer(node),

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

        if (node.Operator == TokenType.Question)
        {
            Obj value;
            try
            {
                value = Eval(node.Children[0]);
            }
            catch (Error e)
            {
                var err = new Err(e.Message, e.Header);
                if (context.Frames.Count > 0)
                    throw new PropagateFlow(err, node.Start, node.Length);
                throw;
            }
            if (value is Err err2)
            {
                if (context.Frames.Count > 0)
                    throw new PropagateFlow(err2, node.Start, node.Length);
                throw new Error(err2.Message, node.Start, node.Length, context.Source, header: err2.Header);
            }
            return value;
        }

        var value2 = Eval(node.Children[0]);

        var result = node.Operator switch
        {
            TokenType.Plus => value2.Pos(),
            TokenType.Minus => value2.Neg(),
            TokenType.Not => value2.Not(),
            TokenType.BNot => value2.BNot(),
            TokenType.Asterisk => value2.Spread(),
            _ => throw new Error($"invalid unary operator {node.Operator}", node, context.Source)
        };
      
        return Unwrap(result, node);
    }

    private Obj EvalBinary(Node node)
    {
        var left = Eval(node.Children[0]);

        if (node.Operator == TokenType.And)
        {
            var cond = Unwrap(left.ToBool(), node).As<Bool>(out var condValue) && condValue.Value;
            return cond ? Eval(node.Children[1]) : left;
        }

        if (node.Operator == TokenType.Or)
        {
            var cond = Unwrap(left.ToBool(), node).As<Bool>(out var condValue) && condValue.Value;

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
            TokenType.IsNot => NegateIs(right is TObj || left is TObj ? right.Is(left) : left.Is(right)),
            _ => throw new Error($"invalid binary operator {node.Operator}", node, context.Source)
        };

        return Unwrap(result, node);
    }

    private static Obj NegateIs(Obj o) => o is Bool b ? Bool.From(!b.Value) : o;

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

    private Obj EvalSlice(Node node)
    {
        var target = Eval(node.Children[0]);
        Obj? startObj = node.Children.Count > 1 && node.Children[1] != null ? Eval(node.Children[1]) : null;
        Obj? endObj = node.Children.Count > 2 && node.Children[2] != null ? Eval(node.Children[2]) : null;
        Obj? stepObj = node.Children.Count > 3 && node.Children[3] != null ? Eval(node.Children[3]) : null;

        return Unwrap(target.Slice(startObj, endObj, stepObj), node);
    }

    private Tup EvalArguments(Node tupleNode)
    {
        var values = new List<Obj>(tupleNode.Children.Count);
        var names = new List<string>(tupleNode.Children.Count);

        foreach (var arg in tupleNode.Children)
        {
            switch (arg.Kind)
            {
                case NodeKind.Spread:
                    {
                        var unpack = Unpack(Eval(arg.Children[0]));

                        if (unpack.Length == 0)
                            break;

                        if (unpack[0] is Err err)
                            throw new Error(err.Message, arg, context.Source, header: err.Header);

                        foreach (var value in unpack)
                        {
                            values.Add(value);
                            names.Add(string.Empty);
                        }
                        break;
                    }

                case NodeKind.KwSpread:
                    {
                        var value = Eval(arg.Children[0]);

                        if (value is not Dict dict)
                            throw new Error("** argument must be dict", arg, context.Source);

                        foreach (var (key, item) in dict.Value)
                        {
                            values.Add(item);
                            names.Add(key.As<Str>(out var str) ? str.Value : key.Repr().Value);
                        }
                        break;
                    }

                case NodeKind.Pair:
                    values.Add(Eval(arg.Children[1]));
                    names.Add(GetText(arg.Children[0]));
                    break;

                default:
                    {
                        var value = Eval(arg);

                        if (value is Err err)
                            throw new Error(err.Message, arg, context.Source, header: err.Header);

                        values.Add(value);
                        names.Add(string.Empty);
                        break;
                    }
            }
        }

        return new Tup([.. values], [.. names]);
    }

    private Obj EvalCall(Node node)
    {
        var callable = Eval(node.Children[0]);
        var args = EvalArguments(node.Children[1]);
        Obj result;

        context.PushFrame(new(GetText(node), context.Source, node.Start, node.Length));

        try
        {
            result = Unwrap(callable.Call(args), node);
        }
        finally
        {
            context.PopFrame();
        }

        return result;
    }

    private Obj EvalFunction(Node node)
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

        if (!EvalAnnotations(fn, node.Annotations!).As<Fn>(out var afn))
            return new Err("annotation error");
        
        fn = afn;

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
                    return lit.Eq(value).ToBool().As<Bool>(out var isEqual) && isEqual.Value;
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

    private Tup EvalTuple(Node node)
    {
        List<KeyValuePair<string, Obj>> values = [];

        foreach (var child in node.Children)
        {
            if (child.Kind == NodeKind.Pair)
            {
                var name = GetText(child.Children[0]);
                var value = Eval(child.Children[1]);

                values.Add(new(name, value));

            }
            else
            {
                values.Add(new(string.Empty, Eval(child)));
            }
        }

        return new Tup(values);
    }

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
        var parts = path.Split('.');
        var last = parts[^1];

        Node? imports = null;
        string? moduleAlias = null;

        foreach (var child in node.Children.Skip(1))
        {
            switch (child.Kind)
            {
                case NodeKind.Tuple:
                    imports = child;
                    break;

                case NodeKind.Identifier:
                    moduleAlias = GetText(child);
                    break;
            }
        }

        List<(string Name, string Alias)>? list = null;

        if (imports != null)
        {
            list = [];

            foreach (var child in imports.Children)
            {
                if (child.Kind == NodeKind.Pair)
                {
                    list.Add((
                        GetText(child.Children[0]),
                        GetText(child.Children[1])
                    ));
                }
                else
                {
                    var name = GetText(child);
                    list.Add((name, name));
                }
            }
        }

        if (Global.IsNative(last))
            Global.Include(last, moduleAlias, list);
        else
            Global.Import(parts, context.Source, moduleAlias, list);

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

        if (!Eval(iterable).Iter().As<Iters>(out var iter))
            throw new Error($"{GetText(iterable)} is not iterable", node, context.Source);

        foreach (var item in iter.Value)
        {
            BindPattern(target, item, localOnly: true);

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

            if (!cond.As<Bool>(out var condValue) || !condValue.Value)
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

                var condVal = Eval(condition).ToBool().As<Bool>(out var condValue) && condValue.Value;

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

    private Obj EvalTry(Node node)
    {
        var tryBlock = node.Children[0];
        var catches = new List<Node>();
        Node? finallyNode = null;
        for (int i = 1; i < node.Children.Count; i++)
        {
            var child = node.Children[i];
            if (child.Kind == NodeKind.Catch)
                catches.Add(child);
            else if (child.Kind == NodeKind.Finally)
                finallyNode = child;
        }

        Obj result = Obj.None;
        bool hasError = false;
        Error? pendingError = null;

        try
        {
            result = Eval(tryBlock);
        }
        catch (Error e)
        {
            hasError = true;
            pendingError = e;
            bool caught = false;
            foreach (var catchNode in catches)
            {
                Node block;
                string? varName = null;
                if (catchNode.Children.Count == 2)
                {
                    varName = GetText(catchNode.Children[0]);
                    block = catchNode.Children[1];
                }
                else
                {
                    block = catchNode.Children[0];
                }

                var catchScope = new Scope(context.Scope);
                var prevScope = context.Scope;
                context.Scope = catchScope;
                if (varName != null)
                    catchScope.Set(varName, new Err(e.Message, e.Header));

                try
                {
                    result = Eval(block);
                    caught = true;
                    hasError = false;
                    pendingError = null;
                    break;
                }
                catch (Error)
                {
                    hasError = true;
                    throw;
                }
                finally
                {
                    context.Scope = prevScope;
                }
            }
            if (!caught)
                throw;
        }
        finally
        {
            if (finallyNode != null)
            {
                var finallyBlock = finallyNode.Children[0];
                try
                {
                    Eval(finallyBlock);
                }
                catch (Error)
                {
                    throw;
                }
                if (hasError && pendingError != null)
                    throw pendingError;
            }
            else if (hasError && pendingError != null)
            {
            }
        }

        return result;
    }

    private Obj EvalDefer(Node node)
    {
        var block = node.Children[0];
        context.Defers.Push(block);
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
            if (child.Kind != NodeKind.String)
            {
                Obj o = Eval(child).ToStr();

                if (!o.As<Str>(out var str))
                    throw new Error("f-string expression must be a string", node, context.Source);

                sb.Append(str.Value);
            }
            else
                sb.Append(child.Value);
        }

        return Str.From(sb.ToString());
    }

    private Template BuildTemplate(Node node)
    {
        var strings = new List<string>();
        var values = new List<Obj>();

        foreach (var child in node.Children)
        {
            if (child.Kind == NodeKind.String)
            {
                strings.Add((string)child.Value!);
            }
            else
            {
                values.Add(Eval(child));
            }
        }

        return new Template(strings, values);
    }

    private Obj EvalTaggedTemplate(Node node)
    {
        var tag = Eval(node.Children[0]);

        if (!tag.As<Fn>(out var fn))
            throw new Error("tagged template target must be a function", node, context.Source);

        var template = BuildTemplate(node.Children[1]);

        return fn.Call(new([new List([..template.Values]), new List([..template.Strings.Select(Str.From)])]));
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

    private void BindPattern(Node pattern, Obj value, bool localOnly = false)
    {
        switch (pattern.Kind)
        {
            case NodeKind.Identifier:
                {
                    var name = GetText(pattern);
                    if (localOnly)
                        context.Scope.SetLocalOrDeclare(name, value);
                    else
                        context.Scope.SetLocalOrDeclare(name, value);
                    break;
                }
            case NodeKind.Wildcard:
                break;
            case NodeKind.Typed:
                {
                    var nameNode = pattern.Children[0];
                    var typeNode = pattern.Children[1];

                    var name = GetText(nameNode);

                    if (localOnly)
                        context.Scope.SetLocalOrDeclare(name, value);
                    else
                        context.Scope.SetLocalOrDeclare(name, value);

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
            case NodeKind.Slice:
                {
                    var target = Eval(pattern.Children[0]);
                    if (target is not List list)
                        throw new Error("slice assignment only supports list", pattern, context.Source);

                    Obj startObj = pattern.Children.Count > 1 && pattern.Children[1] != null ? Eval(pattern.Children[1]) : null!;
                    Obj endObj = pattern.Children.Count > 2 && pattern.Children[2] != null ? Eval(pattern.Children[2]) : null!;
                    Obj stepObj = pattern.Children.Count > 3 && pattern.Children[3] != null ? Eval(pattern.Children[3]) : null!;

                    int start = 0, end = list.Count, step = 1;
                    if (startObj != null)
                    {
                        if (!startObj.As<Int>(out var s)) throw new Error("slice start must be int", pattern, context.Source);
                        start = (int)s.Value; if (start < 0) start += list.Count; start = Math.Clamp(start, 0, list.Count);
                    }
                    if (endObj != null)
                    {
                        if (!endObj.As<Int>(out var e)) throw new Error("slice end must be int", pattern, context.Source);
                        end = (int)e.Value; if (end < 0) end += list.Count; end = Math.Clamp(end, 0, list.Count);
                    }
                    if (stepObj != null)
                    {
                        if (!stepObj.As<Int>(out var st)) throw new Error("slice step must be int", pattern, context.Source);
                        step = (int)st.Value; if (step != 1) throw new Error("slice assignment with step != 1 not supported", pattern, context.Source);
                    }
                    if (end < start) end = start;
                    int sliceLen = end - start;

                    if (!value.ToList().As<List>(out var rhs))
                        throw new Error("slice assignment value must be list", pattern, context.Source);

                    int common = Math.Min(sliceLen, rhs.Count);

                    for (int i = 0; i < common; i++)
                        Unwrap(list.SetItem(Int.From(start + i), rhs[i]), pattern);

                    for (int i = common; i < rhs.Count; i++)
                    {
                        var res = List.Insert(list, rhs[i], Int.From(start + i));
                        if (res is Err err) throw new Error(err.Message, pattern, context.Source, header: err.Header);
                    }

                    for (int i = rhs.Count; i < sliceLen; i++)
                    {
                        var res = List.RemoveAt(list, Int.From(start + rhs.Count));
                        if (res is Err err) throw new Error(err.Message, pattern, context.Source, header: err.Header);
                    }
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
            Spreads s => s.Value,
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