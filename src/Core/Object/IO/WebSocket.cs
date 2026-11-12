using System.Net.WebSockets;
using System.Text;

using Un.Object.Collections;
using Un.Object.Primitive;
using Un.Object.Type;
using Un.Reflection;

namespace Un.Object.Web;

[NativeType(Name = "web_socket")]
public class WebSocket(ClientWebSocket value) : Ref<ClientWebSocket>(value, UnType.Create("web_socket"))
{
    public override Obj Init(Tup args) => new Err("'socket' cannot be constructed directly; use websocket.connect(url)");

    public override Bool Eq(Obj other) => Bool.From(other is WebSocket s && ReferenceEquals(s.Value, Value));

    public override Bool ToBool() => Bool.From(Value.State == WebSocketState.Open);

    public override Str ToStr() => Str.From($"<socket {Value.State}>");

    public override Obj Copy() => this;

    public override Obj Clone() => this;

    public override int GetHashCode() => Value.GetHashCode();

    [Native(
        Name = "send",
        Description = "Sends data through socket.",
        Example = "socket.send(text)",
        ReturnType = "none",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Send([Self] WebSocket self, [ArgInfo(Essential = true)] Obj text)
    {
        if (!text.As<Str>(out var str))
            return new Err("expected text is string");

        try
        {
            var bytes = Encoding.UTF8.GetBytes(str.Value);
            self.Value.SendAsync(bytes, WebSocketMessageType.Text, true, CancellationToken.None)
                .GetAwaiter().GetResult();
            return None;
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "receive",
        Description = "Receives data from socket.",
        Example = "socket.receive()",
        ReturnType = "str"
    )]
    public static Obj Receive([Self] WebSocket self)
    {
        try
        {
            var buffer = new byte[8192];
            using var stream = new MemoryStream();
            WebSocketReceiveResult result;

            do
            {
                result = self.Value.ReceiveAsync(buffer, CancellationToken.None).GetAwaiter().GetResult();

                if (result.MessageType == WebSocketMessageType.Close)
                    return None;

                stream.Write(buffer, 0, result.Count);
            } while (!result.EndOfMessage);

            return Str.From(Encoding.UTF8.GetString(stream.ToArray()));
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "close",
        Description = "Closes socket.",
        Example = "socket.close()",
        ReturnType = "none"
    )]
    public static Obj Close([Self] WebSocket self)
    {
        try
        {
            self.Value.CloseAsync(WebSocketCloseStatus.NormalClosure, "closed", CancellationToken.None)
                .GetAwaiter().GetResult();
            return None;
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "is_open",
        Description = "Checks whether a web_socket value open.",
        Example = "socket.is_open()",
        ReturnType = "bool"
    )]
    public static Bool IsOpen([Self] WebSocket self) => Bool.From(self.Value.State == WebSocketState.Open);
}