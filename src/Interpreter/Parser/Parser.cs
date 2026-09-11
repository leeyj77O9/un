using System.Globalization;
using System.Text;
using Un.Util;

namespace Un;

public sealed class Parser(IReadOnlyList<Token> tokens, Context context)
{
    private int position;

    private readonly Context context = context;
    private readonly Source source = context.Source;
    private readonly string code = context.Source.Code;

    private Token Current => position < tokens.Count ? tokens[position] : tokens[^1];

    private bool IsEndOfStatement => Current.Type is TokenType.NewLine or TokenType.Dedent or TokenType.EOF;

    private Token Previous() => tokens[position - 1];

    private Token Next()
    {
        var token = Current;
        position++;
        return token;
    }

    private bool Match(TokenType type)
    {
        if (Current.Type != type)
            return false;

        position++;
        return true;
    }

    private Token Expect(TokenType type, string msg)
    {
        if (Current.Type != type)
            throw new Error(msg, Current.Start, Current.Length, source);

        return Next();
    }

    private void SkipNewLines()
    {
        while (Match(TokenType.NewLine)) { }
    }

    public Node Parse()
    {
        var statements = new List<Node>();

        SkipNewLines();

        while (Current.Type != TokenType.EOF)
        {
            statements.Add(ParseStatement());
            SkipNewLines();
        }

        var root = new Node(0, statements.Count == 0 ? 0 : statements[^1].Start + statements[^1].Length, NodeKind.Block, children: [.. statements]);

        return root;
    }

    private Node ParseStatement()
    {
        var annotations = ParseAnnotations();

        Node statement;

        if (IsAssignmentStart())
        {
            statement = RequireNoAnnotations(annotations, ParseAssignment);
        }
        else
        {
            statement = Current.Type switch
            {
                TokenType.Use => RequireNoAnnotations(annotations, ParseUse),
                TokenType.Using => RequireNoAnnotations(annotations, ParseUsing),

                TokenType.Class => ParseClass(annotations),
                TokenType.Enum => ParseEnum(annotations),
                TokenType.Func => ParseFunction(annotations),

                TokenType.If => RequireNoAnnotations(annotations, ParseIf),

                TokenType.For => RequireNoAnnotations(annotations, ParseFor),
                TokenType.While => RequireNoAnnotations(annotations, ParseWhile),

                TokenType.Try => RequireNoAnnotations(annotations, ParseTry),
                TokenType.Defer => RequireNoAnnotations(annotations, ParseDefer),

                TokenType.Break => RequireNoAnnotations(annotations, ParseBreak),
                TokenType.Skip => RequireNoAnnotations(annotations, ParseSkip),
                TokenType.Return => RequireNoAnnotations(annotations, ParseReturn),

                _ => RequireNoAnnotations(annotations, ParseExpression)
            };
        }

        if (!IsEndOfStatement && !IsStatementStart(Current.Type))
            throw new Error($"unexpected token {Current.Type}", Current.Start, Current.Length, source);

        return statement;
    }

    private bool IsStatementStart(TokenType type) => type switch
    {
        TokenType.Identifier => true,

        TokenType.Use or
        TokenType.Using or
        TokenType.Class or
        TokenType.Enum or
        TokenType.Func or
        TokenType.If or
        TokenType.For or
        TokenType.While or
        TokenType.Try or
        TokenType.Defer or
        TokenType.Break or
        TokenType.Skip or
        TokenType.Wait or
        TokenType.At or
        TokenType.Return => true,

        _ => false
    };

    private Node ParseAnnotation()
    {
        var at = Expect(TokenType.At, "expected '@' to start an annotation");
        var expr = ParseExpression();

        return new Node(at.Start, GetLength(at, expr), NodeKind.Annotation, children: [expr]);
    }

    private List<Node> ParseAnnotations()
    {
        var annotations = new List<Node>();

        while (Current.Type == TokenType.At)
        {
            annotations.Add(ParseAnnotation());
            SkipNewLines();
        }

        return annotations;
    }

    private Node ParseUsePath()
    {
        var path = ParseIdentifier();

        while (Match(TokenType.Dot))
        {
            if (Current.Type == TokenType.Asterisk)
            {
                var star = Next();

                return new Node(path.Start, GetLength(path, star), NodeKind.Unary, TokenType.Wildcard, path);
            }

            var member = ParseIdentifier();

            path = new Node(path.Start, GetLength(path, member), NodeKind.Property, children: [path, member]);
        }

        return path;
    }

