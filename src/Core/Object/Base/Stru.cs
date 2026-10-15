using Un.Object;
using Un.Object.Collections;
using Un.Object.Primitive;
using Un.Object.Type;

namespace Un;

public class Stru(UnType type, string[] names) : Obj(type)
{
    private readonly string[] names = names;

    public override Obj Init(Tup args)
    {
        var allNames = GetAllNames(this);

        if (args.Count != allNames.Length)
            return new Err("invalid initialize");

        int i = 0;

        foreach (var name in allNames)
            Members[name] = args[i++];

        return this;
    }

    public override Str ToStr() => Str.From($"{Type}(" +
        $"{string.Join(", ", names.Select(name => $"{name}: " +
        $"{Str.To(Members[name]).Value}"))})");

    public override Obj Add(Obj other) => other switch
    {
        Str s => ToStr().Add(s),
        _ => new Err($"unsupported operand type(s) for +: '{Type}' and '{other.Type}'")
    };

    public override Obj Copy() => new Stru(Type, [.. names])
    {
        Annotations = Annotations,
        Members = Members.New(),
        Super = Super
    };

    public override Stru Clone() => new(Type, [.. names])
    {
        Annotations = Annotations,
        Members = Members.New(),
        Super = Super
    };

    public override Obj Len() => Int.From(GetAllNames(this).Length);

    public override Obj Spread()
    {
        List list = [];

        foreach (var name in GetAllNames(this))
            List.Append(list, Members[name]);

        return list.Spread();
    }

    public override Obj Eq(Obj other)
    {
        if (Type != other.Type)
            return Bool.False;

        var allNames = GetAllNames(this);

        foreach (var name in allNames)
        {
            var neq = Members[name].NEq(other.Members[name]);
            if (neq.As<Bool>(out var isNotEqual))
            {
                if (isNotEqual.Value)
                    return Bool.False;
                continue;
            }
            if (neq is Err)
                return neq;
            return new Err($"equals operand is must be a boolean");
        }

        return Bool.True;
    }

    public override int GetHashCode()
    {
        HashCode hash = new();

        var allNames = GetAllNames(this);

        foreach (var name in allNames)
            hash.Add(Members[name]);

        return hash.ToHashCode();
    }

    private static string[] GetAllNames(Stru stru)
    {
        var result = new List<string>();

        var current = stru;

        while (current.Super is Stru parent)
        {
            result.AddRange(parent.names);
            current = parent;
        }

        result.Reverse();

        result.AddRange(stru.names);

        return [.. result];
    }
}