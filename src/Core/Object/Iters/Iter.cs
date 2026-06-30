using Un.Object.Primitive;
using Un.Object.Collections;
using Un.Object.Function;
using Un.Object.Type;

namespace Un.Object.Iter;

public class Iters(IEnumerable<Obj> value) : Ref<IEnumerable<Obj>>(value, UnType.Iter)
{
    public Iters() : this([]) { }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 1 } => args[0].Iter(),
        _ => new Err($"invaild '{Type}' initialize"),
    };

    public IEnumerator<Obj> Enumerator { get; private set; } = null!;

    public override Obj Len() => Int.From(Value.Count());

    public override Obj Iter() => this;

    public override Obj ToList() => new List([.. Value]);

    public override Obj ToTuple() => new Tup([.. Value], new string[Value.Count()]);

    public override Obj ToStr() => Str.From(string.Join(", ", Value.Select(x => x.ToStr().As<Str>().Value)));

    public override Obj Next()
    {
        Enumerator ??= Value.GetEnumerator();

        if (Enumerator.MoveNext())
            return Enumerator.Current;
        return new Err("iteration stopped");
    }

    public override Obj Spread() => new Spreads([.. Value]);

    public override Obj Copy() => this;

    public override Obj Clone() => new Iters(Value)
    {
        Annotations = Annotations
    };

    public override Attributes GetOriginal() => new()
    {
        { "take", new NFn()
            {
                Name = "take",
                ReturnType = UnType.Iter,
                Args = [ new Arg("n") { IsEssential = true } ],
                Func = args =>
                {
                    if (args["self"] is not Iters self)
                        return new Err("invalid argument: 'self'");

                    if (!args["n"].As<Int>(out var n))
                        return new Err("invalid argument: 'n'");

                    return new Iters(self.Value.Take((int)n.Value));
                }
            }
        },
        { "skip", new NFn()
            {
                Name = "skip",
                ReturnType = UnType.Iter,
                Args = [ new Arg("n") { IsEssential = true } ],
                Func = args =>
                {
                    if (args["self"].As<Iters>(out var self))
                        return new Err("invalid argument: 'self'");

                    if (!args["n"].As<Int>(out var n))
                        return new Err("invalid argument: 'n'");

                    return new Iters(self.Value.Skip((int)n.Value));
                }
            }
        },
        { "to_list", new NFn()
            {
                Name = "to_list",
                ReturnType = UnType.List,
                Args = [],
                Func = args =>
                {
                    if (args["self"] is not Iters self)
                        return new Err("invalid argument: 'self'");

                    return new List([.. self.Value]);
                }
            }
        },
        { "to_tuple", new NFn()
            {
                Name = "to_tuple",
                ReturnType = UnType.Tuple,
                Args = [],
                Func = args =>
                {
                    if (args["self"] is not Iters self)
                        return new Err("invalid argument: 'self'");

                    return new Tup([.. self.Value]);
                }
            }
        },
        { "count", new NFn()
            {
                Name = "count",
                ReturnType = UnType.Int,
                Args = [],
                Func = args =>
                {
                    if (args["self"] is not Iters self)
                        return new Err("invalid argument: 'self'");

                    return Int.From(self.Value.Count());
                }
            }
        },
        { "map", new NFn()
            {
                Name = "map",
                ReturnType = UnType.Iter,
                Args = [ new Arg("fn") { IsEssential = true } ],
                Func = args =>
                {
                    if (args["self"] is not Iters self)
                        return new Err("invalid argument: 'self'");

                    if (args["fn"] is not Fn fn)
                        return new Err("invalid argument: 'fn'");

                    return new Iters(self.Value.Select(x =>
                    {
                        var res = fn.Call(new([x], [""]));
                        return res;
                    }));
                }
            }
        },
        { "filter", new NFn()
            {
                Name = "filter",
                ReturnType = UnType.Iter,
                Args = [ new Arg("fn") { IsEssential = true } ],
                Func = args =>
                {
                    if (args["self"] is not Iters self)
                        return new Err("invalid argument: 'self'");

                    if (args["fn"] is not Fn fn)
                        return new Err("invalid argument: 'fn'");

                    return new Iters(self.Value.Where(x =>
                    {
                        var res = fn.Call(new([x], [""]));
                        return res is Bool b && b.Value;
                    }));
                }
            }
        },
        { "sum", new NFn()
            {
                Name = "sum",
                Args = [],
                Func = args =>
                {
                    if (args["self"] is not Iters self)
                        return new Err("invalid argument: 'self'");

                    Obj sum = null!;

                    foreach (var v in self.Value)
                        sum = sum is null ? v : sum.Add(v);

                    return sum ?? None;
                }
            }
        },
        { "first", new NFn()
            {
                Name = "first",
                Args = [],
                Func = args =>
                {
                    if (args["self"] is not Iters self)
                        return new Err("invalid argument: 'self'");

                    return self.Value.FirstOrDefault() ?? None;
                }
            }
        },
        { "last", new NFn()
            {
                Name = "last",
                Args = [],
                Func = args =>
                {
                    if (args["self"] is not Iters self)
                        return new Err("invalid argument: 'self'");

                    return self.Value.LastOrDefault() ?? None;
                }
            }
        },
        { "any", new NFn()
            {
                Name = "any",
                Args = [ new Arg("fn") { IsEssential = true } ],
                Func = args =>
                {
                    if (args["self"] is not Iters self)
                        return new Err("invalid argument: 'self'");

                    if (args["fn"] is not Fn fn)
                        return new Err("invalid argument: 'fn'");

                    bool result = self.Value.Any(x =>
                    {
                        var res = fn.Call(new([x], [""]));
                        return res.As<Bool>(out var v) && v.Value;
                    });

                    return Bool.From(result);
                }
            }
        },
        { "all", new NFn()
            {
                Name = "all",
                ReturnType = UnType.Bool,
                Args = [ new Arg("fn") { IsEssential = true } ],
                Func = args =>
                {
                    if (args["self"] is not Iters self)
                        return new Err("invalid argument: 'self'");

                    if (args["fn"] is not Fn fn)
                        return new Err("invalid argument: 'fn'");

                    bool result = self.Value.All(x =>
                    {
                        var res = fn.Call(new([x], [""]));
                        return res.As<Bool>(out var v) && v.Value;
                    });

                    return Bool.From(result);
                }
            }
        }
    };
}