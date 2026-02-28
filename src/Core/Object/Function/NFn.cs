using Un.Object.Collections;

namespace Un.Object.Function;

public class NFn : Fn
{
    public static NFn My => new()
    {
        Name = "self",
        Args = [new Arg("x") { IsEssential = true }],
        Func = (args) => args["x"],
    };

    public Func<Scope, Obj> Func { get; set; } = null!;

    public override Obj Call(Tup args)
    {
        if (Global.CallDepth++ > (int)Global.MAXRECURSIONDEPTH)
            throw new Panic("maximum recursion depth exceeded");

        var scope = new Scope(Closure ?? Scope.Empty);
        Bind(scope, args);
        
        Global.CallDepth--;
        return Func(scope) ?? None;
    }
    
    public override Obj Clone() => new NFn()
    {
        Name = Name,
        Args = [..Args],
        ReturnType = ReturnType,
        Closure = Closure,
        Func = Func,
        Self = Self,
        Super = Super?.Clone()!,
    };
}