    private Node ParseUseImports()
    {
        List<Node> imports = [];

        if (Match(TokenType.Asterisk))
        {
            imports.Add(new Node(Previous().Start, Previous().Length, NodeKind.Unary, TokenType.Wildcard));
        }
        else
        {
            do
            {
                var name = ParseIdentifier();

                if (Match(TokenType.As))
                {
                    var alias = ParseIdentifier();

                    imports.Add(new Node(name.Start, GetLength(name, alias), NodeKind.Pair, children: [name, alias]));
                }
                else
                {
                    imports.Add(name);
                }

            } while (Match(TokenType.Comma));
        }

        return new Node(imports[0].Start, imports[^1].Length, NodeKind.Tuple, children: [.. imports]);
    }

    private Node ParseUse()
    {
        var keyword = Expect(TokenType.Use, "expected 'use' keyword");
        var path = ParseUsePath();

        Node? imports = null;
        Node? alias = null;

        if (Match(TokenType.LBrace))
        {
            imports = ParseUseImports();
            Expect(TokenType.RBrace, "expected '}' to close the use member");
        }

        if (Match(TokenType.As))
            alias = ParseIdentifier();

        var children = new List<Node> { path };

        if (imports != null)
            children.Add(imports);

        if (alias != null)
            children.Add(alias);

        return new Node(keyword.Start, GetLength(keyword, alias ?? imports ?? path), NodeKind.Use, children: [.. children]);
    }

    private Node ParseUsing()
    {
        var keyword = Expect(TokenType.Using, "expected 'using' keyword");
        var expr = ParseAssignment();

        if (expr.Kind != NodeKind.Assign)
            throw new Error("using requires an assignment", expr, source);

        return new Node(keyword.Start, GetLength(keyword, expr), NodeKind.Using, children: [expr]);
    }

    private Node ParseClass(List<Node> annotations)
    {
        var keyword = Expect(TokenType.Class, "expected 'class' keyword");
        var name = ParseIdentifier();

        Node? parameters = null;
        Node? bases = null;

        if (Current.Type == TokenType.LParen)
            parameters = ParseParameters();

        if (Match(TokenType.Colon))
        {
            var types = new List<Node>();

            while (true)
            {
                types.Add(ParseIdentifier());

                if (!Match(TokenType.Comma))
                    break;
            }

            var first = types[0];
            var last = types[^1];

            bases = new Node(first.Start, GetLength(first, last), NodeKind.ClassBases, children: [.. types]);
        }


        SkipNewLines();

        Node body;

        if (tokens[position].Type == TokenType.Indent)
        {
            body = ParseBlock();
        }
        else
        {
            body = new Node(Current.Start, 0, NodeKind.Block);
        }

        List<Node> children = [name];

        if (parameters is not null)
            children.Add(parameters);

        if (bases is not null)
            children.Add(bases);

        children.Add(body);

        return new Node(keyword.Start, GetLength(keyword, body), NodeKind.Class, children: [.. children])
        {
            Annotations = annotations
        };
    }

    private Node ParseEnumMember()
    {
        var name = ParseIdentifier();

        if (!Match(TokenType.Assign))
            return name;

        var value = ParseExpression();

        return new Node(name.Start, GetLength(name, value), NodeKind.Assign, TokenType.Assign, name, value);
    }

    private Node ParseEnum(List<Node> annotations)
    {
        var keyword = Expect(TokenType.Enum, "expected 'enum' keyword");
        var name = ParseIdentifier();

        Expect(TokenType.NewLine, "expected newline after enum name");
        Expect(TokenType.Indent, "expected indented enum members");

        var members = new List<Node>();

        while (Current.Type != TokenType.Dedent && Current.Type != TokenType.EOF)
        {
            SkipNewLines();

            if (Current.Type == TokenType.Dedent)
                break;

            members.Add(ParseEnumMember());

            while (Match(TokenType.Comma))
            {
                SkipNewLines();

                if (Current.Type is TokenType.NewLine or TokenType.Dedent)
                    break;

                members.Add(ParseEnumMember());
            }

            SkipNewLines();
        }

        var closer = Expect(TokenType.Dedent, "expected end of enum block");

        return new Node(keyword.Start, GetLength(keyword, closer), NodeKind.Enum, children: [name, .. members])
        {
            Annotations = annotations
        };
    }

