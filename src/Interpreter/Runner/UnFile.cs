namespace Un;

using Code = (int nesting, string code);

public class UnFile
{
    public string Name { get; private set; }
    public string FullName { get; private set; }
    public string Path { get; private set; }
    public List<Code> Code { get; private set; }
    public int Index { get; private set; }
    public int Line { get; private set; }

    public bool EOF => Line >= Code.Count;
    public bool EOL => EOF || Index >= Code[Line].code.Length;

    private UnFile() { }

    public UnFile(string name, string[] code)
    {
        Path ??= name;
        FullName = name;
        Name = FullName.EndsWith(".un") ? FullName[..^3] : FullName;
        Line = 0;
        Index = 0;
        Code = [];

        InitCode(code);
    }

    public char Peek()
    {
        if (EOF)
            throw new Panic($"end of file {Name} reached");
        if (EOL)
        {
            if (Line + 1 < Code.Count)
                return Code[Line + 1].code[0];
            throw new Panic($"end of line {Name} reached");
        }
        
        return Code[Line].code[Index];
    }

    public char Read()
    {
        if (EOF)
            throw new Panic($"end of file {Name} reached");
        if (EOL)
        {
            if (Line + 1 < Code.Count)
            {
                Line++;
                Index = 0;
            }
            else
                throw new Panic($"end of line {Name} reached");
        }

        return Code[Line].code[Index++];
    }

    public bool TryPeekLine(out string code)
    {
        if (!EOF)
        {
            code = PeekLine();
            return true;
        }

        code = "";
        return false;
    }

    public string PeekLine() => Code[Line].code;

    public string GetLine()
    {
        Index = 0;
        return Code[Line++].code;
    }
    public void Move(int index, int line)
    {
        Index = index;
        Line = line;
    }

    public string[] GetBody()
    {
        var (start, end) = GetBodyRange();
        Index = 0;
        Line = end;

        return [.. Code[start..end].Select(x => x.code)];
    }


    private (int start, int end) GetBodyRange()
    {
        int nesting = Code[Line].nesting;
        int start = Line + 1, end = Line + 1;

        while (end < Code.Count && (Code[end].nesting > nesting || string.IsNullOrEmpty(Code[end].code)))
            end++;

        return (start, end);
    }

    private void InitCode(string[] rawcode)
    {
        foreach (var line in rawcode)
        {
            if (string.IsNullOrWhiteSpace(line))
                continue;

            var nesting = GetNesting(line);

            if (!string.IsNullOrWhiteSpace(line))
                Code.Add((nesting, line));
        }
    }

    private int GetNesting(string line)
    {
        int tab = 0, space = 0;

        foreach (var chr in line)
        {
            if (chr == '\t')
                tab++;
            else if (chr == ' ')
                space++;
            else
                break;
        }

        return Math.Max(tab, space / 4);
    }

    public UnFile Clone() => new()
    {
        Name = Name,
        FullName = FullName,
        Code = [.. Code],
        Path = Path,
        Index = 0,
        Line = 0,
    };
}