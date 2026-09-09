using System.Collections;
using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Iter;
using Un.Object.Type;
using Un.Reflection;

namespace Un.Object.Collections;

[BuiltinType("list", Description = "Dynamic resizable array with heap and search operations.", Example = "l = [1, 2, 3]\nl.push(4)\nio.write(l)")]
public class List(Obj[] value) : Ref<Obj[]>(value, UnType.List), IEnumerable<Obj>
{
    public struct Enumerator(List list) : IEnumerator<Obj>
    {
        private readonly List list = list;
        private int index = -1;

        public readonly Obj Current => list[index];

        readonly object IEnumerator.Current => list[index];

        public bool MoveNext()
        {
            index++;
            return index < list.Count;
        }

        public void Reset()
        {
            index = -1;
        }

        public void Dispose()
        {

        }
    }

    public List() : this([]) { }

    public Obj this[int index]
    {
        get => Value[index];
        set => Value[index] = value;
    }

    public int Count { get; private set; } = value.Length;

    public bool IsFull => Count == Value.Length;

    public override List Init(Tup args) => args switch
    {
        { Count: 0 } => this,
        { Count: 1 } when args[0].ToList().As<List>(out var list) => list,
        _ => args.ToList(),
    };

    public override Bool Eq(Obj other)
    {
        if (other is not List list)
            return Bool.False;

        for (int i = 0; i < Count; i++)
            if (Value[i].NEq(list[i]).As<Bool>(out var neq) && neq.Value)
                return Bool.False;

        return Bool.True;
    }

    public override Obj GetItem(Obj key) => key switch
    {
        Int i => OutOfRange(this, (int)i.Value) ? OutOfRange(this, (int)(i.Value + Count)) ? new Err("list index out of range") : this[(int)(i.Value + Count)] : this[(int)i.Value],
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
        var newList = new List();
        if (st == 1) {
            int count = e - s;
            if (count > 0) {
                var slice = new Obj[count];
                Array.Copy(Value, s, slice, 0, count);
                newList = new List(slice);
            }
        } else if (st > 0) {
            for (int i = s; i < e; i += st) newList.Add(this[i]);
        } else {
            for (int i = s; i > e; i += st) newList.Add(this[i]);
        }
        return newList;
    }

    public override Obj SetItem(Obj key, Obj value)
    {
        if (key is not Int i)
            return new Err("invalid index type");

        if (!OutOfRange(this, (int)i.Value))
            return this[(int)i.Value] = value;

        if (!OutOfRange(this, (int)(i.Value + Count)))
            return this[(int)(i.Value + Count)] = value;

        return new Err("list index out of range");
    }

    public override Obj In(Obj obj) => obj switch
    {
        List list => Bool.From(Overlap(this, list)),
        Tup tup => Bool.From(Overlap(this, tup.ToList())),

        _ => new Err($"cannot check if '{obj.Type}' is in '{Type}'"),
    };

    public override Int Len() => Int.From(Count);

    public override Bool ToBool() => Bool.From(Count != 0);

    public override List ToList()
    {
        var newList = new List(new Obj[Count]);
        for (int i = 0; i < Count; i++)
            newList[i] = this[i].Copy();
        return newList;
    }

    public override Tup ToTuple() => new(Value[..Count], new string[Count]);

    public override Iters Iter() => new(this);

    public override List Copy() => this;

    public override List Clone()
    {
        var newList = new List(new Obj[Count]);
        for (int i = 0; i < Count; i++)
            newList[i] = this[i].Clone();
        return newList;
    }

    public override Str ToStr() => Str.From($"[{string.Join(", ", Value[..Count].Select(Format))}]");

    private string Format(Obj value) => value is Str s ? $"'{s.Value}'" : Str.To(value).Value;

    public override Spreads Spread() => new(Value[..Count]);

    private static bool OutOfRange(List self, int index) => index < 0 || index >= self.Count;

    private static bool Overlap(List self, List list)
    {
        foreach (var item in list)
        {
            if (!self.Value.Contains(item))
                return false;
        }
        return true;
    }

    [Native(
        Name = "add",
        Description = "Adds a value to items.",
        Example = "items.add(value)",
        ReturnType = "none",
        ArgumentTypes = new[] { "any" }
    )]
    public static void Append([Self] List self, [ArgInfo(Essential = true)] Obj value)
    {
        if (self.IsFull) Resize(self);
        self[self.Count] = value;
        self.Count++;
    }

    [Native(
        Name = "extend",
        Description = "Adds values to items.",
        Example = "items.extend(value)",
        ReturnType = "none",
        ArgumentTypes = new[] { "any" }
    )]
    public static void Extend([Self] List self, [ArgInfo(Essential = true)] Obj value)
    {
        if (value.Iter().As<Iters>(out var iters))
        {
            if (ReferenceEquals(value, self))
            {
                var snapshot = self.ToList();
                foreach (var v in snapshot)
                    Append(self, v);
            }
            else
            {
                foreach (var v in iters.Value)
                    Append(self, v);
            }
        }
        else
        {
            Append(self, value);
        }
    }

