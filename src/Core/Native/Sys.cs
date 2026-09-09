using Un.Object;
using Un.Object.Collections;
using Un.Object.Primitive;
using Un.Reflection;

namespace Un.Native;

[NativeModule("sys")]
public static class Sys
{
    [Native(Name = "env", Description = "Reads an environment variable by name.", Example = "write(sys.env(\"HOME\"))", ReturnType = "str", ArgumentTypes = new[] { "str" })]
    public static Obj Env([ArgInfo(Essential = true)] Obj name) => Os.Env(name);

    [Native(Name = "environ", Description = "Returns all environment variables as text entries.", Example = "values = sys.environ()", ReturnType = "list")]
    public static Obj Environ() => Os.Environ();

    [Native(Name = "expand_vars", Description = "Expands environment-variable placeholders in text.", Example = "path = sys.expand_vars(\"$HOME/data\")", ReturnType = "str", ArgumentTypes = new[] { "str" })]
    public static Obj ExpandVars([ArgInfo(Essential = true)] Obj text) => Os.ExpandVars(text);

    [Native(Name = "pid", Description = "Returns the current process identifier.", Example = "write(sys.pid())", ReturnType = "int")]
    public static Obj Pid() => Os.Pid();

    [Native(Name = "name", Description = "Returns the current operating-system name.", Example = "write(sys.name())", ReturnType = "str")]
    public static Obj Name() => Os.Name();

    [Native(Name = "arch", Description = "Returns the operating-system architecture.", Example = "write(sys.arch())", ReturnType = "str")]
    public static Obj Arch() => Os.Arch();

    [Native(Name = "version", Description = "Returns the operating-system version.", Example = "write(sys.version())", ReturnType = "str")]
    public static Obj Version() => Os.Version();

    [Native(Name = "cpuCount", Description = "Returns the available processor count.", Example = "write(sys.cpuCount())", ReturnType = "int")]
    public static Obj CpuCount() => Os.CpuCount();

    [Native(Name = "is64bit", Description = "Checks whether the operating system is 64-bit.", Example = "write(sys.is64bit())", ReturnType = "bool")]
    public static Obj Is64Bit() => Os.Is64Bit();

    [Native(Name = "hostname", Description = "Returns the current machine host name.", Example = "write(sys.hostname())", ReturnType = "str")]
    public static Obj Hostname() => Os.Hostname();

    [Native(Name = "username", Description = "Returns the current operating-system user name.", Example = "write(sys.username())", ReturnType = "str")]
    public static Obj Username() => Os.Username();

    [Native(Name = "args", Description = "Returns the process command-line arguments.", Example = "values = sys.args()", ReturnType = "list")]
    public static List Args() => Os.Args();

    [Native(Name = "sep", Description = "Returns the directory separator character.", Example = "write(sys.sep())", ReturnType = "str")]
    public static Str Sep() => Os.Sep();

    [Native(Name = "pathsep", Description = "Returns the path-list separator character.", Example = "write(sys.pathsep())", ReturnType = "str")]
    public static Str PathSep() => Os.PathSep();

    [Native(Name = "linesep", Description = "Returns the environment line separator.", Example = "write(sys.linesep())", ReturnType = "str")]
    public static Str LineSep() => Os.LineSep();

    [Native(Name = "tickCount", Description = "Returns elapsed system ticks in milliseconds.", Example = "write(sys.tickCount())", ReturnType = "int")]
    public static Int TickCount() => Os.TickCount();

    [Native(Name = "getcalldepth", Description = "Returns current maximum recursion depth.", Example = "write(sys.getcalldepth())", ReturnType = "int")]
    public static Obj GetCallDepth() => Int.From((long)Global.MAXRECURSIONDEPTH);

    [Native(Name = "setcalldepth", Description = "Sets maximum recursion depth.", Example = "sys.setcalldepth(20000)", ReturnType = "none", ArgumentTypes = new[] { "int" })]
    public static Obj SetCallDepth([ArgInfo(Essential = true)] Obj depth)
    {
        if (!depth.As<Int>(out var d)) return new Err("expected 'depth' to be int");
        if (d.Value < 1) return new Err("depth must be >= 1");
        Global.MAXRECURSIONDEPTH = (ulong)d.Value;
        return Obj.None;
    }

    // aliases for familiarity (Python-like)
    [Native(Name = "getrecursionlimit", Description = "Returns current maximum recursion depth (alias).", Example = "write(sys.getrecursionlimit())", ReturnType = "int")]
    public static Obj GetRecursionLimit() => Int.From((long)Global.MAXRECURSIONDEPTH);

    [Native(Name = "setrecursionlimit", Description = "Sets maximum recursion depth (alias).", Example = "sys.setrecursionlimit(20000)", ReturnType = "none", ArgumentTypes = new[] { "int" })]
    public static Obj SetRecursionLimit([ArgInfo(Essential = true)] Obj depth) => SetCallDepth(depth);
}
