using Un.Object.Collections;

namespace Un.Object.Primitive;

public class Bool : Val<bool>
{
    public static Bool True = new(true);
    public static Bool False = new(false);

    public Bool() : this(false) { }
    private Bool(bool value) : base(value, "bool") { }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => False,
        { Count: 1 } => args[0].ToBool(),
        _ => False
    };

    public override Obj Not() => Value ? False : True;

    public override Obj Xor(Obj other) => Value ? other.Not() : this;

    public override Bool Eq(Obj other) => other is Bool b && Value == b.Value ? True : False;

    public override Str ToStr() => Str.From(Value ? "true" : "false");

    public override Obj Copy() => new Bool(Value)
    {
        Annotations = Annotations
    };

    public override Obj Clone() => new Bool(Value)
    {
        Annotations = Annotations
    };

    public override Bool ToBool() => Value ? True : False;

    public static Bool From(bool value) => value ? True : False;
}