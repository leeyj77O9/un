using Un.Object.Collections;

namespace Un.Object.Function;

public class PFn(List<Node> nodes) : Fn
{
    private List<Node> nodes = nodes;

    public override Obj Call(Tup args)
    {
        if (Global.CallDepth++ > (int)Global.MAXRECURSIONDEPTH)
            throw new Panic("maximum recursion depth exceeded");

        var scope = new Scope(new Map(), Closure ?? Scope.Empty);
        Bind(scope, args);

        var parser = new Parser(new(scope, new("lambda", ["lambda"]), []));
        var returned = parser.Parse(nodes);

        Global.CallDepth--;

        return returned ?? None;
    }
    
    public override Obj Clone() => new PFn(nodes)
    {
        Name = Name,
        Args = [..Args],
        ReturnType = ReturnType,
        Closure = Closure,
        Self = Self,
        Super = Super?.Clone()!,
    };
}