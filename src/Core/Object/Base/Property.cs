using Un.Object.Collections;
using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Type;

namespace Un.Object;

/// <summary>
/// Descriptor stored in a class under the property name. A property keeps its
/// own backing value (keyed by <see cref="BackingKey"/>) so accessors can read
/// and write storage without going through attribute lookup again.
/// </summary>
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

    /// <summary>
    /// Backing storage of <paramref name="instance"/>. The class default is
    /// materialized per instance on first access, so mutable defaults like a
    /// list are not shared between instances.
    /// </summary>
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

/// <summary>
/// Local binding for <c>field</c> inside a property accessor. Reading and
/// writing through this object targets the property backing storage directly.
/// </summary>
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
