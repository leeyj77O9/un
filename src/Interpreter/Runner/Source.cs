namespace Un;

public sealed record Source(string Path, string Code)
{
    public string Name => System.IO.Path.GetFileNameWithoutExtension(Path);
    public string FullName => System.IO.Path.GetFileName(Path);

    public int GetLine(int position)
    {
        int line = 1;

        for (int i = 0; i < position && i < Code.Length; i++)
        {
            if (Code[i] == '\n')
                line++;
        }

        return line;
    }

    public int GetColumn(int position)
    {
        int lineStart = 0;

        for (int i = 0; i < position && i < Code.Length; i++)
        {
            if (Code[i] == '\n')
                lineStart = i + 1;
        }

        return position - lineStart + 1;
    }

    public string GetLineText(int position)
    {
        int start = position;

        while (start > 0 && Code[start - 1] != '\n')
            start--;

        int end = position;

        while (end < Code.Length && Code[end] != '\n')
            end++;

        return Code[start..end];
    }

    public int GetLineStart(int position)
    {
        while (position > 0 && Code[position - 1] != '\n')
            position--;

        return position;
    }
}