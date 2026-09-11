using System.Collections.Concurrent;
using Un.Object.Collections;
using Un.Object.Iter;
using Un.Object.Type;
using Un.Reflection;

namespace Un.Object.Primitive;

[BuiltinType("str", Description = "Immutable UTF-8 string with interning and rich text operations.", Example = "text = \"hello, world\"\ntext.split(\", \")\ntext.to_upper()")]
public class Str : Ref<string>
{
    private const int MAX_POOL_SIZE = 10000;
    private const int MAX_POOLED_STRING_LEGNTH = 1000;

    public static readonly Str Empty = new();
    private static ConcurrentDictionary<string, Str> pool = [];

    public Str() : this("") { }

    private Str(string value) : base(value, UnType.Str) { }

    public override Obj Init(Tup args) => args switch
    {
        { Count: 0 } => From(""),
        { Count: 1 } => args[0].ToStr(),
        { Count: 2 } and [Str str, Int i] => From(string.Concat(Enumerable.Repeat(str.Value, (int)Math.Min(i.Value, int.MaxValue)))),
        _ => new Err($"too many arguments"),
    };

    public char this[int index] => Value[index];

    public override Obj Add(Obj other)
    {
        var str = other.ToStr();

        if (str is Err err)
            return err;

        if (!str.As<Str>(out var strValue))
            return new Err($"cannot add '{other.Type}' to '{Type}'");

        return From(Value + strValue.Value);
    }

    public override Obj Sub(Obj other)
    {
        var str = other.ToStr();

        if (str is Err err)
            return err;

        if (!str.As<Str>(out var strValue))
            return new Err($"cannot add '{other.Type}' to '{Type}'");

        return From(Value.Replace(strValue.Value, ""));
    }

    public override Obj Eq(Obj other) => other switch
    {
        Str s => Bool.From(Value.CompareTo(s.Value) == 0),
        Obj o when o.IsNone() => Bool.False,
        _ => new Err($"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'")
    };

    public override Obj NEq(Obj other) => other switch
    {
        Str s => Bool.From(Value.CompareTo(s.Value) != 0),
        Obj o when o.IsNone() => Bool.True,
        _ => new Err($"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'")
    };

    public override Obj Lt(Obj other) => other switch
    {
        Str s => Bool.From(Value.CompareTo(s.Value) < 0),
        _ => new Err($"unsupported operand type(s) for <: '{Type}' and '{other.Type}'")
    };

    public override Obj GetItem(Obj other) => other switch
    {
        Int i => OutOfRange((int)i.Value) ? OutOfRange((int)(i.Value + Value.Length)) ? new Err("list index out of range") : Str.From($"{this[(int)(i.Value + Value.Length)]}") : Str.From($"{this[(int)i.Value]}"),
        _ => new Err("invalid index type"),
    };

    public override Obj Slice(Obj? start, Obj? end, Obj? step)
    {
        int len = Value.Length;
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
        if (st == 1)
        {
            if (e < s) e = s;
            return From(Value.Substring(s, Math.Max(0, e - s)));
        }
        var sb = new System.Text.StringBuilder();
        if (st > 0) { for (int i = s; i < e; i += st) sb.Append(Value[i]); }
        else { for (int i = s; i > e; i += st) sb.Append(Value[i]); }

        return From(sb.ToString());
    }

    public override Int Len() => Int.From(Value.Length);

    public override Obj ToInt() => long.TryParse(Value, out var result) ? Int.From(result) : new Err($"cannot convert '{Value}' to 'int'");

    public override Obj ToFloat() => double.TryParse(Value, out var result) ? new Float(result) : new Err($"cannot convert '{Value}' to 'float'");

    public override Str ToStr() => this;

    public override Bool ToBool() => bool.TryParse(Value, out var result) ? result ? Bool.True : Bool.False : string.IsNullOrEmpty(Value) ? Bool.False : Bool.True;

    public override Iters Iter() => new(Value.Select(c => From($"{c}")));

    public override List ToList()
    {
        var list = new List();
        // 문자열을 List[str]로 변환: 각 문자(char)를 Str로 래핑하여 Append — 2026-09-11 수정 (문자열 슬라이싱/인덱싱 일관성 유지, List.Append가 Str 타입을 보존하도록)
        foreach (var c in Value)
            List.Append(list, From($"{c}"));
        return list;
    }

    public override Tup ToTuple() => ToList().ToTuple();

    private bool OutOfRange(int value)
    {
        if (0 > value || value >= Value.Length)
            return true;
        return false;
    }