    [Native(
        Name = "insert",
        Description = "Inserts a value into items.",
        Example = "items.insert(obj, index)",
        ReturnType = "none",
        ArgumentTypes = new[] { "any", "any" }
    )]
    public static Obj Insert(
        [Self] List self,
        [ArgInfo(Essential = true)] Obj obj,
        [ArgInfo(Essential = true)] Obj index)
    {
        if (!index.As<Int>(out var indexValue))
            return new Err("invalid arguments: index");

        if (self.Count == 0)
        {
            Append(self, obj);
            return self;
        }

        if (self.IsFull)
            Resize(self);

        for (int i = self.Count - 1; i >= indexValue.Value; i--)
            self[i + 1] = self[i];
        self[(int)indexValue.Value] = obj.Copy();
        self.Count++;

        return self;
    }

    [Native(
        Name = "extend_insert",
        Description = "Returns the result of items.extend insert().",
        Example = "items.extend_insert(obj, index)",
        ReturnType = "list",
        ArgumentTypes = new[] { "any", "any" }
    )]
    public static List ExtendInsert(
        [Self] List self,
        [ArgInfo(Essential = true)] Obj obj,
        [ArgInfo(Essential = true)] Obj index)
    {
        if (self.IsFull)
            Resize(self);

        if (obj.Iter().As<Iters>(out var iters))
        {
            foreach (var item in iters.Value)
                Insert(self, item, index);
        }
        else
        {
            Insert(self, obj, index);
        }

        return self;
    }

    [Native(
        Name = "remove",
        Description = "Removes a value from a list value.",
        Example = "items.remove(obj)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "any" }
    )]
    public static Bool Remove([Self] List self, [ArgInfo(Essential = true)] Obj obj)
    {
        for (int i = 0; i < self.Count; i++)
        {
            if (!self[i].Eq(obj).As<Bool>(out var eq))
                continue;

            if (eq.Value)
                return Bool.From(RemoveAt(self, Int.From(i)).As<Bool>(out var res) && res.Value);

        }
        return Bool.False;
    }

