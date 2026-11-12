namespace Un;

public record RunResult(string Stdout, string? Error, bool TimedOut, int ExitCode);

public static class PlaygroundRunner
{
    private static readonly string[] DefaultAllowed = ["io", "math", "iter", "re", "random", "time", "builtin", "atomic"];

    public static RunResult RunCode(string code, string fileName = "playground.un", int timeoutMs = 3000, string[]? allowedModules = null)
    {
        if (!Global.TryGetGlobalVariable("panic", out _))
        {
            Global.Init("memory://");
        }

        var memFs = new MemoryFileSystem();
        var originalFs = Global.FileSystem;
        var originalPath = Global.PATH;
        var originalOut = Native.IO.GetStdout();
        var originalIn = Native.IO.GetStdin();
        var originalAllowed = Global.GetAllowedModules();
        var originalAllowExit = Global.AllowProcessExit;
        var snapshot = Global.CaptureState();

        var stdoutMem = new MemoryStream();
        var stdoutStream = new Object.IO.Stream(stdoutMem);

        try
        {
            Global.FileSystem = memFs;
            Global.SetPath("memory://");
            Global.SetAllowedModules(allowedModules ?? DefaultAllowed);
            Global.AllowProcessExit = false;

            Native.IO.SetStdout(stdoutStream);

            var source = new Source($"memory://{fileName}", code);
            var scope = new Scope(Global.GetGlobalScope());
            var context = new Context(scope, source, []);
            var runner = new Runner(context);

            string? error = null;
            bool timedOut = false;
            int exitCode = 0;

            if (OperatingSystem.IsBrowser())
            {
                try
                {
                    runner.Run();
                }
                catch (Error e)
                {
                    error = e.ToString();
                    exitCode = 1;
                }
                catch (Panic p)
                {
                    error = p.ToString();
                    exitCode = 1;
                }
                catch (Exception ex)
                {
                    error = ex.ToString();
                    exitCode = 1;
                }
            }
            else
            {
                var task = Task.Run(() =>
                {
                    try
                    {
                        runner.Run();
                    }
                    catch (Error e)
                    {
                        error = e.ToString();
                        exitCode = 1;
                    }
                    catch (Panic p)
                    {
                        error = p.ToString();
                        exitCode = 1;
                    }
                    catch (Exception ex)
                    {
                        error = ex.ToString();
                        exitCode = 1;
                    }
                });

                if (!task.Wait(timeoutMs))
                {
                    timedOut = true;
                    error = $"panic : execution timed out after {timeoutMs}ms";
                    exitCode = 124;
                }
            }

            stdoutStream.Writer?.Flush();
            var stdout = System.Text.Encoding.UTF8.GetString(stdoutMem.ToArray());

            return new RunResult(stdout, error, timedOut, exitCode);
        }
        finally
        {
            Global.RestoreState(snapshot);
            Global.FileSystem = originalFs;
            Global.SetPath(originalPath);
            Global.SetAllowedModules(originalAllowed);
            Global.AllowProcessExit = originalAllowExit;
            Native.IO.SetStdout(originalOut);
            Native.IO.SetStdin(originalIn);
        }
    }
}
