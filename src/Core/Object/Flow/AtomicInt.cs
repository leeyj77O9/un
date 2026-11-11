using Un.Object.Collections;
using Un.Object.Primitive;
using Un.Object.Type;
using Un.Reflection;

namespace Un.Object.Flow;

[NativeType(Name = "atomic_int")]
public class AtomicInt : Obj
{
    private long value;

    public AtomicInt() : this(0) { }

    public AtomicInt(long initial) : base(UnType.Create("atomic_int"))
    {
        value = initial;
    }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => new AtomicInt(),
        { Count: 1 } and [Int n] => new AtomicInt(n.Value),
        _ => new Err("atomic_int takes at most one argument"),
    };

    public override Obj Entry() => this;

    public override Obj Copy() => this;

    public override Obj Clone() => new Err("'atomic_int' cannot be cloned");

    [Native(
        Name = "get",
        Description = "Returns the current value.",
        Example = "a = atomic.int(10)\na.get()",
        ReturnType = "int"
    )]
    public static Obj Get([Self] AtomicInt self) => Int.From(Interlocked.Read(ref self.value));

    [Native(
        Name = "set",
        Description = "Sets the value, leaving the previous value discarded.",
        Example = "a.set(42)",
        ReturnType = "none",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Set([Self] AtomicInt self, [ArgInfo(Essential = true)] Obj amount)
    {
        if (!amount.As<Int>(out var n))
            return new Err("expected 'value' argument to be of type 'int'");

        Interlocked.Exchange(ref self.value, n.Value);
        return None;
    }

    [Native(
        Name = "add",
        Description = "Adds an amount and returns the new value.",
        Example = "a.add(5)",
        ReturnType = "int",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Add([Self] AtomicInt self, [ArgInfo(Essential = true)] Obj amount)
    {
        if (!amount.As<Int>(out var n))
            return new Err("expected 'value' argument to be of type 'int'");

        return Int.From(Interlocked.Add(ref self.value, n.Value));
    }

    [Native(
        Name = "inc",
        Description = "Increments the value by one and returns the new value.",
        Example = "a.inc()",
        ReturnType = "int"
    )]
    public static Obj Inc([Self] AtomicInt self) => Int.From(Interlocked.Increment(ref self.value));

    [Native(
        Name = "dec",
        Description = "Decrements the value by one and returns the new value.",
        Example = "a.dec()",
        ReturnType = "int"
    )]
    public static Obj Dec([Self] AtomicInt self) => Int.From(Interlocked.Decrement(ref self.value));

    [Native(
        Name = "exchange",
        Description = "Sets the value and returns the previous value.",
        Example = "old = a.exchange(99)",
        ReturnType = "int",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Exchange([Self] AtomicInt self, [ArgInfo(Essential = true)] Obj amount)
    {
        if (!amount.As<Int>(out var n))
            return new Err("expected 'value' argument to be of type 'int'");

        return Int.From(Interlocked.Exchange(ref self.value, n.Value));
    }

    [Native(
        Name = "compare_and_swap",
        Description = "Sets the value to 'new_value' when it currently equals 'expected', and returns whether the swap happened.",
        Example = "a.compare_and_swap(10, 42)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "int", "int" }
    )]
    public static Obj CompareAndSwap(
        [Self] AtomicInt self,
        [ArgInfo(Essential = true)] Obj expected,
        [ArgInfo(Essential = true)] Obj newValue)
    {
        if (!expected.As<Int>(out var e))
            return new Err("expected 'expected' argument to be of type 'int'");

        if (!newValue.As<Int>(out var n))
            return new Err("expected 'new_value' argument to be of type 'int'");

        return Bool.From(Interlocked.CompareExchange(ref self.value, n.Value, e.Value) == e.Value);
    }
}
