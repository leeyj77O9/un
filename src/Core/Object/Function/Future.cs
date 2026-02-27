using Un.Object.Collections;
using Un.Object.Primitive;

namespace Un.Object.Function;

public class Future(Task<Obj> state) : Obj("future")
{
    public Future() : this(new Task<Obj>(() => None)) { }

    private Task<Obj> state { get; set; } = state;

    public void Run()
    {
        state.Start();
    }

    public Obj Wait() => state.Result;
}