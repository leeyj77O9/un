namespace Un;

public class Buffer
{
    public char[] chars;
    private int length;

    public int Length => length;

    public Buffer(int capacity = 16)
    {
        chars = new char[capacity];
        length = 0;
    }

    public char this[int index]
    {
        get { return chars[index]; } 
        set { chars[index] = value; }
    }

    public void Add(char c)
    {
        if (length >= chars.Length)        
            Grow();
        
        chars[length++] = c;
    }

    public void Add(string s)
    {
        foreach (char c in s)
            Add(c);
    }

    public void Add(Buffer b)
    {
        for (int i = 0; i < b.Length; i++)
            Add(b.chars[i]);
    }

    private void Grow()
    {
        int capacity = chars.Length * 9 / 4 + 1;
        char[] newChars = new char[capacity];
        Array.Copy(chars, newChars, length);
        chars = newChars;      
    }

    public override string ToString()
    {
        return new string(chars, 0, length);
    }

    public void Clear()
    {
        length = 0;
    }
}

