using System.Text;
using Un.Object.Collections;
using Un.Object.Primitive;
using Un.Object.Type;
using Un.Reflection;

namespace Un.Object.Web;

[NativeType(Name = "http_client")]
public class HttpClient(System.Net.Http.HttpClient value) : Ref<System.Net.Http.HttpClient>(value, UnType.Create("http"))
{
    public override Obj Init(Tup args) => new Err("'http' cannot be constructed directly; use http.connect()");

    public override Bool Eq(Obj other) => Bool.From(other is HttpClient h && ReferenceEquals(h.Value, Value));

    [Native(
        Name = "set_header",
        Description = "Sets header on a http_client value.",
        Example = "client.set_header(key, value)",
        ReturnType = "none",
        ArgumentTypes = new[] { "any", "any" }
    )]
    public static Obj SetHeader(
        [Self] HttpClient self,
        [ArgInfo(Essential = true)] Obj key,
        [ArgInfo(Essential = true)] Obj value)
    {
        if (!key.As<Str>(out var k))
            return new Err("expected key is string");

        if (!value.As<Str>(out var v))
            return new Err("expected value is string");

        self.Value.DefaultRequestHeaders.Remove(k.Value);
        self.Value.DefaultRequestHeaders.Add(k.Value, v.Value);

        return None;
    }

    [Native(
        Name = "clear_headers",
        Description = "Returns the result of client.clear headers().",
        Example = "client.clear_headers()",
        ReturnType = "none"
    )]
    public static Obj ClearHeaders([Self] HttpClient self)
    {
        self.Value.DefaultRequestHeaders.Clear();
        return None;
    }

