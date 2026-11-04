using Un.Object;
using Un.Object.Collections;
using Un.Object.Primitive;
using Un.Reflection;

namespace Un.Native;

[NativeModule("io")]
public static class IO
{
    public static Func<string, string?>? BrowserPromptHandler { get; set; }
    private static Object.IO.Stream _stdout = CreateStdout();
    private static Object.IO.Stream _stdin = CreateStdin();

    private static Object.IO.Stream CreateStdout()
    {
        try { return new(Console.OpenStandardOutput()); } catch { return new(new MemoryStream()); }
    }
    private static Object.IO.Stream CreateStdin()
    {
        try { return new(Console.OpenStandardInput()); } catch { return new(new MemoryStream()); }
    }

    public static void SetStdout(Object.IO.Stream stream) => _stdout = stream;
    public static void SetStdin(Object.IO.Stream stream) => _stdin = stream;
    public static Object.IO.Stream GetStdout() => _stdout;
    public static Object.IO.Stream GetStdin() => _stdin;
    public static void ResetStdout()
    {
        try { _stdout = new(Console.OpenStandardOutput()); } catch { _stdout = new(new MemoryStream()); }
    }
    public static void ResetStdin()
    {
        try { _stdin = new(Console.OpenStandardInput()); } catch { _stdin = new(new MemoryStream()); }
    }

    [Native(
        Name = "write",
        Description = "Writes values to standard output.",
        Example = "write(\"Hello\", \"UN\")",
        ReturnType = "none",
        ArgumentTypes = new[] { "tuple", "str", "str", "stream" }
    )]
    public static Obj Write(
        [ArgInfo(Positional = true)] Obj values,
        [ArgInfo(Optional = true)] Obj sep = null!,
        [ArgInfo(Optional = true)] Obj end = null!,
        [ArgInfo(Optional = true)] Obj stream = null!)
    {
        sep ??= Str.From(" ");
        end ??= Str.From("\n");
        stream ??= _stdout;

        if (!stream.As<Object.IO.Stream>(out var streamValue))
            return new Err("expected 'stream' argument to be of type 'stream'");

        if (!streamValue.CanWrite)
            return new Err("stream is not writable");

        if (!values.ToTuple().As<Tup>(out var tup))
            return new Err("expected 'value' argument to be a tuple");

        if (!sep.ToStr().As<Str>(out var sepValue))
            return new Err("expected 'sep' argument to be a string");

        if (!end.ToStr().As<Str>(out var endValue))
            return new Err("expected 'end' argument to be a string");

        var items = tup.Value;

        var cw = streamValue.Writer;

        for (int i = 0; i < items.Length; i++)
        {
            if (items[i].ToStr().As<Str>(out var str))
                cw?.Write(str.Value);
            else if (items[i].Repr().As<Str>(out var repr))
                cw?.Write(repr.Value);
            else
                return new Err("expected all values to be strings or representable as strings");

            if (i != items.Length - 1)
                cw?.Write(sepValue.Value);
        }
        cw?.Write(endValue.Value);
        cw?.Flush();

        return Obj.None;
    }

    [Native(
        Name = "read",
        Description = "Reads one line from an input stream.",
        Example = "name = read(\"Name: \")",
        ReturnType = "str",
        ArgumentTypes = new[] { "str", "stream", "stream" }
    )]
    public static Obj Read(
        [ArgInfo(Optional = true)] Obj prompt = null!,
        [ArgInfo(Optional = true)] Obj input = null!,
        [ArgInfo(Optional = true)] Obj output = null!)
    {
        prompt ??= Str.From("");
        input ??= _stdin;
        output ??= _stdout;

        if (!input.As<Object.IO.Stream>(out var readStreamValue))
            return new Err("expected 'input' argument to be of type 'stream'");

        if (!output.As<Object.IO.Stream>(out var writeStreamValue))
            return new Err("expected 'output' argument to be of type 'stream'");

        if (!writeStreamValue.CanWrite)
            return new Err("stream is not writable");

        if (!readStreamValue.CanRead)
            return new Err("stream is not readable");

        var cr = readStreamValue.Reader;
        var cw = writeStreamValue.Writer;

        if (prompt.ToStr().As<Str>(out var str))
            cw?.Write(str.Value);
        else if (prompt.Repr().As<Str>(out var repr))
            cw?.Write(repr.Value);
        else
            return new Err("expected all values to be strings or representable as strings");

        cw?.Flush();

        if (OperatingSystem.IsBrowser() && cr != null && cr.Peek() == -1 && BrowserPromptHandler != null)
        {
            try
            {
                var promptInput = BrowserPromptHandler(str.Value);
                if (promptInput != null)
                    return Str.From(promptInput);
            }
            catch (NeedInputException)
            {
                throw;
            }
            catch { }
        }

        return Str.From(cr?.ReadLine() ?? "");
    }

