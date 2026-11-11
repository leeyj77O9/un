global using Attributes = System.Collections.Generic.Dictionary<string, Un.Object.Obj>;
global using Map = System.Collections.Generic.Dictionary<string, Un.Object.Obj>;

using System.Reflection;
using Un.Object;
using Un.Object.Function;
using Un.Object.Primitive;
using Un.Object.Type;
using Un.Reflection;

namespace Un;

public static class Global
{
    public static string PATH { get; private set; } = "";

    public static ulong MAXRECURSIONDEPTH = 1000;

    public static IFileSystem FileSystem { get; set; } = new PhysicalFileSystem();

    public static bool AllowProcessExit { get; set; } = true;

    public static void SetPath(string path) => PATH = path;

    private static HashSet<string>? _allowedNatives = null;

    public static void SetAllowedModules(string[]? allow) => _allowedNatives = allow == null ? null : new HashSet<string>(allow, StringComparer.Ordinal);

    public static string[]? GetAllowedModules() => _allowedNatives?.ToArray();

    public static bool IsAllowed(string name) => _allowedNatives == null || _allowedNatives.Contains(name);

    private static readonly Scope scope = new();
    private static readonly Scope classes = new();
    private static readonly Dictionary<string, Attributes> originalClasses = [];
    private static readonly Dictionary<string, string> originalBaseTypes = [];
    private static readonly Dictionary<string, Type> natives = Assembly.GetExecutingAssembly().GetTypes()!
                                                      .Where(t => t.GetCustomAttribute<NativeModuleAttribute>() is not null)
                                                      .ToDictionary(t => t.GetCustomAttribute<NativeModuleAttribute>()!.Name, t => t);
    private static readonly HashSet<string> _loading = [];
    private static readonly Dictionary<string, Map> _moduleCache = [];

    public static void Init(string path)
    {
        PATH = path;

        Builtin();

        scope.Set("__name__", Str.From("__main__"));
    }

    private static void Builtin()
    {
        var allTypes = Assembly.GetExecutingAssembly().GetTypes()!;

        var functions = allTypes.Where(t => t.GetCustomAttribute<BuiltinModuleAttribute>() is not null);

        foreach (var type in functions)
        {
            var module = type.GetCustomAttribute<BuiltinModuleAttribute>()!;

            foreach (var (name, fn) in CreateMethod(type))
                scope.Set(name, fn);
        }

        var primitives = allTypes.Where(t => t.GetCustomAttribute<BuiltinTypeAttribute>() is not null);

        foreach (var type in primitives)
        {
            var attr = type.GetCustomAttribute<BuiltinTypeAttribute>()!;
            Obj instance = (Obj)Activator.CreateInstance(type)!;
            
            originalClasses[attr.Name] = CreateMethod(type);
            classes.Set(attr.Name, instance);
        }

        var nativeTypes = allTypes.Where(t => t.GetCustomAttribute<NativeTypeAttribute>() is not null);

        foreach (var type in nativeTypes)
        {
            var attr = type.GetCustomAttribute<NativeTypeAttribute>()!;
            if (attr.Name is not null && !originalClasses.ContainsKey(attr.Name))
                originalClasses[attr.Name] = CreateMethod(type);
        }

        foreach (var type in allTypes)
        {
            var name = type.GetCustomAttribute<NativeTypeAttribute>()?.Name
                       ?? type.GetCustomAttribute<BuiltinTypeAttribute>()?.Name;

            if (name is null)
                continue;

            for (var baseType = type.BaseType; baseType is not null; baseType = baseType.BaseType)
            {
                var baseName = baseType.GetCustomAttribute<NativeTypeAttribute>()?.Name
                               ?? baseType.GetCustomAttribute<BuiltinTypeAttribute>()?.Name;

                if (baseName is null || baseName == name)
                    continue;

                originalBaseTypes[name] = baseName;
                break;
            }
        }
    }

    public static void Include(string name, string? moduleAlias = null, IReadOnlyList<(string Name, string Alias)>? imports = null)
    {
        if (!IsAllowed(name))
            throw new Panic($"module '{name}' is not allowed in this environment");

        var map = BuildNativeMap(name);

        ImportMap(map, name, name, moduleAlias, imports);
    }

