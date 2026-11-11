using Un.Object.Collections;
using Un.Object.Primitive;
using Un.Object.Type;
using Un.Reflection;

namespace Un.Object.Flow;

[NativeType(Name = "atomic_bool")]
public class AtomicBool : Obj
{
    private int value;

    public AtomicBool() : this(false) { }

    public AtomicBool(bool initial) : base(UnType.Create("atomic_bool"))
    {
        value = initial ? 1 : 0;
    }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => new AtomicBool(),
        { Count: 1 } and [Bool b] => new AtomicBool(b.Value),
        _ => new Err("atomic_bool takes at most one argument"),
    };

    public override Obj Entry() => this;

    public override Obj Copy() => this;

    public override Obj Clone() => new Err("'atomic_bool' cannot be cloned");

    [Native(
        Name = "get",
        Description = "Returns the current value.",
        Example = "flag = atomic.bool(false)\nflag.get()",
        ReturnType = "bool"
    )]
    public static Obj Get([Self] AtomicBool self) => Bool.From(Interlocked.CompareExchange(ref self.value, 0, 0) == 1);

    [Native(
        Name = "set",
        Description = "Sets the value, leaving the previous value discarded.",
        Example = "flag.set(true)",
        ReturnType = "none",
        ArgumentTypes = new[] { "bool" }
    )]
    public static Obj Set([Self] AtomicBool self, [ArgInfo(Essential = true)] Obj amount)
    {
        if (!amount.As<Bool>(out var b))
            return new Err("expected 'value' argument to be of type 'bool'");

        Interlocked.Exchange(ref self.value, b.Value ? 1 : 0);
        return None;
    }

    [Native(
        Name = "exchange",
        Description = "Sets the value and returns the previous value.",
        Example = "old = flag.exchange(true)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "bool" }
    )]
    public static Obj Exchange([Self] AtomicBool self, [ArgInfo(Essential = true)] Obj amount)
    {
        if (!amount.As<Bool>(out var b))
            return new Err("expected 'value' argument to be of type 'bool'");

        return Bool.From(Interlocked.Exchange(ref self.value, b.Value ? 1 : 0) == 1);
    }

    [Native(
        Name = "compare_and_swap",
        Description = "Sets the value to 'new_value' when it currently equals 'expected', and returns whether the swap happened.",
        Example = "flag.compare_and_swap(false, true)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "bool", "bool" }
    )]
    public static Obj CompareAndSwap(
        [Self] AtomicBool self,
        [ArgInfo(Essential = true)] Obj expected,
        [ArgInfo(Essential = true)] Obj newValue)
    {
        if (!expected.As<Bool>(out var e))
            return new Err("expected 'expected' argument to be of type 'bool'");

        if (!newValue.As<Bool>(out var n))
            return new Err("expected 'new_value' argument to be of type 'bool'");

        return Bool.From(Interlocked.CompareExchange(ref self.value, n.Value ? 1 : 0, e.Value ? 1 : 0) == (e.Value ? 1 : 0));
    }
}
