using Un.Object;
using Un.Object.Flow;
using Un.Object.Primitive;
using Un.Reflection;

namespace Un.Native;

[NativeModule("atomic", typeof(AtomicInt), typeof(AtomicBool))]
public static class Atomic
{
    [Native(
        Name = "int",
        Description = "Creates an atomic integer counter and operations on it.",
        Example = "counter = atomic.int(0)",
        ReturnType = "atomic_int",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Int([ArgInfo(Optional = true)] Obj initial = null!)
    {
        if (initial is null)
            return new AtomicInt();

        if (!initial.As<Un.Object.Primitive.Int>(out var n))
            return new Err("expected 'initial' argument to be of type 'int'");

        return new AtomicInt(n.Value);
    }

    [Native(
        Name = "bool",
        Description = "Creates an atomic boolean flag with operations on it.",
        Example = "flag = atomic.bool(false)",
        ReturnType = "atomic_bool",
        ArgumentTypes = new[] { "bool" }
    )]
    public static Obj Bool([ArgInfo(Optional = true)] Obj initial = null!)
    {
        if (initial is null)
            return new AtomicBool();

        if (!initial.As<Un.Object.Primitive.Bool>(out var b))
            return new Err("expected 'initial' argument to be of type 'bool'");

        return new AtomicBool(b.Value);
    }
}
