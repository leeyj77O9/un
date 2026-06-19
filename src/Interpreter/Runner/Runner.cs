using Un.Object;
using Un.Object.Type;

namespace Un;

public class Runner(Context context, Context? parentContext = null!)
{
    public Context Context { get; private set; } = context;
    public Context? ParentContext { get; private set; } = parentContext;

    public Obj Run()
    {
        Obj returned = Obj.None;
        try
        {
            var tokenizer = new Tokenizer();
            var lexer = new Lexer();
            var parser = new Parser(Context);

            while (!Context.File.EOF && parser.ReturnValue is null)
            {
                var tokens = tokenizer.Tokenize(Context.File);
                var nodes = lexer.Lex(tokens);
                returned = parser.Parse(nodes);

                if (Context.File.EOL)
                    Context.File.Move(0, Context.File.Line + 1);
            }

            returned = parser.ReturnValue!;

        }
        catch
        {
            if (ParentContext is not null)
                foreach (var block in Context.BlockStack)
                    ParentContext.BlockStack.Add(block);
            throw;
        }
        finally
        {
            Free();
            Defer();
        }

        if ((returned?.Type == UnType.Skip || returned?.Type == UnType.Break) && Context.CurrentBlock?.Type != "loop")
            throw new Panic($"'{returned?.Type}' keyword can only be used inside a loop");

        return returned!;
    }

    public void Reset()
    {
        Context.File.Move(0, 0);
        Context.Defers.Clear();
        Context.Usings.Clear();
    }

    private void Free()
    {
        foreach (var obj in Context.Usings)
        {
            obj.Exit();
        }
    }

    private void Defer()
    {
        foreach (var nodes in Context.Defers)
        {
            var parser = new Parser(new Context(Context.Scope, new("defer", []), []));
            parser.Parse(nodes);
        }
    }

    public static Runner Load(string path, Scope scope)
    {
        var allPath = Path.Combine(Global.PATH, path) + (path.EndsWith(".un") ? "" : ".un");
        var name = path.EndsWith(".un") ? path[..^3] : path;

        if (!Path.Exists(allPath))
            throw new Panic($"file {name} not found in {allPath}");

        return new(new(scope, new UnFile(name, File.ReadAllLines(allPath)), []));
    }

    public static Runner Load(Context context, Context parentContext) => new(context, parentContext);
}
