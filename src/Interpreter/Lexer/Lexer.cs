namespace Un;

public enum NumberState
{
    Integer,
    Fraction,
    Exponent,
    ExponentDigits,
    Hex,
    Binary,
    Octal
}

public class Lexer(Source source)
{
    private int index = 0;
    private readonly Source source = source;
    private readonly Stack<int> indents = new([0]);
    private bool lineStart = true;

    private bool EOF => index >= source.Code.Length;

    public List<Token> Tokenize()
    {
        List<Token> tokens = [];

        while (!EOF)
        {
            if (lineStart)
            {
                int indent = 0;
                int start = index;

                while (true)
                {
                    if (Consume(' '))
                    {
                        indent++;
                    }
                    else if (Consume('\t'))
                    {
                        indent += 4;
                    }
                    else
                    {
                        break;
                    }
                }

                if (Peek('\n'))
                {
                    index++;
                    tokens.Add(new Token(start, 1, TokenType.NewLine));
                    lineStart = true;
                    continue;
                }

                int current = indents.Peek();

                if (indent > current)
                {
                    indents.Push(indent);
                    tokens.Add(new Token(start, indent, TokenType.Indent));
                }
                else
                {
                    while (indent < indents.Peek())
                    {
                        indents.Pop();
                        tokens.Add(new Token(start, 0, TokenType.Dedent));
                    }

                    if (indent != indents.Peek())
                    {
                        tokens.Add(Error(start, "invalid indentation"));
                        break;
                    }
                }

                lineStart = false;
            }

            if (Consume('\n'))
            {
                tokens.Add(new Token(index - 1, 1, TokenType.NewLine));
                lineStart = true;
                continue;
            }

            if (char.IsWhiteSpace(Peek()))
            {
                index++;
                continue;
            }

            var token = Peek() switch
            {
                '"' or '\'' => GetStringToken(),
                '`' => GetFStringToken(),
                >= '0' and <= '9' => GetNumberToken(),
                _ when char.IsAsciiLetter(Peek()) || Peek() == '_' => GetIdentifierToken(),
                _ => GetOperatorToken()
            };

            if (token.Type == TokenType.Error)
                throw new Error(token.Message ?? "syntax error", token.Start, token.Length, source);

            tokens.Add(token);
        }

        while (indents.Count > 1)
        {
            indents.Pop();
            tokens.Add(new Token(index, 0, TokenType.Dedent));
        }

        tokens.Add(new Token(index, 0, TokenType.EOF));

        return tokens;
    }

