using Un.Object;

namespace Un;

public sealed class Runner(Context context, Context? parentContext = null)
{
    public Context Context { get; } = context;
    public Context? ParentContext { get; } = parentContext;
    public OptimizationResult? LastOptimization { get; private set; }

    public Obj Run()
    {
        return RunCore(null);
    }

    public Obj Run(Node parsedAst)
    {
        ArgumentNullException.ThrowIfNull(parsedAst);
        return RunCore(parsedAst);
    }

    private Obj RunCore(Node? parsedAst)
    {
        LastOptimization = null;

        try
        {
            var ast = parsedAst ?? Parse();

            var desugarer = new Desugarer();
            var desugaredAst = desugarer.Desugar(ast);

            var optimization = Optimizer.OptimizeWithStats(desugaredAst);
            LastOptimization = optimization;

            var evaluator = new Evaluator(Context);

            return evaluator.Eval(optimization.Root);
        }
        catch (PropagateFlow pf)
        {
            var err = pf.Error as Err;
            throw new Error(err?.Message ?? "propagated error", pf.Start, pf.Length, Context.Source, header: err?.Header ?? "error", inner: pf);
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
            try
            {
                RunDefers();
            }
            finally
            {
                FreeUsings();
            }
        }
    }

    private Node Parse()
    {
        var lexer = new Lexer(Context.Source);
        var tokens = lexer.Lex();

        var parser = new Parser(tokens, Context);
        return parser.Parse();
    }

    public void Reset()
    {
        Context.Defers.Clear();
        Context.Usings.Clear();
        Context.Frames.Clear();
    }

    private void FreeUsings()
    {
        while (Context.Usings.Count > 0)
            Context.Usings.Pop().Exit();
    }

    private void RunDefers()
    {
        while (Context.Defers.Count > 0)
        {
            var block = Context.Defers.Pop();
            var evaluator = new Evaluator(Context);
            evaluator.Eval(block);
        }
    }

    public static Runner Load(string path, Scope scope)
    {
        var fullPath = Path.Combine(Global.PATH, path.EndsWith(".un") ? path : $"{path}.un");

        if (!Global.FileSystem.FileExists(fullPath))
            throw new Panic($"file '{path}' not found");

        var code = Global.FileSystem.ReadAllText(fullPath).Replace("\r\n", "\n").Replace('\r', '\n');
        var file = new Source(fullPath, code);

        return new(new(scope, file, []));
    }

    public static Runner Load(Context context, Context parentContext) => new(context, parentContext);
}