    [Native(
        Name = "open",
        Description = "Opens a sandbox-relative file stream.",
        Example = "stream = open(\"notes.txt\", \"r\")",
        ReturnType = "stream",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static Obj Open(
        [ArgInfo(Essential = true)] Obj path,
        [ArgInfo(Optional = true)] Obj mode = null!)
    {
        if (!path.ToStr().As<Str>(out var pathValue))
            return new Err("expected 'path' argument to be a string");

        mode ??= Str.From("r");

        if (!mode.ToStr().As<Str>(out var modeValue))
            return new Err("expected 'mode' argument to be a string");

        var modeType = modeValue.Value;
        var fullPath = Path.Combine(Global.PATH, pathValue.Value);

        if (modeType != "r" && modeType != "w" && modeType != "a" && modeType != "rw")
            return new Err("invalid 'mode' argument, expected one of 'r', 'w', 'a', or 'rw'");

        if (Global.FileSystem is MemoryFileSystem mem)
        {
            if (modeType == "r" && !mem.FileExists(fullPath))
                return new Err("file not found");
            if (modeType == "r")
            {
                var content = mem.ReadAllText(fullPath);
                return new Object.IO.Stream(new MemoryStream(System.Text.Encoding.UTF8.GetBytes(content)));
            }
            else if (modeType == "w")
            {
                return new Object.IO.Stream(new MemoryFileSystemStream(fullPath, mem, FileMode.Create));
            }
            else if (modeType == "a")
            {
                var existing = mem.FileExists(fullPath) ? mem.ReadAllText(fullPath) : "";
                var ms = new MemoryStream(System.Text.Encoding.UTF8.GetBytes(existing));
                ms.Seek(0, SeekOrigin.End);
                return new Object.IO.Stream(new MemoryFileSystemStream(fullPath, mem, FileMode.Append, ms));
            }
            else // rw
            {
                var existing = mem.FileExists(fullPath) ? mem.ReadAllText(fullPath) : "";
                var ms = new MemoryStream(System.Text.Encoding.UTF8.GetBytes(existing));
                return new Object.IO.Stream(new MemoryFileSystemStream(fullPath, mem, FileMode.OpenOrCreate, ms));
            }
        }

        if (modeType == "r" && !File.Exists(fullPath))
            return new Err("file not found");

        return modeType switch
        {
            "r" => new Object.IO.Stream(File.Open(fullPath, FileMode.Open, FileAccess.Read)),
            "w" => new Object.IO.Stream(File.Open(fullPath, FileMode.Create, FileAccess.Write)),
            "a" => new Object.IO.Stream(File.Open(fullPath, FileMode.Append, FileAccess.Write)),
            "rw" => new Object.IO.Stream(File.Open(fullPath, FileMode.OpenOrCreate, FileAccess.ReadWrite)),
            _ => new Err($"invalid file mode '{modeType}'")
        };
    }

    private sealed class MemoryFileSystemStream : System.IO.MemoryStream
    {
        private readonly string _path;
        private readonly MemoryFileSystem _fs;
        public MemoryFileSystemStream(string path, MemoryFileSystem fs, FileMode mode) : base() { _path = path; _fs = fs; }
        public MemoryFileSystemStream(string path, MemoryFileSystem fs, FileMode mode, MemoryStream existing) : base(existing.ToArray()) { _path = path; _fs = fs; if (mode == FileMode.Append) Seek(0, SeekOrigin.End); }
        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                var content = System.Text.Encoding.UTF8.GetString(ToArray());
                _fs.AddFile(_path, content);
            }
            base.Dispose(disposing);
        }
    }

    [Native(
        Name = "clear",
        Description = "Clears the active output stream.",
        Example = "clear()",
        ReturnType = "none"
    )]
    public static Obj Clear()
    {
        try { Console.Clear(); } catch { /* WASM has no console */ }
        return Obj.None;
    }
}
