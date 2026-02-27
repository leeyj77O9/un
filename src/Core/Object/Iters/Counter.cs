using Un.Object.Primitive;
using Un.Object.Collections;
using System.Collections;

namespace Un.Object.Iter;

public class Counter : Iters
{
    protected long current = 0;

    public Counter(long start = 0)
    {
        Type = "counter";
        current = start;
        Value = Default(start);
    }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => new Counter(),
        _ => new Err($"invaild '{Type}' initialize"),
    };

    public override Obj Len() => new Int(long.MaxValue);

    public override Obj Iter() => this;

    public override List ToList() => throw new Panic("counter is infinite");

    public override Tup ToTuple() => throw new Panic("counter is infinite");

    public override Str ToStr() => throw new Panic("counter is infinite");

    public override Spreads Spread() => throw new Panic("counter is infinite");

    public override Obj Clone() => new Counter(current)
    {
        Annotations = Annotations
    };

    protected IEnumerable<Obj> Default(long start)
    {
        long i = start;
        while (true)
            yield return new Int(i++);
    }
}