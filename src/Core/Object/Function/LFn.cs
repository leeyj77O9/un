using Un.Object.Collections;

namespace Un.Object.Function;

public class LFn(List<Node> body, Context closure) : Fn(closure)
{
    private readonly List<Node> body = body;

    public override Obj Call(Tup args)
    {
        if (Closure.CallDepth >= (int)Global.MAXRECURSIONDEPTH)
            return new Err("maximum recursion depth exceeded");

        Closure.CallDepth++;

        var scope = new Scope(Closure.Scope ?? Scope.Empty);
        var error = Bind(scope, args);

        if (!error.IsNone())
            return error;

        var context = new Context(scope, Closure.Source, Closure.Frames);
        var evaluator = new Evaluator(context);

        try
        {

            foreach (Node node in body)
                evaluator.Eval(node);
        }
        catch (ReturnFlow rf)
        {
            return rf.Value;
        }
        finally
        {
            if (context.Defers is { Count: > 0 })
            {
                foreach(Node node in context.Defers)
                    evaluator.Eval(node);
            }

            if (context.Usings is { Count: > 0 })
            {
                foreach (var obj in context.Usings)
                    obj.Exit();
            }

            Closure.CallDepth--;
        }

        return None;
    }

    public override Obj Clone() => new LFn(body, Closure)
    {
        Name = Name,
        Args = [..Args],
        ReturnType = ReturnType,
        Self = Self,
        Super = Super?.Clone()!,
    };
}