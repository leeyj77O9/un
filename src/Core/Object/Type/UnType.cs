namespace Un.Object.Type;

public sealed class UnType : BaseType
{
    private static readonly Dictionary<string, UnType> Cache = [];

    public static readonly UnType Obj = Register("obj");
    public static readonly UnType Error = Register("error");
    public static readonly UnType Null = Register("null");
    public static readonly UnType None = Register("none");
    public static readonly UnType Any = Register("any");

    public static readonly UnType Int = Register("int");
    public static readonly UnType Float = Register("float");
    public static readonly UnType Bool = Register("bool");
    public static readonly UnType Str = Register("str");

    public static readonly UnType Date = Register("date");

    public static readonly UnType List = Register("list");
    public static readonly UnType Tuple = Register("tuple");
    public static readonly UnType Set = Register("set");
    public static readonly UnType Dict = Register("dict");

    public static readonly UnType Func = Register("func");
    public static readonly UnType Iter = Register("iter");
    public static readonly UnType Spread = Register("spread");
    public static readonly UnType Future = Register("future");
    public static readonly UnType Type = Register("type");

    public static readonly UnType TGeneric = Register("T");
    public static readonly UnType UGeneric = Register("U");
    public static readonly UnType Infinity = Register("...");

    public static readonly UnType Skip = Register("skip");
    public static readonly UnType Break = Register("break");

    public string Name { get; }

    private UnType(string name)
    {
        Name = name;
    }

    private static UnType Register(string name)
    {
        var type = new UnType(name);
        Cache[name] = type;
        return type;
    }

    public static UnType Create(string name)
    {
        if (Cache.TryGetValue(name, out var type))
            return type;

        type = new UnType(name);
        Cache[name] = type;

        return type;
    }

    public static UnType From(string name) => Cache.TryGetValue(name, out var type) ? type : throw new Panic($"type not found: {name}");

    public static BaseType operator |(UnType a, UnType b)
    {
        if (ReferenceEquals(a, Any) || ReferenceEquals(b, Any))
            return Any;

        if (ReferenceEquals(a, b))
            return a;

        return UnionType.Create(a, b);
    }

    public static bool operator ==(UnType? a, UnType? b) => ReferenceEquals(a, b);

    public static bool operator !=(UnType? a, UnType? b) => !ReferenceEquals(a, b);

    public override bool Equals(object? obj) => ReferenceEquals(this, obj);

    public override int GetHashCode() => Name.GetHashCode();

    public override string ToString() => Name;
}