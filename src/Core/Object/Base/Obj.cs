using Un.Object.Collections;
using Un.Object.Primitive;
using Un.Object.Type;

namespace Un.Object;

public class Obj(UnType type) : IComparable<Obj>
{
    public readonly static Obj Error = new(UnType.Error);
    public readonly static Obj None = new(UnType.None);
    public readonly static Obj Null = new(UnType.Null);

    public virtual UnType Type { get; set; } = type;
    public virtual BaseType Types { get; set; } = UnionType.Create(UnType.Obj, type);
    public virtual Obj Self { get; set; } = None;
    public virtual Obj Super { get; set; } = None;
    public virtual Attributes Members { get; set; } = [];
    public virtual Attributes Annotations { get; set; } = [];

    public Obj() : this(UnType.Obj) { }

    public virtual Obj Init(Tup args) 
    {
        if (TryMethod("__init__", out _, args))
            return this;

        if (Super is not null && !Super.IsNone())
            return Super.Init(args);

        return this;
    }

    public virtual Obj Add(Obj other) => Binary("__add__", other, "other", s => s.Add(other), () => new Err($"unsupported operand type(s) for +: '{Type}' and '{other.Type}'"));

    public virtual Obj Sub(Obj other) => Binary("__sub__", other, "other", s => s.Sub(other), () => new Err($"unsupported operand type(s) for -: '{Type}' and '{other.Type}'"));

    public virtual Obj Mul(Obj other) => Binary("__mul__", other, "other", s => s.Mul(other), () => new Err($"unsupported operand type(s) for *: '{Type}' and '{other.Type}'"));

    public virtual Obj Div(Obj other) => Binary("__div__", other, "other", s => s.Div(other), () => new Err($"unsupported operand type(s) for /: '{Type}' and '{other.Type}'"));

    public virtual Obj IDiv(Obj other) => Binary("__idiv__", other, "other", s => s.IDiv(other), () => new Err($"unsupported operand type(s) for //: '{Type}' and '{other.Type}'"));

    public virtual Obj Mod(Obj other) => Binary("__mod__", other, "other", s => s.Mod(other), () => new Err($"unsupported operand type(s) for %: '{Type}' and '{other.Type}'"));

    public virtual Obj Pow(Obj other) => Binary("__pow__", other, "other", s => s.Pow(other), () => new Err($"unsupported operand type(s) for **: '{Type}' and '{other.Type}'"));

    public virtual Obj BAnd(Obj other) => Binary("__band__", other, "other", s => s.BAnd(other), () => new Err($"unsupported operand type(s) for &: '{Type}' and '{other.Type}'"));

    public virtual Obj BOr(Obj other) => Binary("__bor__", other, "other", s => s.BOr(other), () => new Err($"unsupported operand type(s) for |: '{Type}' and '{other.Type}'"));

    public virtual Obj BXor(Obj other) => Binary("__bxor__", other, "other", s => s.BXor(other), () => new Err($"unsupported operand type(s) for ^: '{Type}' and '{other.Type}'"));

    public virtual Obj LShift(Obj other) => Binary("__lsh__", other, "other", s => s.LShift(other), () => new Err($"unsupported operand type(s) for <<: '{Type}' and '{other.Type}'"));

    public virtual Obj RShift(Obj other) => Binary("__rsh__", other, "other", s => s.RShift(other), () => new Err($"unsupported operand type(s) for >>: '{Type}' and '{other.Type}'"));

    public virtual Obj Call(Tup args) => Invoke("__call__", args, s => s.Call(args), () => new Err($"unsupported operand type(s) for (): '{Type}'"));

    public virtual Obj GetItem(Obj key) => Binary("__getitem__", key, "key", s => s.GetItem(key), () => new Err($"unsupported operand type(s) for []: '{Type}'"));

    public virtual Obj Pos() => Unary("__pos__", s => s.Pos(), () => new Err($"unsupported operand type(s) for +: '{Type}'"));

    public virtual Obj Neg() => Unary("__neg__", s => s.Neg(), () => new Err($"unsupported operand type(s) for -: '{Type}'"));

    public virtual Obj BNot() => Unary("__bnot__", s => s.BNot(), () => new Err($"unsupported operand type(s) for ~: '{Type}'"));

    public virtual Obj Len() => Unary("__len__", s => s.Len(), () => new Err("unsupported operand type(s) for len()"));

    public virtual Obj Hash() => Unary("__hash__", s => s.Hash(), () => new Err("cannot hashable object"));

