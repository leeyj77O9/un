namespace Un.Object.Type;

public class CollectionType : BaseType
{
    private static readonly Dictionary<(UnType Kind, BaseType ItemType), CollectionType> Cache = [];

    public UnType Kind { get; }

    public BaseType ItemType { get; }

    private CollectionType(UnType kind, BaseType itemType)
    {
        Kind = kind;
        ItemType = itemType;
    }

    public static CollectionType Create(UnType kind, BaseType itemType)
    {
        var key = (kind, itemType);

        if (Cache.TryGetValue(key, out var type))
            return type;

        type = new CollectionType(kind, itemType);
        Cache[key] = type;

        return type;
    }

    public override bool Equals(object? obj) => ReferenceEquals(this, obj);

    public override int GetHashCode() => HashCode.Combine(Kind, ItemType);

    public override string ToString() => $"{Kind}[{ItemType}]";

    public static bool operator ==(CollectionType? left, CollectionType? right) => ReferenceEquals(left, right);
    
    public static bool operator !=(CollectionType? left, CollectionType? right) => !ReferenceEquals(left, right);

}