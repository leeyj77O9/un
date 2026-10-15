using Un.Object;
using Un.Object.Collections;
using Un.Object.Primitive;
using Un.Reflection;

namespace Un.Native;

[NativeModule("random", typeof(Object.Util.Random))]
public static class Random
{
    private static System.Random shared = new();

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
    public static Int Next() => Object.Primitive.Int.From(shared.Next());

    [Native(
        Name = "int",
        Description = "Returns a random integer in an inclusive range.",
        Example = "value = int(1, 6)",
        ReturnType = "random",
        ArgumentTypes = new[] { "int", "int" }
    )]
    public static Obj Int(
        [ArgInfo(Essential = true)] Obj min,
        [ArgInfo(Essential = true)] Obj max)
    {
        if (!min.As<Int>(out var minValue))
            return new Err("expected 'random.int' argument 'min' is int");
        if (!max.As<Int>(out var maxValue))
            return new Err("expected 'random.int' argument 'max' is int");
        if (minValue.Value > maxValue.Value)
            return new Err("'min' cannot be greater than 'max'");

        return Object.Primitive.Int.From(shared.Next((int)minValue.Value, (int)maxValue.Value + 1));
    }

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
    {
        if (min == null && max == null)
            return new Float(shared.NextDouble());

        if (min == null || max == null)
            return new Err("'min' and 'max' must be specified together");

        if (!min.As<Float>(out var minValue))
            return new Err("expected 'random.float' argument 'min' is float");
        if (!max.As<Float>(out var maxValue))
            return new Err("expected 'random.float' argument 'max' is float");
        if (minValue.Value > maxValue.Value)
            return new Err("'min' cannot be greater than 'max'");

        return new Float(shared.NextDouble() * (maxValue.Value - minValue.Value) + minValue.Value);
    }

    [Native(
        Name = "bool",
        Description = "Returns a pseudo-random boolean value.",
        Example = "write(bool())",
        ReturnType = "bool"
    )]
    public static Bool Bool() => Object.Primitive.Bool.From(shared.Next(2) == 0);

