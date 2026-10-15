using System.Collections.Concurrent;
using System.Text.RegularExpressions;
using Un.Object;
using Un.Object.Collections;
using Un.Object.Primitive;
using Un.Reflection;

namespace Un.Native;

[NativeModule("re")]
public static class Re
{
    private const int MAX_CACHE_SIZE = 1000;

    private static readonly ConcurrentDictionary<string, Regex> cache = new();

    static bool GetString(Obj obj, out string value, out Obj err)
    {
        if (!obj.As<Str>(out var str))
        {
            value = "";
            err = new Err("expected value is string");
            return false;
        }

        value = str.Value;
        err = Obj.None;
        return true;
    }

    static Regex GetOrCompile(string pattern)
    {
        if (cache.TryGetValue(pattern, out var regex))
            return regex;

        regex = new Regex(pattern, RegexOptions.None, TimeSpan.FromSeconds(1));

        if (cache.Count >= MAX_CACHE_SIZE)
            cache.Clear(); 

        cache.TryAdd(pattern, regex);
        return regex;
    }

    static bool TryCompile(string pattern, out Regex regex, out Obj err)
    {
        try
        {
            regex = GetOrCompile(pattern);
            err = Obj.None;
            return true;
        }
        catch (ArgumentException e)
        {
            regex = null!;
            err = new Err($"invalid regex pattern: {e.Message}");
            return false;
        }
    }

    [Native(
        Name = "test",
        Description = "Checks whether text matches a pattern.",
        Example = "write(test(\"\\d+\", text))",
        ReturnType = "bool",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Test(
        [ArgInfo(Essential = true)] Obj pattern,
        [ArgInfo(Essential = true)] Obj text)
    {
        if (!GetString(pattern, out var p, out var err))
            return err;

        if (!GetString(text, out var t, out err))
            return err;

        if (!TryCompile(p, out var regex, out err))
            return err;

        return Bool.From(regex.IsMatch(t));
    }

    [Native(
        Name = "match",
        Description = "Matches a pattern at the start of text.",
        Example = "write(match(\"^UN\", text))",
        ReturnType = "any",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Match(
        [ArgInfo(Essential = true)] Obj pattern,
        [ArgInfo(Essential = true)] Obj text)
    {
        if (!GetString(pattern, out var p, out var err))
            return err;

        if (!GetString(text, out var t, out err))
            return err;

        if (!TryCompile(p, out var regex, out err))
            return err;

        var m = regex.Match(t);

        if (!m.Success || m.Index != 0)
            return Obj.None;

        return Str.From(m.Value);
    }

    [Native(
        Name = "search",
        Description = "Searches text for a pattern match.",
        Example = "write(search(\"UN\", text))",
        ReturnType = "any",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Search(
        [ArgInfo(Essential = true)] Obj pattern,
        [ArgInfo(Essential = true)] Obj text)
    {
        if (!GetString(pattern, out var p, out var err))
            return err;

        if (!GetString(text, out var t, out err))
            return err;

        if (!TryCompile(p, out var regex, out err))
            return err;

        var m = regex.Match(t);

        return m.Success ? Str.From(m.Value) : Obj.None;
    }

    [Native(
        Name = "find_all",
        Description = "Finds every regular-expression match.",
        Example = "matches = find_all(\"\\d+\", text)",
        ReturnType = "list",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj FindAll(
        [ArgInfo(Essential = true)] Obj pattern,
        [ArgInfo(Essential = true)] Obj text)
    {
        if (!GetString(pattern, out var p, out var err))
            return err;

        if (!GetString(text, out var t, out err))
            return err;

        if (!TryCompile(p, out var regex, out err))
            return err;

        return new List([
            ..regex.Matches(t).Select(m => (Obj)Str.From(m.Value))
        ]);
    }

    [Native(
        Name = "groups",
        Description = "Returns capture groups from a match.",
        Example = "write(groups(pattern, text))",
        ReturnType = "tuple",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Groups(
        [ArgInfo(Essential = true)] Obj pattern,
        [ArgInfo(Essential = true)] Obj text)
    {
        if (!GetString(pattern, out var p, out var err))
            return err;

        if (!GetString(text, out var t, out err))
            return err;

        if (!TryCompile(p, out var regex, out err))
            return err;

        var m = regex.Match(t);

        if (!m.Success)
            return Obj.None;

        var list = new System.Collections.Generic.List<Obj>();

        for (int i = 1; i < m.Groups.Count; i++)
        {
            var group = m.Groups[i];
            list.Add(group.Success ? Str.From(group.Value) : Obj.None);
        }

        return new List([.. list]);
    }

    [Native(
        Name = "replace",
        Description = "Replaces the first pattern match.",
        Example = "result = replace(\"cat\", text, \"dog\")",
        ReturnType = "str",
        ArgumentTypes = new[] { "str", "str", "str" }
    )]
    public static Obj Replace(
        [ArgInfo(Essential = true)] Obj pattern,
        [ArgInfo(Essential = true)] Obj text,
        [ArgInfo(Essential = true)] Obj replacement)
    {
        if (!GetString(pattern, out var p, out var err))
            return err;

        if (!GetString(text, out var t, out err))
            return err;

        if (!GetString(replacement, out var r, out err))
            return err;

        if (!TryCompile(p, out var regex, out err))
            return err;

        return Str.From(regex.Replace(t, r, 1));
    }

    [Native(
        Name = "replace_all",
        Description = "Replaces every pattern match.",
        Example = "result = replace_all(\"\\s+\", text, \" \")",
        ReturnType = "str",
        ArgumentTypes = new[] { "str", "str", "str" }
    )]
    public static Obj ReplaceAll(
        [ArgInfo(Essential = true)] Obj pattern,
        [ArgInfo(Essential = true)] Obj text,
        [ArgInfo(Essential = true)] Obj replacement)
    {
        if (!GetString(pattern, out var p, out var err))
            return err;

        if (!GetString(text, out var t, out err))
            return err;

        if (!GetString(replacement, out var r, out err))
            return err;

        if (!TryCompile(p, out var regex, out err))
            return err;

        return Str.From(regex.Replace(t, r));
    }

    [Native(
        Name = "split",
        Description = "Splits text on a regular-expression pattern.",
        Example = "parts = split(\",\\s*\", text)",
        ReturnType = "list",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Split(
        [ArgInfo(Essential = true)] Obj pattern,
        [ArgInfo(Essential = true)] Obj text)
    {
        if (!GetString(pattern, out var p, out var err))
            return err;

        if (!GetString(text, out var t, out err))
            return err;

        if (!TryCompile(p, out var regex, out err))
            return err;

        return new List([
            ..regex.Split(t).Select(Str.From)
        ]);
    }

    [Native(
        Name = "escape",
        Description = "Escapes text for a regular expression.",
        Example = "write(escape(\"a+b\"))",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Escape([ArgInfo(Essential = true)] Obj text)
    {
        if (!GetString(text, out var t, out var err))
            return err;

        return Str.From(Regex.Escape(t));
    }
}
