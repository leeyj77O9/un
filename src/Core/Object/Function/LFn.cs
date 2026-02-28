using Un;
using Un.Object.Collections;

namespace Un.Object.Function;

public class LFn : Fn
{
    private UnFile file = null!; 

    public LFn() { }

    public LFn(string[] body)
    {
        file = new UnFile("fn", body);
    }

    public override Obj Call(Tup args)
    {
        if (Global.CallDepth++ > (int)Global.MAXRECURSIONDEPTH)
            throw new Panic("maximum recursion depth exceeded");

        var scope = new Scope(Closure ?? Scope.Empty);
        Bind(scope, args);
        file.Move(0, 0);

        var context = new Context(scope, file, []);
        Obj? returned = null;
        var parser = new Parser(context);

        try
        {
            var tokenizer = new Tokenizer();
            var lexer = new Lexer();

            while (!context.File.EOF && parser.ReturnValue is null)
            {
                var tokens = tokenizer.Tokenize(context.File);
                var nodes = lexer.Lex(tokens);
                returned = parser.Parse(nodes);

                if (context.File.EOL)
                    context.File.Move(0, context.File.Line + 1);
            }

            returned = parser.ReturnValue!;
        }
        catch
        {
            throw;
        } 
        finally
        {
            if (context.Defers.Count > 0)
            {
                parser = new Parser(new Context(context.Scope, new("defer", []), []));
                foreach (var nodes in context.Defers)
                {
                    parser.Parse(nodes);
                }                
            }

            if (context.Usings.Count > 0)
            {
                foreach (var obj in context.Usings)
                {
                    obj.Exit();
                }
            }
        }

        Global.CallDepth--;

        return returned ?? None;
    }

    public override Obj Clone() => new LFn()
    {
        Name = Name,
        Args = [..Args],
        ReturnType = ReturnType,
        Closure = Closure,
        file = file,
        Self = Self,
        Super = Super?.Clone()!,
    };
}