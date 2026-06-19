using Un.Object.Collections;
using Un.Object.Type;

namespace Un.Object.Iter;

public class Reverse : Iters
{
    public Reverse() : base()
    {
        Type = UnType.Create("reverse");
    }

    public Reverse(IEnumerable<Obj> values) : base(values)
    {
        Type = UnType.Create("reverse");
    }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 1 } => new Reverse(args[0].Iter().As<Iters>().Value.Reverse()),
        _ => new Err($"invaild '{Type}' initialize"),
    };

    public override Obj Clone() => new Reverse(Value)
    {
        Annotations = Annotations
    };
}