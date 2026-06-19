using Un.Object;
using Un.Object.Primitive;
using Un.Object.Collections;
using System.Collections.Concurrent;
using Un.Object.Function;
using Un.Object.Type;

namespace Un.Object.Flow;

public class Lock : Obj
{
    private readonly object syncRoot = new();
    private readonly ThreadLocal<bool> isHeld = new(() => false);

    public Lock() : base(UnType.Create("lock"))
    {
        Members = GetOriginal();
    }

    public override Obj Entry() => this;

    public override Obj Exit()
    {
        if (isHeld.Value)
        {
            Monitor.Exit(syncRoot);
            isHeld.Value = false;
        }
        return None;
    }

    public override Attributes GetOriginal() => new()
    {
        { "acquire", new NFn()
            {
                Name = "acquire",
                ReturnType = UnType.Create("lock"),
                Func = args =>
                {
                    Monitor.Enter(syncRoot);
                    isHeld.Value = true;
                    return this;
                }
            }
        },
        { "try_acquire", new NFn()
            {
                Name = "try_acquire",
                ReturnType = UnType.Bool,
                Func = args =>
                {
                    bool success = Monitor.TryEnter(syncRoot);
                    isHeld.Value = success;
                    return Bool.From(success);
                }
            }
        },
        { "release", new NFn()
            {
                Name = "release",
                ReturnType = UnType.Create("lock"),
                Func = args =>
                {
                    if (isHeld.Value)
                    {
                        Monitor.Exit(syncRoot);
                        isHeld.Value = false;
                    }
                    else
                    {
                        return new Err("lock not held");
                    }
                    return this;
                }
            }
        },
        { "dispose", new NFn()
            {
                Name = "dispose",
                ReturnType = UnType.Create("lock"),
                Func = args =>
                {
                    if (isHeld.Value)
                    {
                        Monitor.Exit(syncRoot);
                        isHeld.Value = false;
                    }
                    return this;
                }
            }
        }
    };
}