    [Native(
        Name = "get",
        Description = "Gets a value from client.",
        Example = "client.get(url)",
        ReturnType = "str",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Get([Self] HttpClient self, [ArgInfo(Essential = true)] Obj url)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        try
        {
            return Str.From(self.Value.GetStringAsync(u.Value).GetAwaiter().GetResult());
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "get_async",
        Description = "Asynchronously sends a GET request and returns its response text.",
        Example = "body = client.get_async(\"https://example.com\")",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static async Task<Obj> GetAsync([Self] HttpClient self, [ArgInfo(Essential = true)] Obj url)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        try
        {
            return Str.From(await self.Value.GetStringAsync(u.Value));
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "delete",
        Description = "Deletes a value from client.",
        Example = "client.delete(url)",
        ReturnType = "str",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Delete([Self] HttpClient self, [ArgInfo(Essential = true)] Obj url)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        try
        {
            var res = self.Value
                .DeleteAsync(u.Value)
                .GetAwaiter()
                .GetResult();

            return Str.From(
                res.Content
                    .ReadAsStringAsync()
                    .GetAwaiter()
                    .GetResult());
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "delete_async",
        Description = "Asynchronously sends a DELETE request and returns its response text.",
        Example = "body = client.delete_async(\"https://example.com/item\")",
        ReturnType = "str",
        ArgumentTypes = new[] { "str" }
    )]
    public static async Task<Obj> DeleteAsync([Self] HttpClient self, [ArgInfo(Essential = true)] Obj url)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        try
        {
            var res = await self.Value.DeleteAsync(u.Value);
            return Str.From(await res.Content.ReadAsStringAsync());
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "post",
        Description = "Returns the result of client.post().",
        Example = "client.post(url, body)",
        ReturnType = "str",
        ArgumentTypes = new[] { "any", "any" }
    )]
    public static Obj Post(
        [Self] HttpClient self,
        [ArgInfo(Essential = true)] Obj url,
        [ArgInfo(Essential = true)] Obj body)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        if (!body.As<Str>(out var b))
            return new Err("expected body is string");

        try
        {
            using var content = new StringContent(b.Value, Encoding.UTF8, "text/plain");

            var res = self.Value.PostAsync(u.Value, content).GetAwaiter().GetResult();

            return Str.From(res.Content.ReadAsStringAsync().GetAwaiter().GetResult());
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "post_async",
        Description = "Asynchronously sends a POST request with text content.",
        Example = "body = client.post_async(\"https://example.com\", \"data\")",
        ReturnType = "str",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static async Task<Obj> PostAsync(
        [Self] HttpClient self,
        [ArgInfo(Essential = true)] Obj url,
        [ArgInfo(Essential = true)] Obj body)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        if (!body.As<Str>(out var b))
            return new Err("expected body is string");

        try
        {
            using var content = new StringContent(b.Value, Encoding.UTF8, "text/plain");

            var res = await self.Value.PostAsync(u.Value, content);

            return Str.From(await res.Content.ReadAsStringAsync());
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "put",
        Description = "Returns the result of client.put().",
        Example = "client.put(url, body)",
        ReturnType = "str",
        ArgumentTypes = new[] { "any", "any" }
    )]
    public static Obj Put(
        [Self] HttpClient self,
        [ArgInfo(Essential = true)] Obj url,
        [ArgInfo(Essential = true)] Obj body)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        if (!body.As<Str>(out var b))
            return new Err("expected body is string");

        try
        {
            using var content = new StringContent(b.Value, Encoding.UTF8, "text/plain");

            var res = self.Value.PutAsync(u.Value, content).GetAwaiter().GetResult();

            return Str.From(res.Content.ReadAsStringAsync().GetAwaiter().GetResult());
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "put_async",
        Description = "Asynchronously sends a PUT request with text content.",
        Example = "body = client.put_async(\"https://example.com\", \"data\")",
        ReturnType = "str",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static async Task<Obj> PutAsync(
        [Self] HttpClient self,
        [ArgInfo(Essential = true)] Obj url,
        [ArgInfo(Essential = true)] Obj body)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        if (!body.As<Str>(out var b))
            return new Err("expected body is string");

        try
        {
            using var content = new StringContent(b.Value, Encoding.UTF8, "text/plain");

            var res = await self.Value.PutAsync(u.Value, content);

            return Str.From(await res.Content.ReadAsStringAsync());
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "patch",
        Description = "Returns the result of client.patch().",
        Example = "client.patch(url, body)",
        ReturnType = "str",
        ArgumentTypes = new[] { "any", "any" }
    )]
    public static Obj Patch(
        [Self] HttpClient self,
        [ArgInfo(Essential = true)] Obj url,
        [ArgInfo(Essential = true)] Obj body)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        if (!body.As<Str>(out var b))
            return new Err("expected body is string");

        try
        {
            using var content = new StringContent(b.Value, Encoding.UTF8, "text/plain");

            var req = new HttpRequestMessage(HttpMethod.Patch, u.Value)
            {
                Content = content
            };

            var res = self.Value
                .SendAsync(req)
                .GetAwaiter()
                .GetResult();

            return Str.From(
                res.Content
                    .ReadAsStringAsync()
                    .GetAwaiter()
                    .GetResult());
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "patch_async",
        Description = "Asynchronously sends a PATCH request with text content.",
        Example = "body = client.patch_async(\"https://example.com\", \"data\")",
        ReturnType = "str",
        ArgumentTypes = new[] { "str", "str" }
    )]
    public static async Task<Obj> PatchAsync(
        [Self] HttpClient self,
        [ArgInfo(Essential = true)] Obj url,
        [ArgInfo(Essential = true)] Obj body)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        if (!body.As<Str>(out var b))
            return new Err("expected body is string");

        try
        {
            using var content = new StringContent(b.Value, Encoding.UTF8, "text/plain");

            var req = new HttpRequestMessage(HttpMethod.Patch, u.Value)
            {
                Content = content
            };

            var res = await self.Value.SendAsync(req);

            return Str.From(await res.Content.ReadAsStringAsync());
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "head",
        Description = "Returns the result of client.head().",
        Example = "client.head(url)",
        ReturnType = "int",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Head([Self] HttpClient self, [ArgInfo(Essential = true)] Obj url)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        try
        {
            var req = new HttpRequestMessage(HttpMethod.Head, u.Value);
            var res = self.Value.SendAsync(req).GetAwaiter().GetResult();

            return Int.From((int)res.StatusCode);
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "head_async",
        Description = "Asynchronously sends a HEAD request and returns its status code.",
        Example = "status = client.head_async(\"https://example.com\")",
        ReturnType = "int",
        ArgumentTypes = new[] { "str" }
    )]
    public static async Task<Obj> HeadAsync([Self] HttpClient self, [ArgInfo(Essential = true)] Obj url)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        try
        {
            var req = new HttpRequestMessage(HttpMethod.Head, u.Value);

            var res = await self.Value.SendAsync(req);

            return Int.From((int)res.StatusCode);
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "options",
        Description = "Returns the result of client.options().",
        Example = "client.options(url)",
        ReturnType = "list",
        ArgumentTypes = new[] { "any" }
    )]
    public static Obj Options([Self] HttpClient self, [ArgInfo(Essential = true)] Obj url)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        try
        {
            var req = new HttpRequestMessage(HttpMethod.Options, u.Value);

            var res = self.Value.SendAsync(req).GetAwaiter().GetResult();

            List list = [];

            if (res.Headers.TryGetValues("Allow", out var values))
                foreach (var value in values)
                    foreach (var method in value.Split(',', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries))
                        list.Add(Str.From(method));

            return list;
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }

    [Native(
        Name = "options_async",
        Description = "Asynchronously lists HTTP methods allowed by an endpoint.",
        Example = "methods = client.options_async(\"https://example.com\")",
        ReturnType = "list",
        ArgumentTypes = new[] { "str" }
    )]
    public static async Task<Obj> OptionsAsync([Self] HttpClient self, [ArgInfo(Essential = true)] Obj url)
    {
        if (!url.As<Str>(out var u))
            return new Err("expected url is string");

        try
        {
            var req = new HttpRequestMessage(HttpMethod.Options, u.Value);

            var res = await self.Value.SendAsync(req);

            List list = [];

            if (res.Headers.TryGetValues("Allow", out var values))
                foreach (var value in values)
                    foreach (var method in value.Split(',', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries))
                        list.Add(Str.From(method));

            return list;
        }
        catch (Exception e)
        {
            return new Err(e.Message);
        }
    }
}
