using System.Collections;
using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Iter;
using Un.Object.Type;

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

    public Tup(Obj[] values) : base(values, UnType.Tuple)
    {
        Name = new string[values.Length];
        for (int i = 0; i < values.Length; i++)
            Name[i] = string.Empty;
    }

    public Tup(Obj[] values, string[] names) : base(values, UnType.Tuple)
    {
        Name = names;

        for (int i = 0; i < Name.Length; i++)
            if (!string.IsNullOrEmpty(Name[i]))
                Members[Name[i]] = values[i];
    }

    public Tup(IEnumerable<KeyValuePair<string, Obj>> pairs) : base([..pairs.Select(x => x.Value)], UnType.Tuple)
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
        Str s => Members.TryGetValue(s.Value, out Obj? value) ? value : new Err($"tuple has no attribute '{s.Value}'"),
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

    public override Obj Len() => Int.From(Count);

    public override Bool ToBool() => Bool.From(Count != 0);

    public override Str ToStr() => Str.From($"({string.Join(", ", Value.Select(v => v.ToStr().As<Str>().Value))})");

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

    public override bool Equals(object? obj)
    {
        if (obj is not Tup other)
            return false;

        if (Count != other.Count)
            return false;

        for (int i = 0; i < Count; i++)
        {
            if (!Value[i].Eq(other.Value[i]).As<Bool>().Value)
                return false;
        }

        return true;
    }

    public override int GetHashCode()
    {
        HashCode hash = new();

        foreach (var value in Value)
            hash.Add(value);

        return hash.ToHashCode();
    }

    private bool OutOfRange(int index) => index < 0 || index >= Count;

    public bool IsSingle() => Count == 1 && (Name.Length == 0 || string.IsNullOrEmpty(Name[0]));

    public IEnumerator<Obj> GetEnumerator() => new Enumerator(this);

    IEnumerator IEnumerable.GetEnumerator() => new Enumerator(this);
}