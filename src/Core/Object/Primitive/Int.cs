using Un.Object.Collections;
using Un.Object.Function;

namespace Un.Object.Primitive;

public class Int(long value) : Val<long>(value, "int")
{
    public Int() : this(0) {}

    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => new Int(),
        { Count: 1 } => args[0].ToInt(),
        _ => new Err($"cannot convert to '{Type}'"),
    };

    public override Obj Add(Obj other) => other switch
    {
        Int i => new Int(Value + i.Value),
        Float f => new Float(Value + f.Value),
        Str s => new Str(Value.ToString() + s.Value),
        _ => new Err($"unsupported operand type(s) for +: 'int' and '{other.Type}'")
    };

    public override Obj Sub(Obj other) => other switch
    {
        Int i => new Int(Value - i.Value),
        Float f => new Float(Value - f.Value),
        _ => new Err($"unsupported operand type(s) for -: 'int' and '{other.Type}'")
    };

    public override Obj Mul(Obj other) => other switch
    {
        Int i => new Int(Value * i.Value),
        Float f => new Float(Value * f.Value),
        _ => new Err($"unsupported operand type(s) for *: 'int' and '{other.Type}'")
    };

    public override Obj Div(Obj other) => other switch
    {
        Int i => i.Value == 0 ? new Err($"{Type} division by zero") : new Float((double)Value / i.Value),
        Float f => f.Value == 0 ? new Err($"{Type} division by zero") : new Float(Value / f.Value),
        _ => new Err($"unsupported operand type(s) for /: 'int' and '{other.Type}'")
    };

    public override Obj Mod(Obj other) => other switch
    {
        Int i => new Int(Value % i.Value),
        Float f => new Float(Value % f.Value),
        _ => new Err($"unsupported operand type(s) for %: 'int' and '{other.Type}'")
    };

    public override Obj IDiv(Obj other) => other switch
    {
        Int i => i.Value == 0 ? new Err($"{Type} division by zero") : new Int((long)((double)Value / i.Value)),
        Float f => f.Value == 0 ? new Err($"{Type} division by zero") : new Int((long)(Value / f.Value)),
        _ => new Err($"unsupported operand type(s) for //: 'int' and '{other.Type}'")
    };

    public override Obj Pow(Obj other) => other switch
    {
        Int i => new Int((long)Math.Pow(Value, i.Value)),
        Float f => new Float(Math.Pow(Value, f.Value)),
        _ => new Err($"unsupported operand type(s) for **: 'int' and '{other.Type}'")
    };

    public override Obj Neg() => new Int(-Value);

    public override Obj Pos() => new Int(+Value);

    public override Obj BAnd(Obj other) => other switch
    {
        Int i => new Int(Value & i.Value),
        _ => new Err($"unsupported operand type(s) for &: 'int' and '{other.Type}'")
    };

    public override Obj BOr(Obj other) => other switch
    {
        Int i => new Int(Value | i.Value),
        _ => new Err($"unsupported operand type(s) for |: 'int' and '{other.Type}'")
    };

    public override Obj BXor(Obj other) => other switch
    {
        Int i => new Int(Value ^ i.Value),
        _ => new Err($"unsupported operand type(s) for ^: 'int' and '{other.Type}'")
    };

    public override Obj BNot() => new Int(~Value);

    public override Obj LShift(Obj other) => other switch
    {
        Int i => new Int(Value << (int)i.Value),
        _ => new Err($"unsupported operand type(s) for <<: 'int' and '{other.Type}'")
    };

    public override Obj RShift(Obj other) => other switch
    {
        Int i => new Int(Value >> (int)i.Value),
        _ => new Err($"unsupported operand type(s) for >>: 'int' and '{other.Type}'")
    };

    public override Obj Eq(Obj other) => other switch
    {
        Int i => new Bool(Value == i.Value),
        Float f => new Bool(Value == f.Value),
        Obj o when o.IsNone() => Bool.False,
        _ => new Err($"unsupported operand type(s) for ==: 'int' and '{other.Type}'")
    };

    public override Obj Lt(Obj other) => other switch
    {
        Int i => new Bool(Value < i.Value),
        Float f => new Bool(Value < f.Value),
        _ => new Err($"unsupported operand type(s) for <: 'int' and '{other.Type}'")
    };

    public override Obj Gt(Obj other) => other switch
    {
        Int i => new Bool(Value > i.Value),
        Float f => new Bool(Value > f.Value),
        _ => new Err($"unsupported operand type(s) for >: 'int' and '{other.Type}'")
    };

    public override Obj LtOrEq(Obj other) => other switch
    {
        Int i => new Bool(Value <= i.Value),
        Float f => new Bool(Value <= f.Value),
        _ => new Err($"unsupported operand type(s) for <=: 'int' and '{other.Type}'")
    };

    public override Obj GtOrEq(Obj other) => other switch
    {
        Int i => new Bool(Value >= i.Value),
        Float f => new Bool(Value >= f.Value),
        _ => new Err($"unsupported operand type(s) for >=: 'int' and '{other.Type}'")
    };

    public override Obj NEq(Obj other) => other switch
    {
        Int i => new Bool(Value != i.Value),
        Float f => new Bool(Value != f.Value),
        Obj o when o.IsNone() => Bool.True,
        _ => new Err($"unsupported operand type(s) for !=: 'int' and '{other.Type}'")
    };

    public override Obj Len() => new Int(1);

    public override Int ToInt() => new(Value);

    public override Float ToFloat() => new(Value);

    public override Str ToStr() => new(Value.ToString());

    public override Bool ToBool() => new(Value != 0);

    public override Obj Copy() => new Int(Value)
    {
        Annotations = Annotations
    };

    public override Obj Clone() => new Int(Value)
    {
        Annotations = Annotations
    };
}