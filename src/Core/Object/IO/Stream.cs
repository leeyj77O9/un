using Un.Object.Type;

namespace Un.Object.IO;

public class Stream : Ref<System.IO.Stream>, IDisposable
{
    public StreamReader? Reader { get; }
    public StreamWriter? Writer { get; }

    public bool CanRead => Value.CanRead;
    public bool CanWrite => Value.CanWrite;

    public Stream(System.IO.Stream stream) : base(stream, UnType.Create("stream"))
    {
        if (stream.CanRead)
            Reader = new StreamReader(stream, leaveOpen: true);
        if (stream.CanWrite)
            Writer = new StreamWriter(stream, leaveOpen: true)
            {
                AutoFlush = false
            };
    }

    public void Close()
    {
        Dispose();
    }

    public void Dispose()
    {
        Writer.Dispose();
        Reader.Dispose();
        Value.Dispose();

        GC.SuppressFinalize(this);
    }
}