    private Node ParseIf()
    {
        var keyword = Expect(TokenType.If, "expected 'if' keyword");

        var cases = new List<Node>();

        var condition = ParseExpression();
        var thenBranch = ParseBlock();

        cases.Add(new Node(keyword.Start, GetLength(keyword, thenBranch), NodeKind.Branch, default, condition, thenBranch));

        Node? elseNode = null;

        while (true)
        {
            if (Match(TokenType.ElIf))
            {
                var elifCond = ParseExpression();
                var elifBody = ParseBlock();

                cases.Add(new Node(Previous().Start, GetLength(Previous(), elifBody), NodeKind.ElIf, default, elifCond, elifBody));
            }
            else if (Match(TokenType.Else))
            {
                var elseBody = ParseBlock();

                elseNode = new Node(Previous().Start, GetLength(Previous(), elseBody), NodeKind.Else, default, elseBody);

                break;
            }
            else
            {
                break;
            }
        }

        var children = new List<Node>(cases);

        if (elseNode != null)
            children.Add(elseNode);

        return new Node(keyword.Start, GetLength(keyword, children[^1]), NodeKind.If, default, [.. children]);
    }

    private Node ParseMatchBraceBody()
    {
        Expect(TokenType.LBrace, "expected '{' to start match body");

        var cases = new List<Node>();

        SkipNewLines();

        while (!Match(TokenType.RBrace))
        {
            var pattern = ParsePattern();

            Expect(TokenType.Return, "expected '->' after match pattern");

            var body = ParseBinary();

            cases.Add(new Node(pattern.Start, GetLength(pattern, body), NodeKind.Case, children: [pattern, body]));

            Match(TokenType.Comma); 
            SkipNewLines();
        }

        return new Node(cases.Count > 0 ? cases[0].Start : 0, cases.Count > 0 ? GetLength(cases[0], cases[^1]) : 0, NodeKind.Block, children: [.. cases]);
    }

    private Node ParseMatch()
    {
        var keyword = Expect(TokenType.Match, "expected 'match' keyword");

        var expr = ParseExpression();
        var body = ParseMatchBraceBody();

        return new Node(keyword.Start, GetLength(keyword, body), NodeKind.Match, children: [expr, ..body.Children]);
    }

    private Node ParseFor()
    {
        var keyword = Expect(TokenType.For, "expected 'for' keyword");
        var target = ParsePattern();

        Expect(TokenType.In, "expected 'in' after for target");

        var iterable = ParseExpression();
        SkipNewLines();
        var body = ParseBlock();

        return new Node(keyword.Start, GetLength(keyword, body), NodeKind.For, children: [target, iterable, body]);
    }

    private Node ParseWhile()
    {
        var keyword = Expect(TokenType.While, "expected 'while' keyword");

        var condition = ParseExpression();
        var body = ParseBlock();

        return new Node(keyword.Start, GetLength(keyword, body), NodeKind.While, children: [condition, body]);
    }

    private Node ParseTry()
    {
        var keyword = Expect(TokenType.Try, "expected 'try' keyword");

        var body = ParseBlock();

        var catches = new List<Node>();
        Node? finallyNode = null;

        while (Match(TokenType.Catch))
        {
            var catchKeyword = Previous();

            Node? exception = null;

            if (Current.Type == TokenType.Identifier)
                exception = ParseIdentifier();

            var catchBody = ParseBlock();
            var catchNode = new Node(catchKeyword.Start, GetLength(catchKeyword, catchBody), NodeKind.Catch, children: exception is null ? [catchBody] : [exception, catchBody]);

            catches.Add(catchNode);
        }

        if (Match(TokenType.Finally))
        {
            var finallyKeyword = Previous();
            var finallyBody = ParseBlock();

            finallyNode = new Node(finallyKeyword.Start, GetLength(finallyKeyword, finallyBody), NodeKind.Finally, children: [finallyBody]);
        }

        var children = new List<Node>
        {
            body
        };

        children.AddRange(catches);

        if (finallyNode is not null)
            children.Add(finallyNode);

        return new Node(keyword.Start, GetLength(keyword, children[^1]), NodeKind.Try, children: [.. children]);
    }

    private Node ParseDefer()
    {
        var keyword = Expect(TokenType.Defer, "expected 'defer' keyword");

        Node body;

        SkipNewLines();

        if (Current.Type == TokenType.Indent)
        {
            body = ParseBlock();
        }
        else
        {
            var expr = ParseExpression();

            body = new Node(expr.Start, expr.Length, NodeKind.Block, children: [expr]);
        }

        return new Node(keyword.Start, GetLength(keyword, body), NodeKind.Defer, children: [body]);
    }

    private Node ParseBreak()
    {
        var keyword = Expect(TokenType.Break, "expected 'break' keyword");

        return new Node(keyword.Start, keyword.Length, NodeKind.Break);
    }

    private Node ParseSkip()
    {
        var keyword = Expect(TokenType.Skip, "expected 'skip' keyword");

        return new Node(keyword.Start, keyword.Length, NodeKind.Skip);
    }