    [Native(
        Name = "remove_at",
        Description = "Removes  at from a list value.",
        Example = "items.remove_at(index)",
        ReturnType = "any",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj RemoveAt([Self] List self, [ArgInfo(Essential = true)] Obj index)
    {
        if (!index.As<Int>(out var idx))
            return new Err("invalid arguments: index");
        if (OutOfRange(self, (int)idx.Value))
            return Bool.False;

        for (int i = (int)idx.Value; i < self.Count - 1; i++)
            self[i] = self[i + 1];
        self.Count--;
        return Bool.True;
    }

    [Native(
        Name = "index_of",
        Description = "Finds a value index in items.",
        Example = "items.index_of(obj)",
        ReturnType = "int",
        ArgumentTypes = new[] { "any" }
    )]
    public static Int IndexOf([Self] List self, [ArgInfo(Essential = true)] Obj obj)
    {
        for (int i = 0; i < self.Count; i++)
        {
            if (!self[i].Eq(obj).As<Bool>(out var eq))
                continue;

            if (eq.Value)
                return Int.From(i);
        }
        return Int.From(-1);
    }

    [Native(
        Name = "contains",
        Description = "Checks whether items contains a value.",
        Example = "items.contains(obj)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "any" }
    )]
    public static Bool Contains([Self] List self, [ArgInfo(Essential = true)] Obj obj)
        => Bool.From(IndexOf(self, obj).Value != -1);

    [Native(
        Name = "order",
        Description = "Returns the result of items.order().",
        Example = "items.order(fn)",
        ReturnType = "list",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Order([Self] List self, [ArgInfo(Essential = true)] Obj fn)
    {
        if (fn is not Fn)
            return new Err("invalid arguments: fn");

        Array.Sort(self.Value, 0, self.Count, Comparer<Obj>.Create((i, j) => fn.Call(new([i], [])).CompareTo(fn.Call(new([j], [])))));
        return None;
    }

    [Native(
        Name = "sort",
        Description = "Sorts items in place.",
        Example = "items.sort()",
        ReturnType = "none"
    )]
    public static Obj Sort([Self] List self)
    {
        Array.Sort(self.Value, 0, self.Count);
        return None;
    }

    [Native(
        Name = "reverse",
        Description = "Reverses the order of items.",
        Example = "items.reverse()",
        ReturnType = "none"
    )]
    public static Obj Reverse([Self] List self)
    {
        Array.Reverse(self.Value, 0, self.Count);
        return None;
    }

    [Native(
        Name = "binary_search",
        Description = "Returns the result of items.binary search().",
        Example = "items.binary_search(obj)",
        ReturnType = "int",
        ArgumentTypes = new[] { "any" }
    )]
    public static Int BinarySearch([Self] List self, [ArgInfo(Essential = true)] Obj obj)
        => Int.From(Array.BinarySearch(self.Value, 0, self.Count, obj));

    [Native(
        Name = "lower_bound",
        Description = "Returns the result of items.lower bound().",
        Example = "items.lower_bound(obj)",
        ReturnType = "int",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj LowerBound([Self] List self, [ArgInfo(Essential = true)] Obj obj)
    {
        int l = 0, r = self.Count - 1, m = 0;
        while (r > l)
        {
            m = (l + r) / 2;
            var lt = self[m].Lt(obj);

            if (!lt.As<Bool>(out var ltValue))
                return lt is Err ? lt : new Err($"lower_bound requires '<' to return bool");

            if (ltValue.Value) l = m + 1;
            else r = m;
        }
        return Int.From(r);
    }

    [Native(
        Name = "upper_bound",
        Description = "Returns the result of items.upper bound().",
        Example = "items.upper_bound(obj)",
        ReturnType = "int",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj UpperBound([Self] List self, [ArgInfo(Essential = true)] Obj obj)
    {
        int l = 0, r = self.Count - 1;
        while (r > l)
        {
            int m = (l + r) / 2;
            var gt = self[m].Gt(obj);

            if (!gt.As<Bool>(out var gtValue))
                return gt is Err ? gt : new Err($"upper_bound requires '>' to return bool");

            if (gtValue.Value) r = m;
            else l = m + 1;
        }
        return Int.From(r);
    }

    [Native(
        Name = "hpush",
        Description = "Returns the result of items.hpush().",
        Example = "items.hpush(obj)",
        ReturnType = "none",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj HPush([Self] List self, [ArgInfo(Essential = true)] Obj obj)
    {
        int child = self.Count;
        Append(self, obj);

        while (child != 0)
        {
            int parent = (child - 1) / 2;

            if (self[parent].CompareTo(self[child]) <= 0) break;

            (self[parent], self[child]) = (self[child], self[parent]);

            child = parent;
        }

        return None;
    }

    [Native(
        Name = "hpop",
        Description = "Returns the result of items.hpop().",
        Example = "items.hpop()",
        ReturnType = "any"
    )]
    public static Obj HPop([Self] List self)
    {
        if (self.Count == 0)
            return new Err("list is empty");

        Obj value = self[0];
        self[0] = self[^1];
        self.Count--;

        int parent = 0;

        while (true)
        {
            int smallest = parent; int left = 2 * parent + 1, right = 2 * parent + 2;

            if (left < self.Count && self[left].CompareTo(self[smallest]) < 0) 
                smallest = left;
            if (right < self.Count && self[right].CompareTo(self[smallest]) < 0) 
                smallest = right;

            if (smallest == parent) break;

            (self[parent], self[smallest]) = (self[smallest], self[parent]);

            parent = smallest;
        }

        return value;
    }

    [Native(
        Name = "pop",
        Description = "Returns the result of items.pop().",
        Example = "items.pop(index)",
        ReturnType = "any",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Pop([Self] List self, [ArgInfo(Optional = true)] Obj? index = null)
    {
        index ??= Int.From(self.Count - 1);

        if (!index.As<Int>(out var idx))
            return new Err("invalid argumnets: index");

        if (idx.Value < 0)
            idx = Int.From(idx.Value + self.Count);

        if (OutOfRange(self, (int)idx.Value))
            return new Err("list index out of range");

        Obj value = self[(int)idx.Value];
        RemoveAt(self, index);
        return value;
    }

    [Native(
        Name = "map",
        Description = "Returns the result of items.map().",
        Example = "items.map(type)",
        ReturnType = "list",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Map([Self] List self, [ArgInfo(Essential = true)] Obj type)
    {
        var list = new List();

        foreach (var item in self)
        {
            var res = type.Call(Tup.One("", item));
            if (res is Err)
                return res;
            list.Add(res);
        }

        return list;
    }

    [Native(
        Name = "resize",
        Description = "Returns the result of items.resize().",
        Example = "items.resize()",
        ReturnType = "none"
    )]
    public static Obj Resize([Self] List self)
    {
        var newValue = new Obj[self.Value.Length * 2 + 1];
        for (var i = 0; i < self.Value.Length; i++)
            newValue[i] = self.Value[i];
        self.Value = newValue;
        return None;
    }

    public override int GetHashCode()
    {
        HashCode hash = new();

        foreach (var value in Value)
            hash.Add(value);

        return hash.ToHashCode();
    }

    public IEnumerator<Obj> GetEnumerator() => new Enumerator(this);

    IEnumerator IEnumerable.GetEnumerator() => new Enumerator(this);

}