    public virtual Obj ToInt() => Unary("__int__", s => s.ToInt(), () => new Err($"unsupported operand type(s) for int(): '{Type}'"));

    public virtual Obj ToFloat() => Unary("__float__", s => s.ToFloat(), () => new Err($"unsupported operand type(s) for float(): '{Type}'"));

    public virtual Obj ToBool() => Unary("__bool__", s => s.ToBool(), () => new Err($"unsupported operand type(s) for bool(): '{Type}'"));

    public virtual Obj ToList() => Unary("__list__", s => s.ToList(), () => new Err($"unsupported operand type(s) for list(): '{Type}'"));

    public virtual Obj ToTuple() => Unary("__tuple__", s => s.ToTuple(), () => new Err($"unsupported operand type(s) for tuple(): '{Type}'"));

    public virtual Obj ToStr() => IsNone() ? Str.From("none") : Unary("__str__", s => s.ToStr(), () => new Err($"unsupported operand type(s) for str(): '{Type}'"));

    public virtual Obj Entry() => Unary("__entry__", s => s.Entry(), () => new Err($"unsupported operand type(s) for using entry: '{Type}'"));

    public virtual Obj Exit() => Unary("__exit__", s => s.Exit(), () => new Err($"unsupported operand type(s) for using exit: '{Type}'"));

    public virtual Obj Iter() => Unary("__iter__", s => s.Iter(), () => new Err($"unsupported operand type(s) for iter(): '{Type}'"));

    public virtual Obj Next() => Unary("__next__", s => s.Next(), () => new Err($"unsupported operand type(s) for next(): '{Type}'"));

    public virtual Obj Copy() => Unary("__copy__", s => s.Copy(), () => this);

    public virtual Obj Spread() => Unary("__spread__", s => s.Spread(), () => new Err($"unsupported operand type(s) for *: '{Type}'"));

    public virtual Obj Xor(Obj other)
    {
        if (ToBool().As<Bool>().Value ^ other.ToBool().As<Bool>().Value) return Bool.True;
        return Bool.False;
    }

    public virtual Obj Not() => Bool.From(!ToBool().As<Bool>().Value);

    public virtual Obj Eq(Obj other) => IsNone() && other.IsNone() ? Bool.True
        : Binary("__eq__", other, "other", s => s.Eq(other), () => new Err($"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'"));
 
    public virtual Obj NEq(Obj other) => Bool.From(!Eq(other).As<Bool>().Value);

    public virtual Obj Lt(Obj other) => Binary("__lt__", other, "other", s => s.Lt(other), () => new Err($"unsupported operand type(s) for <: '{Type}' and '{other.Type}'"));

    public virtual Obj Gt(Obj other) => Bool.From(!Lt(other).As<Bool>().Value && !Eq(other).As<Bool>().Value);

    public virtual Obj LtOrEq(Obj other) => Bool.From(Lt(other).As<Bool>().Value || Eq(other).As<Bool>().Value);

    public virtual Obj GtOrEq(Obj other) => Bool.From(!Lt(other).As<Bool>().Value);

    public virtual Obj Slicer(Int to, Int from, Int step)
    {
        List list = [];
        long a = to.Value;
        long b = from.Value == -1 ? Len().As<Int>().Value : from.Value;

        do
        {
            list.Append(GetItem(Int.From(a)));
            a += step.Value;
        } while (a < b);

        return list;
    }

    public virtual Obj SetAttr(string name, Obj value)
    {
        if (TryMethod("__setattr__", out _, new([Str.From(name), value], ["key", "value"])))
            return value;
        if (Super is not null && !Super.IsNone())
            Super.SetAttr(name, value);

        return Members[name] = value;
    }

    public virtual Obj GetAttr(string name)
    {
        if (TryMethod("__getattr__", out Obj? value, new([Str.From(name)], [])))
            goto Found;
        if (Members.TryGetValue(name, out value))
            goto Found;
        if (Super is not null && !Super.IsNone() && Super.Has(name))
        {
            value = Super.GetAttr(name);
            goto Found;
        }
        if (Global.TryGetOriginalValue(Type.Name, name, out value))
            goto Found;

        return new Err($"'{Type}' object has no attribute '{name}'");

    Found:
        if (value is null)
            return new Err($"'{Type}' object has no attribute '{name}'");
            
        value.Self = this;
        value.Super = Super!;
        return value;
    }


    public virtual Obj SetItem(Obj key, Obj value)
    {
        if (TryMethod("__setitem__", out _, new([key, value], ["key", "value"])))
            return value;
        else if (Super is not null && !Super.IsNone())
            return Super.SetItem(key, value);
        else
            return new Err($"unsupported operand type(s) for [] = 'value': '{Type}'");
    }

