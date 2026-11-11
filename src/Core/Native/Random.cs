using Un.Object;
using Un.Object.Primitive;
using Un.Reflection;

namespace Un.Native;

[NativeModule("random", typeof(Object.Util.Random))]
public static class Random
{
    private static Object.Util.Random shared = new();

    [Native(
        Name = "random",
        Description = "Creates a random-number generator object.",
        Example = "generator = random()",
        ReturnType = "random"
    )]
    public static Object.Util.Random _Random() => new();

    [Native(
        Name = "next",
        Description = "Returns a non-negative pseudo-random integer.",
        Example = "write(next())",
        ReturnType = "int"
    )]
    public static Object.Primitive.Int Next() => Object.Util.Random.Next(shared);

    [Native(
        Name = "int",
        Description = "Returns a random integer in an inclusive range.",
        Example = "value = int(1, 6)",
        ReturnType = "int",
        ArgumentTypes = new[] { "int", "int" }
    )]
    public static Obj Int(
        [ArgInfo(Essential = true)] Obj min,
        [ArgInfo(Essential = true)] Obj max)
        => Object.Util.Random.Int(shared, min, max);

    [Native(
        Name = "float",
        Description = "Returns a random floating-point value, optionally within a range.",
        Example = "value = float(0.0, 1.0)",
        ReturnType = "float",
        ArgumentTypes = new[] { "float", "float" }
    )]
    public static Obj Float(
        [ArgInfo(Optional = true)] Obj min = null!,
        [ArgInfo(Optional = true)] Obj max = null!)
        => Object.Util.Random.Float(shared, min, max);

    [Native(
        Name = "bool",
        Description = "Returns a pseudo-random boolean value.",
        Example = "write(bool())",
        ReturnType = "bool"
    )]
    public static Object.Primitive.Bool Bool() => Object.Util.Random.Bool(shared);

    [Native(
        Name = "bytes",
        Description = "Returns a list of pseudo-random byte values.",
        Example = "values = bytes(16)",
        ReturnType = "list",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Bytes([ArgInfo(Essential = true)] Obj length)
        => Object.Util.Random.Bytes(shared, length);

    [Native(
        Name = "shuffle",
        Description = "Shuffles a list in place.",
        Example = "shuffle(items)",
        ReturnType = "none",
        ArgumentTypes = new[] { "list" }
    )]
    public static Obj Shuffle([ArgInfo(Essential = true)] Obj obj)
        => Object.Util.Random.Shuffle(shared, obj);

    [Native(
        Name = "choice",
        Description = "Returns a random value from a list.",
        Example = "item = choice(items)",
        ReturnType = "any",
        ArgumentTypes = new[] { "list" }
    )]
    public static Obj Choice([ArgInfo(Essential = true)] Obj obj)
        => Object.Util.Random.Choice(shared, obj);

    [Native(
        Name = "choices",
        Description = "Returns multiple random values from a list with replacement.",
        Example = "items = choices(values, 3)",
        ReturnType = "list",
        ArgumentTypes = new[] { "list", "int" }
    )]
    public static Obj Choices(
        [ArgInfo(Essential = true)] Obj obj,
        [ArgInfo(Essential = true)] Obj count)
        => Object.Util.Random.Choices(shared, obj, count);

    [Native(
        Name = "sample",
        Description = "Returns unique random values from a list.",
        Example = "items = sample(values, 3)",
        ReturnType = "list",
        ArgumentTypes = new[] { "list", "int" }
    )]
    public static Obj Sample(
        [ArgInfo(Essential = true)] Obj obj,
        [ArgInfo(Essential = true)] Obj count)
        => Object.Util.Random.Sample(shared, obj, count);

    [Native(
        Name = "chance",
        Description = "Returns true according to a probability from zero to one.",
        Example = "if chance(0.25)\n    write(\"selected\")",
        ReturnType = "bool",
        ArgumentTypes = new[] { "float" }
    )]
    public static Obj Chance([ArgInfo(Essential = true)] Obj probability)
        => Object.Util.Random.Chance(shared, probability);

    [Native(
        Name = "seed",
        Description = "Sets the shared pseudo-random generator seed.",
        Example = "seed(42)",
        ReturnType = "none",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Seed([ArgInfo(Essential = true)] Obj seed)
        => Object.Util.Random.Seed(shared, seed);

    [Native(
        Name = "uuid",
        Description = "Returns a new UUID string.",
        Example = "id = uuid()",
        ReturnType = "str"
    )]
    public static Str UUID() => Str.From(Guid.NewGuid().ToString());

    [Native(
        Name = "str",
        Description = "Returns an alphanumeric pseudo-random string.",
        Example = "token = string(12)",
        ReturnType = "str",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj String([ArgInfo(Essential = true)] Obj length)
        => Object.Util.Random.String(shared, length);

    [Native(
        Name = "weighted",
        Description = "Returns a list value selected by matching numeric weights.",
        Example = "item = weighted(values, weights)",
        ReturnType = "any",
        ArgumentTypes = new[] { "list", "list" }
    )]
    public static Obj Weighted(
        [ArgInfo(Essential = true)] Obj values,
        [ArgInfo(Essential = true)] Obj weights)
        => Object.Util.Random.Weighted(shared, values, weights);

    [Native(
        Name = "range",
        Description = "Returns a shuffled list of integers in a range.",
        Example = "values = range(1, 10)",
        ReturnType = "list",
        ArgumentTypes = new[] { "int", "int" }
    )]
    public static Obj Range(
        [ArgInfo(Essential = true)] Obj start,
        [ArgInfo(Optional = true)] Obj end = null!)
        => Object.Util.Random.Range(shared, start, end);

    [Native(
        Name = "value",
        Description = "Returns a random value from an enum instance.",
        Example = "status = value(Status)",
        ReturnType = "enum",
        ArgumentTypes = new[] { "enum" }
    )]
    public static Obj Value([ArgInfo(Essential = true)] Obj obj) => Object.Util.Random.Values(shared, obj);
}
