using Un.Object.Type;

namespace Un.Object.Function;

public class Future(Task<Obj> state) : Obj(UnType.Future)
{
    public Future() : this(new Task<Obj>(() => None)) { }

    private Task<Obj> State { get; set; } = state;

    public void Run()
    {
        State.Start();
    }

    public Obj Wait() => State.Result;
}