    public virtual Obj Is(Obj obj)
    {
        if (TryMethod("__is__", out Obj? value, new([obj], ["obj"])))
            return value;
  
        if (Types is UnionType unionTypes && unionTypes.Contains(obj.Type))
            return Bool.True;
        if (Types is UnType singleType && singleType == obj.Type)
            return Bool.True;

        return Super is not null && !Super.IsNone() ? Super.Is(obj) : Bool.False;
    }

    public virtual Obj In(Obj obj)
    {
        if (TryMethod("__in__", out Obj? value, new([obj], [])))
            return value;
        return Super is not null && !Super.IsNone() ? Super.In(obj) : new Err($"unsupported operand type(s) for in: '{Type}'");
    }

    public virtual Obj Repr()
    {
        if (TryMethod("__repr__", out Obj? value, []))
            return value;
        return Super is not null && !Super.IsNone() && Super.Repr().As<Str>().Value != Super.Type.Name ? Super.Repr(): Str.From(Type.Name);
    }

    public virtual Obj Clone()
    {
        if (TryMethod("__clone__", out Obj? value, []))
            return value;
        return Super is not null && !Super.IsNone() && Super.Has("__clone__") ? Super.Clone() : new Obj(Type)
        {
            Types = Types,
            Members = Members.New(),
            Annotations = Annotations,
            Self = Self,
            Super = Super?.Clone()!,
        };
    }

    public bool As<T>(out T value) where T : Obj
    {
        if (this is T obj)
        {
            value = obj;
            return true;
        }

        value = null!;
        return false;
    }

    public bool As<T, U>(out Obj value) where T : Obj where U : Obj
    {
        if (this is T obj1)
        {
            value = obj1;
            return true;
        }
        if (this is U obj2)
        {
            value = obj2;
            return true;
        }

        value = null!;
        return false;
    }

    public T As<T>() where T : Obj
    {
        if (this is T obj)
            return obj;

        throw new Panic($"internal type mismatch: expected {typeof(T).Name}, got {Type}");
    }

    public Obj As<T, U>() where T : Obj where U : Obj
    {
        if (this is T obj1)
            return obj1;
        if (this is U obj2)
            return obj2;

       return new Err($"cannot cast {Type} to {typeof(T).Name.ToLower()} or {typeof(U).Name.ToLower()}");
    }

    public T As<T>(string message) where T : Obj
    {
        if (this is T obj)
            return obj;

        throw new Panic(message);
    }

    protected Obj Unary(string method, Func<Obj, Obj> superCall, Func<Obj> fallback)
    {
        if (TryMethod(method, out var value, []))
            return value;

        if (Super is not null && !Super.IsNone())
            return superCall(Super);

        return fallback();
    }

    protected Obj Binary(string method, Obj other, string argName, Func<Obj, Obj> superCall, Func<Obj> fallback)
    {
        if (TryMethod(method, out var value, new([other], [argName])))
            return value;

        if (Super is not null && !Super.IsNone())
            return superCall(Super);

        return fallback();
    }

    protected Obj Invoke(string method, Tup args, Func<Obj, Obj> superCall, Func<Obj> fallback)
    {
        if (TryMethod(method, out var value, args))
            return value;

        if (Super is not null && !Super.IsNone())
            return superCall(Super);

        return fallback();
    }

    protected bool TryMethod(string name, out Obj value, Tup args)
    {
        if (Members.TryGetValue(name, out Obj? method))
        {
            method.Self = this;
            method.Super = Super;
            value = method.Call(args);
            return true;
        }
        
        return (value = null!) is not null;
    }

    public bool IsNone() => Type == UnType.None;

    public bool Has(string name)
    {
        if (Members.ContainsKey(name))
            return true;
        if (Super is not null && !Super.IsNone())
            return Super.Has(name);
        return false;
    }

    public virtual Attributes GetOriginal() => [];

    public override bool Equals(object? other) => other switch
    {
        Obj o => Eq(o).As<Bool>().Value,
        _ => false,
    };

    public override int GetHashCode() => Hash().As<Int>().Value.GetHashCode();
    
    public int CompareTo(Obj? other)
    {
        if (other == null) return 0;
        if (Eq(other).As<Bool>().Value) return 0;
        if (Lt(other).As<Bool>().Value) return -1;
        if (Gt(other).As<Bool>().Value) return 1;
        throw new Panic("types that are not comparable to each other.");
    }
}
