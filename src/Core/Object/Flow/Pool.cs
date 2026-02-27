using Un.Object;
using Un.Object.Primitive;
using Un.Object.Collections;
using System.Collections.Concurrent;
using Un.Object.Function;

namespace Un.Object.Flow;

public class Pool : Obj
{
    private readonly BlockingCollection<Future> queue = [];
    private readonly List<Thread> threads = [];

    public Pool(long workers) : base("pool")
    {
        for (int i = 0; i < workers; i++)
        {
            var thread = new Thread(() =>
            {
                foreach (var future in queue.GetConsumingEnumerable())
                    future.Run();
            });
            thread.Start();
            threads.Add(thread);
        }

        Members = GetOriginal();
    }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => new Pool(4),
        { Count: 1 } => args[0] is Int count ? new Pool(count.Value) : new Err("expected a worker count as an integer"),
        _ => new Err("pool takes at most one argument")
    };

    public override Obj Entry() => this;

    public override Obj Exit()
    {
        queue.CompleteAdding();
        foreach (var thread in threads)
            thread.Join();
        return None;
    }

    public override Obj Clone() => this;

    public override Attributes GetOriginal() => new()
    {
        { "submit", new NFn()
            {
                Name = "submit",
                ReturnType = "future",
                Args = [
                    new Arg("fn")
                    {
                        Type = "func",
                        IsEssential = true,
                    },
                    new Arg("args")
                    {
                        Type = "tuple[T]",
                        IsPositional = true,
                    }
                ],
                Func = (args) =>
                {
                    if (!args["fn"].As<Fn>(out var fn))
                        return new Err("expected 'fn' argument to be of type 'func'");

                    var future = new Future(new Task<Obj>(() => fn.Call(args["args"].As<Tup>())));
                    queue.Add(future);
                    return future;
                }
            }
        },
        { "map", new NFn()
            {
                Name = "map",
                Args = [
                    new Arg("fn") {
                        Type = "func",
                        IsEssential = true,
                    },
                    new Arg("vargs") {
                        Type = "tuple[T]",
                        IsPositional = true,
                    }
                ],
                Func = (args) =>
                {
                    if (!args["fn"].As<Fn>(out var fn))
                        return new Err("expected 'fn' argument to be of type 'func'");

                    if (!args["vargs"].As<Tup>(out var vargs))
                        return new Err("expected 'iterable' argument to be of type 'iterable'");

                    var len = vargs.Count;
                    var result = new List<Obj>();
                    var countdown = new CountdownEvent(len);

                    foreach (var varg in vargs.Value)
                    {
                        queue.Add(new Future(new Task<Obj>(() =>
                            {
                                var res = fn.Call(varg is Tup t ? t : new([varg], [""]));
                                lock (result)
                                {
                                    result.Add(res);
                                }
                                countdown.Signal();
                                return None;
                            })));
                    }

                    countdown.Wait();

                    return new List([.. result]);
                }
            }
        },
        { "close", new NFn()
            {
                Name = "close",
                Args = [],
                Func = (args) =>
                {
                    queue.CompleteAdding();
                    foreach (var thread in threads)
                        thread.Join();
                    return None;
                }
            }
        },

    };
}
