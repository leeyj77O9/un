using Un.Object.Function;
using Un.Object.Collections;
using Un.Object.Iter;
using Un.Object.Type;

namespace Un.Object.Primitive;

public class Str : Ref<string>
{   
    public static Str Empty = new();
    private static Dictionary<string, Str> pool = [];

    public Str() : this("") { }
    
    private Str(string value) : base(value, UnType.Str) { }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => new Str(""),
        { Count: 1 } => args[0].ToStr(),
        _ => new Err($"too many arguments"),
    };

    public char this[int index] => Value[index];

    public override Obj Add(Obj other) => new Str(Value + other.ToStr().As<Str>().Value);

    public override Obj Sub(Obj other) => new Str(Value.Replace(other.ToStr().As<Str>().Value, ""));

    public override Obj Eq(Obj other) => other switch
    {
        Str s => Bool.From(Value.CompareTo(s.Value) == 0),
        Obj o when o.IsNone() => Bool.False,
        _ => new Err($"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'")
    };

    public override Obj NEq(Obj other) => other switch
    {
        Str s => Bool.From(Value.CompareTo(s.Value) != 0),
        Obj o when o.IsNone() => Bool.True,
        _ => new Err($"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'")
    };

    public override Obj Lt(Obj other) => other switch
    {
        Str s => Bool.From(Value.CompareTo(s.Value) < 0),
        _ => new Err($"unsupported operand type(s) for <: '{Type}' and '{other.Type}'")
    };

    public override Obj GetItem(Obj other) => other switch
    {
        Int i => OutOfRange((int)i.Value) ? OutOfRange((int)(i.Value + Value.Length)) ? new Err("list index out of range") : 
        Str.From($"{this[(int)(i.Value + Value.Length)]}") : Str.From($"{this[(int)i.Value]}"),
        _ => new Err("invalid index type"),
    };

    public override Int Len() => Int.From(Value.Length);

    public override Obj ToInt() => long.TryParse(Value, out var result) ? Int.From(result) : new Err($"cannot convert '{Value}' to 'int'");

    public override Obj ToFloat() => double.TryParse(Value, out var result) ? new Float(result) : new Err($"cannot convert '{Value}' to 'float'");

    public override Str ToStr() => this;

    public override Obj ToBool() => bool.TryParse(Value, out var result) ? result ? Bool.True : Bool.False : string.IsNullOrEmpty(Value) ? Bool.False : Bool.True;

    public override List ToList()
    {
        var list = new List();
        foreach (var c in Value)
            list.Add(new Str($"{c}"));
        return list;
    }

    public override Tup ToTuple() => ToList().ToTuple();

    private bool OutOfRange(int value)
    {
        if (Value.Length <= value)
            return true;
        return false;
    }

    public override Obj Copy() => new Str(Value)
    {
        Annotations = Annotations
    };

    public override Obj Clone() => new Str(Value)
    {
        Annotations = Annotations
    };

    public override int GetHashCode() => Value.GetHashCode();

    public override string ToString() => Value.ToString();

    public override Int Hash() => Int.From(GetHashCode());

    public static Str From(string value)
    {
        if (value == null) return Empty;

        if (value.Length > 32)
            return new Str(value);

        if (pool.TryGetValue(value, out var cached))
            return cached;

        var result = new Str(value);
        pool[value] = result;

        return result;
    }

    public static Str From(string value, bool intern)
    {
        if (value == null) return Empty;

        if (!intern) return new Str(value);

        if (pool.TryGetValue(value, out var cached))
            return cached;

        var result = new Str(value);
        pool[value] = result;

        return result;
    }

    public override Attributes GetOriginal() => new()
    {
        { "is_empty", new NFn
            {
                Name = "is_empty",
                ReturnType = UnType.Bool,
                Args = [],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");

                    return Bool.From(string.IsNullOrEmpty(self.Value));
                }
            }
        },
        { "index_of", new NFn
            {
                Name = "index_of",
                ReturnType = UnType.Int,
                Args = [new Arg("value") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");
                    if (!args["value"].As<Str>(out var value))
                        return new Err("invalid argument: value");

                    return Int.From(self.Value.IndexOf(value.Value));
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
                    if (!args["self"].As<Str>(out var self))
                       return new Err("invalid argument: self");
                    if (!args["value"].As<Str>(out var value))
                       return new Err("invalid argument: value");

                    return Bool.From(self.Value.Contains(value.Value));
                }
            }
        },
        { "starts_with", new NFn
            {
                Name = "starts_with",
                ReturnType = UnType.Bool,
                Args = [new Arg("value") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");
                    if (!args["value"].As<Str>(out var value))
                        return new Err("invalid argument: value");

                    return Bool.From(self.Value.StartsWith(value.Value));
                }
            }
        },
        { "ends_with", new NFn
            {
                Name = "ends_with",
                ReturnType = UnType.Bool,
                Args = [new Arg("value") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return  new Err("invalid argument: self");
                    if (!args["value"].As<Str>(out var value))
                        return new Err("invalid argument: value");

                    return Bool.From(self.Value.EndsWith(value.Value));
                }
            }
        },
        { "to_upper", new NFn
            {
                Name = "to_upper",
                ReturnType = UnType.Str,
                Args = [],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");

                    return new Str(self.Value.ToUpper());
                }
            }
        },
        { "to_lower", new NFn
            {
                Name = "to_lower",
                ReturnType = UnType.Str,
                Args = [],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");

                    return new Str(self.Value.ToLower());
                }
            }
        },
        { "split", new NFn
            {
                Name = "split",
                ReturnType = UnType.List,
                Args = [new Arg("sep") { IsOptional = true, DefaultValue = new Str(" ")}],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");
                    if (!args["sep"].As<Str>(out var sep))
                        return new Err("invalid argument: sep");

                    var parts = self.Value.Split(sep.Value);
                    return new List([..parts.Select(p => new Str(p))]);
                }
            }
        },
        { "trim", new NFn
            {
                Name = "trim",
                ReturnType = UnType.Str,
                Args = [ new Arg("chars") { IsOptional = true, DefaultValue = new Str("") }],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");
                    if (!args["chars"].As<Str>(out var chars))
                        return new Err("invalid argument: chars");

                    return new Str(self.Value.Trim(chars.Value.ToCharArray()));
                }
            }
        },
        { "join", new NFn
            {
                Name = "join",
                ReturnType = UnType.Str,
                Args = [new Arg("values") {IsEssential = true}],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");

                    var parts = args["values"].Iter().As<Iters>().Value.Select(v => v.ToStr().As<Str>().Value);
                    return new Str(string.Join(self.Value, parts));
                }
            }
        },
        { "is_number", new NFn
            {
                Name = "is_number",
                ReturnType = UnType.Bool,
                Args = [],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");

                    bool result = self.Value.All(char.IsDigit);
                    return Bool.From(result);
                }
            }
        },
        { "is_alphabet", new NFn
            {
                Name = "is_alphabet",
                ReturnType = UnType.Bool,
                Args = [],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");

                    bool result = self.Value.All(char.IsLetter);
                    return Bool.From(result);
                }
            }
        },
        { "center", new NFn
            {
                Name = "center",
                ReturnType = UnType.Str,
                Args = [
                    new Arg("width") { IsEssential = true },
                    new Arg("fill") { IsOptional = true, DefaultValue = new Str(" ") }
                ],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");
                    if (!args["width"].As<Int>(out var width))
                        return new Err("invalid argument: width");
                    if (!args["fill"].As<Str>(out var fill))
                        return new Err("invalid argument: fill");

                    var pad = Math.Max(0, width.Value - self.Value.Length);
                    var left = pad / 2;
                    var right = pad - left;
                    var fillChar = fill.Value.Length > 0 ? fill.Value[0] : ' ';
                    return new Str(new string(fillChar, (int)left) + self.Value + new string(fillChar, (int)right));
                }
            }
        },
        { "left", new NFn
            {
                Name = "left",
                ReturnType = UnType.Str,
                Args = [
                    new Arg("width") { IsEssential = true },
                    new Arg("fill") { IsOptional = true, DefaultValue = new Str(" ") }
                ],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");
                    if (!args["width"].As<Int>(out var width))
                        return new Err("invalid argument: width");
                    if (!args["fill"].As<Str>(out var fill))
                        return new Err("invalid argument: fill");

                    var pad = Math.Max(0, width.Value - self.Value.Length);
                    var fillChar = fill.Value.Length > 0 ? fill.Value[0] : ' ';
                    return new Str(self.Value + new string(fillChar, (int)pad));
                }
            }
        },
        { "right", new NFn
            {
                Name = "right",
                ReturnType = UnType.Str,
                Args = [
                    new Arg("width") { IsEssential = true },
                    new Arg("fill") { IsOptional = true, DefaultValue = new Str(" ") }
                ],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");
                    if (!args["width"].As<Int>(out var width))
                        return new Err("invalid argument: width");
                    if (!args["fill"].As<Str>(out var fill))
                        return new Err("invalid argument: fill");

                    var pad = Math.Max(0, width.Value - self.Value.Length);
                    var fillChar = fill.Value.Length > 0 ? fill.Value[0] : ' ';
                    return new Str(new string(fillChar, (int)pad) + self.Value);
                }
            }
        },
        { "replace", new NFn
            {
                Name = "replace",
                ReturnType = UnType.Str,
                Args = [
                    new Arg("old") { IsEssential = true },
                    new Arg("new") { IsEssential = true }
                ],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");
                    if (!args["old"].As<Str>(out var oldStr))
                        return new Err("invalid argument: old");
                    if (!args["new"].As<Str>(out var newStr))
                        return new Err("invalid argument: new");

                    return new Str(self.Value.Replace(oldStr.Value, newStr.Value));
                }
            }
        },
        { "find", new NFn
            {
                Name = "find",
                ReturnType = UnType.Int,
                Args = [
                    new Arg("substr") { IsEssential = true }
                ],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        return new Err("invalid argument: self");
                    if (!args["substr"].As<Str>(out var substr))
                        return new Err("invalid argument: substr");

                    return Int.From(self.Value.IndexOf(substr.Value));
                }
            }
        },

    };
    
}