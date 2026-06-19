using Un.Object.Type;

namespace Un.Object;

public class Val<T>(T value, UnType type) : Obj(type)
    where T : struct
{
    public T Value { get; set; } = value;

    public override int GetHashCode() => Value.GetHashCode();

    public override string ToString() => Value.ToString() ?? string.Empty;
}