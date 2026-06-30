using Un.Object;

namespace Un;

public sealed class Runner(Context context, Context? parentContext = null)
{
    public Context Context { get; } = context;
    public Context? ParentContext { get; } = parentContext;

    public Obj Run()
    {
        try
        {
            var tokenizer = new Lexer(Context.Source);
            var tokens = tokenizer.Tokenize();

            var parser = new Parser(tokens, Context);
            var ast = parser.Parse();

            var desugarer = new Desugarer();
            var desuraredAst = desugarer.Desugar(ast);

            var optimizedAst = Optimizer.Optimize(desuraredAst);

            var evaluator = new Evaluator(Context);

            return evaluator.Eval(desuraredAst);
        }
        catch (BreakFlow bf)
        {
            throw new Error("'break' outside loop", bf.Start, bf.Length, Context.Source);
        }
        catch (SkipFlow sf)
        {
            throw new Error("'skip' outside loop", sf.Start, sf.Length, Context.Source);
        }
        catch (ReturnFlow rf)
        {
            throw new Error("'->' outside function", rf.Start, rf.Length, Context.Source);
        }
        finally
        {
            RunDefers();
            FreeUsings();
        }
    }

    public void Reset()
    {
        Context.Defers.Clear();
        Context.Usings.Clear();
        Context.Frames.Clear();
    }

    private void FreeUsings()
    {
        foreach (var obj in Context.Usings)
            obj.Exit();
    }

    private void RunDefers()
    {
        foreach (var block in Context.Defers)
        {
            var evaluator = new Evaluator(Context);
            evaluator.Eval(block);
        }
    }

    public static Runner Load(string path, Scope scope)
    {
        var fullPath = Path.Combine(Global.PATH, path.EndsWith(".un") ? path : $"{path}.un");

        if (!File.Exists(fullPath))
            throw new Panic($"file '{path}' not found");

        var code = File.ReadAllText(fullPath).Replace("\r\n", "\n").Replace('\r', '\n');
        var file = new Source(fullPath, code);

        return new Runner(new Context(scope, file, []));
    }

    public static Runner Load(Context context, Context parentContext) => new(context, parentContext);
}