    public static void Import(string[] path, Source currentSource, string? moduleAlias = null, IReadOnlyList<(string Name, string Alias)>? imports = null)
    {
        string fullPath;
        var currentDir = Path.GetDirectoryName(currentSource.Path);
        if (currentDir != null)
        {
            var rel = Path.Combine(currentDir, Path.Combine(path));
            if (FileSystem.FileExists(rel + ".un") || FileSystem.FileExists(Path.Combine(rel, "mod.un")) || FileSystem.DirectoryExists(rel))
                fullPath = rel;
            else
                fullPath = Path.Combine(PATH, Path.Combine(path));
        }
        else
        {
            fullPath = Path.Combine(PATH, Path.Combine(path));
        }

        var map = Load(fullPath);

        ImportMap(map, string.Join('.', path), path[^1], moduleAlias, imports);
    }

    public static void Import(string[] path, string? moduleAlias = null, IReadOnlyList<(string Name, string Alias)>? imports = null)
    {
        var map = Load(Path.Combine(PATH, Path.Combine(path)));

        ImportMap(map, string.Join('.', path), path[^1], moduleAlias, imports);
    }

    private static void ImportMap(Map map, string moduleName, string moduleObjectName, string? moduleAlias, IReadOnlyList<(string Name, string Alias)>? imports)
    {
        const string Wildcard = "*";

        if (imports != null)
        {
            if (moduleAlias != null)
            {
                if (scope.ContainsKey(moduleAlias))
                {
                    var existingModule = new Obj(UnType.Create(moduleAlias));
                    foreach (var (name, alias) in imports)
                    {
                        if (name == Wildcard)
                        {
                            foreach (var (k, v) in map) existingModule.Members[k] = v;
                            continue;
                        }
                        if (!map.TryGetValue(name, out var value))
                            throw new Panic($"module '{moduleName}' has no member '{name}'");
                        existingModule.Members[alias] = value;
                    }
                    scope.Set(moduleAlias, existingModule);
                    return;
                }

                var module = new Obj(UnType.Module);

                foreach (var (name, alias) in imports)
                {
                    if (name == Wildcard)
                    {
                        foreach (var (k, v) in map)
                        {
                            if (!module.Members.TryAdd(k, v))
                                throw new Panic($"'{k}' already exists in module '{moduleAlias}'");
                        }

                        continue;
                    }

                    if (!map.TryGetValue(name, out var value))
                        throw new Panic($"module '{moduleName}' has no member '{name}'");

                    if (!module.Members.TryAdd(alias, value))
                        throw new Panic($"'{alias}' already exists in module '{moduleAlias}'");
                }

                scope.Set(moduleAlias, module);
                return;
            }

            foreach (var (name, alias) in imports)
            {
                if (name == Wildcard)
                {
                    foreach (var (k, v) in map)
                    {
                        scope.Set(k, v);
                    }

                    continue;
                }

                if (!map.TryGetValue(name, out var value))
                    throw new Panic($"module '{moduleName}' has no member '{name}'");

                scope.Set(alias, value);
            }

            return;
        }

        var objectName = moduleAlias ?? moduleObjectName;

        if (scope.ContainsKey(objectName))
        {
            scope.Set(objectName, new Obj(UnType.Module)
            {
                Members = new(map)
            });
            return;
        }

        scope.Set(objectName, new Obj(UnType.Module)
        {
            Members = new(map)
        });
    }

    private static Map BuildNativeMap(string name)
    {
        var type = natives.GetValueOrDefault(name) ?? throw new Panic($"native module '{name}' not found");

        foreach (var _class in type.GetCustomAttribute<NativeModuleAttribute>()!.Types)
        {
            var className = _class.GetCustomAttribute<NativeTypeAttribute>()!.Name!;
            if (originalClasses.ContainsKey(className)) continue;
            if (natives.TryGetValue(className, out var t))
                originalClasses[className] = BuildNativeMap(className);
            else
                originalClasses[className] = CreateMethod(_class);
        }

        return CreateMethod(type);
    }