    private Node ParseReturn()
    {
        var keyword = Expect(TokenType.Return, "expected '->' keyword");

        if (IsEndOfStatement)
            return new Node(keyword.Start, keyword.Length, NodeKind.Return);
        
        var expr = ParseExpression();

        return new Node(keyword.Start, GetLength(keyword, expr), NodeKind.Return, children: [expr]);
    }

    private Node ParseExpression() => ParseBinary();

    private Node ParseComma()
    {
        var items = new List<Node>
        {
            ParseBinary() 
        };

        while (Match(TokenType.Comma))
        {
            items.Add(ParseBinary());
        }

        if (items.Count == 1)
            return items[0];

        var first = items[0];
        var last = items[^1];

        return new Node(first.Start, GetLength(first, last), NodeKind.Tuple, children: [.. items]);
    }

    private Node ParseAssignment()
    {
        var left = ParsePattern();

        if (!IsAssignmentOperator(Current.Type))
            return left;

        var op = Next();
        var right = ParseComma();

        if (IsAssignmentOperator(Current.Type))
            throw new Error("assignment chaining is not supported", Current.Start, Current.Length, source);

        return new Node(left.Start, right.Start + right.Length - left.Start, NodeKind.Assign, op.Type, left, right);
    }

    private Node ParseBinary(int parentPrecedence = 0)
    {
        var left = ParseUnary();

        while (true)
        {
            TokenType opType = Current.Type;
            int precedence;
            Token op;

            if (opType == TokenType.Is && position + 1 < tokens.Count && tokens[position + 1].Type == TokenType.Not)
            {
                precedence = GetBinaryPrecedence(TokenType.IsNot);
                if (precedence <= parentPrecedence)
                    break;
                var isTok = Next();
                var notTok = Next();
                op = new Token(isTok.Start, notTok.Start + notTok.Length - isTok.Start, TokenType.IsNot);
            }
            else
            {
                precedence = GetBinaryPrecedence(opType);
                if (precedence <= parentPrecedence)
                    break;
                op = Next();
            }
            var right = ParseBinary(precedence);

            left = new Node(left.Start, right.Start + right.Length - left.Start, NodeKind.Binary, op.Type, left, right);
        }

        return left;
    }

    private Node ParseUnary()
    {
        if (Current.Type is TokenType.Not or TokenType.Plus or TokenType.Minus or 
            TokenType.BNot or TokenType.Wait or TokenType.Go)
        {
            var op = Next();
            var expr = ParseUnary();

            return new Node(op.Start, expr.Start + expr.Length - op.Start, NodeKind.Unary, op.Type, expr);
        }

        return ParsePostfix();
    }

    private Node ParsePostfix()
    {
        var expr = ParsePrimary();

        while (true)
        {
            switch (Current.Type)
            {
                case TokenType.LParen:
                    expr = ParseCall(expr);
                    continue;

                case TokenType.Dot:
                    {
                        Next(); 
                        var member = ParseIdentifier();

                        expr = new Node(expr.Start, GetLength(expr, member), NodeKind.Property, children: [expr, member]);

                        continue;
                    }

                case TokenType.FString:
                    {
                        var template = ParseFString();

                        expr = new Node(expr.Start, GetLength(expr, template), NodeKind.Tagged, children: [expr, template]);

                        continue;
                    }

                case TokenType.QuestionDot:
                    {
                        Next(); 
                        var member = ParseIdentifier();

                        expr = new Node(expr.Start,
                            GetLength(expr, member),
                            NodeKind.Property,
                            children: [expr, member]);

                        continue;
                    }

                case TokenType.LBrack:
                    expr = ParseIndexOrSlice(expr);
                    continue;

                case TokenType.Question:
                    {
                        var q = Next();
                        expr = new Node(expr.Start, q.Start + q.Length - expr.Start, NodeKind.Unary, TokenType.Question, expr);
                        continue;
                    }
            }

            break;
        }

        return expr;
    }

    private Node ParseArgument()
    {
        Node expr;

        if (Match(TokenType.Asterisk))
        {
            expr = ParseExpression();
            return new Node(expr.Start, expr.Length, NodeKind.Spread, children: [expr]);
        }

        if (Match(TokenType.DoubleAsterisk))
        {
            expr = ParseExpression();
            return new Node(expr.Start, expr.Length, NodeKind.KwSpread, children: [expr]);
        }

        expr = ParseExpression();

        if (expr.Kind == NodeKind.Identifier && Match(TokenType.Assign))
        {
            var value = ParseExpression();
            return new Node(expr.Start, GetLength(expr, value), NodeKind.Pair, children: [expr, value]);
        }

        return expr;
    }

