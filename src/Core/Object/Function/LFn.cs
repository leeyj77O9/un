using Un.Object.Collections;

namespace Un.Object.Function;

public class LFn(List<Node> body, Context closure) : Fn(closure)
{
    private readonly List<Node> body = body;

    public override Obj Call(Tup args)
    {
        lock (Closure.Gate)
        {
            if (Closure.CallDepth >= (int)Global.MAXRECURSIONDEPTH)
                return new Err("maximum recursion depth exceeded");

            Closure.CallDepth++;
        }

        var scope = new Scope(Closure.Scope ?? Scope.Empty, isFunctionScope: true);
        var error = Bind(scope, args);

        if (!error.IsNone())
        {
            lock (Closure.Gate)
                Closure.CallDepth--;
            return error;
        }

        var context = new Context(scope, Closure.Source, [.. Closure.Frames]);
        var evaluator = new Evaluator(context);

        try
        {
            foreach (Node node in body)
                evaluator.Eval(node);
        }
        catch (PropagateFlow pf)
        {
            return pf.Error;
        }
        catch (ReturnFlow rf)
        {
            return rf.Value;
        }
        finally
        {
            try
            {
                while (context.Defers.Count > 0)
                {
                    var node = context.Defers.Pop();
                    evaluator.Eval(node);
                }

                while (context.Usings.Count > 0)
                    context.Usings.Pop().Exit();
            }
            finally
            {
                lock (Closure.Gate)
                    Closure.CallDepth--;
            }
        }

        return None;
    }

    public override LFn Clone() => new(body, Closure.Fresh())
    {
        Name = Name,
        Args = Args,
        ReturnType = ReturnType,
        Self = Self,
        Super = Super,
        Parents = [.. Parents],
        Property = Property,
        Types = Types,
        Annotations = new(Annotations),
    };
}