using System.Collections;
using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Iter;

namespace Un.Object.Collections;

public class Tup : Ref<Obj[]>, IEnumerable<Obj>
{
    public struct Enumerator(Tup tup) : IEnumerator<Obj>
    {
        private readonly Obj[] arr = tup.Value;
        private int index = -1; 

        public readonly Obj Current => arr[index];

        readonly object IEnumerator.Current => arr[index];

        public bool MoveNext()
        {
            index++;
            return index < arr.Length;
        }

        public void Reset()
        {
            index = -1;
        }

        public void Dispose()
        {

        }
    }

    public string[] Name { get; private set; }
    public int Count => Value.Length;

    public Tup() : this([], []) {}

    public Tup(Obj[] values) : base(values, "tuple")
    {
        Name = new string[values.Length];
        for (int i = 0; i < values.Length; i++)
            Name[i] = string.Empty;
    }

    public Tup(Obj[] values, string[] names) : base(values, "tuple")
    {
        Name = names;

        for (int i = 0; i < Name.Length; i++)
            if (!string.IsNullOrEmpty(Name[i]))
                Members[Name[i]] = values[i];
    }

    public Tup(IEnumerable<KeyValuePair<string, Obj>> pairs) : base([..pairs.Select(x => x.Value)], "tuple")
    {
        Name = [.. pairs.Select(x => x.Key)];
    }

    public Obj this[int index]
    {
        get => OutOfRange(index) ? new Err("tuple index out of range") : Value[index];
    }

    public override Bool Eq(Obj other)
    {
        if (other is not Tup tup)
            return Bool.False;

        for (int i = 0; i < Count; i++)
            if (Value[i].NEq(tup[i]).As<Bool>().Value)
                return Bool.False;

        return Bool.True;
    }

    public override Obj GetItem(Obj key) => key switch
    {
        Int i => OutOfRange((int)i.Value) ? new Err("tuple index out of range") : this[(int)i.Value],
        _ => new Err("invalid index type")
    };

    public override Obj In(Obj obj)
    {
        foreach (var value in Value)
        {
            if (value.Eq(obj).As<Bool>().Value)
                return Bool.True;
        }
        return Bool.False;
    }

    public override Obj Len() => new Int(Count);

    public override Bool ToBool() => new(Count != 0);

    public override Str ToStr() => new($"({string.Join(", ", Value.Select(v => v.ToStr().As<Str>().Value))})");

    public override List ToList() => new([..Value]);

    public override Tup ToTuple() => new([..Value], [..Name]);

    public override Iters Iter() => new(this);

    public override Spreads Spread() => new(Value);

    public override Obj Copy()
    {
        var obj = new Obj[Count];
        var names = new string[Count];

        for (int i = 0; i < Count; i++)
        {
            obj[i] = this[i].Copy();
            names[i] = Name[i];
        }

        return new Tup(obj, names);
    }

    public override Obj Clone()
    {
        var obj = new Obj[Count];
        var names = new string[Count];
        
        for (int i = 0; i < Count; i++)
        {
            obj[i] = this[i].Clone();
            names[i] = Name[i];
        }

        return new Tup(obj, names);
    }

    public override int GetHashCode()
    {
        int hash = Global.BASEHASH;
        foreach (var value in Value)
        {
            hash <<= Global.HASHPRIME;
            hash *= value.GetHashCode();
            hash >>= Global.HASHPRIME;
        }
        return Math.Abs(hash);
    }

    private bool OutOfRange(int index) => index < 0 || index >= Count;

    public bool IsSingle() => Count == 1 && (Name.Length == 0 || string.IsNullOrEmpty(Name[0]));

    public IEnumerator<Obj> GetEnumerator() => new Enumerator(this);

    IEnumerator IEnumerable.GetEnumerator() => new Enumerator(this);
}