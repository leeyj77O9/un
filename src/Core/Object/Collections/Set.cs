using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Type;

namespace Un.Object.Collections;

public class Set(HashSet<Obj> value) : Ref<HashSet<Obj>>(value, UnType.Set)
{
    public Set() : this([]) { }

    public override Obj Init(Tup args) => new Set([..args.ToList()]);

    public override Obj Add(Obj other) => other switch
    {
        Set otherSet => new Set([.. Value.Union(otherSet.Value)]),
        _ => new Err($"unsupported operand type(s) for +: 'set' and '{other.Type}'")
    };

    public override Obj Sub(Obj other)=> other switch
    {
        Set otherSet => new Set([.. Value.Except(otherSet.Value)]),
        _ => new Err($"unsupported operand type(s) for -: 'set' and '{other.Type}'")
    };

    public override Obj BXor(Obj other)=> other switch
    {
        Set otherSet => new Set([.. Value.Intersect(otherSet.Value)]),
        _ => new Err($"unsupported operand type(s) for ^: 'set' and '{other.Type}'")
    };

    public override Int Len() => Int.From(Value.Count);

    public override Obj GetItem(Obj key) => Value.TryGetValue(key, out var value) ? value : new Err($"key {key.ToStr().As<Str>().Value} not found in set");

    public override Obj Copy() => this;

    public override Obj Clone() => new Set([.. Value]);

    public override Str ToStr() => Str.From($"{{{string.Join(", ", Value.Select(x => x.ToStr().As<Str>().Value))}}}");

    public override Spreads Spread() => new([.. Value]);

    public override Attributes GetOriginal() => new()
    {
        { "add", new NFn
            {
                Name = "add",
                ReturnType = UnType.Bool,
                Args = [new Arg("value") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Set>(out var self))
                        return new Err("invalid argument: self");
                    return Bool.From(self.Value.Add(args["value"]));
                }
            }
        },
        { "remove", new NFn
            {
                Name = "remove",
                ReturnType = UnType.Bool,
                Args = [new Arg("value") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Set>(out var self))
                        return new Err("invalid argument: self");
                    return Bool.From(self.Value.Remove(args["value"]));
                }
            }
        },
        { "contains", new NFn
            {
                Name = "contains",
                ReturnType = UnType.Bool,
                Args = [new Arg("value") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Set>(out var self))
                        return new Err("invalid argument: self");
                    return Bool.From(self.Value.Contains(args["value"]));
                }
            }
        },
        { "clear", new NFn
            {
                Name = "clear",
                Args = [],
                Func = args =>
                {
                    if (!args["self"].As<Set>(out var self))
                        return new Err("invalid argument: self");
                    self.Value.Clear();
                    return None;
                }
            }
        },
        { "clone", new NFn
            {
                Name = "clone",
                ReturnType = UnType.Set,
                Args = [],
                Func = args =>
                {
                    if (!args["self"].As<Set>(out var self))
                        return new Err("invalid argument: self");
                    return self.Clone();
                }
            }
        },
        { "union", new NFn
            {
                Name = "union",
                ReturnType = UnType.Set,
                Args = [new Arg("other") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Set>(out var self))
                        return new Err("invalid argument: self");
                    if (!args["other"].As<Set>(out var other))
                        return new Err("invalid argument: other");
                    return self.Add(other);
                }
            }
        },
        { "intersect", new NFn
            {
                Name = "intersect",
                ReturnType = UnType.Set,
                Args = [new Arg("other") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Set>(out var self))
                        return new Err("invalid argument: self");
                    if (!args["other"].As<Set>(out var other))
                        return new Err("invalid argument: other");
                    return self.Sub(other);
                }
            }
        },
        { "difference", new NFn
            {
                Name = "difference",
                ReturnType = UnType.Set,
                Args = [new Arg("other") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Set>(out var self))
                        return new Err("invalid argument: self");
                    if (!args["other"].As<Set>(out var other))
                        return new Err("invalid argument: other");
                    return self.BXor(other);
                }
            }
        }
    };
}