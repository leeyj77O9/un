using Un.Object.Primitive;
using Un.Object.Type;

namespace Un.Object;

public class TObj(BaseType type) : Ref<BaseType>(type, UnType.Type)
{
    public override Obj BOr(Obj other) => other switch
    {
        TObj t => new TObj(UnionType.Create(Value, t.Value)),
        _ => base.BOr(other)
    };

    public override Bool Eq(Obj other) => other switch
    {
        TObj o => Bool.From(Value == o.Value),
        _ => Bool.False
    };

    public override Bool Is(Obj obj) => obj switch
    {
        TObj o => Bool.From(Value == o.Value),
        _ => Bool.From(ReferenceEquals(Value, obj.Type))
    };

    public override Bool In(Obj obj)
    {
        var type = obj is TObj t ? t.Value : obj.Type;

        if (ReferenceEquals(Value, type)) return Bool.True;
        if (Value is UnionType union && union.Contains(type)) return Bool.True;
        return Bool.False;
    }

    public override Str ToStr() => Str.From($"<type: {Value}>");

    public override int GetHashCode() => Value.GetHashCode();
}

