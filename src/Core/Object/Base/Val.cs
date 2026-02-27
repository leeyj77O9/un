namespace Un.Object;

public class Val<T>(T value, string type) : Obj
    where T : struct
{
    public T Value { get; set; } = value;
    public override string Type { get; protected set; } = type;

    public override int GetHashCode() => Value.GetHashCode();

    public override string ToString() => Value.ToString() ?? string.Empty;
}