using Un.Object.Collections;
using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Type;

namespace Un.Object;

public sealed class Property(string name) : Obj(UnType.Obj)
{
    public string Name { get; } = name;

    public string BackingKey { get; } = "$" + name;

    public Obj Default { get; set; } = None;

    public Fn? Getter { get; set; }

    public Fn? Setter { get; set; }

    public Obj Get(Obj instance)
    {
        if (Getter is null)
            return Read(instance);

        Getter.Self = instance;
        Getter.Super = instance.Super;
        Getter.Property = this;

        return Getter.Call(Tup.Empty);
    }

    public Obj Set(Obj instance, Obj value)
    {
        if (Setter is null)
        {
            Write(instance, value);
            return value;
        }

        Setter.Self = instance;
        Setter.Super = instance.Super;
        Setter.Property = this;

        return Setter.Call(new Tup([value]));
    }

    public Obj Read(Obj instance)
    {
        if (instance.Members.TryGetValue(BackingKey, out var value))
            return value;

        var initial = Default.Clone();
        instance.Members[BackingKey] = initial;
        return initial;
    }

    public void Write(Obj instance, Obj value) => instance.Members[BackingKey] = value;

    public override Str Repr() => Str.From($"<property {Name}>");

    public override Property Clone() => this;

    public override Obj Copy() => this;
}

public sealed class FieldRef(Obj owner, Property property) : Obj(UnType.Any)
{
    public Obj Value
    {
        get => property.Read(owner);
        set => property.Write(owner, value);
    }

    public override Obj Copy() => this;

    public override FieldRef Clone() => this;
}
