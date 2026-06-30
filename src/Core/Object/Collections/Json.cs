using System.Text.Json;
using Un.Object.Primitive;
using Un.Object.Type;

namespace Un.Object.Collections;

public class Json : Ref<Obj>
{
    public int Count => Value switch
    {
        Dict dict => dict.Value.Count,
        List list => list.Count,
        _ => 1
    };

    public Json() : base(None, UnType.Create("json"))
    {
        
    }

    public Json(Obj obj) : base(obj, UnType.Create("json"))
    {
        
    }


    public override Obj Init(Tup args)
    {
        if (args.Count == 0)
            return new Json(None);

        if (args.Count != 1)
            return new Err($"invalid json: expected 1 argument, got {args.Count}");

        if (args[0] is Str s)
            return new Json(ToJson(s.Value));
        if (args[0] is Dict d)
            return new Json(d);
        if (args[0] is List l)
            return new Json(l);

        return new Err($"invalid json: expected string, dict, or list, got {args[0].Type}");
    }

    public override Int Len() => Int.From(Count);

    public override Obj GetItem(Obj key) => Value.GetItem(key);

    public override Obj SetItem(Obj key, Obj value) => Value.SetItem(key, value);

    public override Str ToStr() => Str.From(Stringfy(Value));

    public override Obj Clone() => new Json(Value.Clone());

    public static string Stringfy(Obj obj, int depth = 0)
    {
        if (obj is Dict dict)
        {
            string buf = "{\n";

            foreach (var (key, value) in dict.Value)
                buf += $"{new string(' ', 3 * depth + 1)}\"{key}\": {Stringfy(value, depth + 1)},\n";
            buf = buf.TrimEnd(',', '\n');
            buf += $"\n{new string(' ', 3 * depth)}}}";
            return buf;
        }

        return obj switch
        {
            Obj o when o.IsNone() => "null",
            Json json => Stringfy(json.Value, depth),
            Int i => i.Value.ToString(),
            Float f => f.Value.ToString(),
            Bool b => b.Value ? "true" : "false",
            List list => "[" + string.Join(", ", list.Value.Select(v => Stringfy(v, depth + 1))) + "]",
            _ => $"\"{obj.ToStr().As<Str>().Value}\""
        };
    }

    private Obj ToJson(string text)
    {
        using var doc = JsonDocument.Parse(text);
        return Convert(doc.RootElement);
    }

    private Obj Convert(JsonElement element) => element.ValueKind switch
    {
        JsonValueKind.Object => ConvertObject(element),
        JsonValueKind.Array => ConvertArray(element),
        JsonValueKind.String => Str.From(element.GetString()!),
        JsonValueKind.Number => element.TryGetInt64(out var i) ? Int.From(i) : new Float(element.GetDouble()),
        JsonValueKind.True => Bool.True,
        JsonValueKind.False => Bool.False,
        JsonValueKind.Null => None,
        _ => None
    };

    private Dict ConvertObject(JsonElement element)
    {
        var dict = new Dict();

        foreach (var property in element.EnumerateObject())
            dict.Value[Str.From(property.Name)] = Convert(property.Value);

        return dict;
    }

    private List ConvertArray(JsonElement element)
    {
        List list = [];

        foreach (var item in element.EnumerateArray())
            list.Add(Convert(item));

        return list;
    }
}