using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Type;

namespace Un.Object.Util;

public class Time : Obj, IPack
{
    public override UnType Type => UnType.Create("time");

    public Attributes GetOriginalMembers() => [];

    public Attributes GetOriginalMethods() => new()
    {
        { "now", new NFn()
            {
                Name = "now",
                ReturnType = UnType.Date,
                Func = args => new Date(DateTime.Now)
            }
        }
    };
}