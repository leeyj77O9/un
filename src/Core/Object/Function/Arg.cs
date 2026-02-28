namespace Un.Object.Function;

public struct Arg(string name)
{
    public readonly static Arg Null = new("null");

    public string Name { get; set; } = name;
    public string Type { get; set; } = "any";

    public bool IsEssential { get; set; }
    public bool IsOptional { get; set; }
    public bool IsPositional { get; set; }
    public bool IsKeyword { get; set; }

    public Obj? DefaultValue { get; set; }
    
    public Arg New() => new(Name)
    {
        Type = Type,
        IsEssential = IsEssential,
        IsOptional = IsOptional,
        IsPositional = IsPositional,
        IsKeyword = IsKeyword,
        DefaultValue = DefaultValue,
    };

    public readonly bool IsNull() => Name == "null";
}
