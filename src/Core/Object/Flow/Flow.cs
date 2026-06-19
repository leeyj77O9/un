using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Type;

namespace Un.Object.Flow;

public class Flow : Obj, IPack
{
    public override UnType Type => UnType.Create("flow");

    public Attributes GetOriginalMembers() => [];

    public Attributes GetOriginalMethods() => new()
    {
        { "spawn", new NFn()
            {
                Name = "spawn",
                Args = [ new Arg("worker") { Type = UnType.Int, IsOptional = true, DefaultValue = Int.From(4) } ],
                ReturnType = UnType.Create("pool"),
                Func = args => new Pool(args["worker"].As<Int>($"expected 'worker' argument to be of type 'int'").Value)
            }
        },
        { "lock", new NFn()
            {
                Name = "lock",
                ReturnType = UnType.Create("lock"),
                Func = args => new Lock()
            }
        }
    };
}