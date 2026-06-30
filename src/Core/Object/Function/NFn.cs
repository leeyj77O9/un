using Un.Object.Collections;

namespace Un.Object.Function;

public class NFn(Context context) : Fn(context)
{
    public static NFn My => new()
    {
        Name = "self",
        Args = [new Arg("x") { IsEssential = true }],
        Func = (args) => args["x"],
    };

    public Func<Scope, Obj> Func { get; set; } = null!;

    public NFn() : this(new Context(Global.GetGlobalScope(), null!, null!)) { }

    public override Obj Call(Tup args)
    {
        if (Closure.CallDepth++ > (int)Global.MAXRECURSIONDEPTH)
            return new Err("maximum recursion depth exceeded");

        var scope = new Scope(Closure.Scope ?? Scope.Empty);
        var error = Bind(scope, args);

        if (!error.IsNone())
            return error;

        var value = Func(scope);

        Closure.CallDepth--;
        return value ?? None;
    }
    
    public override Obj Clone() => new NFn(Closure)
    {
        Name = Name,
        Args = [..Args],
        ReturnType = ReturnType,
        Func = Func,
        Self = Self,
        Super = Super?.Clone()!,
    };
}