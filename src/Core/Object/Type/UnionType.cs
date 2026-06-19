namespace Un.Object.Type;

public class UnionType : BaseType
{
    private static readonly Dictionary<string, UnionType> Cache = [];

    public HashSet<BaseType> Types { get; }

    private UnionType(HashSet<BaseType> types)
    {
        Types = types;
    }

    public static BaseType Create(params BaseType[] types)
    {
        HashSet<BaseType> normalized = [];

        foreach (var type in types)
        {
            if (ReferenceEquals(type, UnType.Any))
                return UnType.Any;

            normalized.Add(type);
        }

        if (normalized.Count == 1)
            return normalized.First();

        string key = string.Join("|", normalized.Select(t => t.ToString()).OrderBy(x => x));

        if (Cache.TryGetValue(key, out var union))
            return union;

        union = new UnionType(normalized);
        Cache[key] = union;

        return union;
    }


    public bool In(UnType type) => Types.Contains(type);

    public bool In(string name) => In(UnType.From(name));

    public static BaseType operator |(UnionType a, UnType b) => Create([.. a.Types, b]);

    public static BaseType operator |(UnType a, UnionType b) => Create([a, .. b.Types]);

    public static BaseType operator |(UnionType a, UnionType b) => Create([.. a.Types, .. b.Types]);

    public static bool operator ==(UnionType? a, UnionType? b)
    {
        if (ReferenceEquals(a, b))
            return true;

        if (a is null || b is null)
            return false;

        return a.Types.SetEquals(b.Types);
    }

    public static bool operator !=(UnionType? a, UnionType? b) => !(a == b);

    public override bool Equals(object? obj) => obj is UnionType other && Types.SetEquals(other.Types);

    public override int GetHashCode()
    {
        int hash = 0;

        foreach (var type in Types)
            hash ^= type.GetHashCode();

        return hash;
    }

    public override string ToString() => string.Join(" | ", Types.Select(t => t.ToString()).OrderBy(x => x));
}