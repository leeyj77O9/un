using Un.Object.Collections;

namespace Un.Object.Primitive;

public class Float(double value) : Val<double>(value, "float")
{
    public Float() : this(0) { }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => new Float(),
        { Count: 1 } => args[0].ToFloat(),
        _ => new Err($"cannot convert to '{Type}'"),
    };

    public override Obj Add(Obj other) => other switch
    {
        Int i => new Float(Value + i.Value),
        Float f => new Float(Value + f.Value),
        Str s => new Str(Value.ToString() + s.Value),
        _ => new Err($"unsupported operand type(s) for +: 'float' and '{other.Type}'")
    };

    public override Obj Sub(Obj other) => other switch
    {
        Int i => new Float(Value - i.Value),
        Float f => new Float(Value - f.Value),
        _ => new Err($"unsupported operand type(s) for -: 'float' and '{other.Type}'")
    };

    public override Obj Mul(Obj other) => other switch
    {
        Int i => new Float(Value * i.Value),
        Float f => new Float(Value * f.Value),
        _ => new Err($"unsupported operand type(s) for *: 'float' and '{other.Type}'")
    };

    public override Obj Div(Obj other) => other switch
    {
        Int i => i.Value == 0 ? new Err($"{Type} division by zero") : new Float(Value / i.Value),
        Float f => f.Value == 0 ? new Err($"{Type} division by zero") : new Float(Value / f.Value),
        _ => new Err($"unsupported operand type(s) for /: 'float' and '{other.Type}'")
    };

    public override Obj Mod(Obj other) => other switch
    {
        Int i => new Float(Value % i.Value),
        Float f => new Float(Value % f.Value),
        _ => new Err($"unsupported operand type(s) for %: 'float' and '{other.Type}'")
    };

    public override Obj IDiv(Obj other) => other switch
    {
        Int i => i.Value == 0 ? new Err($"{Type} division by zero") : new Int((long)Value / i.Value),
        Float f => f.Value == 0 ? new Err($"{Type} division by zero") : new Int((long)(Value / f.Value)),
        _ => new Err($"unsupported operand type(s) for //: 'float' and '{other.Type}'")
    };

    public override Obj Pow(Obj other) => other switch
    {
        Int i => new Float(Math.Pow(Value, i.Value)),
        Float f => new Float(Math.Pow(Value, f.Value)),
        _ => new Err($"unsupported operand type(s) for **: 'float' and '{other.Type}'")
    };

    public override Obj Neg() => new Float(-Value);

    public override Obj Pos() => new Float(+Value);

    public override Obj Eq(Obj other) => other switch
    {
        Int i => new Bool(Value == i.Value),
        Float f => new Bool(Value == f.Value),
        Obj o when o.IsNone() => Bool.False,
        _ => new Err($"unsupported operand type(s) for ==: 'float' and '{other.Type}'")
    };

    public override Obj NEq(Obj other) => other switch
    {
        Int i => new Bool(Value != i.Value),
        Float f => new Bool(Value != f.Value),
        Obj o when o.IsNone() => Bool.True,
        _ => new Err($"unsupported operand type(s) for !=: 'float' and '{other.Type}'")
    };

    public override Obj Lt(Obj other) => other switch
    {
        Int i => new Bool(Value < i.Value),
        Float f => new Bool(Value < f.Value),
        _ => new Err($"unsupported operand type(s) for <: 'float' and '{other.Type}'")
    };

    public override Obj Gt(Obj other) => other switch
    {
        Int i => new Bool(Value > i.Value),
        Float f => new Bool(Value > f.Value),
        _ => new Err($"unsupported operand type(s) for >: 'float' and '{other.Type}'")
    };

    public override Obj LtOrEq(Obj other) => other switch
    {
        Int i => new Bool(Value <= i.Value),
        Float f => new Bool(Value <= f.Value),
        _ => new Err($"unsupported operand type(s) for <=: 'float' and '{other.Type}'")
    };

    public override Obj GtOrEq(Obj other) => other switch
    {
        Int i => new Bool(Value >= i.Value),
        Float f => new Bool(Value >= f.Value),
        _ => new Err($"unsupported operand type(s) for >=: 'float' and '{other.Type}'")
    };

    public override Int ToInt() => new((long)Value);
    public override Float ToFloat() => new(Value);
    public override Str ToStr() => new(Value.ToString());
    public override Bool ToBool() => new(Value != 0);

    public override Obj Copy() => new Float(Value)
    {
        Annotations = Annotations
    };
    
    public override Obj Clone() => new Float(Value)
    {
        Annotations = Annotations
    };
}