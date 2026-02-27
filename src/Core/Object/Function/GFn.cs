using Un.Object.Collections;

namespace Un.Object.Function;

public class GFn(Fn func) : Fn
{
    public Fn func = func;

    public override Obj Call(Tup args) => new Future(Task.Run(() => func.Call(args)));

    public override Obj Clone() => new GFn(func)
    {
        Name = Name,
        Args = [..Args.Select(arg => arg.New() ?? throw new Panic("failed to clone argument"))],
        ReturnType = ReturnType,
        Self = Self,
        Super = Super?.Clone()!,
    };
}