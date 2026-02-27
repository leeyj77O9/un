using Un.Object.Primitive;
using Un.Object.Collections;

namespace Un.Object.Iter;

public class Reverse : Iters
{
    public Reverse(IEnumerable<Obj> values) : base(values)
    {
        Type = "reverse";
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