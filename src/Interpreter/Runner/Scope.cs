using Un.Object;

namespace Un;

public class Scope
{
    public static readonly Scope Empty = new Scope(null);

    private readonly Scope? parentScope;

    // 🔥 name → slot index
    private readonly Dictionary<string, int> symbols = new();

    // 🔥 slot index → Obj
    private readonly List<Obj> slots = new();

    public Scope(Scope? parentScope = null)
    {
        this.parentScope = parentScope;
    }

    public Obj this[string key]
    {
        get => Get(key);
        set => Set(key, value);
    }

    // 🔥 TryGet
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

    // 🔥 Get
    public Obj Get(string key)
    {
        if (symbols.TryGetValue(key, out var idx))
            return slots[idx];

        if (parentScope != null)
            return parentScope.Get(key);

        return new Err($"variable '{key}' not found");
    }

    // 🔥 Set (기존 동작 그대로 유지)
    public void Set(string key, Obj value)
    {
        // 현재 scope에 있으면 수정
        if (symbols.TryGetValue(key, out var idx))
        {
            slots[idx] = value;
            return;
        }

        // 부모에 있으면 부모에 설정
        if (parentScope != null && parentScope.ContainsKey(key))
        {
            parentScope.Set(key, value);
            return;
        }

        // 없으면 현재 scope에 새 슬롯 생성
        symbols[key] = slots.Count;
        slots.Add(value);
    }

    public bool ContainsKey(string key)
    {
        if (symbols.ContainsKey(key))
            return true;

        return parentScope != null && parentScope.ContainsKey(key);
    }

    public bool ContainsKeyInTop(string key)
        => symbols.ContainsKey(key);

    // 🔥 명시적 선언 (선택적 사용)
    public void Declare(string key, Obj? initial = null)
    {
        if (symbols.ContainsKey(key))
            return;

        symbols[key] = slots.Count;
        slots.Add(initial ?? Obj.None);
    }

    // 디버깅용
    public IReadOnlyDictionary<string, int> GetSymbolTable() => symbols;
    public IReadOnlyList<Obj> GetSlots() => slots;
}