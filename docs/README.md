# Un

A modern scripting language with concise syntax, explicit asynchronous execution, and pattern matching.

Un is implemented as a tree-walking interpreter in C# on .NET 8. The processing pipeline is:

`Lexer -> Parser -> Desugarer -> Optimizer -> Evaluator`

<p align="center">
  <img src="https://img.shields.io/badge/.NET-8.0-512BD4?style=flat-square&logo=dotnet&logoColor=white" alt=".NET 8" />
  <img src="https://img.shields.io/badge/license-MIT-22C55E?style=flat-square" alt="MIT License" />
</p>

## Introduction

Un is a dynamically typed scripting language aimed at readable, compact programs. It combines Python-like indentation-based blocks with explicit concurrency primitives, structural pattern matching, and a small standard library.

The language is implemented entirely in C# as an interpreter. There is no separate compilation step or bytecode virtual machine: source files are parsed into an AST, desugared, optimized, and evaluated directly.

Design goals:

- Keep syntax small and readable.
- Make asynchronous execution explicit through `go` and `wait`.
- Provide practical collection and IO modules out of the box.
- Support gradual type annotations for documentation without requiring a static checker.

## Features

### Language core (stable)

- Variables and destructuring assignment
- Functions with default, keyword, and variadic parameters
- Lambdas and closures
- `if` / `elif` / `else`, `for`, `while`, `break`, `skip`
- `match` with pattern matching
- Classes, inheritance, `Stru`, and `Enu`
- Modules with `use`, aliases, and selective imports
- Resource management with `using` and `defer`
- String interpolation and tagged templates
- Collections: `list`, `dict`, `set`, `tuple`, `json`
- Slicing and indexing
- Null/error propagation with postfix `?`
- C-style ternary expression `cond ? a : b`
- Compound assignment `+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `**=`, `&=`, `|=`, `^=`, `<<=`, `>>=`, `??=`
- Assignment as an expression with `:=` (returns the assigned value)
- Null coalescing with `??`
- Safe property/method access with `?.`
- `try` / `catch` / `finally`

### Type system (experimental)

- Optional type annotations: `x: int`, `a: list[int]`, `dict[str, int]`, `int | str`
- Annotations are currently used for readability and gradual checking; strict runtime enforcement is being strengthened

### Concurrency (stable)

- `go` to run an expression in the background, returning a `Future`
- `wait` to await a `Future`
- `flow.spawn(n)` worker pools with `map`

### Planned

- Stronger static type checking
- LSP and editor integration
- Package manager

## Syntax Preview

Variables and functions:

```un
use io

fn add(a, b)
    -> a + b

fn greet(name = "world")
    -> `hello {name}`

io.write(add(3, 4))   # 7
io.write(greet())      # hello world
mul = fn(a, b) -> a * b
io.write(mul(6, 7))   # 42
```

Control flow and pattern matching:

```un
use io

x = 5
if x > 5
    io.write("big")
elif x == 5
    io.write("equal")
else
    io.write("small")

res = match x {
    0 -> "zero",
    1 -> "one",
    _ -> "other",
}
io.write(res)
```

Classes:

```un
use io

class Point(x, y)
    fn len()
        -> self.x + self.y

p = Point(10, 20)
io.write(p.x)      # 10
io.write(p.len())  # 30
```

Inheritance lists the bases after the class name. Multiple inheritance inherits
every base in declaration order, and the first base wins a name conflict.

```un
class BaseA
    a = 1
class BaseB
    b = 2
class C: BaseA, BaseB
    c = 3

io.write(C().a, C().b, C().c)        # 1 2 3
io.write(C() is BaseA, C() is BaseB)  # true true
```

Asynchronous execution:

```un
use io
use time

fn slow(x)
    time.sleep(100)
    -> x * 2

a = go slow(5)
io.write(wait a)   # 10
```

## Getting Started

### Requirements

- .NET SDK 8.0 or later

### Build

```bash
dotnet build -c Release
```

### Run

```bash
dotnet run -c Release -- run path/to/file.un

# or use the built executable
./bin/Release/net8.0/Un run path/to/file.un
```

On Windows the executable is `bin\Release\net8.0\Un.exe`.

### First program

```un
use io

fn main()
    io.write("Hello, Un!")

