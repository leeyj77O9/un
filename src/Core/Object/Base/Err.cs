using Un.Object;
using Un.Object.Primitive;

namespace Un;

public class Err(string message, string header = "Error") : Obj
{
    public string Message => message;
    public string Header => header;

    public override Obj ToStr() => Str.From($"{header}: {message}");
}