    private Token GetStringToken()
    {
        int start = index;
        char quote;

        if (Consume('"'))
            quote = '"';
        else if (Consume('\''))
            quote = '\'';
        else
            return Error(start, "expected opening quote for string");

        bool triple = Consume(quote) && Consume(quote);

        while (index < source.Code.Length)
        {
            if (Peek('\\'))
            {
                index += 2;
                continue;
            }

            if (!triple)
            {
                if (Peek(quote))
                {
                    index++;
                    return new Token(start, index - start, TokenType.String);
                }
            }
            else
            {
                if (!OutOfRange(index + 2) && Peek(quote) && source.Code[index + 1] == quote && source.Code[index + 2] == quote)
                {
                    index += 3;
                    return new Token(start, index - start, TokenType.String);
                }
            }

            index++;
        }

        return Error(start, triple ? $"expected closing {new string(quote, 3)}" : $"expected closing {quote}");
    }
    private Token GetFStringToken()
    {
        int start = index;

        if (!Consume('`'))
            return Error(start, "expected opening backtick");

        bool triple = Consume('`') && Consume('`');

        while (!OutOfRange(index))
        {
            if (Peek('\\'))
            {
                if (OutOfRange(index + 1))
                    return Error(start, "unterminated escape sequence");

                index += 2;
                continue;
            }

            if (!triple)
            {
                if (Peek('`'))
                {
                    index++;
                    return new Token(start, index - start, TokenType.FString);
                }
            }
            else
            {
                if (!OutOfRange(index + 2) && Peek('`') && source.Code[index + 1] == '`' && source.Code[index + 2] == '`')
                {
                    index += 3;
                    return new Token(start, index - start, TokenType.FString);
                }
            }

            index++;
        }

        return Error(start, triple ? "expected closing triple backtick" : "expected closing backtick");
    }
    private Token GetNumberToken()
    {
        int start = index;
        bool isFloat = false;
        bool lastUnderscore = false;

        bool ReadDigits(Func<char, bool> predicate)
        {
            bool found = false;

            while (true)
            {
                if (Consume(predicate))
                {
                    found = true;
                    lastUnderscore = false;
                    continue;
                }

                if (Consume('_'))
                {
                    if (!found)
                        return false;

                    if (lastUnderscore)
                        throw new Exception("consecutive underscores are not allowed");

                    lastUnderscore = true;
                    continue;
                }

                break;
            }

            return found;
        }

        if (Consume('0'))
        {
            if (Consume('x') || Consume('X'))
            {
                try
                {
                    if (!ReadDigits(char.IsAsciiHexDigit))
                        return Error(start, "expected hex digit");
                }
                catch (Exception e)
                {
                    return Error(start, e.Message);
                }

                goto Validate;
            }

            if (Consume('b') || Consume('B'))
            {
                try
                {
                    if (!ReadDigits(c => c is '0' or '1'))
                        return Error(start, "expected binary digit");
                }
                catch (Exception e)
                {
                    return Error(start, e.Message);
                }

                goto Validate;
            }

            if (Consume('o') || Consume('O'))
            {
                try
                {
                    if (!ReadDigits(c => c is >= '0' and <= '7'))
                        return Error(start, "expected octal digit");
                }
                catch (Exception e)
                {
                    return Error(start, e.Message);
                }

                goto Validate;
            }

            index--;
        }

        try
        {
            if (!ReadDigits(char.IsDigit))
                return Error(start, "expected digit");

            if (Consume('.'))
            {
                if (lastUnderscore)
                    return Error(start, "underscore cannot precede decimal point");

                isFloat = true;

                if (Peek() == '_')
                    return Error(start, "underscore cannot follow decimal point");

                ReadDigits(char.IsDigit);
            }

            if (Consume('e') || Consume('E'))
            {
                if (lastUnderscore)
                    return Error(start, "underscore cannot precede exponent");

                isFloat = true;

                _ = Consume('+') || Consume('-');

                if (!ReadDigits(char.IsDigit))
                    return Error(start, "expected digit after exponent");
            }
        }
        catch (Exception e)
        {
            return Error(start, e.Message);
        }

    Validate:

        if (lastUnderscore)
            return Error(start, "number cannot end with underscore");

        if (Consume(c => char.IsAsciiLetterOrDigit(c) || c == '_'))
            return Error(start, "invalid number format");

        return new Token(start, index - start, isFloat ? TokenType.Float : TokenType.Integer);
    }
    private Token GetIdentifierToken()
    {
        int start = index;

        if (!Consume(c => char.IsAsciiLetter(c) || c == '_'))
            return Error(start, "expected identifier");

        while (Consume(c => char.IsAsciiLetterOrDigit(c) || c == '_')) { }

        ReadOnlySpan<char> text = source.Code.AsSpan(start, index - start);

        TokenType type = text switch
        {
            "fn" => TokenType.Func,
            "class" => TokenType.Class,
            "enum" => TokenType.Enum,

            "true" => TokenType.True,
            "false" => TokenType.False,

            "if" => TokenType.If,
            "elif" => TokenType.ElIf,
            "else" => TokenType.Else,
            "match" => TokenType.Match,

            "for" => TokenType.For,
            "while" => TokenType.While,

            "break" => TokenType.Break,
            "skip" => TokenType.Skip,

            "use" => TokenType.Use,
            "using" => TokenType.Using,

            "in" => TokenType.In,
            "is" => TokenType.Is,
            "as" => TokenType.As,

            "and" => TokenType.And,
            "or" => TokenType.Or,
            "xor" => TokenType.Xor,
            "not" => TokenType.Not,

            "go" => TokenType.Go,
            "wait" => TokenType.Wait,

            "try" => TokenType.Try,
            "defer" => TokenType.Defer,

            "none" => TokenType.None,
            "_" => TokenType.Underscore,

            _ => TokenType.Identifier
        };

        return new Token(start, text.Length, type);
    }
    private Token GetOperatorToken()
    {
        int start = index;

        return Peek() switch
        {
            '(' => Create(TokenType.LParen, 1),
            ')' => Create(TokenType.RParen, 1),

            '{' => Create(TokenType.LBrace, 1),
            '}' => Create(TokenType.RBrace, 1),

            '[' => Create(TokenType.LBrack, 1),
            ']' => Create(TokenType.RBrack, 1),

            ',' => Create(TokenType.Comma, 1),
            ':' => Create(TokenType.Colon, 1),

            _ => GetComplexOperatorToken()
        };

        Token Create(TokenType type, int length)
        {
            index += length;
            return new Token(start, length, type);
        }
    }
    private Token GetComplexOperatorToken()
    {
        int start = index;

        if (Match("??=")) return Token(TokenType.DoubleQuestionAssign, 3);
        if (Match("<<=")) return Token(TokenType.LeftShiftAssign, 3);
        if (Match(">>=")) return Token(TokenType.RightShiftAssign, 3);
        if (Match("//=")) return Token(TokenType.DoubleSlashAssign, 3);
        if (Match("**=")) return Token(TokenType.DoubleAsteriskAssign, 3);

        if (Match("??")) return Token(TokenType.DoubleQuestion, 2);
        if (Match("?.")) return Token(TokenType.QuestionDot, 2);

        if (Match("+=")) return Token(TokenType.PlusAssign, 2);
        if (Match("-=")) return Token(TokenType.MinusAssign, 2);
        if (Match("*=")) return Token(TokenType.AsteriskAssign, 2);
        if (Match("/=")) return Token(TokenType.SlashAssign, 2);
        if (Match("%=")) return Token(TokenType.PercentAssign, 2);

        if (Match("&=")) return Token(TokenType.BAndAssign, 2);
        if (Match("|=")) return Token(TokenType.BOrAssign, 2);
        if (Match("^=")) return Token(TokenType.BXorAssign, 2);

        if (Match("==")) return Token(TokenType.Equal, 2);
        if (Match("!=")) return Token(TokenType.Unequal, 2);

        if (Match("<=")) return Token(TokenType.LessOrEqual, 2);
        if (Match(">=")) return Token(TokenType.GreaterOrEqual, 2);

        if (Match("<<")) return Token(TokenType.LeftShift, 2);
        if (Match(">>")) return Token(TokenType.RightShift, 2);

        if (Match("//")) return Token(TokenType.DoubleSlash, 2);
        if (Match("**")) return Token(TokenType.DoubleAsterisk, 2);

        if (Match("->")) return Token(TokenType.Return, 2);

        return Peek() switch
        {
            '=' => Token(TokenType.Assign, 1),
            '+' => Token(TokenType.Plus, 1),
            '-' => Token(TokenType.Minus, 1),
            '*' => Token(TokenType.Asterisk, 1),
            '/' => Token(TokenType.Slash, 1),
            '%' => Token(TokenType.Percent, 1),

            '&' => Token(TokenType.BAnd, 1),
            '|' => Token(TokenType.BOr, 1),
            '^' => Token(TokenType.BXor, 1),

            '~' => Token(TokenType.BNot, 1),
            '!' => Token(TokenType.Bang, 1),

            '<' => Token(TokenType.LessThan, 1),
            '>' => Token(TokenType.GreaterThan, 1),

            '.' => Token(TokenType.Dot, 1),
            '?' => Token(TokenType.Question, 1),

            '@' => Token(TokenType.At, 1),

            _ => Error(start, "unknown operator")
        };

        Token Token(TokenType type, int length)
        {
            index += length;
            return new Token(start, length, type);
        }
    }

    private Token Error(int start, string message)
    {
        return new Token(start, index - start, TokenType.Error)
        {
            Message = message
        };
    }

    private char Peek()
    {
        if (EOF) return '\0';
        return source.Code[index];
    }

    private bool Peek(char c)
    {
        if (EOF) return false;
        return source.Code[index] == c;
    }

    private bool Consume(char c)
    {
        if (EOF || source.Code[index] != c) return false;
        index++;
        return true;
    }
    
    private bool Consume(Func<char, bool> predicate)
    {
        if (EOF || !predicate(source.Code[index])) return false;
        index++;        
        return true;
    }

    private bool Match(string text)
    {
        if (OutOfRange(index + text.Length))
            return false;

        return source.Code.AsSpan(index, text.Length).SequenceEqual(text);
    }

    private bool OutOfRange(int length) => source.Code.Length <= length;

}