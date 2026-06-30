using Un.Object;

namespace Un;

public class Scope(Scope? parentScope = null)
{
    public static readonly Scope Empty = new Scope(null);

    private readonly Scope? parentScope = parentScope;

    private readonly Dictionary<string, int> symbols = new();

    private readonly List<Obj> slots = new();

    public Obj this[string key]
    {
        get => Get(key);
        set => Set(key, value);
    }

    public bool Get(string key, out Obj value)
    {
        if (symbols.TryGetValue(key, out var idx))
        {
            value = slots[idx];
            return true;
        }

        if (parentScope != null)
            return parentScope.Get(key, out value);

        value = null!;
        return false;
    }

    public Obj Get(string key)
    {
        if (symbols.TryGetValue(key, out var idx))
            return slots[idx];

        if (parentScope != null)
            return parentScope.Get(key);

        return new Err($"variable '{key}' not found");
    }

    public void Set(string key, Obj value)
    {
        if (symbols.TryGetValue(key, out var idx))
        {
            slots[idx] = value;
            return;
        }

        if (parentScope != null && parentScope.ContainsKey(key))
        {
            parentScope.Set(key, value);
            return;
        }

        symbols[key] = slots.Count;
        slots.Add(value);
    }

    public bool ContainsKey(string key)
    {
        if (symbols.ContainsKey(key))
            return true;

        return parentScope != null && parentScope.ContainsKey(key);
    }

    public bool ContainsKeyInTop(string key) => symbols.ContainsKey(key);

    public void Declare(string key, Obj? initial = null)
    {
        if (symbols.ContainsKey(key))
            return;

        symbols[key] = slots.Count;
        slots.Add(initial ?? Obj.None);
    } 

    public bool Remove(string key) => symbols.Remove(key);

    public IReadOnlyDictionary<string, int> GetSymbolTable() => symbols;
    public IReadOnlyList<Obj> GetSlots() => slots;
}