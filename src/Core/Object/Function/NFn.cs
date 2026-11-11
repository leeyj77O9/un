using Un.Object.Collections;

namespace Un.Object.Function;

public class NFn(Context context) : Fn(context)
{
    public Func<Scope, Obj> Func { get; set; } = null!;

    public NFn() : this(new Context(Global.GetGlobalScope(), null!, null!)) { }

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

        try
        {
            var value = Func(scope);
            return value ?? None;
        }
        finally
        {
            lock (Closure.Gate)
                Closure.CallDepth--;
        }
    }
    
    public override NFn Clone() => new(Closure.Fresh())
    {
        Name = Name,
        Args = Args,
        ReturnType = ReturnType,
        Func = Func,
        Self = Self,
        Super = Super,
        Parents = [.. Parents],
        Types = Types,
        Annotations = new(Annotations),
    };
}