main()
```

Save as `hello.un` and run:

```bash
dotnet run -c Release -- run hello.un
```

### Tests and Benchmarks

Build and run a script directly:

```bash
dotnet build -c Release
dotnet run -c Release -- run test/test_suite.un
```

The full suite runs through a runner. It executes the `group|path` rows of
`test/MANIFEST.txt` with the built executable, counts exit codes and the
`N passed, M failed` line on stdout, and runs the rows of
`test/negative/manifest.txt` that check exit codes and stderr.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test.ps1
```

Files listed in `test/KnownFailures.txt` are reported as KNOWN FAIL and are not
counted in the Positive pass/fail totals.

The benchmark scripts in `bench/` are measured with warmup, repeated runs, and
statistics, and saved to `bench/results/<timestamp>/results.csv` and `results.md`.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group all
```

`docs/BENCHMARK_GUIDE.md` defines the comparison rules (equal input, algorithm,
warmup, repeated runs, recorded environment) and `docs/BENCHMARK_LOG.md` records the
measured values and their limits. `docs/TEST_GUIDE.md` describes the test layout and
rules.

## Language Overview

### Variables and destructuring

```un
x = 10
name = "Un"
value: int = 20
a, b = (1, 2)
```

### Functions

```un
fn add(a, b)
    -> a + b

fn greet(name = "world")
    -> `hello {name}`

mul = fn(a, b) -> a * b
```

Functions use an indented block for their body. Lambdas use `->` on a single line.

### Control flow

```un
if x > 5
    io.write("big")
elif x == 5
    io.write("equal")
else
    io.write("small")

for v in [1, 2, 3]
    io.write(v)

n = 0
while n < 5
    io.write(n)
    n += 1
```

`break` and `skip` are available inside loops. A `->` inside a loop body ends
the loop and returns from the enclosing function.

### Pattern matching

```un
res = match value {
    0 -> "zero",
    (x, 0) -> `x={x}`,
    _ -> "other",
}
```

### Types

```un
x: int = 10
y: str = "hi"
a: list[int] = [1, 2]
b: dict[str, int] = {}
c: int | none = none
```

### Asynchronous execution

```un
use time

fn slow(x)
    time.sleep(100)
    -> x * 2

a = go slow(5)
io.write(wait a)
```

### Modules

```un
use io
use fs as filesystem
use re {test, find_all}
use re as re

io.write(re.test("\\d+", "abc123"))
```

### Resource management

```un
using file = io.open("log.txt", "w")
file.write("hello")

fn work()
    defer io.write("done")
    io.write("start")
```

## Standard Library

| Module | Main API |
|---|---|
| `io` | `write`, `read`, `open`, `clear` |
| `fs` | `read`/`write`/`append`, `exists`/`file`/`dir`, `list`/`files`/`dirs`/`walk`, `copy`/`move`/`delete`, `mkdir`/`rmdir`, `join`/`parent`/`name`/`ext` |
| `re` | `test`/`match`/`search`, `find_all`/`groups`, `replace`/`replace_all`/`split`/`escape` |
| `math` | `abs`/`sqrt`/`pow`/`log`/`sin`/`cos`/`tan`, `gcd`/`lcm`/`clamp`, `pi`/`e`/`tau` |
| `iter` | `range`/`counter`/`reverse`/`repeat`, `zip`/`enumerate`, `map`/`filter`/`take`/`skip`/`chain`/`flatten`, `sum`/`max`/`min`/`sorted` |
| `random` | `int`/`float`/`bool`/`choice`/`shuffle`, `sample`/`choices`/`weighted`, `seed`/`uuid` |
| `os` | `env`/`environ`/`exec`, `pid`/`hostname`/`args`, `sep`/`pathsep` |
| `time` | `sleep`/`now`/`stopwatch` |
| `http`/`ws` | `http.connect().get`/`post`/`put`/`delete`, `ws.connect().send`/`receive` |
| `inspect` | `attr`/`hasattr`/`getattr`/`setattr` |

Collection methods include `list.add`/`extend`/`sort`/`reverse`/`hpush`/`hpop`, `dict.get`/`keys`/`values`, `set.add`/`union`/`intersect`, and string methods such as `split`/`join`/`trim`/`center`/`replace`.

## Architecture

The interpreter is a single-pass pipeline over an AST:

```
Source (.un)
  |
  v
Lexer          indentation-aware tokenizer
  |
  v
Parser         AST construction (Node / NodeKind)
  |
  v
Desugarer      compound assignment expansion, if/elif normalization
  |
  v
Optimizer      constant folding, algebraic simplification, up to 64 passes
  |
  v
Evaluator      Context / Scope / Frame based execution, Future for go/wait
  |
  v
