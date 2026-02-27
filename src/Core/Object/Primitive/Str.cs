using Un.Object.Function;
using Un.Object.Collections;
using Un.Object.Iter;

namespace Un.Object.Primitive;

public class Str(string value) : Ref<string>(value, "str")
{
    public Str() : this("") { }

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
        Str s => new Bool(Value.CompareTo(s.Value) == 0),
        Obj o when o.IsNone() => Bool.False,
        _ => new Err($"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'")
    };

    public override Obj NEq(Obj other) => other switch
    {
        Str s => new Bool(Value.CompareTo(s.Value) != 0),
        Obj o when o.IsNone() => Bool.True,
        _ => new Err($"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'")
    };

    public override Obj Lt(Obj other) => other switch
    {
        Str s => new Bool(Value.CompareTo(s.Value) < 0),
        _ => new Err($"unsupported operand type(s) for <: '{Type}' and '{other.Type}'")
    };

    public override Obj GetItem(Obj other) => other switch
    {
        Int i => new Str($"{Value[(int)i.Value]}"),
        _ => new Err("invalid index type"),
    };

    public override Int Len() => new(Value.Length);

    public override Obj ToInt() => long.TryParse(Value, out var result) ? new Int(result) : new Err($"cannot convert '{Value}' to 'int'");

    public override Obj ToFloat() => double.TryParse(Value, out var result) ? new Float(result) : new Err($"cannot convert '{Value}' to 'float'");

    public override Str ToStr() => this;

    public override Obj ToBool() => bool.TryParse(Value, out var result) ? new Bool(result) : string.IsNullOrEmpty(Value) ? Bool.False : Bool.True;

    public override List ToList()
    {
        var list = new List();
        foreach (var c in Value)
            list.Add(new Str($"{c}"));
        return list;
    }

    public override Tup ToTuple() => ToList().ToTuple();

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

    public override Int Hash() => new(GetHashCode());

    public override Attributes GetOriginal() => new()
    {
        { "is_empty", new NFn
            {
                Name = "is_empty",
                Args = [],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");

                    return new Bool(string.IsNullOrEmpty(self.Value));
                }
            }
        },
        { "index_of", new NFn
            {
                Name = "index_of",
                Args = [new Arg("value") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");
                    if (!args["value"].As<Str>(out var value))
                        new Err("invalid argument: value");

                    return new Int(self.Value.IndexOf(value.Value));
                }
            }
        },
        { "contains", new NFn
            {
                Name = "contains",
                Args = [new Arg("value") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");
                    if (!args["value"].As<Str>(out var value))
                        new Err("invalid argument: value");

                    return new Bool(self.Value.Contains(value.Value));
                }
            }
        },
        { "starts_with", new NFn
            {
                Name = "starts_with",
                Args = [new Arg("value") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");
                    if (!args["value"].As<Str>(out var value))
                        new Err("invalid argument: value");

                    return new Bool(self.Value.StartsWith(value.Value));
                }
            }
        },
        { "ends_with", new NFn
            {
                Name = "ends_with",
                Args = [new Arg("value") { IsEssential = true }],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");
                    if (!args["value"].As<Str>(out var value))
                        new Err("invalid argument: value");

                    return new Bool(self.Value.EndsWith(value.Value));
                }
            }
        },
        { "to_upper", new NFn
            {
                Name = "to_upper",
                Args = [],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");

                    return new Str(self.Value.ToUpper());
                }
            }
        },
        { "to_lower", new NFn
            {
                Name = "to_lower",
                Args = [],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");

                    return new Str(self.Value.ToLower());
                }
            }
        },
        { "split", new NFn
            {
                Name = "split",
                Args = [new Arg("sep") { IsOptional = true, DefaultValue = new Str(" ")}],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");
                    if (!args["sep"].As<Str>(out var sep))
                        new Err("invalid argument: sep");

                    var parts = self.Value.Split(sep.Value);
                    return new List([..parts.Select(p => new Str(p))]);
                }
            }
        },
        { "trim", new NFn
            {
                Name = "trim",
                Args = [ new Arg("chars") { IsOptional = true, DefaultValue = new Str("") }],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");
                    if (!args["chars"].As<Str>(out var chars))
                        new Err("invalid argument: chars");

                    return new Str(self.Value.Trim(chars.Value.ToCharArray()));
                }
            }
        },
        { "join", new NFn
            {
                Name = "join",
                Args = [new Arg("values") {IsEssential = true}],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");

                    var parts = args["values"].Iter().As<Iters>().Value.Select(v => v.ToStr().As<Str>().Value);
                    return new Str(string.Join(self.Value, parts));
                }
            }
        },
        { "is_number", new NFn
            {
                Name = "is_number",
                Args = [],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");

                    bool result = self.Value.All(char.IsDigit);
                    return new Bool(result);
                }
            }
        },
        { "is_alphabet", new NFn
            {
                Name = "is_alphabet",
                Args = [],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");

                    bool result = self.Value.All(char.IsLetter);
                    return new Bool(result);
                }
            }
        },
        { "center", new NFn
            {
                Name = "center",
                Args = [
                    new Arg("width") { IsEssential = true },
                    new Arg("fill") { IsOptional = true, DefaultValue = new Str(" ") }
                ],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");
                    if (!args["width"].As<Int>(out var width))
                        new Err("invalid argument: width");
                    if (!args["fill"].As<Str>(out var fill))
                        new Err("invalid argument: fill");

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
                Args = [
                    new Arg("width") { IsEssential = true },
                    new Arg("fill") { IsOptional = true, DefaultValue = new Str(" ") }
                ],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");
                    if (!args["width"].As<Int>(out var width))
                        new Err("invalid argument: width");
                    if (!args["fill"].As<Str>(out var fill))
                        new Err("invalid argument: fill");

                    var pad = Math.Max(0, width.Value - self.Value.Length);
                    var fillChar = fill.Value.Length > 0 ? fill.Value[0] : ' ';
                    return new Str(self.Value + new string(fillChar, (int)pad));
                }
            }
        },
        { "right", new NFn
            {
                Name = "right",
                Args = [
                    new Arg("width") { IsEssential = true },
                    new Arg("fill") { IsOptional = true, DefaultValue = new Str(" ") }
                ],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");
                    if (!args["width"].As<Int>(out var width))
                        new Err("invalid argument: width");
                    if (!args["fill"].As<Str>(out var fill))
                        new Err("invalid argument: fill");

                    var pad = Math.Max(0, width.Value - self.Value.Length);
                    var fillChar = fill.Value.Length > 0 ? fill.Value[0] : ' ';
                    return new Str(new string(fillChar, (int)pad) + self.Value);
                }
            }
        },
        { "replace", new NFn
            {
                Name = "replace",
                Args = [
                    new Arg("old") { IsEssential = true },
                    new Arg("new") { IsEssential = true }
                ],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");
                    if (!args["old"].As<Str>(out var oldStr))
                        new Err("invalid argument: old");
                    if (!args["new"].As<Str>(out var newStr))
                        new Err("invalid argument: new");

                    return new Str(self.Value.Replace(oldStr.Value, newStr.Value));
                }
            }
        },
        { "find", new NFn
            {
                Name = "find",
                Args = [
                    new Arg("substr") { IsEssential = true }
                ],
                Func = args =>
                {
                    if (!args["self"].As<Str>(out var self))
                        new Err("invalid argument: self");
                    if (!args["substr"].As<Str>(out var substr))
                        new Err("invalid argument: substr");

                    return new Int(self.Value.IndexOf(substr.Value));
                }
            }
        },

    };
    
}