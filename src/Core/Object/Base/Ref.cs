using Un.Object.Type;

namespace Un.Object;

public class Ref<T>(T value, UnType type) : Obj(type)
    where T : class
{
    public T Value { get; set; } = value;

    public override int GetHashCode() => Value.GetHashCode();
}