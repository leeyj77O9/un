using Un.Object.Function;
using Un.Object.Primitive;

namespace Un.Object.Util;

public class Time : Obj, IPack
{
    public override string Type => "time";

    public Attributes GetOriginalMembers() => [];

    public Attributes GetOriginalMethods() => new()
    {
        { "now", new NFn()
            {
                Name = "now",
                ReturnType = "date",
                Func = args => new Date(DateTime.Now)
            }
        }
    };
}