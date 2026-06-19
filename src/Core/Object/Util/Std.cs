using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Collections;
using Un.Object.Iter;
using Un.Object.Type;

namespace Un.Object.Util;

public class Std : IPack
{
    private static readonly IO.Stream so = new(Console.OpenStandardOutput());
    private static readonly StreamWriter ew = new(Console.OpenStandardError()) { AutoFlush = false };
    private static readonly IO.Stream sr = new(Console.OpenStandardInput());

    private static readonly Dictionary<Fn, Dictionary<int, Obj>> memo = []; 

    private static readonly Random random = new();

    public Attributes GetOriginalMembers() => [];

    public Attributes GetOriginalMethods() => new()
    {
        { "write", new NFn()
            {
                Name = "write",
                ReturnType = UnType.None,
                Args = [
                   new Arg("value") {
                    Type = CollectionType.Create(UnType.Tuple, UnType.Any),
                    IsPositional = true,
                }, new Arg("sep") {
                    Type = UnType.Str,
                    IsOptional = true,
                    DefaultValue = Str.From(" ")
                }, new Arg("end"){
                    Type = UnType.Str,
                    IsOptional = true,
                    DefaultValue = Str.From("\n")
                }, new Arg("stream"){
                    Type = UnType.Create("stream"),
                    IsOptional = true,
                    DefaultValue = so
                }],
                Annotations = { 
                    ["doc"] = new Tup(
                        [
                            Str.From("Writes the given values to the specified stream (default is standard output) with an optional separator and end string."),
                            Str.From("value: variable argumnet of values to write"),
                            Str.From("sep: string inserted between values, default is a space"),
                            Str.From("end: string appended after the last value, default is a newline"),
                            Str.From("stream: the stream to write to, default is standard output"),
                            Str.From("returns: none")
                        ],
                        [
                            "description", "value", "sep", "end", "stream", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    if (!args["stream"].As<IO.Stream>(out var stream))
                        return new Err("expected 'stream' argument to be of type 'stream'");

                    var items = args["value"].ToTuple().As<Tup>().Value;

                    var cw = new StreamWriter(stream.Value)
                    {
                        AutoFlush = false
                    };

                    for (int i = 0; i < items.Length; i++)
                    {
                        var text = items[i].ToStr().As<Str>(out var str) ? str.Value : items[i].Repr().As<Str>().Value;
                        cw.Write(text);
                        if (i != items.Length - 1)
                            cw.Write(args["sep"].ToStr().As<Str>(out var sep) ? sep.Value : args["sep"].Repr().As<Str>().Value);
                    }
                    cw.Write(args["end"].ToStr().As<Str>(out var end) ? end.Value : args["end"].Repr().As<Str>().Value);
                    cw.Flush();

                    return Obj.None;
                }
            }
        },
        { "read", new NFn()
            {
                Name = "read",
                ReturnType = UnType.Str,
                Args = [
                   new Arg("prompt") {
                    Type = UnType.Str,
                    IsOptional = true,
                    DefaultValue = Str.Empty,
                }, new Arg("stream") {
                    Type = UnType.Create("stream"),
                    IsOptional = true,
                    DefaultValue = sr
                }],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Reads a line of input from the specified stream (default is standard input) with an optional prompt."),
                            Str.From("prompt: an optional string to display as a prompt before reading input"),
                            Str.From("stream: the stream to write to, default is standard output"),
                            Str.From("returns: the line of input read as a string")
                        ],
                        [
                            "description", "promt", "stream", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    if (!args["stream"].As<IO.Stream>(out var stream))
                        return new Err("expected 'stream' argument to be of type 'stream'");

                    var cr = new StreamReader(stream.Value);
                    var cw = new StreamWriter(so.Value)
                    {
                        AutoFlush = false
                    };

                    cw.Write(args["prompt"].ToStr().As<Str>(out var str) ? str.Value : args["prompt"].Repr().As<Str>().Value);
                    cw.Flush();
                    return Str.From(cr.ReadLine() ?? "");
                }
            }
        },
        { "log", new NFn()
            {
                Name = "log",
                ReturnType = UnType.None,
                Args = [
                   new Arg("value") {
                    Type = CollectionType.Create(UnType.Tuple, UnType.Any),
                    IsPositional = true,
                }, new Arg("sep") {
                    Type = UnType.Str,
                    IsOptional = true,
                    DefaultValue = Str.From(" ")
                }, new Arg("end"){
                    Type = UnType.Str,
                    IsOptional = true,
                    DefaultValue = Str.From("\n")
                }],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Writes the given values to the specified standard error stream with an optional separator and end string."),
                            Str.From("value: variable argumnet of values to write"),
                            Str.From("sep: string inserted between values, default is a space"),
                            Str.From("end: string appended after the last value, default is a newline"),
                            Str.From("returns: none")
                        ],
                        [
                            "description", "value", "sep", "end", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    var items = args["value"].ToTuple().As<Tup>().Value;
                    for (int i = 0; i < items.Length; i++)
                    {
                        var text = items[i].ToStr().As<Str>(out var str) ? str.Value : items[i].Repr().As<Str>().Value;
                        ew.Write(text);
                        if (i != items.Length - 1)
                            ew.Write(args["sep"].ToStr().As<Str>(out var sep) ? sep.Value : args["sep"].Repr().As<Str>().Value);
                    }
                    ew.Write(args["end"].ToStr().As<Str>(out var end) ? end.Value : args["end"].Repr().As<Str>().Value);
                    ew.Flush();

                    return Obj.None;
                }
            }
        },
        { "len", new NFn()
            {
                Name = "len",
                Args = [
                   new Arg("value") {
                    IsEssential = true,
                }],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns the length of the given value."),
                            Str.From("value: the value to get the length of"),
                            Str.From("returns: the length of the value as an integer")
                        ],
                        [
                            "description", "value", "return"
                        ]
                    )
                },
                Func = (args) => args["value"].Len()
            }
        },
        { "clear", new NFn()
            {
                Name = "clear",
                ReturnType = UnType.None,
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Clears the console screen."),
                            Str.From("returns: none")
                        ],
                        [
                            "description", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    Console.Clear();
                    return Obj.None;
                }
            }
        },
        { "exit", new NFn()
            {
                Name = "exit",
                ReturnType = UnType.None,
                Args = [
                    new Arg("code") {
                        Type = UnType.Int,
                        IsOptional = true,
                        DefaultValue = Int.From(0)
                }],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Exits the program with the specified exit code."),
                            Str.From("code: the exit code to use when exiting the program, default is 0"),
                            Str.From("returns: none")
                        ],
                        [
                            "description", "code", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    Environment.Exit((int)args["code"].ToInt().As<Int>().Value);
                    return Obj.None;
                }
            }
        },
        { "type", new NFn()
            {
                Name = "type",
                ReturnType = UnType.Str,
                Args = [
                   new Arg("value") {
                    IsEssential = true,
                }],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns the type of the given value as a string."),
                            Str.From("value: the value to get the type of"),
                            Str.From("returns: the type of the value as a string")
                        ],
                        [
                            "description", "value", "return"
                        ]
                    )
                },
                Func = (args) => Str.From(args["value"].Type.Name)
            }
        },
        { "array", new NFn()
            {
                Name = "array",
                ReturnType = CollectionType.Create(UnType.List, UnType.TGeneric),
                Args = [
                   new Arg("default") {
                    Type = UnType.TGeneric,
                    IsEssential = true,
                }, new Arg("size") {
                    Type = CollectionType.Create(UnType.Tuple, UnType.Int),
                    IsPositional = true,
                    DefaultValue = new Tup([Int.From(1)], [])
                }],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Creates a multi-dimensional list (array) with the specified default value and size."),
                            Str.From("default: the default value to fill the array with"),
                            Str.From("size: a tuple of integers specifying the size of each dimension of the array, default is (1,) for a one-dimensional array of size 1"),
                            Str.From("returns: a multi-dimensional list (array) filled with the default value")
                        ],
                        [
                            "description", "default", "size", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    return Create(args["size"].ToList().As<List>());

                    List Create(List lengths)
                    {
                        List list = [];

                        var count = lengths[0].As<Int>("length argument is expected an integer");

                        for (int i = 0; i < count.Value; i++)
                            list.Append(lengths.Count == 1 ? args["default"].Clone() : Create([.. lengths.Value[1..]]));

                        return list;
                    }
                }
            }
        },
        { "range", new NFn()
            {
                Name = "range",
                ReturnType = UnType.Create("range"),
                Args = [
                    new Arg("start") {
                        Type = UnType.Int,
                        IsOptional = true,
                        DefaultValue = Int.From(0),
                    },
                    new Arg("stop") {
                        Type = UnType.Int,
                        IsEssential = true
                    },
                    new Arg("step") {
                        Type = UnType.Int,
                        IsOptional = true,
                        DefaultValue = Int.From(1),
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Generates a sequence of integers from start (inclusive) to stop (exclusive) with a specified step."),
                            Str.From("start: the starting value of the sequence, default is 0"),
                            Str.From("stop: the end value of the sequence (exclusive)"),
                            Str.From("step: the increment value between each integer in the sequence, default is 1"),
                            Str.From("returns: a list of integers in the specified range")
                        ],
                        [
                            "description", "start", "stop", "step", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    long stop = args["stop"].ToInt().As<Int>().Value,
                         start = args["start"].ToInt().As<Int>().Value,
                         step  = args["step"].ToInt().As<Int>().Value;

                    return new Iter.Range(start, stop, step);
                }
            }
        },
        { "enumerate", new NFn()
            {
                Name = "enumerate",
                ReturnType = CollectionType.Create(UnType.Iter, CollectionType.Create(UnType.Tuple, UnionType.Create(UnType.Int, UnType.TGeneric))),
                Args = [
                    new Arg("array") {
                        Type = UnionType.Create(CollectionType.Create(UnType.List, UnType.TGeneric), CollectionType.Create(UnType.Tuple, UnType.TGeneric)),
                        IsEssential = true
                    },
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns a list of tuples, where each tuple contains an index and the corresponding value from the input array."),
                            Str.From("array: a list or tuple to enumerate"),
                            Str.From("returns: a list of tuples, where each tuple contains an index and the corresponding value from the input array")
                        ],
                        [
                            "description", "array", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    if (!args["array"].As<List, Tup>(out var array))
                        return new Err("expected 'array' argument to be of type 'list' or 'tuple'");

                    return new Iters(array.Iter().As<Iters>().Value.Select((x, i) => new Tup([Int.From(i), x], ["index", "value"])));
                }
            }
        },
        { "zip", new NFn()
            {
                Name = "zip",
                ReturnType = CollectionType.Create(UnType.List, CollectionType.Create(UnType.Tuple, UnionType.Create(UnType.TGeneric, UnType.UGeneric, UnType.Infinity))),
                Args = [
                    new Arg("arrays") {
                        Type = UnionType.Create(CollectionType.Create(UnType.List, UnType.TGeneric), CollectionType.Create(UnType.Tuple, UnType.TGeneric)),
                        IsPositional = true
                    },
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns a list of tuples, where the i-th tuple contains the i-th element from each of the input arrays. The length of the returned list is equal to the length of the longest input array."),
                            Str.From("arrays: a variable number of lists or tuples to zip together"),
                            Str.From("returns: a list of tuples, where the i-th tuple contains the i-th element from each of the input arrays")
                        ],
                        [
                            "description", "arrays", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    if (!args["arrays"].ToList().As<List, Tup>(out var arrays))
                        return new Err("expected 'array' argument to be of type 'list' or 'tuple'");

                    var source = arrays.Iter().As<Iters>();
                    int length = source.Value.Max(a => (int)a.Len().As<Int>().Value);
                    List list = [];

                    for (int i = 0; i < length; i++)
                    {
                        List tuple = [];
                        foreach (var array in source.Value)
                            tuple.Append(array.Len().As<Int>().Value <= i ? Obj.None : array.GetItem(Int.From(i)));
                        list.Append(new Tup([..tuple], [.. new string(' ', tuple.Count).Split()]));
                    }

                    return list;
                }
            }
        },
        { "hash", new NFn()
            {
                Name = "hash",
                ReturnType = UnType.Int,
                Args = [
                    new Arg("value") {
                        IsEssential = true,
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns the hash code of the given value as an integer."),
                            Str.From("value: the value to get the hash code of"),
                            Str.From("returns: the hash code of the value as an integer")
                        ],
                        [
                            "description", "value", "return"
                        ]
                    )
                },
                Func = (args) => Int.From(args["value"].GetHashCode())
            }
        },
        { "open", new NFn()
            {
                Name = "open",
                ReturnType = UnType.Create("stream"),
                Args = [
                    new Arg("value") {
                        IsEssential = true,
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Opens a file and returns a stream object for it."),
                            Str.From("value: the path to the file to open as a string"),
                            Str.From("returns: a stream object for the opened file")
                        ],
                        [
                            "description", "value", "return"
                        ]
                    )
                },
                Func = (args) => args["value"] switch
                {
                    Str s => File.Exists(s.Value) ? new IO.Stream(File.Open(s.Value, FileMode.Open)) : throw new Panic($"file {s.Value} dose not exist"),
                    _ => throw new Panic("invalid type"),
                }
            }
        },
        { "sum", new NFn()
            {
                Name = "sum",
                ReturnType = UnType.TGeneric,
                Args = [
                    new Arg("value") {
                        Type = UnionType.Create(UnType.Tuple, UnType.TGeneric),
                        IsPositional = true,
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns the sum of the given values."),
                            Str.From("value: a variable argument of values to sum, can be a tuple of values or a single list/tuple/range/iters containing the values"),
                            Str.From("returns: the sum of the given values")
                        ],
                        [
                            "description", "value", "return"
                        ]
                    )
                },
                Func = (args) => {
                    var tuple = args["value"].ToTuple().As<Tup>();

                    if (tuple.Count == 0)
                        throw new Panic("expected more than one argument");

                    if (tuple.Count == 1)
                    {
                        if (tuple[0].As<List>(out var l))
                            tuple = l.ToTuple();
                        else if (tuple[0].As<Tup>(out var t))
                            tuple = t;
                        else if (tuple[0].As<Iter.Range>(out var r))
                            return Int.From(r.Value.Sum(x => x.As<Int>().Value));
                        else if (tuple[0].As<Iters>(out var it))
                            tuple = it.ToTuple();
                        else
                            return tuple[0];
                    }

                    Obj sum = tuple[0];
                    for (int i = 1; i < tuple.Count; i++)
                        sum = sum.Add(tuple[i]);

                    return sum;
                }
            }
        },
        { "max", new NFn()
            {
                Name = "max",
                ReturnType = UnType.TGeneric,
                Args = [
                    new Arg("value") {
                        Type = UnionType.Create(UnType.Tuple, UnType.TGeneric),
                        IsPositional = true,
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns the maximum value among the given values."),
                            Str.From("value: a variable argument of values to compare, can be a tuple of values or a single list/tuple/range/iters containing the values"),
                            Str.From("returns: the maximum value among the given values")
                        ],
                        [
                            "description", "value", "return"
                        ]
                    )
                },
                Func = (args) => {
                    var tuple = args["value"].ToTuple().As<Tup>();

                    if (tuple.Count == 0)
                        return new Err("expected more than one argument");

                    if (tuple.Count == 1)
                    {
                        if (tuple[0].As<List>(out var l))
                            tuple = l.ToTuple();
                        else if (tuple[0].As<Tup>(out var t))
                            tuple = t;
                        else
                            return tuple[0];
                    }

                    Obj max = tuple[0];
                    for (int i = 1; i < tuple.Count; i++)
                        if (max.Lt(tuple[i]).As<Bool>().Value)
                            max = tuple[i];

                    return max;
                }
            }
        },
        { "min", new NFn()
            {
                Name = "min",
                ReturnType = UnType.TGeneric,
                Args = [
                    new Arg("value") {
                        Type = UnionType.Create(UnType.Tuple, UnType.TGeneric),
                        IsPositional = true,
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns the minimum value among the given values."),
                            Str.From("value: a variable argument of values to compare, can be a tuple of values or a single list/tuple/range/iters containing the values"),
                            Str.From("returns: the minimum value among the given values")
                        ],
                        [
                            "description", "value", "return"
                        ]
                    )
                },
                Func = (args) => {
                    var tuple = args["value"].ToTuple().As<Tup>();

                    if (tuple.Count == 0)
                        return new Err("expected more than one argument");

                    if (tuple.Count == 1)
                    {
                        if (tuple[0].As<List>(out var l))
                            tuple = l.ToTuple();
                        else if (tuple[0].As<Tup>(out var t))
                            tuple = t;
                        else
                            return tuple[0];
                    }

                    Obj min = tuple[0];
                    for (int i = 1; i < tuple.Count; i++)
                        if (min.Gt(tuple[i]).As<Bool>().Value)
                            min = tuple[i];

                    return min;
                }
            }
        },
        { "round", new NFn()
            {
                Name = "round",
                ReturnType = UnionType.Create(UnType.Int, UnType.Float),
                Args = [
                    new Arg("value") {
                        Type = UnionType.Create(UnType.Int, UnType.Float),
                        IsEssential = true
                    },
                    new Arg("digit") {
                        Type = UnionType.Create(UnType.Int),
                        IsOptional = true,
                        DefaultValue = Int.From(0),
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Rounds a number to a specified number of decimal places."),
                            Str.From("value: the number to round, can be an integer or a float"),
                            Str.From("digit: the number of decimal places to round to, default is 0"),
                            Str.From("returns: the rounded number, as an integer if digit is 0 or the result is an integer, otherwise as a float")
                        ],
                        [
                            "description", "value", "digit", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    double v = args["value"] switch
                    {
                        Int i => i.Value,
                        Float f => f.Value,
                        _ => double.NaN,
                    };

                    if (v == double.NaN)
                        return new Err("expected number type");

                    if (!args["digit"].As<Int>(out var digit) || digit.Value > 15 || digit.Value < 0)
                        return new Err("digit is must be int and greater then 0 and less then 15");

                    int d = (int)digit.Value;

                    v = Math.Round(v, d);

                    return d == 0 || double.IsInteger(v) ? Int.From((long) v) : new Float(v);
                }
            }
        },
        { "abs", new NFn()
            {
                Name = "abs",
                ReturnType = UnionType.Create(UnType.Int, UnType.Float),
                Args = [
                    new Arg("value") {
                        Type = UnionType.Create(UnType.Int, UnType.Float),
                        IsEssential = true,
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns the absolute value of a number."),
                            Str.From("value: the number to get the absolute value of, can be an integer or a float"),
                            Str.From("returns: the absolute value of the given number, as an integer if the input is an integer and the result is an integer, otherwise as a float")
                        ],
                        [
                            "description", "value", "return"
                        ]
                    )
                },
                Func = (args) => args["value"] switch
                {
                    Int i => Int.From(Math.Abs(i.Value)),
                    Float f => new Float(Math.Abs(f.Value)),
                    _ => new Float(double.NaN),
                }
            }
        },
        { "ceil", new NFn()
            {
                Name = "ceil",
                ReturnType = UnionType.Create(UnType.Int, UnType.Float),
                Args = [
                    new Arg("value") {
                        Type = UnionType.Create(UnType.Int, UnType.Float),
                        IsEssential = true,
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns the smallest integer greater than or equal to a number."),
                            Str.From("value: the number to round up, can be an integer or a float"),
                            Str.From("returns: the smallest integer greater than or equal to the given number, as an integer if the input is an integer and the result is an integer, otherwise as a float")
                        ],
                        [
                            "description", "value", "return"
                        ]
                    )
                },
                Func = (args) => args["value"] switch
                {
                    Int i => Int.From(i.Value),
                    Float f => Math.Ceiling(f.Value) switch
                    {
                        double d when d == (long)d => Int.From((long) d),
                        double d => new Float(d),
                    },
                    _ => new Float(double.NaN),
                }
            }
        },
        { "sqrt", new NFn()
            {
                Name = "sqrt",
                ReturnType = UnType.Float,
                Args = [
                    new Arg("value") {
                        Type = UnionType.Create(UnType.Int, UnType.Float),
                        IsEssential = true,
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns the square root of a number."),
                            Str.From("value: the number to get the square root of, can be an integer or a float"),
                            Str.From("returns: the square root of the given number, as an integer if the input is an integer and the result is an integer, otherwise as a float")
                        ],
                        [
                            "description", "value", "return"
                        ]
                    )
                },
                Func = (args) => args["value"] switch
                {
                    Int i => new Float(Math.Sqrt(i.Value)),
                    Float f => new Float(Math.Sqrt(f.Value)),
                    _ => new Float(double.NaN),
                }
            }
        },
        { "floor", new NFn()
            {
                Name = "floor",
                ReturnType = UnionType.Create(UnType.Int, UnType.Float),
                Args = [
                    new Arg("value") {
                        Type = UnionType.Create(UnType.Int, UnType.Float),
                        IsEssential = true,
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns the largest integer less than or equal to a number."),
                            Str.From("value: the number to round down, can be an integer or a float"),
                            Str.From("returns: the largest integer less than or equal to the given number, as an integer if the input is an integer and the result is an integer, otherwise as a float")
                        ],
                        [
                            "description", "value", "return"
                        ]
                    )
                },
                Func = (args) => args["value"] switch
                {
                    Int i => Int.From(i.Value),
                    Float f => Math.Floor(f.Value) switch
                    {
                        double d when d == (long)d => Int.From((long) d),
                        double d => new Float(d),
                    },
                    _ => new Float(double.NaN),
                }
            }
        },
        { "memo", new NFn()
            {
                Name = "memo",
                Args = [
                    new Arg("func") {
                        Type = UnType.Func,
                        IsEssential = true
                    },
                    new Arg("args") {
                        Type = UnType.Tuple,
                        IsPositional = true,
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns a memoized version of the given function, which caches the results of previous calls with the same arguments to improve performance on subsequent calls."),
                            Str.From("func: the function to memoize"),
                            Str.From("args: a variable argument of values to pass to the function when calling it, can be a tuple of values or a single list/tuple containing the values"),
                            Str.From("returns: a memoized version of the given function")
                        ],
                        [
                            "description", "func", "args", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    var fn = args["func"].As<Fn>();

                    if (!memo.TryGetValue(fn, out var cache))
                       memo.Add(fn, cache = []);

                    var targs = args["args"].ToTuple().As<Tup>();
                    int hash = targs.GetHashCode();

                    if (!cache.TryGetValue(hash, out var result))
                    {
                        result = fn.Call(targs);
                        cache[hash] = result;
                    }

                    return result;
                }
            }
        },
        { "sleep", new NFn()
            {
                Name = "sleep",
                ReturnType = UnType.None,
                Args = [
                    new Arg("time")
                    {
                        Type = UnionType.Create(UnType.Int, UnType.Float),
                        IsEssential = true
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Pauses the execution of the program for a specified amount of time."),
                            Str.From("time: the amount of time to sleep, can be an integer representing milliseconds or a float representing seconds"),
                            Str.From("returns: none")
                        ],
                        [
                            "description", "time", "return"
                        ]
                    )
                },
                Func = args =>
                {
                    var milliseconds = args["time"] switch
                    {
                        Int i => (int)i.Value,
                        Float f => (int)(f.Value * 1000),
                        Obj o => (int)o.ToInt().As<Int>().Value,
                        _ => -1,
                    };

                    if (milliseconds == -1)
                        return new Err("expected 'time' argument to be of type 'int' or 'float' or convertible to int");

                    Thread.Sleep(milliseconds);
                    return Obj.None;
                }
            }
        },
        { "delay", new NFn()
            {
                Name = "delay",
                ReturnType = UnType.TGeneric,
                Args = [
                    new Arg("time")
                    {
                        Type = UnionType.Create(UnType.Int, UnType.Float),
                        IsEssential = true
                    },
                    new Arg("fn")
                    {
                        Type = UnType.Func,
                        IsEssential = true
                    },
                    new Arg("args")
                    {
                        Type = UnType.Tuple,
                        IsPositional = true,
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Delays the execution of a function by a specified amount of time and returns the result of the function call."),
                            Str.From("time: the amount of time to delay, can be an integer representing milliseconds or a float representing seconds"),
                            Str.From("fn: the function to call after the delay"),
                            Str.From("args: a variable argument of values to pass to the function when calling it, can be a tuple of values or a single list/tuple containing the values"),
                            Str.From("returns: the result of the function call after the delay")
                        ],
                        [
                            "description", "time", "fn", "args", "return"
                        ]
                    )
                },
                Func = args =>
                {
                    if (!args["fn"].As<Fn>(out var fn))
                        return new Err("expected 'fn' argument to be of type 'func'");
                    var vargs = args["args"].ToTuple().As<Tup>();

                    var milliseconds = args["time"] switch
                    {
                        Int i => (int)i.Value,
                        Float f => (int)(f.Value * 1000),
                        Obj o => (int)o.ToInt().As<Int>().Value,
                        _ => -1,
                    };

                    if (milliseconds == -1)
                        return new Err("expected 'time' argument to be of type 'int' or 'float' or convertible to int");

                    Thread.Sleep(milliseconds);

                    return fn.Call(vargs);
                }
            }
        },
        { "panic", new NFn()
            {
                Name = "panic",
                ReturnType = UnType.None,
                Args = [
                    new Arg("message") {
                        Type = UnType.Str,
                        IsEssential = true
                    },
                    new Arg("name") {
                        Type = UnType.Str,
                        IsOptional = true,
                        DefaultValue = Str.From("panic")
                    },
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Raises a panic with a specified message and name, which can be caught and handled by the program."),
                            Str.From("message: the message to include in the panic, as a string"),
                            Str.From("name: the name of the panic, as a string, default is 'panic'"),
                            Str.From("returns: none")
                        ],
                        [
                            "description", "message", "name", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    throw new Panic(args["message"].ToStr().As<Str>().Value, args["name"].ToStr().As<Str>().Value);
                }
            }
        },
        { "meta", new NFn()
            {
                Name = "meta",
                ReturnType = UnType.None,
                Args = [
                    new Arg("value") {
                        Type = UnType.Any,
                        IsEssential = true
                    },
                    new Arg("name") {
                        Type = UnType.Str,
                        IsEssential = true
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns the value of a specified annotation on a given value, or none if the annotation does not exist."),
                            Str.From("value: the value to get the annotation from, can be of any type"),
                            Str.From("name: the name of the annotation to get, as a string"),
                            Str.From("returns: the value of the specified annotation on the given value, or none if the annotation does not exist")
                        ],
                        [
                            "description", "value", "name", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    var name = args["name"].ToStr().As<Str>().Value;

                    if (args["value"].Annotations.Contains(name))
                        return args["value"].Annotations[name] as Obj ?? new Err("invalid annotation");
                    return Obj.None;
                }
            }
        },
        { "random", new NFn()
            {
                Name = "random",
                ReturnType = UnType.Float,
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns a random floating-point number in the range [0.0, 1.0)."),
                            Str.From("returns: a random floating-point number in the range [0.0, 1.0)")
                        ],
                        [
                            "description", "return"
                        ]
                    )
                },
                Func = (args) => new Float(random.NextDouble())
            }
        },
        { "breakpoint", new NFn()
            {
                Name = "breakpoint",
                ReturnType = UnType.None,
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Pauses the execution of the program and waits for user input to continue, allowing for debugging and inspection of the program state at the point where the breakpoint is hit."),
                            Str.From("returns: none")
                        ],
                        [
                            "description", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    Console.WriteLine("breakpoint hit. Press Enter to continue...");
                    Console.Read();
                    return Obj.None;
                }
            }
        },
        { "gc", new NFn()
            {
                Name = "gc",
                ReturnType = UnType.None,
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Forces the garbage collector to run and free up memory that is no longer in use by the program."),
                            Str.From("returns: none")
                        ],
                        [
                            "description", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    GC.Collect();
                    return Obj.None;
                }
            }
        },
        { "bench", new NFn()
            {
                Name = "bench",
                ReturnType = UnType.Int,
                Args = [
                    new Arg("fn") {
                        Type = UnType.Func,
                        IsEssential = true
                    },
                    new Arg("args") {
                        Type = CollectionType.Create(UnType.Tuple, UnType.Any),
                        IsPositional = true,
                    }
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Benchmarks the execution time of a function with the given arguments and returns the result along with the time taken to execute the function."),
                            Str.From("fn: the function to benchmark"),
                            Str.From("args: a variable argument of values to pass to the function when calling it, can be a tuple of values or a single list/tuple containing the values"),
                            Str.From("returns: a tuple containing the result of the function call and the time taken to execute the function in milliseconds")
                        ],
                        [
                            "description", "fn", "args", "return"
                        ]
                    )
                },
                Func = (args) =>
                {
                    if (!args["fn"].As<Fn>(out var fn))
                        return new Err("expected 'fn' argument to be of type 'func'");
                    var vargs = args["args"].ToTuple().As<Tup>();

                    var sw = System.Diagnostics.Stopwatch.StartNew();
                    var returned = fn.Call(vargs);
                    sw.Stop();

                    var w = new StreamWriter(so.Value)
                    {
                        AutoFlush = false
                    };

                    return new Tup([returned, Int.From(sw.ElapsedMilliseconds)], ["value", "time"]);
                }
            }
        },
        { "attr", new NFn()
            {
                Name = "attr",
                ReturnType = CollectionType.Create(UnType.List, UnType.Str),
                Args = [
                    new Arg("value") {
                        Type = UnType.Any,
                        IsEssential = true
                    },
                ],
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns a list of attribute names for a given value, which can be used to inspect the properties and methods of the value."),
                            Str.From("value: the value to get the attribute names from, can be of any type"),
                            Str.From("returns: a list of attribute names for the given value")
                        ],
                        [
                            "description", "value", "return"
                        ]
                    )
                },
                Func = (args) => new List([..args["value"].Members.Keys.Select(Str.From)])
            }
        },
        { "global", new NFn()
            {
                Name = "global",
                ReturnType = CollectionType.Create(UnType.Dict, UnionType.Create(UnType.Str, UnType.Any)),
                Annotations = {
                    ["doc"] = new Tup(
                        [
                            Str.From("Returns a dictionary containing all global variables and their values in the current execution context, which can be used to inspect and manipulate the global state of the program."),
                            Str.From("returns: a dictionary containing all global variables and their values in the current execution context")
                        ],
                        [
                            "description", "return"
                        ])
                },
                Func = (args) =>
                {
                    var dict = new Dict();
                    foreach (var kv in Global.GetGlobalScope().GetSymbolTable().Keys)
                        dict.Value[Str.From(kv)] = Global.GetGlobalVariable(kv);
                    return dict;
                }
            } 
        }
    };
}