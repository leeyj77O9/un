using Un.Object;
using Un.Object.Primitive;

using Un.Reflection;

namespace Un.Native;

[BuiltinModule("builtin")]
public static class Builtin
{
    [Native(
        Name = "len",
        Description = "Returns the number of items in a collection.",
        Example = "write(len(items))",
        ReturnType = "int",
        ArgumentTypes = new[] { "collection" }
    )]
    public static Obj Len([ArgInfo(Essential = true)] Obj value) => value.Len();

    [Native(
        Name = "exit",
        Description = "Stops the current UN program with an exit code.",
        Example = "exit(0)",
        ReturnType = "none",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Exit([ArgInfo(Optional = true)] Obj code)
    {
        Environment.Exit(code.As<Int>(out var codeObj) ? (int)codeObj.Value : 0);
        return Obj.None;
    }

    [Native(
        Name = "type",
        Description = "Returns the UN type of a value.",
        Example = "write(type(value))",
        ReturnType = "type",
        ArgumentTypes = new[] { "any" }
    )]
    public static TObj Type([ArgInfo(Essential = true)] Obj value) => new(value.Type);

    [Native(
        Name = "hash",
        Description = "Returns a hash value for an object.",
        Example = "write(hash(value))",
        ReturnType = "int",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Hash([ArgInfo(Essential = true)] Obj value) => value.Hash();

    [Native(
        Name = "throw",
        Description = "Raises a named runtime error.",
        Example = "throw(\"invalid state\")",
        ReturnType = "error",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Throw(
        [ArgInfo(Essential = true)] Obj message,
        [ArgInfo(Optional = true)] Obj name = null!)
    {
        if (!message.ToStr().As<Str>(out var messageValue))
            return new Err("cannot convert argument 'message' to a str");

        name ??= Str.From("error");

        if (!name.ToStr().As<Str>(out var nameValue))
            return new Err("cannot convert argument 'name' to a str");

        return new Err(messageValue.Value, nameValue.Value);
    }

    [Native(
        Name = "throw_if",
        Description = "Raises a named runtime error if the condition is true.",
        Example = "throw_if(condition, \"invalid state\")",
        ReturnType = "error",
        ArgumentTypes = new[] { "bool", "str", "str" }
    )]
    public static Obj ThrowIf(
        [ArgInfo(Essential = true)] Obj condition, 
        [ArgInfo(Essential = true)] Obj message,
        [ArgInfo(Optional = true)] Obj name = null!)
    {
        if (!condition.As<Bool>(out var cond))
            return new Err("throw_if: expected bool condition");

        if (cond.Value)
            return Throw(message, name);

        return Obj.None;
    }

    [Native(
        Name = "throw_unless",
        Description = "Raises a named runtime error if the condition is false.",
        Example = "throw_unless(condition, \"invalid state\")",
        ReturnType = "error",
        ArgumentTypes = new[] { "bool", "str", "str" }
    )]
    public static Obj ThrowUnless(
        [ArgInfo(Essential = true)] Obj condition,
        [ArgInfo(Essential = true)] Obj message,
        [ArgInfo(Optional = true)] Obj name = null!)
    {
        if (!condition.As<Bool>(out var cond))
            return new Err("throw_unless: expected bool condition");

        if (!cond.Value)
            return Throw(message, name);

        return Obj.None;
    }

    [Native(
        Name = "chr",
        Description = "Converts an ASCII/Unicode code point to a single-character string.",
        Example = "chr(65) // \"A\"",
        ReturnType = "str",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Chr([ArgInfo(Essential = true)] Obj code)
    {
        if (!code.As<Int>(out var i))
            return new Err("chr: expected int code");
        if (i.Value < 0 || i.Value > 0x10FFFF)
            return new Err("chr: code point out of range (0..1114111)");
        return Str.From(char.ConvertFromUtf32((int)i.Value));
    }

    [Native(
        Name = "ord",
        Description = "Converts a single-character string to its ASCII/Unicode code point.",
        Example = "ord(\"A\") // 65",
        ReturnType = "int",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Ord([ArgInfo(Essential = true)] Obj ch)
    {
        if (!ch.As<Str>(out var s))
            return new Err("ord: expected str");
        if (s.Value.Length == 0)
            return new Err("ord: expected a single character, got empty string");
        return Int.From(char.ConvertToUtf32(s.Value, 0));
    }

    [Native(
        Name = "bin",
        Description = "Converts an integer to a binary string with '0b' prefix.",
        Example = "bin(10) // \"0b1010\"",
        ReturnType = "str",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Bin([ArgInfo(Essential = true)] Obj value)
    {
        if (!value.As<Int>(out var i))
            return new Err("bin: expected int");
        var v = i.Value;
        if (v < 0) return Str.From("-0b" + Convert.ToString(-v, 2));
        return Str.From("0b" + Convert.ToString(v, 2));
    }

    [Native(
        Name = "oct",
        Description = "Converts an integer to an octal string with '0o' prefix.",
        Example = "oct(10) // \"0o12\"",
        ReturnType = "str",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Oct([ArgInfo(Essential = true)] Obj value)
    {
        if (!value.As<Int>(out var i))
            return new Err("oct: expected int");
        var v = i.Value;
        if (v < 0) return Str.From("-0o" + Convert.ToString(-v, 8));
        return Str.From("0o" + Convert.ToString(v, 8));
    }

    [Native(
        Name = "hex",
        Description = "Converts an integer to a hexadecimal string with '0x' prefix.",
        Example = "hex(255) // \"0xff\"",
        ReturnType = "str",
        ArgumentTypes = new[] { "int" }
    )]
    public static Obj Hex([ArgInfo(Essential = true)] Obj value)
    {
        if (!value.As<Int>(out var i))
            return new Err("hex: expected int");
        var v = i.Value;
        if (v < 0) return Str.From("-0x" + (-v).ToString("x"));
        return Str.From("0x" + v.ToString("x"));
    }
}
