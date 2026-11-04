namespace Un.Native;

public sealed class NeedInputException(string prompt) : Exception("Input required")
{
    public string Prompt { get; } = prompt;
}