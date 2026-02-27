using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Collections;
using Un.Object.Iter;

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
                ReturnType = "none",
                Args = [
                   new Arg("value") {
                    Type = "tuple[any]",
                    IsPositional = true,
                }, new Arg("sep") {
                    Type = "str",
                    IsOptional = true,
                    DefaultValue = new Str(" ")
                }, new Arg("end"){
                    Type = "str",
                    IsOptional = true,
                    DefaultValue = new Str("\n")
                }, new Arg("stream"){
                    Type = "stream",
                    IsOptional = true,
                    DefaultValue = so
                }],
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
                ReturnType = "str",
                Args = [
                   new Arg("prompt") {
                    Type = "str",
                    IsOptional = true,
                    DefaultValue = new Str("")
                }, new Arg("stream") {
                    Type = "stream",
                    IsOptional = true,
                    DefaultValue = sr
                }],
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
                    return new Str(cr.ReadLine() ?? "");
                }
            }
        },
        { "log", new NFn()
            {
                Name = "log",
                ReturnType = "none",
                Args = [
                   new Arg("value") {
                    Type = "tuple[any]",
                    IsPositional = true,
                }, new Arg("sep") {
                    Type = "str",
                    IsOptional = true,
                    DefaultValue = new Str(" ")
                }, new Arg("end"){
                    Type = "str",
                    IsOptional = true,
                    DefaultValue = new Str("\n")
                }],
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
                Func = (args) => args["value"].Len()
            }
        },
        { "clear", new NFn()
            {
                Name = "clear",
                ReturnType = "none",
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
                ReturnType = "none",
                Args = [
                   new Arg("code") {
                    Type = "int",
                    IsOptional = true,
                    DefaultValue = new Int(0)
                }],
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
                ReturnType = "str",
                Args = [
                   new Arg("value") {
                    IsEssential = true,
                }],
                Func = (args) => new Str(args["value"].Type)
            }
        },
        { "array", new NFn()
            {
                Name = "array",
                ReturnType = "list[T]",
                Args = [
                   new Arg("default") {
                    Type = "T",
                    IsEssential = true,
                }, new Arg("size") {
                    Type = "tuple[int]",
                    IsPositional = true,
                    DefaultValue = new Tup([new Int(1)], [])
                }],
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
                ReturnType = "list[int]",
                Args = [
                    new Arg("start") {
                        Type = "int",
                        IsOptional = true,
                        DefaultValue = new Int(0),
                    },
                    new Arg("stop") {
                        Type = "int",
                        IsEssential = true
                    },
                    new Arg("step") {
                        Type = "int",
                        IsOptional = true,
                        DefaultValue = new Int(1),
                    }
                ],
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
                ReturnType = "list[(int, T)]",
                Args = [
                    new Arg("array") {
                        Type = "list[T] | tuple[T]",
                        IsEssential = true
                    },
                ],
                Func = (args) =>
                {
                    if (!args["array"].As<List, Tup>(out var array))
                        return new Err("expected 'array' argument to be of type 'list' or 'tuple'");
                    var iter = array.Iter().As<Iters>();
                    var len = iter.Len().As<Int>().Value;

                    List enumerate = [];

                    for (int i = 0; i < len; i++)
                        enumerate.Append(new Tup([new Int(i), iter.Next()], ["index", "value"]));

                    return enumerate;
                }
            }
        },
        { "zip", new NFn()
            {
                Name = "zip",
                ReturnType = "list[(T, U, ...)]",
                Args = [
                    new Arg("arrays") {
                        Type = "list[list[any] | tuple[any]]",
                        IsPositional = true
                    },
                ],
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
                            tuple.Append(array.Len().As<Int>().Value <= i ? Obj.None : array.GetItem(new Int(i)));
                        list.Append(new Tup([..tuple], [.. new string(' ', tuple.Count).Split()]));
                    }

                    return list;
                }
            }
        },
        { "hash", new NFn()
            {
                Name = "hash",
                ReturnType = "int",
                Args = [
                    new Arg("value") {
                        IsEssential = true,
                    }
                ],
                Func = (args) => new Int(args["value"].GetHashCode())
            }
        },
        { "open", new NFn()
            {
                Name = "open",
                ReturnType = "stream",
                Args = [
                    new Arg("value") {
                        IsEssential = true,
                    }
                ],
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
                ReturnType = "T",
                Args = [
                    new Arg("value") {
                        Type = "tuple[T]",
                        IsPositional = true,
                    }
                ],
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
                            return new Int(r.Value.Sum(x => x.As<Int>().Value));
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
                ReturnType = "T",
                Args = [
                    new Arg("value") {
                        Type = "tuple[T]",
                        IsPositional = true,
                    }
                ],
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
                ReturnType = "T",
                Args = [
                    new Arg("value") {
                        Type = "tuple[T]",
                        IsPositional = true,
                    }
                ],
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
                ReturnType = "int | floor",
                Args = [
                    new Arg("value") {
                        Type = "int | floor",
                        IsEssential = true
                    },
                    new Arg("digit") {
                        Type = "int",
                        IsOptional = true,
                        DefaultValue = new Int(0),
                    }
                ],
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

                    return d == 0 || double.IsInteger(v) ? new Int((long)v) : new Float(v);
                }
            }
        },
        { "abs", new NFn()
            {
                Name = "abs",
                ReturnType = "int | floor",
                Args = [
                    new Arg("value") {
                        Type = "int | floor",
                        IsEssential = true,
                    }
                ],
                Func = (args) => args["value"] switch
                {
                    Int i => new Int(Math.Abs(i.Value)),
                    Float f => new Float(Math.Abs(f.Value)),
                    _ => new Float(double.NaN),
                }
            }
        },
        { "ceil", new NFn()
            {
                Name = "ceil",
                ReturnType = "int | floor",
                Args = [
                    new Arg("value") {
                        Type = "int | floor",
                        IsEssential = true,
                    }
                ],
                Func = (args) => args["value"] switch
                {
                    Int i => new Int(i.Value),
                    Float f => Math.Ceiling(f.Value) switch
                    {
                        double d when d == (long)d => new Int((long)d),
                        double d => new Float(d),
                    },
                    _ => new Float(double.NaN),
                }
            }
        },
        { "sqrt", new NFn()
            {
                Name = "sqrt",
                ReturnType = "floor",
                Args = [
                    new Arg("value") {
                        Type = "int | floor",
                        IsEssential = true,
                    }
                ],
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
                ReturnType = "int | floor",
                Args = [
                    new Arg("value") {
                        Type = "int | floor",
                        IsEssential = true,
                    }
                ],
                Func = (args) => args["value"] switch
                {
                    Int i => new Int(i.Value),
                    Float f => Math.Floor(f.Value) switch
                    {
                        double d when d == (long)d => new Int((long)d),
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
                        Type = "func",
                        IsEssential = true
                    },
                    new Arg("args") {
                        Type = "tuple",
                        IsPositional = true,
                    }
                ],
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
                ReturnType = "none",
                Args = [
                    new Arg("time")
                    {
                        Type = "int | float",
                        IsEssential = true
                    }
                ],
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
                ReturnType = "T",
                Args = [
                    new Arg("time")
                    {
                        Type = "int | float",
                        IsEssential = true
                    },
                    new Arg("fn")
                    {
                        Type = "func",
                        IsEssential = true
                    },
                    new Arg("args")
                    {
                        Type = "tuple[any]",
                        IsPositional = true,
                    }
                ],
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
                ReturnType = "none",
                Args = [
                    new Arg("message") {
                        Type = "str",
                        IsEssential = true
                    },
                    new Arg("name") {
                        Type = "str",
                        IsOptional = true,
                        DefaultValue = new Str("panic")
                    },
                ],
                Func = (args) =>
                {
                    throw new Panic(args["message"].ToStr().As<Str>().Value, args["name"].ToStr().As<Str>().Value);
                }
            }
        },
        { "meta", new NFn()
            {
                Name = "meta",
                ReturnType = "none",
                Args = [
                    new Arg("value") {
                        Type = "any",
                        IsEssential = true
                    },
                    new Arg("name") {
                        Type = "str",
                        IsEssential = true
                    }
                ],
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
                ReturnType = "float",
                Args = [],
                Func = (args) => new Float(random.NextDouble())
            }
        },
        { "breakpoint", new NFn()
            {
                Name = "breakpoint",
                ReturnType = "none",
                Args = [],
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
                ReturnType = "none",
                Args = [],
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
                ReturnType = "float",
                Args = [
                    new Arg("fn") {
                        Type = "func",
                        IsEssential = true
                    },
                    new Arg("args") {
                        Type = "tuple[any]",
                        IsPositional = true,
                    }
                ],
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

                    return new Tup([returned, new Int(sw.ElapsedMilliseconds)], ["value", "time"]);
                }
            }

        }
    };
}