    public override Str Copy() => new(Value);

    public override Str Clone() => new(Value);

    public override int GetHashCode() => Value.GetHashCode();

    public override string ToString() => Value.ToString();

    public override Int Hash() => Int.From(GetHashCode());

    public static Str From(string value)
    {
        if (value.Length > MAX_POOLED_STRING_LEGNTH)
            return new Str(value);
        if (pool.Count >= MAX_POOL_SIZE)
            pool.Clear();

        return pool.GetOrAdd(value, static v => new Str(v));
    }

    public static Str From(string value, bool intern)
    {
        if (value == null) return Empty;

        if (!intern) return From(value);

        if (pool.TryGetValue(value, out var cached))
            return cached;

        var result = From(value);
        pool[value] = result;

        return result;
    }

    public static Str To(Obj obj)
    {
        if (obj.As<Str>(out var str)) return str;
        if (obj.ToStr().As<Str>(out str)) return str;
        return obj.Repr();
    }

    [Native(
        Name = "is_empty",
        Description = "Checks whether a str value empty.",
        Example = "text.is_empty()",
        ReturnType = "bool"
    )]
    public static Obj IsEmpty([Self] Str self) => Bool.From(string.IsNullOrEmpty(self.Value));

    [Native(
        Name = "is_number",
        Description = "Checks whether a str value number.",
        Example = "text.is_number()",
        ReturnType = "bool"
    )]
    public static Obj IsNumber([Self] Str self) => Bool.From(self.Value.All(char.IsDigit));

    [Native(
        Name = "is_alphabet",
        Description = "Checks whether a str value alphabet.",
        Example = "text.is_alphabet()",
        ReturnType = "bool"
    )]
    public static Obj IsAlphabet([Self] Str self) => Bool.From(self.Value.All(char.IsLetter));

