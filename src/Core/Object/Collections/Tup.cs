using System.Collections;
using Un.Object.Iter;
using Un.Object.Primitive;
using Un.Object.Type;
using Un.Reflection;

namespace Un.Object.Collections;

[BuiltinType(
    "tuple", 
    Description = "Immutable ordered sequence with destructuring support.", 
    Example = "t = (1, 2, 3)\nx, y, z = t\nio.write(x)"
)]
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

    public static Tup Empty => new([], []);
    
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

    public Tup(IEnumerable<KeyValuePair<string, Obj>> pairs) : this([.. pairs]) { }

    private Tup(List<KeyValuePair<string, Obj>> pairs) : this([.. pairs.Select(x => x.Value)], [.. pairs.Select(x => x.Key)]) { }

    public Obj this[int index] => Value[index];

    public override Tup Init(Tup args) => args switch
    {
        { Count: 0 } => this,
        { Count: 1 } => args[0].ToTuple().As<Tup>(out var t) ? t : args,
        _ => args
    };

    public override Obj Eq(Obj other)
    {
        if (other is not Tup tup)
            return Bool.False;

        if (Count != tup.Count)
            return Bool.False;

        for (int i = 0; i < Count; i++)
        {
            var neq = Value[i].NEq(tup[i]);
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

    public override Obj GetItem(Obj key) => key switch
    {
        Int i => OutOfRange((int)i.Value) ? OutOfRange((int)(i.Value + Count)) ? new Err("tuple index out of range") : this[(int)(i.Value + Count)] : this[(int)i.Value],
        Str s => Members.TryGetValue(s.Value, out Obj? value) ? value : new Err($"tuple has no attribute '{s.Value}'"),
        _ => new Err("invalid index type")
    };

    public override Obj Slice(Obj? start, Obj? end, Obj? step)
    {
        int len = Count;
        int s = 0, e = len, st = 1;
        if (start != null)
        {
            if (!start.As<Int>(out var sInt)) return new Err("slice start must be int");
            s = (int)sInt.Value; if (s < 0) s += len; s = Math.Clamp(s, 0, len);
        }
        if (end != null)
        {
            if (!end.As<Int>(out var eInt)) return new Err("slice end must be int");
            e = (int)eInt.Value; if (e < 0) e += len; e = Math.Clamp(e, 0, len);
        }
        if (step != null)
        {
            if (!step.As<Int>(out var stInt)) return new Err("slice step must be int");
            st = (int)stInt.Value; if (st == 0) return new Err("slice step cannot be zero");
        }
        var newValues = new List<Obj>();
        var newNames = new List<string>();
        if (st > 0) { for (int i = s; i < e; i += st) { newValues.Add(Value[i]); newNames.Add(Name[i]); } }
        else { for (int i = s; i > e; i += st) { newValues.Add(Value[i]); newNames.Add(Name[i]); } }
        return new Tup([.. newValues], [.. newNames]);
    }

    public override Obj In(Obj obj)
    {
        foreach (var value in Value)
        {
            var eq = value.Eq(obj);
            if (eq.As<Bool>(out var isEqual))
            {
                if (isEqual.Value)
                    return Bool.True;
                continue;
            }
            if (eq is Err)
                return eq;
            return new Err($"equals operand is must be a boolean");
        }
        return Bool.False;
    }

    public override Int Len() => Int.From(Count);

    public override Bool ToBool() => Bool.From(Count != 0);

    public override Str ToStr() => Str.From($"({string.Join(", ", Value.Select(x => Str.To(x).Value))})");

    public override List ToList() => new([..Value]);

    public override Tup ToTuple() => new([..Value], [..Name]);

    public override Iters Iter() => new(this);

    public override Spreads Spread() => new(Value);

    public override Tup Copy()
    {
        var obj = new Obj[Count];
        var names = new string[Count];

        for (int i = 0; i < Count; i++)
        {
            obj[i] = this[i].Copy();
            names[i] = Name[i];
        }

        return new(obj, names);
    }

    public override Tup Clone()
    {
        var obj = new Obj[Count];
        var names = new string[Count];

        for (int i = 0; i < Count; i++)
        {
            obj[i] = this[i].Clone();
            names[i] = Name[i];
        }

        return new(obj, names);
    }

    public override bool Equals(object? obj)
    {
        if (obj is not Tup other)
            return false;

        if (Count != other.Count)
            return false;

        for (int i = 0; i < Count; i++)
        {
            if (!Value[i].Eq(other.Value[i]).As<Bool>(out var isEqual) || !isEqual.Value)
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

    public static Tup One(string name, Obj value) => new([value], [name]);

    public static Tup Two(string n1, Obj v1, string n2, Obj v2) => new([v1, v2], [n1, n2]);

}