    private List<Node> ParseArguments()
    {
        var args = new List<Node>();

        if (Current.Type == TokenType.RParen)
            return args;

        while (true)
        {
            args.Add(ParseArgument());

            if (!Match(TokenType.Comma))
                break;

            if (Current.Type == TokenType.RParen)
                break;
        }

        return args;
    }

    private Node ParseCall(Node expr)
    {
        var opener = Expect(TokenType.LParen, "expected '(' before arguments");
        var args = ParseArguments();
        var closer = Expect(TokenType.RParen, "expected ')' after arguments");

        var tuple = new Node(opener.Start, GetLength(opener, closer), NodeKind.Tuple, children: [.. args]);

        return new Node(expr.Start, GetLength(expr, closer), NodeKind.Call, children: [expr, tuple]);
    }

    private Node ParseIndexOrSlice(Node expr)
    {
        var lbrack = Expect(TokenType.LBrack, "expected '[' before index or slice");

        Node? start = null;
        Node? end = null;
        Node? step = null;

        if (Current.Type != TokenType.Colon && Current.Type != TokenType.RBrack)
            start = ParseExpression();

        if (Match(TokenType.Colon))
        {
            if (Current.Type != TokenType.Colon && Current.Type != TokenType.RBrack)
                end = ParseExpression();

            if (Match(TokenType.Colon))
            {
                if (Current.Type != TokenType.RBrack)
                    step = ParseExpression();
            }
        }
        else
        {
            var rbrack = Expect(TokenType.RBrack, "expected ']' after index");

            return new Node(lbrack.Start, GetLength(lbrack, rbrack), NodeKind.Index, children: [expr, start!]);
        }

        var rbrack2 = Expect(TokenType.RBrack, "expected ']' to close slice");

        return new Node(lbrack.Start, GetLength(lbrack, rbrack2), NodeKind.Slice, children: [expr, start!, end!, step!]);
    }

    private Node ParsePrimary() => Current.Type switch
    {
        TokenType.Identifier => ParseIdentifier(),
        TokenType.Integer => ParseLiteral(NodeKind.Integer),
        TokenType.Float => ParseLiteral(NodeKind.Float),
        TokenType.String => ParseLiteral(NodeKind.String),
        TokenType.FString => ParseFString(),
        TokenType.True or TokenType.False => ParseLiteral(NodeKind.Boolean),
        TokenType.None => ParseLiteral(NodeKind.None),
        TokenType.LParen => ParseTuple(),
        TokenType.LBrace => ParseDictOrSet(),
        TokenType.LBrack => ParseList(),
        TokenType.Func => ParseLambda(),
        TokenType.Match => ParseMatch(),

        _ => throw new Error($"unexpected primary token {Current.Type}", Current.Start, Current.Length, source),
    };

    private Node ParseIdentifier()
    {
        var token = Expect(TokenType.Identifier, "expected identifier");

        return new Node(token.Start, token.Length, NodeKind.Identifier)
        {
            Value = GetText(token.Start, token.Length)
        };
    }

    private Node ParseFString()
    {
        var token = Expect(TokenType.FString, "expected '`' to start an f-string");
        var raw = GetText(token.Start, token.Length);

        int quoteLength = raw.StartsWith("```") ? 3 : 1;
        var text = raw[quoteLength..^quoteLength];

        List<Node> children = [];

        int start = 0;
        int pos = 0;

        while (pos < text.Length)
        {
            if (text[pos] == '\\')
            {
                pos += 2;
                continue;
            }

            if (text[pos] != '{')
            {
                pos++;
                continue;
            }

            if (pos > start)
            {
                children.Add(new Node(0, 0, NodeKind.String)
                {
                    Value = text[start..pos]
                });
            }

            int exprStart = pos + 1;
            int depth = 1;

            pos++;

            while (pos < text.Length && depth > 0)
            {
                if (text[pos] == '\\')
                {
                    pos += 2;
                    continue;
                }

                if (text[pos] == '{')
                    depth++;
                else if (text[pos] == '}')
                    depth--;

                pos++;
            }

            if (depth != 0)
                throw new Error("unclosed interpolation", exprStart, pos - exprStart, source);

            var exprText = text[exprStart..(pos - 1)];

            children.Add(ParseInterpolation(exprText));

            start = pos;
        }

        if (start < text.Length)
        {
            children.Add(new Node(0, 0, NodeKind.String)
            {
                Value = text[start..]
            });
        }

        return new Node(token.Start, token.Length, NodeKind.FString, children: [.. children]);
    }

