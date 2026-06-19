using Un.Object.Type;

namespace Un.Object.IO;

public class Stream(System.IO.Stream stream) : Ref<System.IO.Stream>(stream, UnType.Create("stream"))
{

}