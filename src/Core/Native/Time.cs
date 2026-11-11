using Un.Object;
using Un.Object.Primitive;
using Un.Object.Util;
using Un.Reflection;

namespace Un.Native;

[NativeModule("time", typeof(Stopwatch))]
public static class Time
{
    [Native(
        Name = "sleep",
        Description = "Waits for the given time. int is milliseconds, float is seconds.",
        Example = "sleep(250)\nsleep(0.5)",
        ReturnType = "none",
        ArgumentTypes = new[] { "int | float" }
    )]
    public static Obj Sleep([ArgInfo(Essential = true)] Obj milliseconds)
    {
        var ms = milliseconds switch
        {
            Int i => (int)i.Value,
            Float f => (int)(f.Value * 1000),
            Obj o when o.ToInt().As<Int>(out var intValue) => (int)intValue.Value,
            _ => -1,
        };

        if (ms == -1)
            return new Err("expected 'time' argument to be of type 'int' or 'float' or convertible to int");

        Thread.Sleep(ms);
        return Obj.None;
    }

    [Native(
        Name = "now",
        Description = "Returns the current date and time.",
        Example = "write(now())",
        ReturnType = "date"
    )]
    public static Date Now() => new(DateTime.Now);

    [Native(
        Name = "utc_now",
        Description = "Returns the current UTC date and time.",
        Example = "write(utc_now())",
        ReturnType = "date"
    )]
    public static Date UtcNow() => new(DateTime.UtcNow);

    [Native(
        Name = "epoch",
        Description = "Returns the current Unix time in seconds.",
        Example = "write(epoch())",
        ReturnType = "int"
    )]
    public static Int Epoch() => Int.From(DateTimeOffset.UtcNow.ToUnixTimeSeconds());

    [Native(
        Name = "epoch_ms",
        Description = "Returns the current Unix time in milliseconds.",
        Example = "write(epoch_ms())",
        ReturnType = "int"
    )]
    public static Int EpochMs() => Int.From(DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());

    [Native(
        Name = "from_epoch",
        Description = "Creates a date from Unix time in seconds (local time).",
        Example = "write(from_epoch(1700000000))",
        ReturnType = "date",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj FromEpoch([ArgInfo(Essential = true)] Obj seconds)
    {
        if (!seconds.As<Int>(out var s))
            return new Err("expected 'seconds' argument to be of type 'int'");

        return new Date(DateTimeOffset.FromUnixTimeSeconds(s.Value).LocalDateTime);
    }

    [Native(
        Name = "from_epoch_ms",
        Description = "Creates a date from Unix time in milliseconds (local time).",
        Example = "write(from_epoch_ms(1700000000000))",
        ReturnType = "date",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj FromEpochMs([ArgInfo(Essential = true)] Obj milliseconds)
    {
        if (!milliseconds.As<Int>(out var ms))
            return new Err("expected 'milliseconds' argument to be of type 'int'");

        return new Date(DateTimeOffset.FromUnixTimeMilliseconds(ms.Value).LocalDateTime);
    }

    [Native(
        Name = "monotonic",
        Description = "Returns monotonic seconds (unaffected by system clock changes).",
        Example = "a = monotonic()\nsleep(10)\nio.write(monotonic() - a)",
        ReturnType = "float"
    )]
    public static Float Monotonic() => new(Environment.TickCount64 / 1000.0);

    [Native(
        Name = "monotonic_ms",
        Description = "Returns monotonic milliseconds (unaffected by system clock changes).",
        Example = "a = monotonic_ms()\nsleep(10)\nio.write(monotonic_ms() - a)",
        ReturnType = "int"
    )]
    public static Int MonotonicMs() => Int.From(Environment.TickCount64);

    [Native(
        Name = "stopwatch",
        Description = "Creates a stopwatch object.",
        Example = "timer = stopwatch()",
        ReturnType = "stopwatch"
    )]
    public static Stopwatch Stopwatch() => new();
}
