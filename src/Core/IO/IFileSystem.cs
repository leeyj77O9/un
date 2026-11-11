namespace Un;

public interface IFileSystem
{
    bool FileExists(string path);
    bool DirectoryExists(string path);
    string ReadAllText(string path);
    string[] GetFiles(string path, string pattern);
}

public sealed class PhysicalFileSystem : IFileSystem
{
    public bool FileExists(string path) => File.Exists(path);
    public bool DirectoryExists(string path) => Directory.Exists(path);
    public string ReadAllText(string path) => File.ReadAllText(path);
    public string[] GetFiles(string path, string pattern) => Directory.GetFiles(path, pattern);
}

public sealed class MemoryFileSystem : IFileSystem
{
    private readonly Dictionary<string, string> _files = new(StringComparer.OrdinalIgnoreCase);

    public void AddFile(string path, string content)
    {
        var key = Normalize(path);
        _files[key] = content;
    }

    public bool FileExists(string path) => _files.ContainsKey(Normalize(path));
    public bool DirectoryExists(string path) => _files.Keys.Any(k => k.StartsWith(Normalize(path) + "/", StringComparison.OrdinalIgnoreCase));
    public string ReadAllText(string path) => _files.TryGetValue(Normalize(path), out var c) ? c : throw new FileNotFoundException(path);
    public string[] GetFiles(string path, string pattern)
    {
        var dir = Normalize(path);
        var ext = pattern == "*.un" ? ".un" : "";
        return _files.Keys.Where(k => k.StartsWith(dir + "/") && k.EndsWith(ext, StringComparison.OrdinalIgnoreCase)).ToArray();
    }

    private static string Normalize(string path) => path.Replace("\\", "/").TrimEnd('/').ToLowerInvariant();
}
