using Un.Object;

namespace Un;

public class Context(Scope scope, UnFile file, List<Block> blockStack)
{
    public Scope Scope { get; set; } = scope;
    public UnFile File { get; set; } = file;
    public List<Block> BlockStack = blockStack;
    public ulong Depth { get; set; } = 0;
    public OMap Annotations { get; set; } = [];
    public Stack<List<Node>> Defers { get; set; } = [];
    public Stack<Obj> Usings { get; set; } = [];

    public Block? CurrentBlock => BlockStack.LastOrDefault();

    public void EnterBlock(string type) => BlockStack.Add(new(type, File.PeekLine(), File.Line, Scope));

    public void ExitBlock()
    {
        if (BlockStack.Count > 0)
            BlockStack.RemoveAt(BlockStack.Count - 1);
    }

    public bool InBlock(string type) => BlockStack.Count > 0 && BlockStack.Any(x => x.Type == type);

    public Block[] BlockStackTrace => [.. BlockStack];
}
