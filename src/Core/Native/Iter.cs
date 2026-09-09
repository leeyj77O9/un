using Un.Object;
using Un.Object.Collections;
using Un.Object.Iter;
using Un.Object.Primitive;
using Un.Reflection;

namespace Un.Native;

[NativeModule("iter", typeof(Object.Iter.Range), typeof(Counter), typeof(Reverse), typeof(Repeat))]
public static class Iter
{
    static bool NormalizeToTuple(Obj iterable, out Tup tuple, out Obj err)
    {
        if (!iterable.ToTuple().As<Tup>(out var raw))
        {
            tuple = null!;
            err = new Err("expected 'iterable' argument to be a tuple");
            return false;
        }

        Obj values = raw switch
        {
            { Count: 0 } => new Err("expected 'iterable' argument to have at least one value"),
            { Count: 1 } when raw[0].As<List>(out var l) => l.ToTuple(),
            { Count: 1 } when raw[0].As<Tup>(out var t) => t,
            { Count: 1 } when raw[0].As<Object.Iter.Range>(out var r) => r.ToTuple(),
            { Count: 1 } when raw[0].As<Iters>(out var it) && it.As<Tup>(out var itTuple) => itTuple,
            { Count: 1 } when raw[0].As<Iters>(out _) => new Err("invalid argument type of 'iterable'"),
            { Count: 1 } => raw[0],
            _ => raw
        };

        if (values is Err e)
        {
            tuple = null!;
            err = e;
            return false;
        }

        if (!values.ToTuple().As<Tup>(out var result))
        {
            tuple = null!;
            err = new Err("expected 'iterable' argument to be a tuple");
            return false;
        }

        tuple = result;
        err = Obj.None;
        return true;
    }

    [Native(
        Name = "array",
        Description = "Creates an array from a value and dimensions.",
        Example = "items = array(1, 3)",
        ReturnType = "list",
        ArgumentTypes = new[] { "any", "int" }
    )]
    public static Obj Array(
        [ArgInfo(Essential = true)] Obj value,
        [ArgInfo(Positional = true)] Obj size = null!)
    {
        size ??= new Tup([Int.From(1)]);

        if (!size.ToList().As<List>(out var sizeList))
            return new Err("expected 'size' argument to be a 'list'");

        var sizes = new List<int>();

        foreach (var item in sizeList.Value)
        {
            if (!item.ToInt().As<Int>(out var intItem))
                return new Err("expected all items in 'size' list to be integers");
            sizes.Add((int)intItem.Value);
        }

        if (sizes.Count == 0)
            return new Err("expected 'size' argument to have at least one dimension");

        return Create([.. sizes]);

        List Create(int[] lengths)
        {
            List list = [];

            for (int i = 0; i < lengths[0]; i++)
                List.Append(list, lengths.Length == 1 ? value.Clone() : Create(lengths[1..]));

            return list;
        }
    }

    [Native(
        Name = "range",
        Description = "Creates a sequence of integers.",
        Example = "for index in range(0, 3)\n    write(index)",
        ReturnType = "iter",
        ArgumentTypes = new[] { "int", "int", "int" }
    )]
    public static Obj Range(
        [ArgInfo(Essential = true)] Obj start,
        [ArgInfo(Optional = true)] Obj end = null!,
        [ArgInfo(Optional = true)] Obj step = null!)
    {
        end ??= Int.From(0);
        step ??= Int.From(1);

        if (!start.ToInt().As<Int>(out var intStart))
            return new Err("expected 'start' argument to be an integer");
        if (!end.ToInt().As<Int>(out var intEnd))
            return new Err("expected 'end' argument to be an integer");
        if (!step.ToInt().As<Int>(out var intStep))
            return new Err("expected 'step' argument to be an integer");

        if (intStep.Value == 0)
            return new Err("step argument must not be zero");

        if (intStart.Value > intEnd.Value && intStep.Value > 0)
            (intStart, intEnd) = (intEnd, intStart);

        return new Object.Iter.Range(intStart.Value, intEnd.Value, intStep.Value);
    }