    private Node ParseInterpolation(string text)
    {        
        var source = new Source(this.source.Path, text);
        var context = new Context(this.context.Scope, source, this.context.Frames);
        var lexer = new Lexer(source);

        var tokens = lexer.Lex();

        var parser = new Parser(tokens, context);

        return parser.ParseExpression();
    }

    private Node ParseLiteral(NodeKind kind)
    {
        var token = Next();
        var text = GetText(token.Start, token.Length);

        return new Node(token.Start, token.Length, kind)
        {
            Value = kind switch
            {
                NodeKind.Integer => ParseInteger(text),
                NodeKind.Float => ParseFloat(text),
                NodeKind.String => Unescape(RemoveQuotes(text)),
                NodeKind.Boolean => text == "true",
                NodeKind.None => null,
                _ => throw new Error($"invalid literal {kind}", token.Start, token.Length, source)
            }
        };

        long ParseInteger(string text)
        {
            var t = text.Replace("_", "");
            if (t.StartsWith("0x", StringComparison.OrdinalIgnoreCase))
            {
                if (long.TryParse(t[2..], NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var hv)) return hv;
                throw new Error("integer literal is too large", token.Start, token.Length, source);
            }
            if (t.StartsWith("0b", StringComparison.OrdinalIgnoreCase))
            {
                try { return Convert.ToInt64(t[2..], 2); } catch { throw new Error("integer literal is too large", token.Start, token.Length, source); }
            }
            if (t.StartsWith("0o", StringComparison.OrdinalIgnoreCase))
            {
                try { return Convert.ToInt64(t[2..], 8); } catch { throw new Error("integer literal is too large", token.Start, token.Length, source); }
            }
            if (long.TryParse(t, NumberStyles.Integer, CultureInfo.InvariantCulture, out var value))
                return value;

            throw new Error("integer literal is too large", token.Start, token.Length, source);
        }

        double ParseFloat(string text)
        {
            if (double.TryParse(text.Replace("_", ""), NumberStyles.Float, CultureInfo.InvariantCulture, out var value))
                return value;

            throw new Error("invalid floating-point literal", token.Start, token.Length, source);
        }
    }

    private List<Node> ParseExpressionList(TokenType end)
    {
        var items = new List<Node>();

        if (Current.Type == end)
            return items;

        while (true)
        {
            if (Current.Type == end)
                break;

            items.Add(ParseExpression());

            if (!Match(TokenType.Comma))
                break;
        }

        return items;
    }

    private List<Node> ParseTupleItems()
    {
        var items = new List<Node>();

        while (Current.Type != TokenType.RParen)
        {
            var expr = ParseExpression();

            if (expr.Kind == NodeKind.Identifier && Match(TokenType.Assign))
            {
                var value = ParseExpression();

                expr = new Node(expr.Start, GetLength(expr, value), NodeKind.Pair, default, expr, value);
            }

            items.Add(expr);

            if (!Match(TokenType.Comma))
                break;
        }

        return items;
    }

    private Node ParseTuple()
    {
        var opener = Expect(TokenType.LParen, "expected '(' before tuple items");
        var values = ParseTupleItems();
        var closer = Expect(TokenType.RParen, "expected ')' after tuple items");

        if (values.Count == 0)
            return new Node(opener.Start, GetLength(opener, closer), NodeKind.Tuple);

        if (values.Count == 1 && values[0].Kind != NodeKind.Pair)
            return values[0];

        return new Node(opener.Start, GetLength(opener, closer), NodeKind.Tuple, children: [.. values]);
    }

    private Node ParseDictOrSet()
    {
        var opener = Expect(TokenType.LBrace, "expected '{'");

        var values = new List<Node>();

        Token closer;

        if (Current.Type == TokenType.RBrace)
        {
            closer = Next();
            return new Node(opener.Start, GetLength(opener, closer), NodeKind.Dict);
        }

        var firstKey = ParseExpression();

        NodeKind kind;

        if (Match(TokenType.Colon))
            kind = NodeKind.Dict;
        else
            kind = NodeKind.Set;

        if (kind == NodeKind.Set)
        {
            values.Add(firstKey);
        }
        else
        {
            var value = ParseExpression();

            values.Add(new Node(firstKey.Start, GetLength(firstKey, value), NodeKind.Pair, children: [firstKey, value]));
        }

        while (true)
        {
            if (Current.Type == TokenType.RBrace)
                break;

            Expect(TokenType.Comma, "expected ',' between elements");

            if (Current.Type == TokenType.RBrace)
                break;

            var key = ParseExpression();

            if (kind == NodeKind.Set)
            {
                if (Current.Type == TokenType.Colon)
                    throw new Error("set cannot contain key-value pairs", Current.Start, Current.Length, context.Source);

                values.Add(key);
            }
            else
            {
                Expect(TokenType.Colon, "expected ':' after dictionary key");

                var value = ParseExpression();

                values.Add(new Node(key.Start, GetLength(key, value), NodeKind.Pair, children: [key, value]));
            }
        }

        closer = Expect(TokenType.RBrace, "expected '}' to close collection");

        return new Node(opener.Start, GetLength(opener, closer), kind, children: [.. values]);
    }

