using Un.Object.Primitive;
using Un.Object.Collections;
using Un.Object.Function;
using Un.Object.Type;
using Un.Reflection;

namespace Un.Object.Iter;

[BuiltinType("iter", Description = "Lazy iterable sequence with functional operators.", Example = "for x in iter.range(0, 5)\n    io.write(x)")]
public class Iters(IEnumerable<Obj> value) : Ref<IEnumerable<Obj>>(value, UnType.Iter)
{
    public Iters() : this([]) { }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 1 } => args[0].Iter(),
        _ => new Err($"invaild '{Type}' initialize"),
    };

    public IEnumerator<Obj> Enumerator { get; private set; } = null!;

    public override Int Len() => Int.From(Value.Count());

    public override Obj Iter() => this;

    public override Obj ToList() => new List([.. Value]);

    public override Obj ToTuple() => new Tup([.. Value]);

    public override Obj ToStr() => Str.From(string.Join(", ", Value.Select(x => Str.To(x).Value)));

    public override Obj Next()
    {
        Enumerator ??= Value.GetEnumerator();

        if (Enumerator.MoveNext())
            return Enumerator.Current;
        return new Err("iteration stopped");
    }

    public override Obj Spread() => new Spreads([.. Value]);

    public override Iters Copy() => this;

    public override Iters Clone() => new(Value);

    [Native(
        Name = "take",
        Description = "Returns the result of iterator.take().",
        Example = "iterator.take(n)",
        ReturnType = "iter",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Take([Self] Iters self, [ArgInfo(Essential = true)] Obj n)
    {
        if (!n.As<Int>(out var nValue))
            return new Err("invalid argument: 'n'");

        return new Iters(self.Value.Take((int)nValue.Value));
    }

    [Native(
        Name = "skip",
        Description = "Returns the result of iterator.skip().",
        Example = "iterator.skip(n)",
        ReturnType = "iter",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Skip([Self] Iters self, [ArgInfo(Essential = true)] Obj n)
    {
        if (!n.As<Int>(out var nValue))
            return new Err("invalid argument: 'n'");

        return new Iters(self.Value.Skip((int)nValue.Value));
    }

    [Native(
        Name = "count",
        Description = "Returns the result of iterator.count().",
        Example = "iterator.count()",
        ReturnType = "int"
    )]
    public static Obj Count([Self] Iters self) => Int.From(self.Value.Count());

    [Native(
        Name = "to_list",
        Description = "Converts a iter value to list.",
        Example = "iterator.to_list()",
        ReturnType = "list"
    )]
    public static Obj ToList([Self] Iters self) => new List([.. self.Value]);

    [Native(
        Name = "to_tuple",
        Description = "Converts a iter value to tuple.",
        Example = "iterator.to_tuple()",
        ReturnType = "tuple"
    )]
    public static Obj ToTuple([Self] Iters self) => new Tup([.. self.Value]);

    [Native(
        Name = "map",
        Description = "Returns the result of iterator.map().",
        Example = "iterator.map(fn)",
        ReturnType = "iter",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Map([Self] Iters self, [ArgInfo(Essential = true)] Obj fn)
    {
        if (fn is not Fn)
            return new Err("invalid argument: 'fn'");

        return new Iters(self.Value.Select((x, i) => fn.Call(Tup.Two("", x, "", Int.From(i)))));
    }

    [Native(
        Name = "filter",
        Description = "Returns the result of iterator.filter().",
        Example = "iterator.filter(fn)",
        ReturnType = "iter",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Filter([Self] Iters self, [ArgInfo(Essential = true)] Obj fn)
    {
        if (fn is not Fn)
            return new Err("invalid argument: 'fn'");

        return new Iters(self.Value.Where(x =>
        {
            var res = fn.Call(new([x], [""]));
            return res.As<Bool>(out var v) && v.Value;
        }));
    }

    [Native(
        Name = "any",
        Description = "Returns the result of iterator.any().",
        Example = "iterator.any(fn)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Any([Self] Iters self, [ArgInfo(Essential = true)] Obj fn)
    {
        if (fn is not Fn)
            return new Err("invalid argument: 'fn'");

        bool result = self.Value.Any(x =>
        {
            var res = fn.Call(new([x], [""]));
            return res.As<Bool>(out var v) && v.Value;
        });

        return Bool.From(result);
    }

    [Native(
        Name = "all",
        Description = "Returns the result of iterator.all().",
        Example = "iterator.all(fn)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj All([Self] Iters self, [ArgInfo(Essential = true)] Obj fn)
    {
        if (fn is not Fn)
            return new Err("invalid argument: 'fn'");

        bool result = self.Value.All(x =>
        {
            var res = fn.Call(new([x], [""]));
            return res.As<Bool>(out var v) && v.Value;
        });

        return Bool.From(result);
    }

    [Native(
        Name = "sum",
        Description = "Returns the result of iterator.sum().",
        Example = "iterator.sum()",
        ReturnType = "any"
    )]
    public static Obj Sum([Self] Iters self)
    {
        Obj sum = null!;

        foreach (var v in self.Value)
            sum = sum is null ? v : sum.Add(v);

        return sum ?? None;
    }

    [Native(
        Name = "first",
        Description = "Returns the result of iterator.first().",
        Example = "iterator.first()",
        ReturnType = "any"
    )]
    public static Obj First([Self] Iters self) => self.Value.FirstOrDefault() ?? None;

    [Native(
        Name = "last",
        Description = "Returns the result of iterator.last().",
        Example = "iterator.last()",
        ReturnType = "any"
    )]
    public static Obj Last([Self] Iters self) => self.Value.LastOrDefault() ?? None;
}
