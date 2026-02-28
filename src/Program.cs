using System.Diagnostics;
using Un;

Runner? runner = null;

#if DEBUG
try
{
    Global.Init("C:/Project/un/");
    runner = Runner.Load("src/main.un", Global.GetGlobalScope());
    runner.Run();
}
catch (Exception e)
{
    Environment.ExitCode = 1;
    PrintError(e);
}
#endif
#if RELEASE
if (args.Length == 0)
{
    PrintHelp();
    return;
}


var cmd = args[0];

switch (cmd)
{
    case "run":
        Run();
        break;
    case "help":
        PrintHelp();
        break;
    default:
        Console.WriteLine($"Unknown command: {cmd}");
        PrintHelp();
        break;
}
#endif

void PrintHelp()
{
    Console.WriteLine("Help:");
    Console.WriteLine("  un run <file.un>");
}

void PrintError(Exception e)
{
    Console.ForegroundColor = ConsoleColor.Red;
    Console.WriteLine(e);
    Console.ResetColor();

    if (runner is null) return;

    var stack = runner.Context.BlockStackTrace;
    if (stack.Length <= 1) return;

    Console.ForegroundColor = ConsoleColor.Yellow;
    Console.WriteLine("    trace:");
    Console.ResetColor();

    var blocksToPrint = stack.Length > 11
        ? stack.Skip(1).Take(10).Reverse()
        : stack.Skip(1).Reverse();

    foreach (var block in blocksToPrint)
    {
        Console.Write("\t");

        Console.ForegroundColor = ConsoleColor.Blue;
        Console.Write(block.Code.Trim());
        Console.ResetColor();

        Console.WriteLine($" :[{block.Line + 1}] ({block.Type})");
    }

    if (stack.Length > 10)
    {
        Console.ForegroundColor = ConsoleColor.Gray;
        Console.WriteLine($"\t... (truncated [{stack.Length - 11}+])");
        Console.ResetColor();
    }
}

void Run()
{
    if (args.Length < 2)
    {
        Console.WriteLine("Usage: un run <file>");
        return;
    }

    try
    {
        Global.Init(Environment.CurrentDirectory);
        runner = Runner.Load(args[1], Global.GetGlobalScope());
        runner.Run();
    }
    catch (Exception e)
    {
        Environment.ExitCode = 1;

        PrintError(e);
    }
}