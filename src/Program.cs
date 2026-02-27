using Un;

Runner? runner = null;

try
{
    if (args.Length == 1 && args[0] == "--test")
    {
        foreach (var file in Directory.GetFiles("test/", "*.un"))
        {
            Console.WriteLine($"Running test: {file}");
            Global.Init("");
            runner = Runner.Load(file, Global.GetGlobalScope());
            runner.Run();
            Console.WriteLine($"Test {file} passed.");
        }
    }
    else if (args.Length == 2)
    {
        Global.Init(args[0]);
        runner = Runner.Load(args[1], Global.GetGlobalScope());
        runner.Run();
    }
    else
    {    
        Global.Init("/workspaces/Un/src/");
        runner = Runner.Load("main.un", Global.GetGlobalScope());
        runner.Run();    
        //throw new Panic("not enough arguments");
    }
}
catch (Exception e)
{
    Console.ForegroundColor = ConsoleColor.Red;
    Console.WriteLine(e.ToString());
    Console.ResetColor();

    if (runner is not null && runner.Context.BlockStackTrace.Length > 1)
    {
        // trace 제목 노란색
        Console.ForegroundColor = ConsoleColor.Yellow;
        Console.WriteLine("    trace:");
        Console.ResetColor();

        var blockStack = runner.Context.BlockStackTrace;

        var blocksToPrint = blockStack.Length > 11
            ? blockStack.Skip(1).Take(10).Reverse()
            : blockStack.Skip(1).Reverse();

        foreach (var block in blocksToPrint)
        {
            // 코드 줄: 기본색, 코드 텍스트 강조 파랑
            Console.Write("\t");
            Console.ForegroundColor = ConsoleColor.Blue;
            Console.Write(block.Code.Trim());
            Console.ResetColor();
            Console.WriteLine($" :[{block.Line + 1}] ({block.Type})");
        }

        if (blockStack.Length > 10)
        {
            Console.ForegroundColor = ConsoleColor.Gray;
            Console.WriteLine($"\t... (truncated [{blockStack.Length - 11}+])");
            Console.ResetColor();
        }
    }
}