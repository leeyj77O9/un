using Un.Object;
using Un.Object.Function;
using Un.Object.Primitive;

namespace Un.Object.Flow;

public class Flow : Obj, IPack
{
    public override string Type => "flow";

    public Attributes GetOriginalMembers() => [];

    public Attributes GetOriginalMethods() => new()
    {
        { "spawn", new NFn()
            {
                Name = "spawn",
                Args = [ new Arg("worker") { Type = "int", IsOptional = true, DefaultValue = new Int(4) } ],
                ReturnType = "pool",
                Func = args => new Pool(args["worker"].As<Int>($"expected 'worker' argument to be of type 'int'").Value)
            }
        },
        { "lock", new NFn()
            {
                Name = "lock",
                ReturnType = "lock",
                Func = args => new Lock()
            }
        }
    };
}