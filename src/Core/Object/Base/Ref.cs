namespace Un.Object;

public class Ref<T>(T value, string type) : Obj
    where T : class
{
    public T Value { get; set; } = value;
    public override string Type { get; protected set; } = type;

    public override int GetHashCode() => Value.GetHashCode();
}