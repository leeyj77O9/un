using System.Runtime.Remoting;
using System.Xml.Linq;
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

        if (ValidSuper())
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
        if (!ToBool().As<Bool>(out var left))
            return new Err("left operand must be a boolean");
        if (!other.ToBool().As<Bool>(out var right))
            return new Err("right operand must be a boolean");

        if (left.Value ^ right.Value) return Bool.True;
        return Bool.False;
    }

    public virtual Obj Not() => ToBool().As<Bool>(out var value) ? Bool.From(!value.Value) : new Err("operand must be a boolean");

    public virtual Obj Eq(Obj other)
    {
        if (TryMethod("__eq__", out var value, new([other])))
            return value;

        if (ValidSuper())
            return Super.Eq(other);

        if (IsNone() && other.IsNone())
            return Bool.True;

        if (IsNone() || other.IsNone())
            return Bool.False;

        return new Err($"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'");
    }

    public virtual Obj NEq(Obj other) => Binary("__neq__", other, "other", s => s.NEq(other), () => Eq(other).As<Bool>(out var value) ? Bool.From(!value.Value) : new Err("operands must be booleans"));

    public virtual Obj Lt(Obj other) => Binary("__lt__", other, "other", s => s.Lt(other), () => new Err($"unsupported operand type(s) for <: '{Type}' and '{other.Type}'"));

    public virtual Obj Gt(Obj other) => Binary("__gt__", other, "other", s => s.Gt(other), () =>
    {
        if (!Lt(other).As<Bool>(out var ltValue))
            return new Err($"unsupported operand type(s) for >: '{Type}' and '{other.Type}'");
        if (!Eq(other).As<Bool>(out var eqValue))
            return new Err($"unsupported operand type(s) for >: '{Type}' and '{other.Type}'");

        return Bool.From(!ltValue.Value && !eqValue.Value);
    });

    public virtual Obj LtOrEq(Obj other) => Binary("__ltoreq__", other, "other", s => s.LtOrEq(other), () =>
    {
        if (!Lt(other).As<Bool>(out var ltValue))
            return new Err($"unsupported operand type(s) for <=: '{Type}' and '{other.Type}'");
        if (!Eq(other).As<Bool>(out var eqValue))
            return new Err($"unsupported operand type(s) for <=: '{Type}' and '{other.Type}'");

        return Bool.From(ltValue.Value || eqValue.Value);
    });

    public virtual Obj GtOrEq(Obj other) => Binary("__gtoreq__", other, "other", s => s.GtOrEq(other), () =>
    {
        if (!Lt(other).As<Bool>(out var ltValue))
            return new Err($"unsupported operand type(s) for >=: '{Type}' and '{other.Type}'");
        if (!Eq(other).As<Bool>(out var eqValue))
            return new Err($"unsupported operand type(s) for >=: '{Type}' and '{other.Type}'");

        return Bool.From(!ltValue.Value);
    });

    public virtual Obj Slice(Obj? start, Obj? end, Obj? step)
    {
        if (TryMethod("__slice__", out var value, new([start ?? None, end ?? None, step ?? None])))
            return value;
        if (ValidSuper())
            Super.Slice(start, end, step);

        return new Err($"unsupported slice for '{Type}'");
    }    

    public virtual Obj SetAttr(string name, Obj value)
    {
        if (TryMethod("__setattr__", out _, new([Str.From(name), value])))
            return value;
        if (ValidSuper())
            Super.SetAttr(name, value);

        if (Type.Name is "int" or "float" or "str" or "bool" or "date" or "none" or "null")
            return new Err($"'{Type}' object has no attribute '{name}'");

        return Members[name] = value;
    }

    public virtual Obj GetAttr(string name)
    {
        if (TryMethod("__getattr__", out Obj? value, new([Str.From(name)])))
            goto Found;
        if (Members.TryGetValue(name, out value))
            goto Found;
        if (ValidSuper() && Super.Has(name))
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
        if (TryMethod("__setitem__", out _, new([key, value])))
            return value;
        else if (ValidSuper())
            return Super.SetItem(key, value);
        else
            return new Err($"unsupported operand type(s) for [] = 'value': '{Type}'");
    }

    public virtual Obj Is(Obj obj)
    {
        if (TryMethod("__is__", out Obj? value, new([obj])))
            return value;
  
        if (Types is UnionType unionTypes && unionTypes.Contains(obj.Type))
            return Bool.True;
        if (Types is UnType singleType && singleType == obj.Type)
            return Bool.True;

        return ValidSuper() ? Super.Is(obj) : Bool.False;
    }

    public virtual Obj In(Obj obj)
    {
        if (TryMethod("__in__", out Obj? value, new([obj])))
            return value;
        return ValidSuper() ? Super.In(obj) : new Err($"unsupported operand type(s) for in: '{Type}'");
    }

    public virtual Str Repr()
    {
        if (TryMethod("__repr__", out Obj? value, []))
            return (Str)value;

        if (ValidSuper() && Super.Repr().As<Str>(out var repr))
        {
           if (repr.Value == Super.Type.Name)
                return Str.From(Type.Name);
            return repr;
        }

        return Str.From(Type.Name);
    }

    public virtual Obj Clone()
    {
        if (TryMethod("__clone__", out Obj? value, []))
            return value;
        return ValidSuper() && Super.Has("__clone__") ? Super.Clone() : new Obj(Type)
        {
            Types = Types,
            Members = Members.New(),
            Annotations = Annotations,
            Self = Self,
            Super = Super is null ? None : !Super.IsNone() ? Super.Clone() : Super,
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

    protected Obj Unary(string method, Func<Obj, Obj> superCall, Func<Obj> fallback)
    {
        if (TryMethod(method, out var value, []))
            return value;

        if (ValidSuper())
            return superCall(Super);

        return fallback();
    }

    protected Obj Binary(string method, Obj other, string argName, Func<Obj, Obj> superCall, Func<Obj> fallback)
    {
        if (TryMethod(method, out var value, new([other])))
            return value;

        if (ValidSuper())
            return superCall(Super);

        return fallback();
    }

    protected Obj Invoke(string method, Tup args, Func<Obj, Obj> superCall, Func<Obj> fallback)
    {
        if (TryMethod(method, out var value, args))
            return value;

        if (ValidSuper())
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

    protected bool ValidSuper()
    {
        if (Super is null || Super.IsNone())
            return false;
        return true;
    }

    public bool IsNone() => Type == UnType.None;

    public bool Has(string name)
    {
        if (Members.ContainsKey(name))
            return true;
        if (ValidSuper())
            return Super.Has(name);
        return false;
    }

    public override bool Equals(object? other) => other switch
    {
        Obj o => Eq(o).As<Bool>(out var eqResult) && eqResult.Value,
        _ => false,
    };

    public override int GetHashCode() => Hash().As<Int>(out var hashResult) ? hashResult.Value.GetHashCode() : Type.GetHashCode();

    public int CompareTo(Obj? other)
    {
        if (other == null) return 0;
        if (Eq(other).As<Bool>(out var eqResult) && eqResult.Value) return 0;
        if (Lt(other).As<Bool>(out var ltResult) && ltResult.Value) return -1;
        if (Gt(other).As<Bool>(out var gtResult) && gtResult.Value) return 1;

        throw new Panic("types that are not comparable to each other.");
    }
}