    [Native(
        Name = "bytes",
        Description = "Returns a list of pseudo-random byte values.",
        Example = "values = bytes(16)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Bytes([ArgInfo(Essential = true)] Obj length)
    {
        if (!length.As<Int>(out var lengthValue))
            return new Err("expected 'random.bytes' argument 'length' is int");
        if (lengthValue.Value < 0)
            return new Err("'length' cannot be negative");

        var bytes = new byte[lengthValue.Value];
        shared.NextBytes(bytes);

        return new List([.. bytes.Select(b => Object.Primitive.Int.From(b))]);
    }

    [Native(
        Name = "shuffle",
        Description = "Shuffles a list in place.",
        Example = "shuffle(items)",
        ReturnType = "none",
        ArgumentTypes = new[] { "list" }
    )]
    public static Obj Shuffle([ArgInfo(Essential = true)] Obj obj)
    {
        if (!obj.As<List>(out var list))
            return new Err("expected 'random.shuffle' argument to be a list");
        shared.Shuffle(list.Value.AsSpan(0, list.Count));
        return Obj.None;
    }

    [Native(
        Name = "choice",
        Description = "Returns a random value from a list.",
        Example = "item = choice(items)",
        ReturnType = "any",
        ArgumentTypes = new[] { "list" }
    )]
    public static Obj Choice([ArgInfo(Essential = true)] Obj obj)
    {
        if (obj.As<List>(out var list))
            return list[shared.Next(0, list.Count)];

        return new Err("expected 'random.choice' argument to be a list");
    }

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
    {
        if (!obj.As<List>(out var list))
            return new Err("expected 'random.choices' argument 'obj' is list");
        if (!count.As<Int>(out var countValue))
            return new Err("expected 'random.choices' argument 'count' is int");

        var indexes = new List<int>();

        for (int i = 0; i < countValue.Value; i++)
            indexes.Add(shared.Next(0, list.Count));

        return new List([.. indexes.Select(i => list[i])]);
    }

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
    {
        if (!obj.As<List>(out var list))
            return new Err("expected 'random.sample' argument 'obj' is list");
        if (!count.As<Int>(out var countValue))
            return new Err("expected 'random.sample' argument 'count' is int");
        if (countValue.Value > list.Count)
            return new Err("'random.sample' argument 'count' cannot be greater than the length of 'obj'");

        var clone = (List)list.Clone();
        Shuffle(clone);

        return new List([.. clone.Value.Take((int)countValue.Value)]);
    }

    [Native(
        Name = "chance",
        Description = "Returns true according to a probability from zero to one.",
        Example = "if chance(0.25)\n    write(\"selected\")",
        ReturnType = "bool",
        ArgumentTypes = new[] { "float" }
    )]
    public static Obj Chance([ArgInfo(Essential = true)] Obj probability)
    {
        if (!probability.As<Float>(out var value))
            return new Err("expected 'random.chance' argument 'probability' is float");
        if (value.Value < 0 || value.Value > 1)
            return new Err("'probability' must be between 0 and 1");

        return Object.Primitive.Bool.From(shared.NextDouble() < value.Value);
    }

    [Native(
        Name = "seed",
        Description = "Sets the shared pseudo-random generator seed.",
        Example = "seed(42)",
        ReturnType = "none",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Seed([ArgInfo(Essential = true)] Obj seed)
    {
        if (!seed.As<Int>(out var value))
            return new Err("expected 'random.seed' argument 'seed' is int");

        shared = new System.Random((int)value.Value);
        return Obj.None;
    }

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
    {
        const string Chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

        if (!length.As<Int>(out var value))
            return new Err("expected 'random.string' argument 'length' is int");
        if (value.Value < 0)
            return new Err("'length' cannot be negative");

        return Str.From(new string([.. Enumerable.Range(0, (int)value.Value).Select(_ => Chars[shared.Next(Chars.Length)])]));
    }

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
    {
        if (!values.As<List>(out var valueList))
            return new Err("expected 'values' is list");

        if (!weights.As<List>(out var weightList))
            return new Err("expected 'weights' is list");

        if (valueList.Count != weightList.Count)
            return new Err("'values' and 'weights' must have the same length");

        double total = 0;
        var nums = new double[weightList.Count];

        for (int i = 0; i < weightList.Count; i++)
        {
            if (!weightList[i].As<Float>(out var weight))
                return new Err("weights must be float");

            if (weight.Value < 0)
                return new Err("weights must be non-negative");

            nums[i] = weight.Value;
            total += weight.Value;
        }

        if (total == 0)
            return new Err("sum of weights must be greater than zero");

        double r = shared.NextDouble() * total;

        for (int i = 0; i < nums.Length; i++)
        {
            r -= nums[i];
            if (r <= 0)
                return valueList[i];
        }

        return valueList[^1];
    }

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
    {
        if (!start.As<Int>(out var startValue))
            return new Err("expected 'random.range' argument 'start' is int");

        int min, max;

        if (end == null)
        {
            min = 0;
            max = (int)startValue.Value - 1;
        }
        else
        {
            if (!end.As<Int>(out var endValue))
                return new Err("expected 'random.range' argument 'end' is int");

            min = (int)startValue.Value;
            max = (int)endValue.Value;
        }

        if (min > max)
            return new Err("'start' cannot be greater than 'end'");

        var list = new List([.. Enumerable.Range(min, max - min + 1).Select(x => Object.Primitive.Int.From(x))]);

        Shuffle(list);

        return list;
    }

    [Native(
        Name = "value",
        Description = "Returns a random value from an enum instance.",
        Example = "status = value(Status)",
        ReturnType = "enum",
        ArgumentTypes = new[] { "enum" }
    )]
    public static Obj Value([ArgInfo(Essential = true)] Obj obj)
    {
        if (!obj.As<TObj>(out var t))
            return new Err("expected 'random.value' argument is enum");

        if (!Global.TryGetClass(t.Value, out var cla) || cla is not Enu e)
            return new Err("expected 'random.value' argument is enum");

        var value = shared.Next(0, e.Members.Count);
        var enu = e.Clone();

        enu.Init(Tup.One("", Object.Primitive.Int.From(value)));

        return enu;
    }
}
