using Un.Object.Collections;

namespace Un.Object.Function;

public class NFn(Context context) : Fn(context)
{
    public Func<Scope, Obj> Func { get; set; } = null!;

    public NFn() : this(new Context(Global.GetGlobalScope(), null!, null!)) { }

    public override Obj Call(Tup args)
    {
        if (Closure.CallDepth++ >= (int)Global.MAXRECURSIONDEPTH)
        {
            Closure.CallDepth--;
            return new Err("maximum recursion depth exceeded");
        }

        var scope = new Scope(Closure.Scope ?? Scope.Empty, isFunctionScope: true);
        var error = Bind(scope, args);

        if (!error.IsNone())
        {
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
            Closure.CallDepth--;
        }
    }
    
    public override NFn Clone() => new(Closure)
    {
        Name = Name,
        Args = Args,
        ReturnType = ReturnType,
        Func = Func,
        Self = Self,
        Super = Super,
        Parents = Parents,
    };
}