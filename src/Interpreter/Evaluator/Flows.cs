using Un.Object;

namespace Un;

public abstract class Flows : Exception { }

public class ReturnFlow(Obj value, int start, int length) : Flows 
{
    public int Start => start;
    public int Length => length;
    public Obj Value => value;
}

public class BreakFlow(int start, int length) : Flows 
{
    public int Start => start;
    public int Length => length;
}

public class SkipFlow(int start, int length) : Flows
{
    public int Start => start;
    public int Length => length;
}