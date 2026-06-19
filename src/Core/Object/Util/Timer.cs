using System.Diagnostics;
using Un.Object.Collections;
using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Type;

namespace Un.Object.Util;

public class Timer() : Ref<Stopwatch>(new(), UnType.Create("timer"))
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
                ReturnType = UnType.None,
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
                ReturnType = UnType.None,
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
                ReturnType = UnType.None,
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
                ReturnType = UnType.Int,
                Func = _ => Int.From(Value.ElapsedMilliseconds)
            }
        },
    };
}