Runtime        Obj / Val / Ref / Enu / Stru / TObj and the type system
  |
  v
Stdlib         NativeModule / NativeType reflection binding
```

Key components:

- `src/Interpreter/Runner/Runner.cs` loads, parses, desugars, optimizes, and evaluates a source file. `Source` tracks line and column information.
- `src/Interpreter/Runner/Scope.cs` provides slot-based symbol tables with parent chains.
- `src/Core/Global.cs` registers built-in and native modules and resolves `import` / `include`.
- `src/Interpreter/Reflection/` binds native modules and types through attributes.

There is no intermediate bytecode or separate virtual machine.

## Project Structure

```
Un/
|-- Un.sln
|-- Un.csproj
|-- README.md
|-- LICENSE
|-- src/
|   |-- Program.cs
|   |-- PlaygroundRunner.cs
|   |-- main.un
|   |-- Core/
|   |   |-- Global.cs
|   |   |-- IO/
|   |   |-- Native/        (Builtin, Fs, Http, Inspect, IO, Iter, Math, Os, Random, Re, Sys, Time, Ws)
|   |   `-- Object/       (runtime object model, collections, functions, flow, IO, iters, primitives, types, utils)
|   `-- Interpreter/
|       |-- Lexer/
|       |-- Parser/
|       |-- Desugarer/
|       |-- Optimizer/
|       |-- Evaluator/
|       |-- Runner/
|       |-- Reflection/
|       |-- Error/
|       `-- Util/
|-- test/                  (.un test scripts and fixtures)
|   |-- unit/            (unit/lexer, unit/parser, unit/runtime, unit/native)
|   |-- integration/
|   |-- regression/
|   |-- negative/        (scripts checking exit code and stderr, plus manifest.txt)
|   |-- fixtures/        (circular/, rel_test/, dirmod/, dirall/)
|   |-- MANIFEST.txt     (group|path rows)
|   |-- KnownFailures.txt
|   `-- assert.un        (assert module)
|-- scripts/             (test.ps1, bench.ps1)
|-- bench/               (benchmark scripts, bench/node/, bench/results/)
`-- bin/, obj/             (generated build output)
```

## Examples

Floyd-Warshall (`test/integration/algorithms.un`):

```un
n = 4
INF = 99999999
graph = [[0,5,INF,10],[INF,0,3,INF],[INF,INF,0,1],[INF,INF,INF,0]]
for k in range(0,n)
    for i in range(0,n)
        for j in range(0,n)
            if graph[i][j] > graph[i][k] + graph[k][j]
                graph[i][j] = graph[i][k] + graph[k][j]
```

Heap (`test/integration/algorithms.un`):

```un
class Heap
    data = []
    fn push(x)
        self.data.add(x)
        self.up(len(self.data)-1)
    fn pop()
        top = self.data[0]
        self.data[0] = self.data[len(self.data)-1]
        self.data.pop()
        self.down(0)
        -> top
```

More examples are available in `test/test_suite.un`, `test/integration/algorithms.un`,
`test/unit/runtime/classes.un`, and `bench/`. The procedures for running them are in
`docs/TEST_GUIDE.md` and `docs/BENCHMARK_GUIDE.md`.

## Roadmap

Implemented:

- Interpreter pipeline (lexer, parser, desugarer, optimizer, evaluator)
- Runtime object model and type system
- Standard library modules
- Asynchronous execution and worker pools
- Classes, inheritance, enums, and structs
- Pattern matching and destructuring
- Modules and selective imports
- Slicing, ternary expressions, and null/error propagation
- `try`/`catch`/`finally`, `using`, and `defer`

In progress:

- Gradual type annotation enforcement

Planned:

- Stronger static type checking
- LSP and VS Code extension
- Package manager

## Contributing

1. Fork and clone the repository.
2. Build with `dotnet build -c Release` and confirm zero warnings and errors.
3. Create a branch named `feat/my-feature`.
4. Add `.un` tests to the right group under `test/` and register a `group|path` row in
   `test/MANIFEST.txt`. Tests that check exit codes and stderr go in `test/negative/`
   with a `path|expected exit|expected substring` row in `test/negative/manifest.txt`.
5. Open a pull request. Commits are made by the contributor; automated tooling does not push to git.

Coding conventions: `ImplicitUsings` and `Nullable` are enabled. Native and built-in members are registered through attributes.

## License

MIT License. Copyright (c) 2024 Usejun.

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files, to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software.