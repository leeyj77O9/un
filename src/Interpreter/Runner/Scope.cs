using Un.Object;

namespace Un;

public class Scope(IMap scope, Scope parentscope = null!)
{
    public static readonly Scope Empty = new(new Map(), null!);

    private readonly Scope parentScope = parentscope ?? Empty;
    private readonly IMap scope = scope;

    public Obj this[string key]
    {
        get => Get(key);
        set => Set(key, value);
    }

    public bool Get(string key, out Obj value)
    {
        if (scope.TryGetValue(key, out value!))
            return true;
        if (parentScope != null && parentScope.Get(key, out value))
            return true;
        return false;
    }

    public Obj Get(string key)
    {
        if (scope.TryGetValue(key, out var obj))
            return obj;
        if (parentScope.ContainsKey(key))
            return parentScope.Get(key);
        return new Err("variable not found");
    }


    public void Set(string key, Obj value)
    {
        if (scope.ContainsKey(key))
            scope[key] = value;
        else if (parentScope.ContainsKey(key))
            parentScope.Set(key, value);
        else
            scope[key] = value;
    }

    public bool ContainsKey(string key) => scope.ContainsKey(key) || (parentScope != null && parentScope.ContainsKey(key));

    public bool ContainsKeyInTop(string key) => scope.ContainsKey(key);

    public IMap GetScope() => scope;
}