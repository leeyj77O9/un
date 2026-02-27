using Un.Object;
using Un.Object.Primitive;

namespace Un;

public class Err(string message) : Obj
{
    public string Message => message;

    public override Obj ToStr() => new Str($"Error: {message}");
}