    private Node ParseList()
    {
        var opener = Expect(TokenType.LBrack, "expected '[' to open list");
        var values = ParseExpressionList(TokenType.RBrack);
        var closer = Expect(TokenType.RBrack, "expected ']' to close list");

        return new Node(opener.Start, GetLength(opener, closer), NodeKind.List, children: [.. values]);
    }

    private Node ParseFunction(List<Node> annotations)
    {
        var func = Expect(TokenType.Func, "expected 'fn' keyword");
        var name = ParseIdentifier(); 
        var parameters = ParseParameters();
        SkipNewLines();
        var body = ParseBlock(); 

        return new Node(func.Start, GetLength(func, body), NodeKind.Function, children: [name, parameters, body])
        {
            Annotations = annotations,
        };
    }

    private Node ParseLambda()
    {
        var func = Expect(TokenType.Func, "expected 'fn' keyword");

        var parameters = ParseParameters();

        Expect(TokenType.Return, "expected '->' after lambda parameters");

        var expr = ParseExpression();
        var ret = new Node(expr.Start, expr.Length, NodeKind.Return, children: [expr]);
        var body = new Node(ret.Start, ret.Length, NodeKind.Block, children: [ret]);

        return new Node(func.Start, GetLength(func, body), NodeKind.Lambda, children: [parameters, body]);
    }

    private Node ParseParameters()
    {
        var opener = Expect(TokenType.LParen, "expected '(' before parameters");
        var parameters = new List<Node>();

        if (Current.Type != TokenType.RParen)
        {
            while (true)
            {
                parameters.Add(ParseParameter());

                if (Match(TokenType.Comma))
                    continue;                
                break;
            }
        }

        var closer = Expect(TokenType.RParen, "expected ')' after parameters");

        return new Node(opener.Start, GetLength(opener, closer), NodeKind.Tuple, children: [.. parameters]);
    }

    private Node ParseParameter()
    {
        Token? star = null;

        if (Current.Type is TokenType.Asterisk or TokenType.DoubleAsterisk)
            star = Next();

        var name = ParseIdentifier();
        Node value = name;

        if (star is not null)        
            value = new Node(star.Value.Start, GetLength(star.Value, name), NodeKind.Unary, star.Value.Type, name);        

        if (Match(TokenType.Colon))
        {
            var type = ParseType();

            value = new Node(value.Start, type.Start + type.Length - value.Start, NodeKind.Typed, children: [value, type]);
        }

        if (Match(TokenType.Assign))
        {
            var expr = ParseExpression();

            value = new Node(value.Start, expr.Start + expr.Length - value.Start, NodeKind.Assign, TokenType.Assign, value, expr);
        }

        return new Node(value.Start, value.Length, NodeKind.Parameter, children: [value]);
    }

    private Node ParseType()
    {
        var start = Current.Start;
        var type = ParseIdentifier();

        if (Match(TokenType.LBrack))
        {
            var innerStart = type.Start;
            if (Current.Type != TokenType.RBrack)
            {
                ParseType();
                while (Match(TokenType.Comma))
                    ParseType();
            }
            var rbrack = Expect(TokenType.RBrack, "expected ']' after type parameters");
            var length = GetLength(type, rbrack);
            type = new Node(innerStart, length, NodeKind.Identifier)
            {
                Value = code.Substring(innerStart, length)
            };
            start = innerStart;
        }

        while (Match(TokenType.BOr))
        {
            var right = ParseType();
            var length = GetLength(type, right);
            var combinedStart = type.Start;
            type = new Node(combinedStart, length, NodeKind.Identifier)
            {
                Value = code.Substring(combinedStart, length)
            };
        }

        return type;
    }

    private Node ParseBlock()
    {
        SkipNewLines();

        Expect(TokenType.Indent, "expected indented block");

        var statements = new List<Node>();

        while (Current.Type != TokenType.Dedent && Current.Type != TokenType.EOF)
        {
            SkipNewLines();

            if (Current.Type == TokenType.Dedent)
                break;

            statements.Add(ParseStatement());

            SkipNewLines();
        }

        Expect(TokenType.Dedent, "expected end of block");

        if (statements.Count == 0)
            return new Node(Current.Start, 0, NodeKind.Block);

        return new Node(statements[0].Start, GetLength(statements[0], statements[^1]), NodeKind.Block, children: [.. statements]);
    }

