using System.Collections.Concurrent;

namespace Un.Object.Type;

public class CollectionType : BaseType
{
    private static readonly ConcurrentDictionary<(UnType Kind, BaseType ItemType), CollectionType> Cache = [];

    public UnType Kind { get; }

    public BaseType ItemType { get; }

    private CollectionType(UnType kind, BaseType itemType)
    {
        Kind = kind;
        ItemType = itemType;
    }

    public static CollectionType Create(UnType kind, BaseType itemType)
    {
        return Cache.GetOrAdd((kind, itemType), key => new CollectionType(key.Kind, key.ItemType));
    }

    public override bool Equals(object? obj) => ReferenceEquals(this, obj);

    public override int GetHashCode() => HashCode.Combine(Kind, ItemType);

    public override string ToString() => $"{Kind}[{ItemType}]";

    public static bool operator ==(CollectionType? left, CollectionType? right) => ReferenceEquals(left, right);
    
    public static bool operator !=(CollectionType? left, CollectionType? right) => !ReferenceEquals(left, right);

}