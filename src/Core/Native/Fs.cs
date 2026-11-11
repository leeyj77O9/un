using Un.Object;
using Un.Object.Collections;
using Un.Object.Primitive;
using Un.Reflection;

namespace Un.Native;

[NativeModule("fs")]
public static class Fs
{
    static bool GetBlockedPath(Obj obj, out string path, out Obj err)
    {
        if (Global.FileSystem is not PhysicalFileSystem)
        {
            path = "";
            err = new Err("fs module is not available in this environment");
            return false;
        }

        return GetPath(obj, out path, out err);
    }

    static bool GetPath(Obj obj, out string path, out Obj err)
    {
        if (!obj.As<Str>(out var value))
        {
            path = "";
            err = new Err("expected path is string");
            return false;
        }

        path = value.Value;
        err = Obj.None;
        return true;
    }

    [Native(
        Name = "exists",
        Description = "Checks whether a file or directory exists.",
        Example = "write(exists(\"notes.txt\"))",
        ReturnType = "bool",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Exists([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        return Bool.From(File.Exists(p) || Directory.Exists(p));
    }

    [Native(
        Name = "file",
        Description = "Checks whether a path is an existing file.",
        Example = "write(file(\"notes.txt\"))",
        ReturnType = "bool",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj FileExists([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        return Bool.From(File.Exists(p));
    }

    [Native(
        Name = "dir",
        Description = "Checks whether a path is an existing directory.",
        Example = "write(dir(\"data\"))",
        ReturnType = "bool",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj DirectoryExists([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        return Bool.From(Directory.Exists(p));
    }

    [Native(
        Name = "read",
        Description = "Reads all text from a file.",
        Example = "text = read(\"notes.txt\")",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Read([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (!File.Exists(p))
            return new Err("file not found");

        return Str.From(System.IO.File.ReadAllText(p));
    }

    [Native(
        Name = "write",
        Description = "Writes text to a file, replacing existing content.",
        Example = "write(\"notes.txt\", \"Hello\")",
        ReturnType = "none",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Write(
        [ArgInfo(Essential = true)] Obj path,
        [ArgInfo(Essential = true)] Obj text)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (!text.As<Str>(out var value))
            return new Err("expected text is string");

        File.WriteAllText(p, value.Value);

        return Obj.None;
    }

    [Native(
        Name = "append",
        Description = "Appends text to a file.",
        Example = "append(\"notes.txt\", \"\\nNext line\")",
        ReturnType = "none",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Append(
        [ArgInfo(Essential = true)] Obj path,
        [ArgInfo(Essential = true)] Obj text)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (!text.As<Str>(out var value))
            return new Err("expected text is string");

        File.AppendAllText(p, value.Value);

        return Obj.None;
    }

    [Native(
        Name = "delete",
        Description = "Deletes a file or directory.",
        Example = "delete(\"old.txt\")",
        ReturnType = "none",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Delete([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (File.Exists(p))
            File.Delete(p);
        else if (Directory.Exists(p))
            Directory.Delete(p, true);

        return Obj.None;
    }

    [Native(
        Name = "copy",
        Description = "Copies a file to a destination path.",
        Example = "copy(\"from.txt\", \"to.txt\")",
        ReturnType = "none",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Copy(
        [ArgInfo(Essential = true)] Obj source,
        [ArgInfo(Essential = true)] Obj destination)
    {
        if (!GetBlockedPath(source, out var src, out var err))
            return err;

        if (!GetBlockedPath(destination, out var dst, out err))
            return err;

        File.Copy(src, dst, true);

        return Obj.None;
    }

    [Native(
        Name = "move",
        Description = "Moves a file to a destination path.",
        Example = "move(\"draft.txt\", \"archive/draft.txt\")",
        ReturnType = "none",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Move(
        [ArgInfo(Essential = true)] Obj source,
        [ArgInfo(Essential = true)] Obj destination)
    {
        if (!GetBlockedPath(source, out var src, out var err))
            return err;

        if (!GetBlockedPath(destination, out var dst, out err))
            return err;

        File.Move(src, dst, true);

        return Obj.None;
    }

    [Native(
        Name = "rename",
        Description = "Renames a file within its current directory.",
        Example = "rename(\"draft.txt\", \"final.txt\")",
        ReturnType = "none",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Rename(
        [ArgInfo(Essential = true)] Obj path,
        [ArgInfo(Essential = true)] Obj name)
    {
        if (!GetBlockedPath(path, out var src, out var err))
            return err;

        if (!name.As<Str>(out var value))
            return new Err("expected name is string");

        string parent = Path.GetDirectoryName(src)!;
        string dst = Path.Combine(parent, value.Value);

        File.Move(src, dst, true);

        return Obj.None;
    }

    [Native(
        Name = "touch",
        Description = "Creates a file when it does not exist.",
        Example = "touch(\"notes.txt\")",
        ReturnType = "none",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Touch([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (!File.Exists(p))
            File.Create(p).Dispose();

        return Obj.None;
    }

    [Native(
        Name = "mkdir",
        Description = "Creates a directory and missing parent directories.",
        Example = "mkdir(\"data/output\")",
        ReturnType = "none",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Mkdir([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        Directory.CreateDirectory(p);

        return Obj.None;
    }

    [Native(
        Name = "rmdir",
        Description = "Deletes a directory, optionally including its contents.",
        Example = "rmdir(\"cache\", true)",
        ReturnType = "none",
        ArgumentTypes = new[] { "str", "bool" }
    )]
    public static Obj Rmdir(
        [ArgInfo(Essential = true)] Obj path,
        [ArgInfo(Optional = true)] Obj recursive = null!)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        bool value = true;

        if (recursive != null)
        {
            if (!recursive.As<Bool>(out var b))
                return new Err("expected recursive is bool");

            value = b.Value;
        }

        Directory.Delete(p, value);

        return Obj.None;
    }

    [Native(
        Name = "list",
        Description = "Lists file-system entries in a directory.",
        Example = "items = list(\"data\")",
        ReturnType = "list",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj List([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (!Directory.Exists(p))
            return new Err("directory not found");

        return new List([.. Directory.EnumerateFileSystemEntries(p).Select(Str.From)]);
    }

    [Native(
        Name = "files",
        Description = "Lists files in a directory.",
        Example = "items = files(\"data\")",
        ReturnType = "list",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Files([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (!Directory.Exists(p))
            return new Err("directory not found");

        return new List([.. Directory.EnumerateFiles(p).Select(Str.From)]);
    }

    [Native(
        Name = "dirs",
        Description = "Lists subdirectories in a directory.",
        Example = "items = dirs(\"data\")",
        ReturnType = "list",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Dirs([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (!Directory.Exists(p))
            return new Err("directory not found");

        return new List([.. Directory.EnumerateDirectories(p).Select(Str.From)]);
    }

    [Native(
        Name = "walk",
        Description = "Lists file-system entries recursively.",
        Example = "items = walk(\"data\")",
        ReturnType = "list",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Walk([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (!Directory.Exists(p))
            return new Err("directory not found");

        return new List([.. Directory.EnumerateFileSystemEntries(p, "*", SearchOption.AllDirectories).Select(Str.From)]);
    }

    [Native(
        Name = "size",
        Description = "Returns the byte size of a file.",
        Example = "write(size(\"notes.txt\"))",
        ReturnType = "int",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Size([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (!File.Exists(p))
            return new Err("file not found");

        return Int.From(new FileInfo(p).Length);
    }

    [Native(
        Name = "created",
        Description = "Returns a path creation time in UTC.",
        Example = "write(created(\"notes.txt\"))",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Created([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        DateTime time = File.Exists(p) ? File.GetCreationTimeUtc(p) : Directory.GetCreationTimeUtc(p);

        return Str.From(time.ToString("O"));
    }

    [Native(
        Name = "modified",
        Description = "Returns a path last-modified time in UTC.",
        Example = "write(modified(\"notes.txt\"))",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Modified([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        DateTime time = File.Exists(p) ? File.GetLastWriteTimeUtc(p) : Directory.GetLastWriteTimeUtc(p);

        return Str.From(time.ToString("O"));
    }

    [Native(
        Name = "accessed",
        Description = "Returns a path last-accessed time in UTC.",
        Example = "write(accessed(\"notes.txt\"))",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Accessed([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        DateTime time = File.Exists(p)
            ? File.GetLastAccessTimeUtc(p)
            : Directory.GetLastAccessTimeUtc(p);

        return Str.From(time.ToString("O"));
    }

    [Native(
        Name = "readonly",
        Description = "Checks whether a file is read-only.",
        Example = "write(readonly(\"notes.txt\"))",
        ReturnType = "bool",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Readonly([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (!File.Exists(p))
            return new Err("file not found");

        return Bool.From(File.GetAttributes(p).HasFlag(FileAttributes.ReadOnly));
    }

    [Native(
        Name = "hidden",
        Description = "Checks whether a path has the hidden attribute.",
        Example = "write(hidden(\"notes.txt\"))",
        ReturnType = "bool",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Hidden([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (!File.Exists(p) && !Directory.Exists(p))
            return new Err("path not found");

        return Bool.From(File.GetAttributes(p).HasFlag(FileAttributes.Hidden));
    }

    [Native(
        Name = "join",
        Description = "Joins path segments using the platform separator.",
        Example = "path = join(\"data\", \"notes.txt\")",
        ReturnType = "str",
        ArgumentTypes = new[] { "tuple" }
    )]
    public static Obj Join([ArgInfo(Positional = true)] Obj paths)
    {
        if (!paths.As<Tup>(out var tuple))
            return new Err("expected tuple");

        var values = new string[tuple.Count];

        for (int i = 0; i < tuple.Count; i++)
        {
            if (!tuple[i].As<Str>(out var value))
                return new Err("expected path is string");

            values[i] = value.Value;
        }

        return Str.From(Path.Combine(values));
    }

    [Native(
        Name = "parent",
        Description = "Returns the parent directory of a path.",
        Example = "write(parent(\"data/notes.txt\"))",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Parent([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetPath(path, out var p, out var err))
            return err;

        return Str.From(Path.GetDirectoryName(p) ?? "");
    }

    [Native(
        Name = "name",
        Description = "Returns the file name portion of a path.",
        Example = "write(name(\"data/notes.txt\"))",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Name([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetPath(path, out var p, out var err))
            return err;

        return Str.From(Path.GetFileName(p));
    }

    [Native(
        Name = "stem",
        Description = "Returns a file name without its extension.",
        Example = "write(stem(\"notes.txt\"))",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Stem([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetPath(path, out var p, out var err))
            return err;

        return Str.From(Path.GetFileNameWithoutExtension(p));
    }

    [Native(
        Name = "ext",
        Description = "Returns the extension portion of a path.",
        Example = "write(ext(\"notes.txt\"))",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Extension([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetPath(path, out var p, out var err))
            return err;

        return Str.From(Path.GetExtension(p));
    }

    [Native(
        Name = "absolute",
        Description = "Returns the absolute form of a path.",
        Example = "write(absolute(\"notes.txt\"))",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Absolute([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetPath(path, out var p, out var err))
            return err;

        return Str.From(Path.GetFullPath(p));
    }

    [Native(
        Name = "relative",
        Description = "Returns a path relative to another directory.",
        Example = "write(relative(\"data\", \"data/notes.txt\"))",
        ReturnType = "str",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Relative(
        [ArgInfo(Essential = true)] Obj from,
        [ArgInfo(Essential = true)] Obj to)
    {
        if (!GetPath(from, out var f, out var err))
            return err;

        if (!GetPath(to, out var t, out err))
            return err;

        return Str.From(Path.GetRelativePath(f, t));
    }

    [Native(
        Name = "normalize",
        Description = "Normalizes a path to its absolute form.",
        Example = "write(normalize(\"data/../notes.txt\"))",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Normalize([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetPath(path, out var p, out var err))
            return err;

        return Str.From(Path.GetFullPath(Path.TrimEndingDirectorySeparator(p)));
    }

    [Native(
        Name = "cwd",
        Description = "Returns the current working directory.",
        Example = "write(cwd())",
        ReturnType = "str"
    )]
    public static Obj Cwd() => Str.From(Environment.CurrentDirectory);

    [Native(
        Name = "chdir",
        Description = "Changes the current working directory.",
        Example = "chdir(\"data\")",
        ReturnType = "none",
        ArgumentTypes = new[] { "str" }
    )]
    public static Obj Chdir([ArgInfo(Essential = true)] Obj path)
    {
        if (!GetBlockedPath(path, out var p, out var err))
            return err;

        if (!Directory.Exists(p))
            return new Err("directory not found");

        Environment.CurrentDirectory = p;
        return Obj.None;
    }

    [Native(
        Name = "temp",
        Description = "Returns the system temporary directory.",
        Example = "write(temp())",
        ReturnType = "str"
    )]
    public static Str Temp() => Str.From(Path.GetTempPath());

    [Native(
        Name = "home",
        Description = "Returns the current user's home directory.",
        Example = "write(home())",
        ReturnType = "str"
    )]
    public static Obj Home() => Str.From(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile));

    [Native(
        Name = "exe",
        Description = "Returns the current process executable path.",
        Example = "write(exe())",
        ReturnType = "str"
    )]
    public static Str Exe() => Str.From(Environment.ProcessPath ?? "");

    [Native(
        Name = "app",
        Description = "Returns the application base directory.",
        Example = "write(app())",
        ReturnType = "str"
    )]
    public static Str App() => Str.From(AppContext.BaseDirectory);
}
