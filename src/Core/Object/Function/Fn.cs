using Un.Object.Primitive;
using Un.Object.Collections;

namespace Un.Object.Function;

public class Fn() : Obj("fn")
{
    public string? Name { get; set; }
    public List<Arg> Args { get; set; } = [];
    public string ReturnType { get; set; } = "any";  
    public Scope? Closure { get; set; }

    protected void Bind(Scope scope, Tup args)
    {
        scope["self"] = Self;
        scope["super"] = Super;

        args = UnpackArgs(args);

        var unnamed = new List<Obj>();
        var extraNamed = new Map();

        for (int i = 0; i < args.Count; i++)
        {
            var name = args.Name[i];
            var val = args.Value[i];

            if (string.IsNullOrEmpty(name))
            {
                unnamed.Add(val);
            }
            else
            {
                if (scope.ContainsKeyInTop(name))
                    throw new Panic($"argument '{name}' provided multiple times.");
                extraNamed[name] = val;
            }
        }

        int unnamedIndex = 0;
        var argNames = new HashSet<string>();
        Arg positionalArg = null!;
        Arg keywordArg = null!;

        bool positionalReached = false;

        foreach (var arg in Args)
        {
            argNames.Add(arg.Name);

            if (arg.IsPositional)
            {
                positionalArg = arg;
                positionalReached = true;
                continue;
            }

            if (arg.IsKeyword)
            {
                keywordArg = arg;
                continue;
            }

            if (arg.IsEssential)
            {
                if (unnamedIndex < unnamed.Count)
                {
                    scope[arg.Name] = unnamed[unnamedIndex++];
                }
                else if (extraNamed.TryGetValue(arg.Name, out var val))
                {
                    scope[arg.Name] = val;
                    extraNamed.Remove(arg.Name);
                }
                else
                {
                    throw new Panic($"missing required argument: '{arg.Name}'");
                }
            }
            else if (arg.IsOptional && !positionalReached)
            {
                if (unnamedIndex < unnamed.Count)
                {
                    scope[arg.Name] = unnamed[unnamedIndex++];
                }
                else if (extraNamed.TryGetValue(arg.Name, out var val))
                {
                    scope[arg.Name] = val;
                    extraNamed.Remove(arg.Name);
                }
                else
                {
                    scope[arg.Name] = arg.DefaultValue!;
                }
            }
            else if (extraNamed.TryGetValue(arg.Name, out var val))
            {
                scope[arg.Name] = val;
                extraNamed.Remove(arg.Name);
            }
            else
            {
                scope[arg.Name] = arg.DefaultValue!;
            }
        }

        if (unnamedIndex < unnamed.Count)
        {
            if (positionalArg != null)
            {
                var rest = unnamed.Skip(unnamedIndex);
                scope[positionalArg.Name] = new Tup([.. rest], new string[rest.Count()]);
            }
            else
            {
                throw new Panic("function does not accept positional arguments.");
            }
        }

        if (positionalArg != null && !scope.ContainsKeyInTop(positionalArg.Name))
            scope[positionalArg.Name] = new Tup([], []);

        if (extraNamed.Count > 0)
        {
            if (keywordArg != null)
            {
                var dict = new Dict();
                foreach (var (k, v) in extraNamed)
                    dict.Value.Add(new Str(k), v);
                scope[keywordArg.Name] = dict;
            }
            else
            {
                var unexpected = extraNamed.Keys.First();
                throw new Panic($"unexpected keyword argument: '{unexpected}'");
            }
        }
    }

    public override int GetHashCode()
    {
        unchecked
        {
            int hash = Global.BASEHASH;
            hash = hash * Global.HASHPRIME + (Name?.GetHashCode() ?? 0);
            hash = hash * Global.HASHPRIME + Args.Count;
            return hash;
        }
    }

    public override bool Equals(object? obj)
    {
        if (obj is not Fn other)
            return false;

        return ReferenceEquals(this, other);
    }

    public static Tup UnpackArgs(Tup rawArgs)
    {
        var objs = new List<Obj>();
        var names = new List<string>();

        for (var i = 0; i < rawArgs.Count; i++)
        {
            if (rawArgs[i] is Spreads spread)
            {
                foreach (var v in spread)
                {
                    objs.Add(v);
                    names.Add(rawArgs.Name[i]);
                }
            }
            else
            {
                objs.Add(rawArgs[i]);
                names.Add(rawArgs.Name[i]);
            }
        }

        return new([.. objs], [.. names]);
    }

    public static List<Arg> GetArgs(List<Node> args, Context context)
    {
        List<Arg> result = [];
        List<Node> buf = [];

        string name = "";
        string argType = "";
        bool isOptional = false;
        bool isPositional = false;
        bool isKeyword = false;

        bool hasDefault = false;

        for (int i = 0; i < args.Count; i++)
        {
            (_, var type, _) = args[i];

            if (type == TokenType.Comma)
            {
                result.Add(new Arg(name)
                {
                    Type = argType,
                    IsEssential = !isOptional && !isPositional && !isKeyword,
                    IsOptional = isOptional,
                    IsPositional = isPositional,
                    IsKeyword = isKeyword,
                    DefaultValue = hasDefault ? Operator.On(buf, context) : Null,
                });

                (argType, name) = ("", "");
                (isOptional, isPositional, isKeyword, hasDefault) = (false, false, false, false);

                buf.Clear();
            }
            else if (hasDefault)
                buf.Add(args[i]);
            else if (type == TokenType.Spread)
                isPositional = true;
            else if (type == TokenType.DoubleAsterisk)
                isKeyword = true;
            else if (type == TokenType.Assign)
            {
                hasDefault = true;
                isOptional = true;
            }
            else if (type == TokenType.Colon)
            {
                argType = args[i + 1].Value;
                i++;
            }
            else if (type == TokenType.Identifier)
                name = args[i].Value;
        }

        if (!string.IsNullOrEmpty(name))
            result.Add(new Arg(name)
            {
                Type = argType,
                IsEssential = !isOptional && !isPositional && !isKeyword,
                IsOptional = isOptional,
                IsPositional = isPositional,
                IsKeyword = isKeyword,
                DefaultValue = hasDefault ? Operator.On(buf, context) : Null,
            });

        return result;
    }
}