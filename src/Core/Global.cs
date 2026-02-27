global using Attributes = System.Collections.Generic.Dictionary<string, Un.Object.Obj>;
global using Map = System.Collections.Generic.Dictionary<string, Un.Object.Obj>;
global using IMap = System.Collections.Generic.IDictionary<string, Un.Object.Obj>;
global using OMap = System.Collections.Specialized.OrderedDictionary;

using Un.Object;
using Un.Object.IO;
using Un.Object.Flow;
using Un.Object.Util;
using Un.Object.Iter;
using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Collections;
using System.Collections.Concurrent;
using System.Collections.Specialized;

namespace Un;

public static class Global
{
    public static string PATH { get; private set; } = "";

    public static readonly int BASEHASH = Math.Abs(DateTime.Now.Millisecond * 6929891 + DateTime.Now.Second * 1025957);
    public static readonly int HASHPRIME = 11;
    public static ulong MAXRECURSIONDEPTH = 1000;

    public static int CallDepth = 0;

    private static Scope scope = new(new ConcurrentDictionary<string, Obj>(), null!);
    private static ConcurrentDictionary<string, Obj> classes = new();

    static Global()
    {
        classes["time"] = new Time();
        classes["timer"] = new Object.Util.Timer(); 
        classes["flow"] = new Flow();
        classes["json"] = new Json(Obj.None);
        classes["counter"] = new Counter();
        classes["reverse"] = new Reverse([]);
    }

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

        scope.Set("__name__", new Str("__main__"));

        foreach (var (key, value) in std.GetOriginalMembers())
            scope.Set(key, value);

        foreach (var (key, value) in std.GetOriginalMethods())
            scope.Set(key, value);
    }

    public static void InitTypeFromInstance<T>(T obj)
        where T : Obj, new()
    {
        var name = obj.Type;

        classes[name].Members = obj.GetOriginal();

        if (obj is IPack pack)
        {
            foreach (var (key, value) in pack.GetOriginalMembers())
                scope.Set(key, value);

            var group = pack.GetOriginalMethods();

            if (group.Count != 0)
                scope.Set(name, new Obj(name)
                {
                    Members = group,
                });
        }
    }

    public static void InitTypeByName<T>(string name = null!)
        where T : Obj, new()
    {
        var t = new T();

        classes.TryAdd(name ?? typeof(T).Name.ToLower(), new T
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
        var fullPath = Path.Combine(Global.PATH, Path.Join(path));
        bool isSpread = nickname == "*";
        bool isNickname = !string.IsNullOrEmpty(nickname);

        if (isSpread || isNickname)
        {
            var map = Merge();
            var nameSet = parts.ToHashSet();

            map = nameSet.Count != 0 ? map.Where(x => nameSet.Contains(x.Key)).ToDictionary() : map;

            if (isSpread)
            {
                foreach (var (key, value) in map)               
                    if (value is Obj obj)
                        scope.Set(key, obj);
            }
            else
            {
                if (scope.ContainsKey(nickname))
                    throw new Panic($"'{nickname}' already exists in the global scope");

                var obj = new Obj(nickname)
                {
                    Members = map
                };
                scope.Set(nickname, obj);
            }
        }
        else
        {
            var scope = GetGlobalScope();
            Obj top = scope.Get(path[0], out var existing) ? existing : new Obj(path[0]);
            
            if (existing == null)
                scope.Set(path[0], top);            

            for (int i = 1; i < path.Length; i++)
            {
                if (top.Members.TryGetValue(path[i], out var existingObj))
                    top = existingObj;
                else
                {
                    Obj obj = new(path[i]);
                    top.Members[path[i]] = obj;
                    top = obj;
                }
            }

            var nameSet = parts.ToHashSet();

            top.Members = Merge();
            top.Members = nameSet.Count != 0 ? top.Members.Where(x => nameSet.Contains(x.Key)).ToDictionary() : top.Members;
        }

        Map Merge()
        {
            var topMap = new Map();
            if (Directory.Exists(fullPath))
            {
                foreach (var file in Directory.GetFiles(fullPath, "*.un"))
                {
                    var map = new Map();
                    var inner = new Scope(map, GetGlobalScope());

                    Runner.Load(file, inner).Run();

                    foreach (var (key, value) in map)
                        if (value is Obj obj)
                            topMap.Add(key, obj);
                }
            }
            else if (File.Exists(fullPath.EndsWith(".un") ? fullPath : fullPath + ".un"))
            {
                var map = new Map();
                var inner = new Scope(map, GetGlobalScope());

                Runner.Load(fullPath.EndsWith(".un") ? fullPath : fullPath + ".un", inner).Run();

                foreach (var (key, value) in map)
                    if (value is Obj obj)
                        topMap.Add(key, obj);
            }
            else
            {
                throw new Panic($"file or directory '{fullPath}' not found");
            }

            return topMap;
        }
    }

    public static bool IsClass(string name) => classes.ContainsKey(name);

    public static Obj GetClass(string name)
    {
        if (classes.TryGetValue(name, out var obj))
            return obj;

        throw new Panic($"class '{name}' not found");
    }

    public static bool TryGetClass(string name, out Obj? obj)
    {
        if (classes.TryGetValue(name, out obj))
            return true;

        obj = null;
        return false;
    }

    public static void SetClass(string name, Obj obj)
    {
        classes[name] = obj;
    }


    public static bool TryGetOriginalValue(string type, string name, out Obj? value)
    {
        if (classes.TryGetValue(type, out var original))
            return original.Members.TryGetValue(name, out value);

        value = null!;
        return false;
    }


    public static Obj GetGlobalVariable(string name) => scope.Get(name, out var value) ? value : throw new Panic($"global variable '{name}' not found");

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