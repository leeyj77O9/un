using Un.Object.Primitive;
using Un.Object.Type;
using Un.Reflection;

namespace Un.Object.Iter;

[NativeType(Name = "range")]
public class Range : Iters
{
    private long start;
    private long stop;
    private long step;

    public Range(long start, long stop, long step = 1) : base(Default(start, stop, step))
    {
        Type = UnType.Create("range");
        this.start = start;
        this.stop = stop;
        this.step = step;        
    }

    public override Int Len()
    {
        if ((step > 0 && start >= stop) || (step < 0 && start <= stop))
            return Int.From(0);
        return Int.From((stop - start + step - (step > 0 ? 1 : -1)) / step);
    }

    public override Range Iter() => this;

    public override Str ToStr() => step == 1
        ? Str.From($"{start}..{stop}")
        : Str.From($"{start}..{stop}..{step}");

    public override Obj Eq(Obj other) => other is Range range
        ? Bool.From(start == range.start && stop == range.stop && step == range.step)
        : base.Eq(other);

    public override Obj In(Obj obj) => obj.As<Int>(out var value)
        ? Bool.From(Contains(value.Value))
        : base.In(obj);

    private bool Contains(long value)
    {
        if (step > 0)
            return value >= start && value < stop && (value - start) % step == 0;

        return value <= start && value > stop && (start - value) % -step == 0;
    }

    public override Range Clone() => new(start, stop, step);

    protected static IEnumerable<Obj> Default(long start, long stop, long step)
    {
        if (step == 0) throw new Panic("step cannot be zero");
        if (step > 0)
            for (long i = start; i < stop; i += step)
                yield return Int.From(i);
        else
            for (long i = start; i > stop; i += step)
                yield return Int.From(i);
    }
}
