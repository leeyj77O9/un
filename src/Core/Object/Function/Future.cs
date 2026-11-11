using Un.Object.Type;

namespace Un.Object.Function;

public class Future(Task<Obj> state) : Obj(UnType.Future)
{
    private Task<Obj> State { get; set; } = state;

    public void Run()
    {
        State.Start();
    }

    public Obj Wait()
    {
        try
        {
            return State.Result;
        }
        catch (AggregateException ex)
        {
            throw ex.Flatten().InnerExceptions[0];
        }
    }

    public override Future Clone() => new(State);
}