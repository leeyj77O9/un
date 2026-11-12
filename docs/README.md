# Un

A modern scripting language with concise syntax, explicit asynchronous execution, and pattern matching.

Un is implemented as a tree-walking interpreter in C# on .NET 8. The processing pipeline is:

`Lexer -> Parser -> Desugarer -> Optimizer -> Evaluator`

(An independent Rust implementation of the same language also lives in `rust/`, see the "Rust port" section. This document describes the C# implementation.)

<p align="center">
  <img src="https://img.shields.io/badge/.NET-8.0-512BD4?style=flat-square&logo=dotnet&logoColor=white" alt=".NET 8" />
  <img src="https://img.shields.io/badge/license-MIT-22C55E?style=flat-square" alt="MIT License" />
</p>

## Introduction

Un is a dynamically typed scripting language aimed at readable, compact programs. It combines Python-like indentation-based blocks with explicit concurrency primitives, structural pattern matching, and a small standard library.

The language is implemented primarily as a C# interpreter. There is no separate compilation step or bytecode virtual machine: source files are parsed into an AST, desugared, optimized, and evaluated directly. (An independent Rust bytecode VM port also lives in `rust/` and reproduces most of this document. See the "Rust port" section below.)

Design goals:

- Keep syntax small and readable.
- Make asynchronous execution explicit through `go` and `wait`.
- Provide practical collection and IO modules out of the box.
- Support gradual type annotations for documentation without requiring a static checker.

## Features

### Language core (stable)

- Variables and destructuring assignment
- Functions with default, keyword, and variadic parameters
- Function decorators (`@decorator`)
- Lambdas and closures
- `if` / `elif` / `else`, `for`, `while`, `break`, `skip`
- `match` with pattern matching
- Classes, inheritance, multiple inheritance, and `enum`
- Class properties with `get`/`set` accessors, `field`, and `value`
- Range operator `a..b` (half-open, lazily evaluated)
- Modules with `use`, aliases, and selective imports
- Resource management with `using` and `defer`
- Backtick string interpolation `` `..{expr}..` ``
- Collections: `list`, `dict`, `set`, `tuple`
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
- `flow.spawn(n)` worker pools with `map` and `submit`

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

Ordinary class members have a default getter and setter, and `get`/`set`
accessors override them. Inside an accessor `field` is the stored value of that
property and `value` is the value handed to the setter.

```un
use io

class Counter
    count = 0
    doubled = 0
    get doubled -> field * 2
    set doubled = value / 2

c = Counter()
io.write(c.doubled)   # 0
c.doubled = 10        # field = 5
io.write(c.doubled)   # 10
io.write(c.count)     # 0
```

Inheritance lists the bases after the class name. Multiple inheritance inherits
every base in declaration order, and the first base wins a name conflict.

```un
use io

class BaseA
    a = 1
class BaseB
    b = 2
class C: BaseA, BaseB
    c = 3

io.write(C().a, C().b, C().c)        # 1 2 3
io.write(C() is BaseA, C() is BaseB)  # true true
```

An `enum` gives its members integer values automatically.

```un
use io

enum Color
    RED
    GREEN
    BLUE

io.write(Color.RED, Color.GREEN, Color.BLUE)   # 0 1 2
```

The `..` operator builds a half-open range and does not materialize the values
eagerly. Its precedence sits between addition and comparison, so `1 + 2..10`
is `(1 + 2)..10`. Bounds must be integers or floats with integer values.

```un
use io

for i in 1..5
    io.write(i)              # 1 2 3 4

for 0..5
    io.write("tick")         # iterate without binding

io.write(list(0..3))         # [0, 1, 2]
io.write(len(0..100))        # 100
io.write(list(1 + 2..6))     # [3, 4, 5]
io.write(list((1..10).take(3)))  # [1, 2, 3]
```

When `for` is followed by just an iterable and no `in` target, the loop
iterates without binding each element — the same as discarding with `_`,
but written with the iterable alone.

```un
wild = 0
for 0..3
    wild += 1                # iterates 3 times
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
counted in the Positive pass/fail totals. The current baseline is Positive 36
files / 1330 passed / 0 failed / 0 crashed, Negative 42 checks matched, and
Known 1 file.

A `.NET 8` app runs on a `.NET 10` runtime when `DOTNET_ROLL_FORWARD=Major` is
set (PowerShell: `$env:DOTNET_ROLL_FORWARD='Major'`).

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
A `@decorator` line above a function wraps it with that decorator.

```un
use io

fn tag(target)
    fn wrapped(x)
        -> target(x)
    -> wrapped

@tag
fn tagged(x)
    -> x + 1

io.write(tagged(1))   # 2
```

### Control flow

```un
use io

x = 5
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
use io

value = (3, 0)
res = match value {
    0 -> "zero",
    (x, 0) -> `x={x}`,
    _ -> "other",
}
io.write(res)    # x=3
```

### Types

```un
use io

x: int = 10
y: str = "hi"
a: list[int] = [1, 2]
b: dict[str, int] = {}
c: int | str = 1
io.write(x, y, a, b, c)   # 10 hi [1, 2] {} 1
```

`none` is a reserved keyword, so it cannot be used as a type name in an
annotation. `x: none` and `int | none` are parse errors.

### Classes and properties

```un
use io

class Person
    name = "anon"            # ordinary members use the default getter/setter
    older = 1
    get older -> field + 1   # single line getter
    set older = value - 1    # single line setter
    get display
        -> `name={self.name}`

p = Person()
p.name = "bob"
io.write(p.older)    # 2
p.older = 10         # field = 9
io.write(p.older)    # 10
io.write(p.display)  # name=bob
```

Accessors may take a multi line body. Inside an accessor `field` is the stored
value of that property and `value` is the value handed to the setter. A getter
can read and write other members through `self`. A property default is cloned
per instance, so a mutable default such as `items = []` is not shared between
instances.

With only `get`, assignment writes the stored value directly; with only `set`,
reading returns the stored value. A constructor header (`class Person(name)`)
initializes the stored value without going through the setter. A child accessor
inherits the parent default. Declaring a method with the same name as a
property, or two getters for the same property, is an error.

A getter that reads its own property through `self.name` calls itself again
with no stop, so it ends in a stack overflow; a getter must reach its storage
through `field`. `test/negative/property_recursion.un` records this boundary as
an exit code.

### Ranges

```un
use io

for i in 1..5              # 1 2 3 4 (`..` is half-open)
    io.write(i)

io.write(list(0..3))       # [0, 1, 2]
io.write(len(0..100))      # 100
io.write(3 in 1..5)        # true
io.write(str(1..4))        # 1..4
io.write(list((1..10).map(fn(x) -> x * 2)))   # [2, 4, 6, 8, 10, 12, 14, 16, 18]
```

The range built by `..` belongs to the same lazy family as `iter`, and values
are produced only when something such as `list()` consumes them. `a..b` calls
`a.__range__(b)`, so a class can define range creation through the `__range__`
magic method. Precedence sits between addition and comparison, and a start
above the stop gives an empty range. Bounds must be integers or floats with
integer values.

### Asynchronous execution

```un
use io
use time

fn slow(x)
    time.sleep(100)
    -> x * 2

a = go slow(5)
io.write(wait a)    # 10
```

`flow.spawn(n)` is a worker pool. `pool.map` produces one result per argument, and
`pool.submit` hands back a `Future`. For shared state that must not race, use
`atomic.int`/`atomic.bool` (e.g. `counter.inc()`) to perform atomic operations.

```un
use atomic

counter = atomic.int(0)
counter.inc()              # 1
counter.add(10)            # 11
counter.compare_and_swap(11, 100)   # true
```

```un
use io
use flow
use iter

using pool = flow.spawn(2)
r = pool.map(fn(x) -> x * x, 1, 2, 3, 4)
io.write(list(iter.sorted(r)))   # [1, 4, 9, 16]
s = pool.submit(fn(x) -> x * 3, (7))
io.write(wait s)                 # 21
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
use io

using file = io.open("log.txt", "w")
file.write("hello")

fn work()
    defer io.write("done")
    io.write("start")

work()    # start done
```

## Standard Library

| Module | Main API |
|---|---|
| `atomic` | `atomic.int(initial?)`/`atomic.bool(initial?)` create `atomic_int`/`atomic_bool`. Methods: `get`/`set`/`add` (int only)/`inc`/`dec` (int only)/`exchange`/`compare_and_swap`, backed by `Interlocked` |
| `io` | `write`, `read`, `open`, `clear` |
| `fs` | `read`/`write`/`append`/`touch`, `exists`/`file`/`dir`, `list`/`files`/`dirs`/`walk`, `copy`/`move`/`rename`/`delete`, `mkdir`/`rmdir`, `size`/`created`/`modified`/`accessed`, `join`/`parent`/`name`/`stem`/`ext`, `absolute`/`relative`/`normalize`, `cwd`/`chdir`, `temp`/`home`/`exe`/`app` |
| `re` | `test`/`match`/`search`, `find_all`/`groups`, `replace`/`replace_all`/`split`/`escape` |
| `math` | `round`/`abs`/`ceil`/`floor`/`trunc`/`sign`, `sqrt`/`cbrt`/`pow`/`exp`/`log`/`log2`/`log10`, `sin`/`cos`/`tan`/`asin`/`acos`/`atan`/`atan2`/`hypot`, `gcd`/`lcm`/`clamp`, `is_nan`/`is_infinite`/`is_finite`, `pi`/`e`/`tau` |
| `iter` | `array`/`range`/`counter`/`reverse`/`repeat`, `zip`/`enumerate`, `map`/`filter`/`take`/`skip`/`chain`/`flatten`, `sum`/`max`/`min`/`sorted`, `all`/`any`/`count` |
| `random` | `int`/`float`/`bool`/`bytes`/`chance`, `choice`/`choices`/`sample`/`shuffle`/`weighted`, `range`/`str`/`value`/`next`/`seed`/`uuid` |
| `os` | `env`/`setenv`/`unsetenv`/`environ`/`expand_vars`, `pid`/`name`/`arch`/`version`/`cpuCount`/`is64bit`/`hostname`/`username`, `args`/`exit`/`exec`, `sep`/`pathsep`/`linesep`/`tickCount` |
| `sys` | `env`/`environ`/`expand_vars`, `pid`/`name`/`arch`/`version`, `getcalldepth`/`setcalldepth`/`getrecursionlimit`/`setrecursionlimit` |
| `time` | `sleep` (int=ms, float=s)/`now`/`utc_now`/`epoch`/`epoch_ms`/`from_epoch`/`from_epoch_ms`/`monotonic`/`monotonic_ms`/`stopwatch` |
| `flow` | `spawn` (pool) and `lock` |
| `http`/`ws` | `http.connect().get`/`post`/`put`/`patch`/`delete`/`head`/`options` and their `_async` variants, `ws.connect().send`/`receive`/`close`/`is_open` |
| `inspect` | `attr`/`global`/`getattr`/`hasattr`/`setattr`/`delattr`/`meta` |

Collection methods include `list.add`/`insert`/`extend`/`remove`/`remove_at`/`index_of`/`contains`/`order`/`sort`/`reverse`/`binary_search`/`hpush`/`hpop`/`pop`/`map`/`resize`, `dict.add`/`get`/`remove`/`keys`/`values`/`contains_key`/`contains_value`/`clear`, `set.add`/`union`/`intersect`/`difference`/`contains`/`remove`/`clone`/`clear`, and string methods such as `split`/`join`/`trim`/`center`/`left`/`right`/`replace`/`find`/`sub_str`/`to_upper`/`to_lower`/`starts_with`/`ends_with`.

The built-in functions available without a module are `len`/`type`/`hash`/`exit`/`throw`/`throw_if`/`throw_unless`/`chr`/`ord`/`bin`/`oct`/`hex`/`repr`/`ascii`, and the constructors are `int`/`float`/`str`/`bool`/`list`/`set`/`tuple`/`date`. Values such as `math.pi`, `math.e`, `math.tau`, `os.sep`, `os.pathsep`, `os.linesep`, `os.cpuCount`, and `os.is64bit` are exposed as functions, so call them.

Chaining `iter` methods (`(0..10).map(fn(x) -> x * 2).take(3)`) is lazy: nothing runs until the result is consumed. The global `iter.map`/`iter.filter`/`iter.take` evaluate their arguments immediately and return a `list`. `iter.zip` consumes both sources at the same rate, so an infinite `iter.counter` can be paired with a finite range.

`skip` is a reserved keyword, so `iter.skip(...)` and `.skip(...)` are parse errors and cannot be called by name. Use `take` or slicing to drop a leading segment.

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
Runtime        Obj / Val / Ref / Enu / Stru / TObj / Property and the type system
  |
  v
Stdlib         NativeModule / NativeType reflection binding
```

Key components:

- `src/Interpreter/Runner/Runner.cs` loads, parses, desugars, optimizes, and evaluates a source file. `Source` tracks line and column information.
- `src/Interpreter/Runner/Scope.cs` provides slot-based symbol tables with parent chains.
- `src/Core/Global.cs` registers built-in and native modules and resolves `import` / `include`.
- `src/Interpreter/Reflection/` binds native modules and types through attributes.

The C# implementation has no intermediate bytecode or separate virtual machine. (The Rust port runs through a bytecode compiler and a stack-based VM.)

## Rust port (experimental)

A second implementation of the same language lives under `rust/`: a bytecode compiler and stack-based VM written in Rust (`unvm`). It reproduces the C# implementation's grammar, semantics, standard library, and error messages, and is verified to be byte-identical on the scripts in `test/MANIFEST.txt` — stdout, exit code, and `help` output included — via `scripts/differential.ps1` (105 checked / 0 unexpected DIFFs, with only the 4 documented negative divergences).

- Build: `cargo +stable-x86_64-pc-windows-gnu build --release --manifest-path rust/Cargo.toml`
- Run: `rust\target\release\unvm.exe run <file>`
- Verify: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/differential.ps1`
- Performance: on par with or faster than the C# reference on most benchmarks (e.g. fib24 3.2x and sort 4.0x faster; only `dict` and the nested loop bench trail).
- Docs: `docs/rust-migration/`

A handful of divergences are documented (stack overflow exit code, `go`/`pool` single-threaded execution, no TLS). The rest of this document describes the C# implementation, but the grammar and standard library are the same in both (some error messages and exit codes differ as documented).

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
|-- scripts/             (test.ps1, bench.ps1, differential.ps1)
|-- rust/                (source of the Rust bytecode VM port)
|-- bench/               (benchmark scripts, bench/node/, bench/results/)
`-- bin/, obj/             (generated build output)
```

## Examples

Floyd-Warshall (`test/integration/algorithms.un`):

```un
n = 4
INF = 99999999
graph = [[0,5,INF,10],[INF,0,3,INF],[INF,INF,0,1],[INF,INF,INF,0]]
for k in 0..n
    for i in 0..n
        for j in 0..n
            if graph[i][j] > graph[i][k] + graph[k][j]
                graph[i][j] = graph[i][k] + graph[k][j]
```

Property (`test/unit/runtime/property.un`):

```un
use io
class Temperature
    celsius = 0.0
    get fahrenheit -> self.celsius * 9 / 5 + 32
    set fahrenheit
        self.celsius = (value - 32) * 5 / 9

t = Temperature()
t.fahrenheit = 212
io.write(t.celsius)
io.write(t.fahrenheit)
```

Ranges and laziness (`test/unit/runtime/range.un`), for without a binding (`test/unit/runtime/for_no_bind.un`):

```un
use io
total = 0
for n in 1..10
    total += n
io.write(total)
io.write((0..10).map(fn(x) -> x * 2).take(3))
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
- Classes, multiple inheritance, `enum`, and classes with a field list (`class Point(x, y)`)
- Class property accessors (`get`/`set`, `field`, `value`)
- The range operator `a..b` and lazy iterator chaining
- Pattern matching and destructuring
- Modules and selective imports
- Slicing, ternary expressions, and null/error propagation
- `try`/`catch`/`finally`, `using`, and `defer`

In progress:

- Gradual type annotation enforcement
- Rust bytecode VM port (`rust/`)

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