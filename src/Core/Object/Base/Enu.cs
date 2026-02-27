using Un.Object.Collections;
using Un.Object.Primitive;

namespace Un.Object;

public class Enu(string type, int n) : Obj(type)
{
    public int N => n;

    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => new Enu(Type, 0),
        { Count: 1 } when args[0] is Int i => new Enu(Type, (int)i.Value),
        _ => new Err($"'{Type}' takes at most 1 argument, {args.Count} given")
    };

    public override Str ToStr() => new(Type);

    public override Int ToInt() => new(N);

    public override Obj Eq(Obj other) => other switch
    {
        Int i => new Bool(N == i.Value),
        Enu e => Type == other.Type ? new Bool(N == e.N) : base.Eq(e),
        _ => new Err($"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'")
    };

    public override Obj Lt(Obj other) => other switch
    {
        Int i => new Bool(N < i.Value),
        Enu e => Type == other.Type ? new Bool(N < e.N) : base.Eq(e),
        _ => new Err($"unsupported operand type(s) for <: '{Type}' and '{other.Type}'")
    };
}