    [Native(
        Name = "counter",
        Description = "Creates an unbounded integer counter.",
        Example = "numbers = counter(0)",
        ReturnType = "counter",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Counter([ArgInfo(Optional = true)] Obj start = null!)
    {
        start ??= Int.From(0);

        if (!start.ToInt().As<Int>(out var intStart))
            return new Err("expected 'start' argument to be an integer");
        return new Counter(intStart.Value);
    }

    [Native(
        Name = "reverse",
        Description = "Iterates over values in reverse order.",
        Example = "items = reverse(values)",
        ReturnType = "reverse",
        ArgumentTypes = new[] { "iter" }
    )]
    public static Obj Reverse([ArgInfo(Essential = true)] Obj obj)
    {
        if (!obj.Iter().As<Iters>(out var iter))
            return new Err("expected 'obj' argument to be of type 'iter'");
        return new Reverse(iter.Value);
    }

    [Native(
        Name = "repeat",
        Description = "Repeats an iterable a fixed number of times.",
        Example = "items = repeat(values, 3)",
        ReturnType = "repeat",
        ArgumentTypes = new[] { "iter", "int" }
    )]
    public static Obj Repeat(
        [ArgInfo(Essential = true)] Obj obj,
        [ArgInfo(Optional = true)] Obj count = null!)
    {
        if (!obj.Iter().As<Iters>(out var iter))
            return new Err("expected 'obj' argument to be of type 'iter'");

        count ??= Int.From(-1);

        if (!count.As<Int>(out var countValue))
            return new Err("expected 'count' argument to be of type 'int'");

        return new Repeat(iter.Value, countValue.Value);
    }

    [Native(
        Name = "zip",
        Description = "Combines corresponding values from iterables.",
        Example = "pairs = zip(first, second)",
        ReturnType = "list",
        ArgumentTypes = new[] { "iter" }
    )]
    public static Obj Zip([ArgInfo(Positional = true)] Obj iterables)
    {
        if (!iterables.As<Iters>(out var arrays))
            return new Err("expected 'iterables' argument to be of type 'iter'");

        if (arrays.Len().ToInt().As<Int>(out var intLen) && intLen.Value == 0)
            return new Err("expected 'iterables' argument to have a non-zero length");

        int length = int.MaxValue;

        foreach (var i in arrays.Value)
        {
            if (!i.Len().As<Int>(out var arrayLength))
                return new Err("expected all 'iterables' items to have a valid length");
            if (arrayLength.Value < length)
                length = (int)arrayLength.Value;
        }

        List list = [];

        for (int i = 0; i < length; i++)
        {
            List buf = [];
            foreach (var array in arrays.Value)
                buf.Append(array.GetItem(Int.From(i)));

            list.Append(new Tup([.. buf], new string[buf.Count]));
        }

        return list;
    }

    [Native(
        Name = "enumerate",
        Description = "Pairs iterable values with their indexes.",
        Example = "for pair in enumerate(items)\n    write(pair)",
        ReturnType = "iter",
        ArgumentTypes = new[] { "iter" }
    )]
    public static Obj Enumerate([ArgInfo(Essential = true)] Obj iterable)
    {
        if (!iterable.Iter().As<Iters>(out var iter))
            return new Err("expected 'iterable' argument to be of type 'iter'");

        return new Iters(iter.Value.Select((x, i) => new Tup([Int.From(i), x], ["index", "value"])));
    }

    [Native(
        Name = "sum",
        Description = "Adds all values in an iterable.",
        Example = "write(sum(values))",
        ReturnType = "any",
        ArgumentTypes = new[] { "iter" }
    )]
    public static Obj Sum([ArgInfo(Positional = true)] Obj iterable)
    {
        if (!NormalizeToTuple(iterable, out var values, out var err))
            return err;

        Obj total = values[0];

        for (int i = 1; i < values.Count; i++)
        {
            var sum = total.Add(values[i]);

            if (sum is Err)
                return sum;

            total = sum;
        }

        return total;
    }

    [Native(
        Name = "max",
        Description = "Returns the greatest value in an iterable.",
        Example = "write(max(values))",
        ReturnType = "any",
        ArgumentTypes = new[] { "iter" }
    )]
    public static Obj Max([ArgInfo(Positional = true)] Obj iterable)
    {
        if (!NormalizeToTuple(iterable, out var values, out var err))
            return err;

        Obj max = values[0];

        for (int i = 1; i < values.Count; i++)
        {
            var lt = max.Lt(values[i]);

            if (!lt.As<Bool>(out var isLess))
                return new Err($"{max} < {values[i]} is not a boolean");

            if (isLess.Value)
                max = values[i];
        }

        return max;
    }

    [Native(
        Name = "min",
        Description = "Returns the smallest value in an iterable.",
        Example = "write(min(values))",
        ReturnType = "any",
        ArgumentTypes = new[] { "iter" }
    )]
    public static Obj Min([ArgInfo(Positional = true)] Obj iterable)
    {
        if (!NormalizeToTuple(iterable, out var values, out var err))
            return err;

        Obj min = values[0];

        for (int i = 1; i < values.Count; i++)
        {
            var gt = min.Gt(values[i]);

            if (!gt.As<Bool>(out var isGreater))
                return new Err($"{min} > {values[i]} is not a boolean");

            if (isGreater.Value)
                min = values[i];
        }

        return min;
    }

    [Native(
        Name = "filter",
        Description = "Keeps iterable values for which a predicate returns true.",
        Example = "positives = filter(is_positive, values)",
        ReturnType = "list",
        ArgumentTypes = new[] { "func", "iter" }
    )]
    public static Obj Filter(
        [ArgInfo(Essential = true)] Obj predicate,
        [ArgInfo(Essential = true)] Obj iterable)
    {
        if (!iterable.Iter().As<Iters>(out var iter))
            return new Err("expected 'iterable' argument to be of type 'iter'");

        List list = [];

        foreach (var item in iter.Value)
        {
            var result = predicate.Call(new Tup([item]));

            if (result is Err)
                return result;

            if (!result.As<Bool>(out var isTrue))
                return new Err("expected 'predicate' to return a boolean");

            if (isTrue.Value)
                List.Append(list, item);
        }

        return list;
    }

    [Native(
        Name = "map",
        Description = "Transforms every value in an iterable.",
        Example = "doubled = map(double, values)",
        ReturnType = "list",
        ArgumentTypes = new[] { "func", "iter" }
    )]
    public static Obj Map(
        [ArgInfo(Essential = true)] Obj transform,
        [ArgInfo(Essential = true)] Obj iterable)
    {
        if (!iterable.Iter().As<Iters>(out var iter))
            return new Err("expected 'iterable' argument to be of type 'iter'");

        List list = [];

        foreach (var item in iter.Value)
        {
            var result = transform.Call(new Tup([item]));

            if (result is Err)
                return result;

            List.Append(list, result);
        }

        return list;
    }

    [Native(
        Name = "take",
        Description = "Takes a number of values from an iterable.",
        Example = "first = take(3, values)",
        ReturnType = "list",
        ArgumentTypes = new[] { "int", "iter" }
    )]
    public static Obj Take(
        [ArgInfo(Essential = true)] Obj count,
        [ArgInfo(Essential = true)] Obj iterable)
    {
        if (!count.ToInt().As<Int>(out var n))
            return new Err("expected 'count' argument to be an integer");

        if (n.Value < 0)
            return new Err("expected 'count' argument to be non-negative");

        if (!iterable.Iter().As<Iters>(out var iter))
            return new Err("expected 'iterable' argument to be of type 'iter'");

        List list = [];
        long i = 0;

        foreach (var item in iter.Value)
        {
            if (i >= n.Value)
                break;

            List.Append(list, item);
            i++;
        }

        return list;
    }

    [Native(
        Name = "skip",
        Description = "Skips a number of values in an iterable.",
        Example = "rest = skip(2, values)",
        ReturnType = "list",
        ArgumentTypes = new[] { "int", "iter" }
    )]
    public static Obj Skip(
        [ArgInfo(Essential = true)] Obj count,
        [ArgInfo(Essential = true)] Obj iterable)
    {
        if (!count.ToInt().As<Int>(out var n))
            return new Err("expected 'count' argument to be an integer");

        if (n.Value < 0)
            return new Err("expected 'count' argument to be non-negative");

        if (!iterable.Iter().As<Iters>(out var iter))
            return new Err("expected 'iterable' argument to be of type 'iter'");

        List list = [];
        long i = 0;

        foreach (var item in iter.Value)
        {
            if (i >= n.Value)
                List.Append(list, item);
            i++;
        }

        return list;
    }

    [Native(
        Name = "chain",
        Description = "Concatenates multiple iterables into one sequence.",
        Example = "items = chain(first, second)",
        ReturnType = "list",
        ArgumentTypes = new[] { "iter" }
    )]
    public static Obj Chain([ArgInfo(Positional = true)] Obj iterables)
    {
        if (!iterables.As<Iters>(out var arrays))
            return new Err("expected 'iterables' argument to be of type 'iter'");

        List list = [];

        foreach (var array in arrays.Value)
        {
            if (!array.Iter().As<Iters>(out var iter))
                return new Err("expected all 'iterables' items to be of type 'iter'");

            foreach (var item in iter.Value)
                List.Append(list, item);
        }

        return list;
    }

    [Native(
        Name = "flatten",
        Description = "Flattens nested iterable values.",
        Example = "flat = flatten(groups)",
        ReturnType = "list",
        ArgumentTypes = new[] { "iter" }
    )]
    public static Obj Flatten([ArgInfo(Essential = true)] Obj iterable)
    {
        if (!iterable.Iter().As<Iters>(out var iter))
            return new Err("expected 'iterable' argument to be of type 'iter'");

        List list = [];

        foreach (var item in iter.Value)
        {
            if (item.Iter().As<Iters>(out var inner))
            {
                foreach (var sub in inner.Value)
                    List.Append(list, sub);
            }
            else
            {
                List.Append(list, item);
            }
        }

        return list;
    }

    [Native(
        Name = "all",
        Description = "Checks whether every iterable item matches a predicate.",
        Example = "all(is_valid, values)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "func", "iter" }
    )]
    public static Obj All(
        [ArgInfo(Essential = true)] Obj predicate,
        [ArgInfo(Essential = true)] Obj iterable)
    {
        if (!iterable.Iter().As<Iters>(out var iter))
            return new Err("expected 'iterable' argument to be of type 'iter'");

        foreach (var item in iter.Value)
        {
            var result = predicate.Call(new Tup([item]));

            if (result is Err)
                return result;

            if (!result.As<Bool>(out var isTrue))
                return new Err("expected 'predicate' to return a boolean");

            if (!isTrue.Value)
                return Bool.From(false);
        }

        return Bool.From(true);
    }

    [Native(
        Name = "any",
        Description = "Checks whether at least one iterable item matches a predicate.",
        Example = "any(is_ready, tasks)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "func", "iter" }
    )]
    public static Obj Any(
        [ArgInfo(Essential = true)] Obj predicate,
        [ArgInfo(Essential = true)] Obj iterable)
    {
        if (!iterable.Iter().As<Iters>(out var iter))
            return new Err("expected 'iterable' argument to be of type 'iter'");

        foreach (var item in iter.Value)
        {
            var result = predicate.Call(new Tup([item]));

            if (result is Err)
                return result;

            if (!result.As<Bool>(out var isTrue))
                return new Err("expected 'predicate' to return a boolean");

            if (isTrue.Value)
                return Bool.From(true);
        }

        return Bool.From(false);
    }

    [Native(
        Name = "count",
        Description = "Counts iterable items that match a predicate.",
        Example = "write(count(is_even, values))",
        ReturnType = "int",
        ArgumentTypes = new[] { "func", "iter" }
    )]
    public static Obj Count(
        [ArgInfo(Essential = true)] Obj predicate,
        [ArgInfo(Essential = true)] Obj iterable)
    {
        if (!iterable.Iter().As<Iters>(out var iter))
            return new Err("expected 'iterable' argument to be of type 'iter'");

        int total = 0;

        foreach (var item in iter.Value)
        {
            var result = predicate.Call(new Tup([item]));

            if (result is Err)
                return result;

            if (!result.As<Bool>(out var isTrue))
                return new Err("expected 'predicate' to return a boolean");

            if (isTrue.Value)
                total++;
        }

        return Int.From(total);
    }

    [Native(
        Name = "sorted",
        Description = "Returns iterable values in sorted order.",
        Example = "ordered = sorted(values)",
        ReturnType = "list",
        ArgumentTypes = new[] { "iter" }
    )]
    public static Obj Sorted([ArgInfo(Essential = true)] Obj iterable)
    {
        if (!iterable.Iter().As<Iters>(out var iter))
            return new Err("expected 'iterable' argument to be of type 'iter'");

        var items = new List<Obj>(iter.Value);

        for (int i = 1; i < items.Count; i++)
        {
            var key = items[i];
            int j = i - 1;

            while (j >= 0)
            {
                var gt = items[j].Gt(key);

                if (!gt.As<Bool>(out var isGreater))
                    return new Err($"{items[j]} > {key} is not a boolean");

                if (!isGreater.Value)
                    break;

                items[j + 1] = items[j];
                j--;
            }

            items[j + 1] = key;
        }

        List list = [];

        foreach (var item in items)
            List.Append(list, item);

        return list;
    }
}
