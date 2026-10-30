using Un.Object.Primitive;
using Un.Object.Iter;
using Un.Object.Type;
using Un.Reflection;

namespace Un.Object.Collections;

[BuiltinType(
    "dict", 
    Description = "Hash map storing key-value pairs.", 
    Example = "d = {\"a\": 1, \"b\": 2}\nio.write(d[\"a\"])"
)]
public class Dict(Dictionary<Obj, Obj> value) : Ref<Dictionary<Obj, Obj>>(value, UnType.Dict)
{
    public Dict() : this([]) { }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => new Dict(),
        { Count: 1 } when args[0] is Tup t => new Dict(t.Name.Select(name => ((Obj)Str.From(name), t.Members[name])).ToDictionary()),
        { Count: 1 } when args[0] is Stru st => Init(new([st.ToTuple()])),
        _ => new Err($"invaild '{Type}' initialize"),
    };

    public override Int Len() => Int.From(Value.Count);

    public override Obj GetItem(Obj key) => Value.TryGetValue(key, out var value) ? value : new Err($"key '{Str.To(key).Value}' not found in dictionary");

    public override Obj SetItem(Obj key, Obj value) => Value[key] = value;

    public override Obj In(Obj obj) => obj switch
    {
        Dict dict => Bool.From(Overlap(dict)),
        _ => Bool.From(Value.ContainsKey(obj)),
    };

    public override Dict Copy() => this;

    public override Dict Clone() => new(new Dictionary<Obj, Obj>(Value));

    public override Str ToStr() => Str.From($"{{{string.Join(", ", Value.Select(x => $"{Str.To(x.Key).Value}:{Str.To(x.Value).Value}"))}}}");

    public override List ToList() => new([.. Value.Keys.Zip(Value.Values).Select(x => new Tup([x.First, x.Second], ["key", "value"]))]);

    public override Tup ToTuple() => new([.. Value.Keys.Zip(Value.Values).Select(x => new Tup([x.First, x.Second], ["key", "value"]))], []);

    public override Iters Iter() => new([.. Value.Keys.Zip(Value.Values).Select(x => new Tup([x.First, x.Second], ["key", "value"]))]);

    public override Spreads Spread() => new([.. Value.Select(i => new Tup([i.Key, i.Value], ["key", "value"]))]);

    private bool Overlap(Dict dict)
    {
        foreach (var (key, value) in dict.Value)
        {
            if (!Value.TryGetValue(key, out var v) || !v.Eq(value).As<Bool>(out var eqResult) || !eqResult.Value)
                return false;
        }
        return true;
    }

    [Native(
        Name = "add",
        Description = "Adds a value to data.",
        Example = "data.add(key, value)",
        ReturnType = "none",
        ArgumentTypes = new[] { "any", "any" }
    )]
    public static Obj Add(
        [Self] Dict self,
        [ArgInfo(Essential = true)] Obj key,
        [ArgInfo(Essential = true)] Obj value)
    {
        self.Value.Add(key, value);
        return None;
    }

    [Native(
        Name = "remove",
        Description = "Removes a value from a dict value.",
        Example = "data.remove(key)",
        ReturnType = "any",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Remove(
        [Self] Dict self,
        [ArgInfo(Essential = true)] Obj key)
        => Bool.From(self.Value.Remove(key));

    [Native(
        Name = "get",
        Description = "Gets a value from data.",
        Example = "data.get(key, defaultValue)",
        ReturnType = "any",
        ArgumentTypes = new[] { "any", "any" }
    )]
    public static Obj Get(
        [Self] Dict self,
        [ArgInfo(Essential = true)] Obj key,
        [ArgInfo(Optional = true, Name = "default_value")] Obj defaultValue = null!)
        => self.Value.TryGetValue(key, out var value) ? value : (defaultValue ?? None);

    [Native(
        Name = "contains_key",
        Description = "Returns the result of data.contains key().",
        Example = "data.contains_key(key)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj ContainsKey(
        [Self] Dict self,
        [ArgInfo(Essential = true)] Obj key)
        => Bool.From(self.Value.ContainsKey(key));

    [Native(
        Name = "contains_value",
        Description = "Returns the result of data.contains value().",
        Example = "data.contains_value(value)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj ContainsValue([Self] Dict self, [ArgInfo(Essential = true)] Obj value)
        => Bool.From(self.Value.ContainsValue(value));

    [Native(
        Name = "clear",
        Description = "Removes all values from data.",
        Example = "data.clear()",
        ReturnType = "none"
    )]
    public static Obj Clear([Self] Dict self)
    {
        self.Value.Clear();
        return None;
    }

    [Native(
        Name = "keys",
        Description = "Returns the result of data.keys().",
        Example = "data.keys()",
        ReturnType = "list"
    )]
    public static List Keys([Self] Dict self) => new([.. self.Value.Keys]);

    [Native(
        Name = "values",
        Description = "Returns the result of data.values().",
        Example = "data.values()",
        ReturnType = "list"
    )]
    public static List Values([Self] Dict self) => new([.. self.Value.Values]);
}
