namespace Un;

public class Panic(string message, string name = "panic") : Exception(message)
{
    public string Name { get; } = name;
    public override string ToString() =>
$"""

{Name} : {Message}
""";
}
