using Un.Object.Collections;

namespace Un.Object.Primitive;

public class Bool(bool value) : Val<bool>(value, "bool")
{
    public static Bool True = new(true);
    public static Bool False = new(false);

    public Bool() : this(false) { }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => new Bool(),
        { Count: 1 } => args[0].ToBool(),
        _ => False
    };

    public override Obj Not() => Value ? False : True;

    public override Obj Xor(Obj other) => Value ? other.Not() : this;

    public override Bool Eq(Obj other) => other is Bool b && Value == b.Value ? True : False;

    public override Str ToStr() => new(Value ? "true" : "false");

    public override Obj Copy() => new Bool(Value)
    {
        Annotations = Annotations
    };

    public override Obj Clone() => new Bool(Value)
    {
        Annotations = Annotations
    };

    public override Bool ToBool() => new(Value);
}