    private static Map Load(string fullPath)
    {
        var cacheKey = Path.GetFullPath(fullPath);

        if (_moduleCache.TryGetValue(cacheKey, out var cached))
            return cached.New();

        if (!_loading.Add(cacheKey))
            throw new Panic($"circular import detected: '{fullPath}'");

        try
        {
            Map map = [];

            var filePath = fullPath.EndsWith(".un") ? fullPath : fullPath + ".un";

            if (FileSystem.FileExists(filePath))
            {
                LoadFile(filePath);
                _moduleCache[cacheKey] = map.New();
                return map;
            }
      
            if (FileSystem.DirectoryExists(fullPath))
            {
                var mod = Path.Combine(fullPath, "mod.un");

                if (FileSystem.FileExists(mod))
                {
                    LoadFile(mod);
                    _moduleCache[cacheKey] = map.New();
                    return map;
                }

                foreach (var file in FileSystem.GetFiles(fullPath, "*.un"))
                    LoadFile(file);

                _moduleCache[cacheKey] = map.New();
                return map;
            }

            throw new Panic($"module '{fullPath}' not found");

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
        finally
        {
            _loading.Remove(cacheKey);
        }
    }

    private static Map CreateMethod(Type type)
    {
        var map = new Map();

        foreach (var method in type.GetMethods(BindingFlags.Public | BindingFlags.Static))
        {
            var native = method.GetCustomAttribute<NativeAttribute>();
            if (native is null)
                continue;

            var parameters = method.GetParameters();
            var isAsyncTask = method.ReturnType == typeof(Task<Obj>);

            var fn = new NFn
            {
                Name = native.Name ?? method.Name,
                Func = args =>
                {
                    var values = new object?[parameters.Length];

                    for (int i = 0; i < parameters.Length; i++)
                    {
                        var p = parameters[i];
                        if (p.GetCustomAttribute<SelfAttribute>() is not null)
                        {
                            values[i] = args["self"];
                            continue;
                        }
                        var info = p.GetCustomAttribute<ArgInfoAttribute>();
                        var argName = info?.Name ?? p.Name!;
                        values[i] = args[argName];
                        if (values[i] is Err)
                            values[i] = args[p.Name!];
                    }

                    if (isAsyncTask)
                        return ((Task<Obj>)Invoke()!).GetAwaiter().GetResult();

                    return (Obj)Invoke()!;

                    object? Invoke()
                    {
                        try
                        {
                            return method.Invoke(null, values);
                        }
                        catch (TargetInvocationException ex) when (ex.InnerException is not null)
                        {
                            throw ex.InnerException;
                        }
                    }
                }
            };

            foreach (var parameter in parameters)
            {
                if (parameter.GetCustomAttribute<SelfAttribute>() is not null)
                    continue;

                var info = parameter.GetCustomAttribute<ArgInfoAttribute>();

                fn.Args.Add(new Arg(info?.Name ?? parameter.Name!)
                {
                    IsEssential = info?.Essential ?? false,
                    IsOptional = info?.Optional ?? false,
                    IsPositional = info?.Positional ?? false,
                });
            }

            map.Add(fn.Name, fn);

        }

        return map;
    }

    public static bool IsNative(string name) => natives.ContainsKey(name);

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

    public static bool TryGetClass(BaseType type, out Obj obj)
    {
        if (type is UnType unType)
            return classes.Get(unType.Name, out obj);
        else if (type is CollectionType colType)
            return classes.Get(colType.Kind.Name, out obj);

        obj = null!;
        return false;
    }

    public static void SetClass(string name, Obj obj)
    {
        classes[name] = obj;
    }

    public static bool TryGetOriginalValue(string type, string name, out Obj? value)
    {
        while (true)
        {
            if (originalClasses.TryGetValue(type, out var original) && original.TryGetValue(name, out value))
                return true;

            if (!originalBaseTypes.TryGetValue(type, out var baseType))
            {
                value = null!;
                return false;
            }

            type = baseType;
        }
    }

    public static bool TryGetOriginalClass(string typeName, out Attributes? attrs)
    {
        if (originalClasses.TryGetValue(typeName, out var original))
        {
            attrs = original;
            return true;
        }

        attrs = null;
        return false;
    }

    public static bool TrySetOriginalClass(string type, string name, Obj value)
    {
        if (originalClasses.TryGetValue(type, out var cla))
        {
            cla.Add(name, value);
            return true;
        }

        return false;
    }

    public static IEnumerable<string> GetAllAttrKeys(Obj value)
    {
        var seen = new HashSet<string>();

        foreach (var key in value.Members.Keys)
            if (seen.Add(key))
                yield return key;

        if (TryGetOriginalClass(value.Type.Name, out var orig) && orig is not null)
            foreach (var key in orig.Keys)
                if (seen.Add(key))
                    yield return key;

        var baseTypeName = value.Type.Name;

        while (originalBaseTypes.TryGetValue(baseTypeName, out var baseTypeNameNext))
        {
            baseTypeName = baseTypeNameNext;

            if (!originalClasses.TryGetValue(baseTypeName, out var baseOrig))
                continue;

            foreach (var key in baseOrig.Keys)
                if (seen.Add(key))
                    yield return key;
        }

        foreach (var baseObj in value.BaseChain())
            foreach (var key in baseObj.Members.Keys)
                if (seen.Add(key))
                    yield return key;
    }

    public static bool HasAttrDeep(Obj value, string name)
    {
        if (value.Members.ContainsKey(name))
            return true;
        if (TryGetOriginalValue(value.Type.Name, name, out _))
            return true;

        foreach (var baseObj in value.BaseChain())
            if (baseObj.Members.ContainsKey(name))
                return true;

        return false;
    }

    public static Obj GetAttrDeep(Obj value, string name)
    {
        if (value.Members.TryGetValue(name, out var v))
            return v;
        if (TryGetOriginalValue(value.Type.Name, name, out v) && v is not null)
            return v;

        foreach (var baseObj in value.BaseChain())
            if (baseObj.Members.TryGetValue(name, out v))
                return v;

        return new Err($"'{value.Type}' object has no attribute '{name}'");
    }

    public static Obj GetGlobalVariable(string name) => scope.Get(name, out var value) ? value : new Err($"global variable '{name}' not found");

    public static void SetGlobalVariable(string name, Obj obj) => scope.Set(name, obj);

    public static void ClearModuleCache() => _moduleCache.Clear();

    public sealed record PlaygroundState(
        Dictionary<string, Obj> GlobalScope,
        Dictionary<string, Obj> Classes,
        Dictionary<string, Attributes> OriginalClasses,
        Dictionary<string, string> OriginalBaseTypes,
        Dictionary<string, Map> ModuleCache,
        HashSet<string> Loading);

    public static PlaygroundState CaptureState() => new(
        scope.Snapshot(),
        classes.Snapshot(),
        originalClasses.ToDictionary(kvp => kvp.Key, kvp => new Attributes(kvp.Value)),
        new Dictionary<string, string>(originalBaseTypes),
        _moduleCache.ToDictionary(kvp => kvp.Key, kvp => new Map(kvp.Value)),
        new HashSet<string>(_loading));

    public static void RestoreState(PlaygroundState state)
    {
        scope.Restore(state.GlobalScope);
        classes.Restore(state.Classes);

        originalClasses.Clear();
        foreach (var (key, value) in state.OriginalClasses)
            originalClasses[key] = new Attributes(value);

        originalBaseTypes.Clear();
        foreach (var (key, value) in state.OriginalBaseTypes)
            originalBaseTypes[key] = value;

        _moduleCache.Clear();
        foreach (var (key, value) in state.ModuleCache)
            _moduleCache[key] = new Map(value);

        _loading.Clear();
        foreach (var key in state.Loading)
            _loading.Add(key);
    }

    public static bool TryGetGlobalVariable(string name, out Obj obj) => scope.Get(name, out obj);

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