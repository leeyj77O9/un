namespace Un.Object.Type;

public sealed class UnionType : BaseType
{
    private static readonly Dictionary<string, UnionType> Cache = [];

    public IReadOnlySet<BaseType> Types { get; }

    private UnionType(HashSet<BaseType> types)
    {
        Types = types;
    }

    public static BaseType Create(params BaseType[] types)
    {
        HashSet<BaseType> normalized = [];

        foreach (var type in types)
        {
            switch (type)
            {
                case null:
                    continue;

                case UnionType union:
                    normalized.UnionWith(union.Types);
                    break;

                default:
                    if (ReferenceEquals(type, UnType.Any))
                        return UnType.Any;

                    normalized.Add(type);
                    break;
            }
        }

        if (normalized.Count == 0)
            return UnType.None;

        if (normalized.Count == 1)
            return normalized.First();

        string key = GetKey(normalized);

        if (Cache.TryGetValue(key, out var cached))
            return cached;

        var created = new UnionType(normalized);

        Cache[key] = created;

        return created;
    }

    private static string GetKey(IEnumerable<BaseType> types) => string.Join("|", types.Select(t => t.ToString()).OrderBy(x => x));

    public bool Contains(BaseType type) => Types.Contains(type);

    public bool Contains(UnionType other) => Types.IsSubsetOf(other.Types);

    public bool Contains(string name) => Contains(UnType.From(name));

    public static BaseType operator |(UnionType a, UnType b) => Create([.. a.Types, b]);

    public static BaseType operator |(UnType a, UnionType b) => Create([a, .. b.Types]);

    public static BaseType operator |(UnionType a, UnionType b) => Create([.. a.Types, .. b.Types]);

    public override bool Equals(object? obj) => ReferenceEquals(this, obj);

    public override int GetHashCode() => GetKey(Types).GetHashCode();

    public override string ToString() => string.Join(" | ", Types.Select(t => t.ToString()).OrderBy(x => x));
}