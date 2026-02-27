using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Collections;
using System.Collections;
using System.Collections.Generic;

namespace Un.Object.Iter;

public class Range : Iters
{
    private long start;
    private long stop;
    private long step;

    public Range(long start, long stop, long step = 1) : base(Default(start, stop, step))
    {
        Type = "range";
        this.start = start;
        this.stop = stop;
        this.step = step;
    }

    public override Obj Len()
    {
        if ((step > 0 && start >= stop) || (step < 0 && start <= stop))
            return new Int(0);
        return new Int((stop - start + step - (step > 0 ? 1 : -1)) / step);
    }

    public override Obj Iter() => this;

    public override List ToList()
    {
        var lst = new List();
        foreach (var v in Default(start, stop, step))
            lst.Add(v);
        return lst;
    }

    public override Tup ToTuple() => ToList().ToTuple();

    public override Str ToStr() => throw new Panic("range to string not implemented");

    public override Spreads Spread() => new(ToList().Value);

    public override Obj Clone() => new Range(start, stop, step)
    {
        Annotations = Annotations
    };

    protected static IEnumerable<Obj> Default(long start, long stop, long step)
    {
        if (step == 0) throw new Panic("step cannot be zero");
        if (step > 0)
            for (long i = start; i < stop; i += step)
                yield return new Int(i);
        else
            for (long i = start; i > stop; i += step)
                yield return new Int(i);
    }
}
