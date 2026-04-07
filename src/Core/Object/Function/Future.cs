using Un.Object.Collections;
using Un.Object.Primitive;

namespace Un.Object.Function;

public class Future(Task<Obj> state) : Obj("future")
{
    public Future() : this(new Task<Obj>(() => None)) { }

    private Task<Obj> State { get; set; } = state;

    public void Run()
    {
        State.Start();
    }

    public Obj Wait() => State.Result;
}