    [Native(
        Name = "index_of",
        Description = "Finds a value index in text.",
        Example = "text.index_of(value)",
        ReturnType = "int",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj IndexOf([Self] Str self, [ArgInfo(Essential = true)] Obj value)
    {
        if (!value.As<Str>(out var str))
            return new Err("invalid argument: value");

        return Int.From(self.Value.IndexOf(str.Value));
    }

    [Native(
        Name = "contains",
        Description = "Checks whether text contains a value.",
        Example = "text.contains(value)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Contains([Self] Str self, [ArgInfo(Essential = true)] Obj value)
    {
        if (!value.As<Str>(out var str))
            return new Err("invalid argument: value");

        return Bool.From(self.Value.Contains(str.Value));
    }

    [Native(
        Name = "starts_with",
        Description = "Returns the result of text.starts with().",
        Example = "text.starts_with(value)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj StartsWith([Self] Str self, [ArgInfo(Essential = true)] Obj value)
    {
        if (!value.As<Str>(out var str))
            return new Err("invalid argument: value");

        return Bool.From(self.Value.StartsWith(str.Value));
    }

    [Native(
        Name = "ends_with",
        Description = "Returns the result of text.ends with().",
        Example = "text.ends_with(value)",
        ReturnType = "bool",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj EndsWith([Self] Str self, [ArgInfo(Essential = true)] Obj value)
    {
        if (!value.As<Str>(out var str))
            return new Err("invalid argument: value");

        return Bool.From(self.Value.EndsWith(str.Value));
    }

    [Native(
        Name = "to_upper",
        Description = "Converts a str value to upper.",
        Example = "text.to_upper()",
        ReturnType = "str"
    )]
    public static Obj ToUpper([Self] Str self) => From(self.Value.ToUpper());

    [Native(
        Name = "to_lower",
        Description = "Converts a str value to lower.",
        Example = "text.to_lower()",
        ReturnType = "str"
    )]
    public static Obj ToLower([Self] Str self) => From(self.Value.ToLower());

    [Native(
        Name = "split",
        Description = "Returns the result of text.split().",
        Example = "text.split(sep)",
        ReturnType = "list",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Split([Self] Str self, [ArgInfo(Essential = true)] Obj sep)
    {
        if (!sep.As<Str>(out var str))
            return new Err("invalid argument: value");

        var parts = self.Value.Split(str.Value);
        return new List([.. parts.Select(From)]);
    }

    [Native(
        Name = "trim",
        Description = "Returns the result of text.trim().",
        Example = "text.trim(chars)",
        ReturnType = "str",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Trim([Self] Str self, [ArgInfo(Essential = true)] Obj chars)
    {
        if (!chars.As<Str>(out var str))
            return new Err("invalid argument: value");

        return From(self.Value.Trim(str.Value.ToCharArray()));
    }

    [Native(
        Name = "join",
        Description = "Returns the result of text.join().",
        Example = "text.join(values)",
        ReturnType = "str",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Join([Self] Str self, [ArgInfo(Essential = true)] Obj values)
    {
        if (!values.Iter().As<Iters>(out var str))
            return new Err("invalid argument: values");

        var strs = new List<string>();

        foreach (var item in str.Value)
        {
            var s = item.ToStr();
            if (s is Err err)
                return err;

            if (!s.As<Str>(out var strValue))
                return new Err($"cannot join '{item.Type}' with '{self.Type}'");

            strs.Add(strValue.Value);
        }

        return From(string.Join(self.Value, strs));
    }

    [Native(
        Name = "center",
        Description = "Returns the result of text.center().",
        Example = "text.center(width, fill)",
        ReturnType = "str",
        ArgumentTypes = new[] { "any", "any" }
    )]
    public static Obj Center(
        [Self] Str self,
        [ArgInfo(Essential = true)] Obj width,
        [ArgInfo(Optional = true)] Obj fill = null!)
    {
        if (!width.As<Int>(out var widthValue))
            return new Err("invalid argument: width");

        fill ??= From(" ");

        if (!fill.As<Str>(out var fillValue))
            return new Err("invalid argument: fill");

        var pad = Math.Max(0, widthValue.Value - self.Value.Length);
        var left = pad / 2;
        var right = pad - left;
        var fillChar = fillValue.Value.Length > 0 ? fillValue.Value[0] : ' ';
        return From(new string(fillChar, (int)left) + self.Value + new string(fillChar, (int)right));
    }

    [Native(
        Name = "left",
        Description = "Returns the result of text.left().",
        Example = "text.left(width, fill)",
        ReturnType = "str",
        ArgumentTypes = new[] { "any", "any" }
    )]
    public static Obj Left(
        [Self] Str self,
        [ArgInfo(Essential = true)] Obj width,
        [ArgInfo(Optional = true)] Obj fill = null!)
    {
        if (!width.As<Int>(out var widthValue))
            return new Err("invalid argument: width");

        fill ??= From(" ");

        if (!fill.As<Str>(out var fillValue))
            return new Err("invalid argument: fill");

        var pad = Math.Max(0, widthValue.Value - self.Value.Length);
        var fillChar = fillValue.Value.Length > 0 ? fillValue.Value[0] : ' ';
        return From(self.Value + new string(fillChar, (int)pad));
    }

    [Native(
        Name = "right",
        Description = "Returns the result of text.right().",
        Example = "text.right(width, fill)",
        ReturnType = "str",
        ArgumentTypes = new[] { "any", "any" }
    )]
    public static Obj Right(
       [Self] Str self,
       [ArgInfo(Essential = true)] Obj width,
       [ArgInfo(Optional = true)] Obj fill = null!)
    {
        if (!width.As<Int>(out var widthValue))
            return new Err("invalid argument: width");

        fill ??= From(" ");

        if (!fill.As<Str>(out var fillValue))
            return new Err("invalid argument: fill");

        var pad = Math.Max(0, widthValue.Value - self.Value.Length);
        var fillChar = fillValue.Value.Length > 0 ? fillValue.Value[0] : ' ';
        return From(new string(fillChar, (int)pad) + self.Value);
    }

    [Native(
        Name = "replace",
        Description = "Returns the result of text.replace().",
        Example = "text.replace(oldValue, newValue)",
        ReturnType = "str",
        ArgumentTypes = new[] { "any", "any" }
    )]
    public static Obj Replace(
       [Self] Str self,
       [ArgInfo(Essential = true, Name = "old_value")] Obj oldValue,
       [ArgInfo(Optional = true, Name = "new_value")] Obj newValue)
    {
        if (!oldValue.As<Str>(out var oldStr))
            return new Err("invalid argument: old_value");
        if (!newValue.As<Str>(out var newStr))
            return new Err("invalid argument: new_value");

        return From(self.Value.Replace(oldStr.Value, newStr.Value));
    }

    [Native(
        Name = "find",
        Description = "Returns the result of text.find().",
        Example = "text.find(subStr)",
        ReturnType = "int",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Find([Self] Str self, [ArgInfo(Essential = true, Name = "sub_str")] Obj subStr)
    {
        if (!subStr.As<Str>(out var subStrValue))
            return new Err("invalid argument: sub_str");

        return Int.From(self.Value.IndexOf(subStrValue.Value));
    }
}