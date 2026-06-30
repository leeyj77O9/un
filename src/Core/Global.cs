global using Attributes = System.Collections.Generic.Dictionary<string, Un.Object.Obj>;
global using Map = System.Collections.Generic.Dictionary<string, Un.Object.Obj>;

using Un.Object;
using Un.Object.Flow;
using Un.Object.Util;
using Un.Object.Iter;
using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Collections;
using Un.Object.Type;

namespace Un;

public static class Global
{
    public static string PATH { get; private set; } = "";

    public static ulong MAXRECURSIONDEPTH = 1000;

    private static readonly Scope scope = new();
    private static readonly Scope classes = new();

    public static Attributes Package { get; private set; } = [];

    public static void Init(string path)
    {
        PATH = path;

        var std = new Std();

        InitTypeByName<Int>();
        InitTypeByName<Float>();
        InitTypeByName<Bool>();
        InitTypeByName<Str>();
        InitTypeByName<Date>();
        InitTypeByName<List>();
        InitTypeByName<Tup>("tuple");
        InitTypeByName<Set>();
        InitTypeByName<Dict>();
        InitTypeByName<Iters>("iter");
        InitTypeByName<Future>();
        InitTypeByName<Time>();
        InitTypeByName<Object.Util.Timer>();
        InitTypeByName<Flow>();
        InitTypeByName<Json>();
        InitTypeByName<Counter>();
        InitTypeByName<Reverse>();

        scope.Set("__name__", Str.From("__main__"));

        foreach (var (key, value) in std.GetOriginalMembers())
            scope.Set(key, value);

        foreach (var (key, value) in std.GetOriginalMethods())
            scope.Set(key, value);
    }

    public static void InitTypeFromInstance<T>(T obj)
        where T : Obj, new()
    {
        var name = obj.Type.Name;

        classes[name].Members = obj.GetOriginal();

        if (obj is IPack pack)
        {
            foreach (var (key, value) in pack.GetOriginalMembers())
                scope.Set(key, value);

            var group = pack.GetOriginalMethods();

            if (group.Count != 0)
                scope.Set(name, new Obj(UnType.Create(name))
                {
                    Members = group,
                });
        }
    }

    public static void InitTypeByName<T>(string name = null!)
        where T : Obj, new()
    {
        var t = new T();

        classes.Set(name ?? typeof(T).Name.ToLower(), new T
        {
            Members = t.GetOriginal()
        });
    }

    public static void Include(string name)
    {
        InitTypeFromInstance(classes[name]);
    }

    public static void Import(string[] path, string nickname, string[] parts)
    {
        var map = Load(Path.Combine(PATH, string.Join('/', path)));

        if (parts.Length != 0)
        {
            var set = parts.ToHashSet();
            map = map.Where(x => set.Contains(x.Key)).ToDictionary();
        }

        if (nickname == "*")
        {
            ImportSpread(map);
            return;
        }

        if (!string.IsNullOrEmpty(nickname))
        {
            ImportAlias(map, nickname);
            return;
        }

        ImportNamespace(path, map);
    }

    private static void ImportSpread(Map map)
    {
        foreach (var (key, value) in map)
            if (value is Obj obj)
                scope.Set(key, obj);
    }

    private static void ImportAlias(Map map, string nickname)
    {
        if (scope.ContainsKey(nickname))
            throw new Panic($"'{nickname}' already exists in the global scope");

        scope.Set(nickname, new Obj(UnType.Create(nickname))
        {
            Members = map
        });
    }

    private static void ImportNamespace(string[] path, Map map)
    {
        var scope = GetGlobalScope();

        Obj top;

        if (scope.Get(path[0], out var existing))
            top = existing;
        else
        {
            top = new Obj(UnType.Create(path[0]));
            scope.Set(path[0], top);
        }

        for (int i = 1; i < path.Length; i++)
        {
            if (!top.Members.TryGetValue(path[i], out var child))
            {
                child = new Obj(UnType.Create(path[i]));
                top.Members[path[i]] = child;
            }

            top = child;
        }

        top.Members = map;
    }

    private static Map Load(string fullPath)
    {
        Map map = [];

        if (Directory.Exists(fullPath))
        {
            foreach (var file in Directory.GetFiles(fullPath, "*.un"))
                LoadFile(file);

            return map;
        }

        var filePath = fullPath.EndsWith(".un") ? fullPath : fullPath + ".un";

        if (!File.Exists(filePath))
            throw new Panic($"file or directory '{fullPath}' not found");

        LoadFile(filePath);

        return map;

        void LoadFile(string file)
        {
            var inner = new Scope(GetGlobalScope());

            Runner.Load(file, inner).Run();

            var symbols = inner.GetSymbolTable();
            var slots = inner.GetSlots();

            foreach (var (key, index) in symbols)
            {
                if (slots[index] is Obj obj)
                    map[key] = obj;
            }
        }
    }

    public static bool IsClass(string name) => classes.ContainsKey(name);

    public static Obj GetClass(string name)
    {
        if (classes.Get(name, out var obj))
            return obj;

        return new Err($"class '{name}' not found");
    }

    public static Obj GetClass(TObj type) => GetClass(type.Value);

    public static Obj GetClass(BaseType type)
    {
        if (type is UnType unType)        
            return GetClass(unType.Name);        
        else if (type is CollectionType colType)
            return GetClass(colType.Kind);

        return new Err($"type '{type}' is not a class");
    }

    public static bool TryGetClass(string name, out Obj obj)
    {
        if (classes.Get(name, out obj))
            return true;

        obj = null!;
        return false;
    }

    public static void SetClass(string name, Obj obj)
    {
        classes[name] = obj;
    }


    public static bool TryGetOriginalValue(string type, string name, out Obj? value)
    {
        if (classes.Get(type, out var original))
            return original.Members.TryGetValue(name, out value);

        value = null!;
        return false;
    }


    public static Obj GetGlobalVariable(string name) => scope.Get(name, out var value) ? value : new Err($"global variable '{name}' not found");

    public static void SetGlobalVariable(string name, Obj obj) => scope.Set(name, obj);

    public static bool TryGetGlobalVariable(string name, out Obj obj) => scope.Get(name, out obj!);


    public static Scope GetGlobalScope() => scope;


    public static Map New(this Map map)
    {
        Map newMap = [];
        foreach (var (key, value) in map)
            if (value is Obj obj)
                newMap[key] = obj.Clone();
        return newMap;
    }
}