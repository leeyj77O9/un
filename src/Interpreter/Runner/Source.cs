using System.Text;

namespace Un;

public class Source(string path, string code)
{
    public string Path => path;
    public string Code => code;
    public string Name => System.IO.Path.GetFileNameWithoutExtension(path);

    public HashSet<int> IgnoredNewLines { get; } = [];

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
        int start = GetLineStart(position);

        var sb = new StringBuilder();

        while (true)
        {
            int end = start;

            while (end < Code.Length && Code[end] != '\n')
                end++;

            sb.Append(Code, start, end - start);

            if (end >= Code.Length || !IgnoredNewLines.Contains(end))
                break;

            start = end + 1;

            while (start < Code.Length &&
                  (Code[start] == ' ' || Code[start] == '\t'))
            {
                start++;
            }
        }

        return sb.ToString();
    }

    public int GetLineStart(int position)
    {
        while (position > 0 && Code[position - 1] != '\n')
            position--;

        return position;
    }
}