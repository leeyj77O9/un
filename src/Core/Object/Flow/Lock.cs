using Un.Object;
using Un.Object.Primitive;
using Un.Object.Collections;
using System.Collections.Concurrent;
using Un.Object.Function;

namespace Un.Object.Flow;

public class Lock : Obj
{
    private readonly object syncRoot = new();
    private readonly ThreadLocal<bool> isHeld = new(() => false);

    public Lock() : base("lock")
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
                ReturnType = "lock",
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
                ReturnType = "bool",
                Func = args =>
                {
                    bool success = Monitor.TryEnter(syncRoot);
                    isHeld.Value = success;
                    return new Bool(success);
                }
            }
        },
        { "release", new NFn()
            {
                Name = "release",
                ReturnType = "lock",
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
                ReturnType = "lock",
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