    private Node ParsePattern()
    {
        var first = ParsePrimaryPattern();

        if (!Match(TokenType.Comma))
            return first;

        var items = new List<Node> { first };

        do
        {
            items.Add(ParsePrimaryPattern());
        }
        while (Match(TokenType.Comma));

        return new Node(first.Start, GetLength(first, items[^1]), NodeKind.Tuple, children: [.. items]);
    }

    private Node ParsePrimaryPattern()
    {
        if (Match(TokenType.Underscore))
        {
            var token = Previous();
            return new Node(token.Start, token.Length, NodeKind.Wildcard);
        }

        var expr = ParsePostfix();

        if (Match(TokenType.Colon))
        {
            var type = ParseType();
            return new Node(expr.Start, GetLength(expr, type), NodeKind.Typed, children: [expr, type]);
        }

        return expr;
    }

    private Node RequireNoAnnotations(List<Node> annotations, Func<Node> parser)
    {
        if (annotations.Count > 0)
            throw new Error("annotations are not allowed here", annotations[0].Start, annotations[^1].Start + annotations[^1].Length - annotations[0].Start, source);

        return parser();
    }

    private string GetText(int start, int length) => code.Substring(start, length);

    private string RemoveQuotes(string text)
    {
        if (text.Length >= 6 &&
            ((text.StartsWith("\"\"\"") && text.EndsWith("\"\"\"")) ||
             (text.StartsWith("'''") && text.EndsWith("'''"))))
            return text[3..^3];

        return text[1..^1];
    }

    private string Unescape(string s)
    {
        var sb = new StringBuilder(s.Length);

        for (int i = 0; i < s.Length; i++)
        {
            if (s[i] != '\\')
            {
                sb.Append(s[i]);
                continue;
            }

            if (++i >= s.Length)
                break;

            sb.Append(s[i] switch
            {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                '\'' => '\'',
                _ => s[i]
            });
        }

        return sb.ToString();
    }

    private int GetLength(ISpan start, ISpan end) => end.Start + end.Length - start.Start;

    private int GetBinaryPrecedence(TokenType type)
    {
        return type switch
        {
            TokenType.Or => 1,
            TokenType.Xor => 2,
            TokenType.And => 3,

            TokenType.Equal => 4,
            TokenType.Unequal => 4,

            TokenType.LessThan => 5,
            TokenType.LessOrEqual => 5,
            TokenType.GreaterThan => 5,
            TokenType.GreaterOrEqual => 5,
            TokenType.Is => 5,
            TokenType.IsNot => 5,
            TokenType.In => 5,

            TokenType.BOr => 6,
            TokenType.BXor => 7,
            TokenType.BAnd => 8,

            TokenType.LeftShift => 9,
            TokenType.RightShift => 9,

            TokenType.Plus => 10,
            TokenType.Minus => 10,

            TokenType.Asterisk => 11,
            TokenType.Slash => 11,
            TokenType.DoubleSlash => 11,
            TokenType.Percent => 11,

            TokenType.DoubleAsterisk => 12,

            TokenType.DoubleQuestion => 13,

            _ => 0
        };
    }

    private bool IsAssignmentStart()
    {
        return Current.Type == TokenType.Identifier || Current.Type == TokenType.LParen || Current.Type == TokenType.Underscore;
    }

    private bool IsValidAssignmentTarget(Node node)
    {
        return node.Kind switch
        {
            NodeKind.Identifier => true,
            NodeKind.Property => true,
            NodeKind.Index => true,
            NodeKind.Slice => true,
            NodeKind.Wildcard => true,
            NodeKind.Typed => IsValidAssignmentTarget(node.Children[0]),
            NodeKind.Tuple => node.Children.All(IsValidAssignmentTarget),

            _ => false
        };
    }

    private bool IsAssignmentOperator(TokenType type)
    {
        return type is
            TokenType.Assign or
            TokenType.PlusAssign or
            TokenType.MinusAssign or
            TokenType.AsteriskAssign or
            TokenType.SlashAssign or
            TokenType.DoubleSlashAssign or
            TokenType.DoubleAsteriskAssign or
            TokenType.PercentAssign or
            TokenType.BAndAssign or
            TokenType.BOrAssign or
            TokenType.BXorAssign or
            TokenType.LeftShiftAssign or
            TokenType.RightShiftAssign or
            TokenType.DoubleQuestionAssign;
    }
}