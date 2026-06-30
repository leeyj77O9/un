using Un.Object;

namespace Un;

public class Context(Scope scope, Source source, List<Frame> frames)
{
    public Scope Scope { get; set; } = scope;
    public Source Source { get; set; } = source;
    public List<Frame> Frames { get; set; } = frames;
    public Stack<Node> Defers { get; set; } = [];
    public Stack<Obj> Usings { get; set; } = [];

    public int CallDepth { get; set; }

    public void PushFrame(Frame frame) => Frames.Add(frame);

    public void PopFrame()
    {
        if (Frames.Count == 0)
            throw new Panic("stack is empty");

        Frames.RemoveAt(Frames.Count - 1);
    }

    public Context Fork()
    {
        return new Context(Scope, Source, [.. Frames])
        {
            CallDepth = 0,
            Defers = [],
            Usings = []
        };
    }
}