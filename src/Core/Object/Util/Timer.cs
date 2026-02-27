using System.Diagnostics;
using Un.Object.Collections;
using Un.Object.Function;
using Un.Object.Primitive;

namespace Un.Object.Util;

public class Timer() : Ref<Stopwatch>(new(), "timer")
{
    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => new Timer(),
        _ => base.Init(args),
    };

    public override Timer Clone() => new();

    public override Attributes GetOriginal() => new()
    {
        { "start", new NFn()
            {
                Name = "start",
                ReturnType = "none",
                Func = _ =>
                {
                    Value.Start();
                    return None;
                }
            }
        },
        { "restart", new NFn()
            {
                Name = "restart",
                ReturnType = "none",
                Func = _ =>
                {
                    Value.Restart();
                    return None;
                }
            }
        },
        { "stop", new NFn()
            {
                Name = "stop",
                ReturnType = "none",
                Func = _ =>
                {
                    Value.Stop();
                    return None;
                }
            }
        },
        { "ms", new NFn()
            {
                Name = "ms",
                ReturnType = "int",
                Func = _ => new Int(Value.ElapsedMilliseconds)
            }
        },
    };
}