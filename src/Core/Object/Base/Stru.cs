using Un.Object.Collections;
using Un.Object.Primitive;

namespace Un.Object;

public class Stru(string type, string[] names) : Obj(type)
{
    private string[] names = names;

    public override Obj Init(Tup args)
    {
        if (Super is Stru st)
            names = [..st.names, ..names];
    
        if (args.Count != names.Length) return new Err("invalid initialize");

        for (int i = 0; i < names.Length; i++)
            Members[names[i]] = args[i];
        
        return this;
    }

    public override Str ToStr() => new($"{Type}({string.Join(", ", names.Select(name => $"{name}: {Members[name].ToStr().As<Str>().Value}"))})");

    public override Obj Add(Obj other) => other switch
    {
        Str s => ToStr().Add(s),
        _ => new Err($"unsupported operand type(s) for +: '{Type}' and '{other.Type}'")
    };

    public override Obj Copy() => new Stru(Type, names)
    {
        Annotations = Annotations,
        Members = Members.New(),
    };

    public override Stru Clone() => new(Type, names)
    {
        Annotations = Annotations,
        Super = Super,
        Members = Members.New(),
    };

    public override Obj Len() => new Int(names.Length);

    public override Obj Spread()
    {
        List list = [];

        foreach (var name in names)
            list.Append(new Tup([Members[name]], [name]));

        return list.Spread();
    }

    public override Bool Eq(Obj other)
    {
        if (Type != other.Type) return Bool.False;

        foreach (var name in names)
            if (Members[name].NEq(other.Members[name]).As<Bool>().Value)
                return Bool.False;

        return Bool.True;
    }
}
