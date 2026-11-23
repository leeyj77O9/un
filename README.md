# Un

A modern scripting language with concise syntax, explicit asynchronous execution, and pattern matching.

Un is implemented as a bytecode compiler and register-based VM written in Rust (`unvm`). Hot-path Int and Float-promoted accumulator functions are compiled to native code by an embedded Cranelift JIT. The processing pipeline is:

`Source -> Lexer -> Parser -> Optimizer (desugar+fold, single pass) -> Bytecode Compiler -> Register VM (optionally JIT)`

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.100-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust" />
  <img src="https://img.shields.io/badge/license-MIT-22C55E?style=flat-square" alt="MIT License" />
</p>

## Introduction

Un is a dynamically typed scripting language aimed at readable, compact programs. It combines Python-like indentation-based blocks with explicit concurrency primitives, structural pattern matching, and a small standard library.

Un is written in Rust: source files are lexed and parsed, optimized in a single bottom-up pass (constant folding with desugaring folded in), compiled to bytecode, and executed by a register-based VM. Hot-path Int and Float-promoted accumulator functions are compiled to native code by an embedded Cranelift JIT. It was previously implemented as a C# tree-walking interpreter on .NET 8; the Rust port was verified byte-identical to it by differential testing before replacing it.

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

- Optional type annotations: `x: int`, `a: list[int]`, `dict[str, int]`, `dict[int, int]`, `int | str`
  parameter types and return types (`fn add(a: int, b: int) -> int`)
- **Generics**: `fn identity[T](x: T) -> T`, `class Box[T]` — type arguments are inferred at call
  sites; explicit `identity[int](42)` is also supported. Bounds such as `[T: int | float]` can be specified.
- Annotations are currently used for readability and gradual checking; strict runtime enforcement is being strengthened

### Concurrency (stable)

- `go` to run an expression in the background, returning a `Future`
- `wait` to await a `Future`
- `flow.spawn(n)` worker pools with `map` and `submit`

### Planned

- Stronger static type checking
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

Members keep their value in the instance slot they'd otherwise use. Class-level
members can carry a type annotation and an optional default — the annotation is
a declaration of intent; the stored value is whatever the member holds at
runtime:

```un
use io

class TypedDefaults
    count: int = 0
    name: str = "anon"

t = TypedDefaults()
io.write(t.count)   # 0
io.write(t.name)    # anon
t.count = 42        # any int is fine; the slot is not dynamically typed
```

A bare annotation without a default initialises the member to `none`:
`class Foo\n    x: int` — `Foo().x == none`.

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

Variants can also carry a payload, similar to Rust enums but with indentation
and a `name: type` declaration. Type parameters (`[T, U]`) are supported.

```un
enum Test[T, U]
    a: int
    b: T
    c: tuple[int, U]

x = Test.a(42)
io.write(x)             # Test.a(42)
io.write(x.value)       # 42
```

- Construct with `Test.a(42)` and read the payload with `.value`.
- Two values are `==` when the enum, variant and payload all match; they also
  work as dictionary keys.
- `match x { Test.a(v) -> v, Test.b(v) -> v, _ -> 0 }` destructures the payload.
- Field types are checked statically by `un check` and erased by `un run`.
- Enums with data variants execute through the Compile Mode VM path; the
  no-VM `un build` path does not support them.

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

- Rust toolchain (`stable-x86_64-pc-windows-gnu`, Rust 1.100+; pinned by `rust-toolchain.toml`). The GNU flavor is used instead of MSVC.

### Build

```bash
rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml
```

The Windows executable is `rust\target\release\un.exe`.

### Run

```bash
.\rust\target\release\un.exe run path/to/file.un
```

### Compile Mode (experimental)

Interpreter Mode (`un run`) and Compile Mode are two execution modes over the
same language. Compile Mode type-checks a program ahead of time and compiles
a subset of functions to Cranelift-native code; anything outside the subset
runs in the interpreter with identical semantics.

```bash
.\rust\target\release\un.exe check path/to/file.un   # static analysis only
.\rust\target\release\un.exe native path/to/file.un  # analysis + native execution
```

- `un check` exits 0 when the program type-checks and 1 otherwise, printing up
  to 50 diagnostics with line/caret markers.
- Compile Mode is stricter: **every function parameter and return type must be
  annotated** (`fn add(a: int, b: int) -> int`). Interpreter-mode programs that
  omit annotations are rejected in compile mode by design.
- `un native` is a superset of `un run`: core-eligible functions (int/float/bool
  params, locals and returns, direct calls) are compiled to native code through
  the register VM's call path; every other function falls back to the
  interpreter transparently. A bail never produces a wrong answer — the VM
  re-runs the call in the interpreter.
- `un run` and `un compile` are unchanged. Compile Mode is documented in
  `docs/compile/COMPILE_DESIGN.md`; cross-check it with
  `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\compile-check.ps1`.
- `un obj <file.un|file.unbc> [-o out.obj] [--entry name]` translates `.unbc`
  bytecode or a `.un` source file into a relocatable native object file (COFF
  `.obj`). The source path also compiles `if`/`while`/`for` loops to native
  code. Only the core subset is supported; `scripts\obj-demo.ps1` links and
  runs the fib demo (`docs/compile/OBJ_MILESTONES.md`).
- `un build <file.un> [-o out.exe]` does the same end to end and links the
  object into a standalone executable (via rustup's self-contained mingw gcc).
  **A plain exe runs only `fn main`** — the module's top-level statements are
  not executed and the process exit code is `main()`'s return value. By
  comparison, `un run` / `un native` / `--embed` execute the top-level
  statements, and `main()` runs only when the file calls it explicitly.
- `un build --embed <file.un> [-o out.exe]` instead bakes the program's
  `.unbc` image into a tiny generated driver and links it against `libunvm.a`
  (the whole VM runtime, statically). This works for **any** Un program —
  classes, collections, IO, string interpolation — not just the Int core
  subset, and the binary is fully self-contained.
- Plain `un build` (without `--embed`) contains **no VM at all**: it links
  `libnrt.a` (the standalone native runtime) and produces a ~5 MB exe. Classes
  (fields, methods, `__init__`, inheritance), exceptions, strings/lists/dicts,
  native modules (io/math/iter/os/sys/fs/codes/re/time/types), default
  arguments, lambdas (including captured and list-stored closures), simple
  user-module imports/globals, and byte-identical tracebacks all work natively.
  Programs needing dynamic `Any`/union types, time-seeded `random`, or
  module-level side effects beyond simple assignments need `un build --embed`.
  Generic collection types (`list[str]`, `list[bool]`, `dict[int, str]`,
  `dict[str, ClassName]`, ...) also compile natively in a plain build by
  recording the element kind in TIR; elements of nested lists are opaque
  handles, so indexing/operations on them are not lowered.
- `un lsp` runs a Language Server Protocol server over stdin/stdout. It reports
  lexer/parser diagnostics by default; Compile Mode type diagnostics are opt-in
  via `initializationOptions.compileMode` or the `un/compileMode` notification.
  Any client may speak to it; the `C:\dev\un-vscode` folder holds the
  `un-language` VS Code extension that launches it — syntax highlighting plus
  live diagnostics, hover, F12 go-to-definition, hierarchical outlines
  (class > methods/fields), Shift+F12 references, F2 rename, folding,
    completion (with member completion after `.`, signature help, and snippets),
     and AST-based semantic tokens. The extension adds an `Un: Run File` button (editor title bar / right-click / status bar) that saves
   the current file and runs it in a terminal as `un run <file>`.

### First program

```un
use io

fn main()
    io.write("Hello, Un!")

main()
```

Save as `hello.un` and run:

```bash
.\rust\target\release\un.exe run hello.un
```

### Tests and Benchmarks

Build and run a script directly:

```bash
rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml
.\rust\target\release\un.exe run test/test_suite.un
```

The full suite runs through a runner. It executes the `group|path` rows of
`test/MANIFEST.txt` with the built executable, counts exit codes and the
`N passed, M failed` line on stdout, and runs the rows of
`test/negative/manifest.txt` that check exit codes and stderr.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test.ps1
```

The JIT is validated the same way: every manifest file is executed twice,
once with `UV_JIT=0` and once with the default, and stdout and exit code
must match (`scripts/differential.ps1`).

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\differential.ps1
```

`UV_JIT=2` is a strict canary: if any JIT candidate falls back to a stub, the
process exits 2 immediately. CI checks `test/jit/strict.un` (Int) and
`test/jit/strict_float.un` (Float math intrinsics). The `.unbc` round-trip check
byte-compares `un run` against `un compile` + `un run <file>.unbc`.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\unbc-roundtrip.ps1 -IncludeNegative
```

Compile Mode cross-checks, no-VM exe equality checks, and cross-lane edge fuzzing
use the same approach (`scripts\compile-check.ps1`, `scripts\exe-diff.ps1`,
`scripts\edge-fuzz.ps1`). CI (`.github/workflows/build.yml`) runs a release build
and all of the above gates (12 steps total) on every push and PR.

Files listed in `test/KnownFailures.txt` are reported as KNOWN FAIL and are not
counted in the Positive pass/fail totals. Files listed in `test/KnownDivergent.txt`
are documented divergences between implementations and are not counted as failures.
The current baseline is Positive 50 files / 1606 passed / 0 failed / 0 crashed,
Negative 51 checks matched + 0 documented divergences, and Known 0 files
(re-measured 2026-11-23). Rust tests: `cargo test --release --workspace` reports
254 passed (nrt 4 + lib 238 + LSP 4 + playground 8). JIT differential: 101 files,
0 differences. `.unbc` round-trip: Positive 50/50, Negative 42 identical
(9 compile-rejected skips), Compile cross-check 36/0, exe equality 42/0.

Benchmarks measure 6 lanes (Un VM, Un JIT, Compile Mode, standalone exe, CPython,
Node.js) with warmup and repeated runs; results go to
`bench/results/<timestamp>/results.csv`, `results.json`, and `results.md`.
`scripts\bench-report.ps1` renders `bench/report.html` and checks regressions with
`-Gate`.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group all
```

`docs/guides/BENCHMARK_GUIDE.md` defines the comparison rules (equal input, algorithm,
warmup, repeated runs, recorded environment). Measured values are not checked in;
re-measure on your machine with `scripts/bench.ps1`. `docs/guides/TEST_GUIDE.md` describes
the test layout and rules. `docs/guides/CLI.md` documents the `un` command-line interface
(`docs/guides/CLI.ko.md` is the Korean translation).

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

### Collections and slicing

```un
use io

l = [1, 2, 3, 4, 5]
io.write(l[1:3])                  # [2, 3]
io.write(l[::2])                  # [1, 3, 5]
io.write(l[::-1])                 # [5, 4, 3, 2, 1]
io.write(l[-2:])                  # [4, 5]
io.write(2 in l)                  # true
io.write(set(1, 2) == set(1, 2))  # true
io.write({"a": 1} == {"a": 1})    # true
```

Slicing follows the same rules for `list`, `tuple` and `str`. A negative start
or end index counts from the end of the sequence, an omitted bound depends on
the sign of the step (a positive step runs from the first to the last element,
a negative step from the last back to the first), the end index is exclusive
and the step must not be zero.

`==` compares `list`, `tuple`, `dict` and `set` element by element. Two dicts
are equal when their keys and values match, two sets are equal when they hold
the same elements regardless of order.

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

`flow.spawn(n)` creates a pool handle, but Un is a cooperative single-thread runtime:
`pool.map` produces one result per argument, and `pool.submit` hands back a `Future`; no OS
threads are created. For shared state that must not race, use `atomic.int`/`atomic.bool`
(e.g. `counter.inc()`) — inside a single cooperative thread they hold a small
`RefCell`-backed state, so they're correct for any interleaving Un can produce, and
`atomic.add`/`inc`/`dec` wrap i64 the same way Un's Int arithmetic does (they will not
panic in debug builds). Compare-and-swap is a flat compare-then-write on that cell.

This is a single-thread model, not preemptive multithreading — `pool.map` runs inline with
the caller, and there is no background worker doing parallel work.

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
| `atomic` | `atomic.int(initial?)`/`atomic.bool(initial?)` create `atomic_int`/`atomic_bool`. Methods: `get`/`set`/`add` (int only)/`inc`/`dec` (int only)/`exchange`/`compare_and_swap`. Backed by `RefCell` (single-thread cooperative runtime) |
| `io` | `write`, `read`, `open`, `clear` |
| `fs` | `read`/`write`/`append`/`touch`, `exists`/`file`/`dir`, `list`/`files`/`dirs`/`walk`, `copy`/`move`/`rename`/`delete`, `mkdir`/`rmdir`, `size`/`created`/`modified`/`accessed`, `join`/`parent`/`name`/`stem`/`ext`, `absolute`/`relative`/`normalize`, `cwd`/`chdir`, `temp`/`home`/`exe`/`app` |
| `re` | `test`/`match`/`search`, `find_all`/`groups`, `replace`/`replace_all`/`split`/`escape` |
| `math` | `round`/`abs`/`ceil`/`floor`/`trunc`/`sign`, `sqrt`/`cbrt`/`pow`/`exp`/`log`/`log2`/`log10`, `sin`/`cos`/`tan`/`asin`/`acos`/`atan`/`atan2`/`hypot`, `gcd`/`lcm`/`clamp`, `is_nan`/`is_infinite`/`is_finite`, `pi`/`e`/`tau` |
| `codes` | `bin`/`oct`/`hex` (integer in base 2/8/16), `ascii` (code of the first UTF-16 unit), `chr`/`ord` (code point <-> single character) |
| `iter` | `array`/`range`/`counter`/`reverse`/`repeat`, `zip`/`enumerate`, `map`/`filter`/`take`/`skip`/`chain`/`flatten`, `sum`/`max`/`min`/`sorted`, `all`/`any`/`count` |
| `random` | `int`/`float`/`bool`/`bytes`/`chance`, `choice`/`choices`/`sample`/`shuffle`/`weighted`, `range`/`str`/`value`/`next`/`seed`/`uuid` |
| `os` | `env`/`setenv`/`unsetenv`/`environ`/`expand_vars`, `pid`/`name`/`arch`/`version`/`cpuCount`/`is64bit`/`hostname`/`username`, `args`/`exit`/`exec`, `sep`/`pathsep`/`linesep`/`tickCount` |
| `sys` | `env`/`environ`/`expand_vars`, `pid`/`name`/`arch`/`version`, `getcalldepth`/`setcalldepth`/`getrecursionlimit`/`setrecursionlimit` |
| `time` | `sleep` (int=ms, float=s)/`now`/`utc_now`/`epoch`/`epoch_ms`/`from_epoch`/`from_epoch_ms`/`monotonic`/`monotonic_ms`/`stopwatch` (with read-only `ms`/`ns` properties) |
| `flow` | `spawn` (pool handle, cooperative single-thread: inline execution) and `lock` |
| `http`/`ws` | `http.connect().get`/`post`/`put`/`patch`/`delete`/`head`/`options`, `ws.connect().send`/`receive`/`close`/`is_open` |
| `inspect` | `attr`/`global`/`getattr`/`hasattr`/`setattr`/`delattr`/`meta` |
| `types` | `u8`/`u16`/`u32`/`u64`, `i8`/`i16`/`i32`/`i64`, `f32`/`f64` — fixed-width integer/float constructors. Range checks happen at conversion time only; out-of-range values raise the uncatchable error panic (printed as `error: <message>`). Afterwards they behave like ordinary `int`/`float` (and `u64` is capped at `i64::MAX` because Un ints are signed 64-bit). `use types` is required — in a module without the import, all fixed-width names (including `u32` etc.) are unknown type annotations |

Collection methods include `list.add`/`insert`/`extend`/`remove`/`remove_at`/`index_of`/`contains`/`order`/`sort`/`reverse`/`binary_search`/`hpush`/`hpop`/`pop`/`map`/`resize`, `dict.add`/`get`/`remove`/`keys`/`values`/`contains_key`/`contains_value`/`clear`, `set.add`/`union`/`intersect`/`difference`/`contains`/`remove`/`clone`/`clear`, and string methods such as `split`/`join`/`trim`/`center`/`left`/`right`/`replace`/`find`/`sub_str`/`to_upper`/`to_lower`/`starts_with`/`ends_with`.

The built-in functions available without a module are `len`/`type`/`hash`/`exit`/`throw`/`throw_if`/`throw_unless`/`repr`, and the constructors are `int`/`float`/`str`/`bool`/`list`/`set`/`tuple`/`date`. Conversion helpers and character codes live behind `use codes` (`codes.hex()`/`codes.bin()`/`codes.oct()`/`codes.ascii()`/`codes.chr()`/`codes.ord()`). Values such as `math.pi`, `math.e`, `math.tau`, `os.sep`, `os.pathsep`, `os.linesep`, `os.cpuCount`, and `os.is64bit` are exposed as functions, so call them.

Chaining `iter` methods (`(0..10).map(fn(x) -> x * 2).take(3)`) is lazy: nothing runs until the result is consumed. The global `iter.map`/`iter.filter`/`iter.take` evaluate their arguments immediately and return a `list`. `iter.zip` consumes both sources at the same rate, so an infinite `iter.counter` can be paired with a finite range.

`skip` is a reserved keyword, so `iter.skip(...)` and `.skip(...)` are parse errors and cannot be called by name. Use `take` or slicing to drop a leading segment.

### Semantic notes

- `time.stopwatch`: `.elapsed` returns a `date` that holds the monotonic duration (not a wall-clock timestamp). `start()` resets the timer — it does not resume it.
- `hash(...)`: a `NaN` hash key is unfindable in dictionaries — every lookup raises `key not found`. This is the IEEE/.NET behaviour, not a bug.
- `2 ** 3 ** 2` evaluates as `2 ** (3 ** 2)` (right-associative, like Python/JS). `2 ** 3` is `8`.
- String escapes: unknown escapes keep the backslash. `"\q"` is `"\q"`, `"\x"` is `"\x"`. Only `\n`, `\r`, `\t`, `\\`, `\"` and `\'` are interpreted.
- A statement may not be followed by another statement on the same line. `x := 1 y := 2` is a parse error: `expected a newline after the previous statement`.
- `b?.v = e` (assignment to an optional-chain target) raises `'none' object has no attribute` when `b` is none — Un does not silently skip the assignment; keep the error or use `if b` first.
- `http.get/post/put/patch/head/options/delete`: redirects (301/302/303/307/308) are followed up to 8 hops. 301/302/303 rewrite the method to GET and drop the body (.NET behaviour); 307/308 preserve method and body. A missing Location header makes the redirect response the final one. `get` keeps the 404-style exception contract; the other verbs return status and body as-is.
- `time`'s `date(str)` constructor validates year/month/day (including leap years), hour/minute/second and millisecond strictly; `date("2024-02-30")` is an error.
- `use a.*` is sugar for `use a { * }` (wildcard import of every public member).
- An error that propagates out of the program prints an Un-style trace: an `un: error trace` header followed by one `-> <function> <file>, line [N]` row plus the source line per frame the error crossed, then the error location (line/span/caret) and `error: <message>`. Repeated frames (deep recursion) collapse to `... (repeated N more times)`; once the trace grows past 10 frames it switches to compact mode and prints line info only, no source lines. A single-frame trace (a shallow error) is omitted, and only the caret block is printed. Errors raised inside imported modules are displayed against the module's own file/line, and the `use` line itself is recorded as a frame.

## Architecture

The interpreter is a multi-pass pipeline over the AST: the lexer and parser
produce the AST, the desugarer and optimizer run (the optimizer folds constant
subexpressions to a fixpoint), the compiler emits bytecode for each proto, and
the VM either translates the stack bytecode to a register plan or executes it:

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
Optimizer      desugar (compound assignment, if/elif) + constant folding,
               algebraic simplification, single bottom-up pass
  |
  v
Bytecode Compiler   Op / Proto / Instruction(flags, operand) encoding
  |
  v
Register VM (interpreter)  Context / Scope / Frame based execution, Future for go/wait
                             + Hot-path Int and Float-promoted accumulator functions are compiled to native code by Cranelift JIT
  |
  v
Runtime         Obj / Ref / Enu / Stru / TObj / Property and the type system
  |
  v
Stdlib          native modules (`io`, `fs`, `re`, `math`, ...)
```

Key components:

- `rust/src/compiler.rs` compiles desugared, optimized AST to bytecode Protos.
- `rust/src/vm/` runs Protos on a register machine (stack bytecode is translated to a register plan by `transfer.rs`); `Op` is an explicit `#[repr(u8)]` enum, `Proto` owns `code`/`consts`/`names`/`protos`/`source_map`.
- `rust/src/runtime/scope.rs` provides slot-based symbol tables with parent chains.
- `rust/src/runtime/world/` owns global state (`path`, `global_scope`, `natives`, `stdout`, `allowed`) and the module cache, split by responsibility into submodules.
- `rust/src/native/` holds the standard library modules. They are registered explicitly in `native::init`, without assembly reflection.
- `rust/playground/` wraps `unvm` as an entry point for the web playground (the Rust equivalent of `PlaygroundRunner.RunCode`).

## Project Structure

```
Un/
|-- README.md                  (English user doc; Korean: docs/README.ko.md)
|-- LICENSE / AGENTS.md / BENCHMARK.md
|-- rust/
|   |-- Cargo.toml / rust-toolchain.toml
|   |-- src/
|   |   |-- main.rs          (CLI: run/compile/check/native/obj/build/lsp/help +
|   |   |                    tokens/ast/desugar/opt/dis/dis-reg diagnostics)
|   |   |-- lib.rs
|   |   |-- bytecode/        (opcodes, instruction, Program, Proto, unbc container)
|   |   |-- lexer.rs / parser.rs / optimize.rs / compiler.rs / vm/ / module.rs
|   |   |-- jit_bc.rs / jit_cache.rs / tir.rs / objify.rs / natcode.rs
|   |   |-- runtime/         (obj, collections, iters, function, world, scope, date, ...)
|   |   `-- native/          (io, math, iter, os, sys, fs, atomic, codes, flow,
|                            types, time, random, re, http, ws, inspect)
|   |-- playground/          (WASM entry point, target of un-playground)
|   |-- nrt/                 (libnrt.a — native runtime for VM-less plain un build)
|   `-- target/              (generated build output, gitignored)
|-- main.un                 (user's scratch; tracked in git)
|-- test/                   (.un test scripts and fixtures)
|   |-- unit/ integration/ regression/ compile/ jit/
|   |-- negative/        (scripts checking exit code and stderr, plus manifest.txt)
|   |-- fixtures/        (circular/, rel_test/, dirmod/, dirall/)
|   |-- MANIFEST.txt     (group|path rows)
|   |-- KnownFailures.txt / KnownDivergent.txt
|   `-- assert.un        (assert module)
|-- scripts/             (test.ps1, bench.ps1, bench-report.ps1, compile-check.ps1,
|                        compile-bench.ps1, differential.ps1, unbc-roundtrip.ps1,
|                        obj-demo.ps1, exe-diff.ps1, edge-fuzz.ps1)
|-- bench/               (benchmark scripts, bench/{node,python}/, bench/results/ — gitignored)
|-- docs/                (guides/, spec/, project/, compile/, audit/, README.md)
`-- .github/workflows/build.yml  (GNU toolchain build + 12 verification steps)
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
`docs/guides/TEST_GUIDE.md` and `docs/guides/BENCHMARK_GUIDE.md`.

## Roadmap

Implemented:

- Bytecode compiler and register-based VM (lexer, parser, desugarer, optimizer, compiler, vm)
- Hot-path Int Cranelift JIT (Int loops, arithmetic and bit ops, `iter.range`, FusedIntMulAdd, class methods over Int fields, IntVec/IntDict intrinsics, math Float intrinsics) (disable with `UV_JIT=0`, strict canary with `UV_JIT=2`). Includes Int-only IntDict intrinsics for `d[k] = v` / `d[k]` patterns and Bool returns.
- Generic functions/classes (`fn f[T]`, `class Box[T]`) with call-site inference, explicit type arguments (`f[int](42)`), and bounds (`[T: int | float]`)
- Type aliases (`type Id = int | str`), guarded/or-patterns, nested destructuring, and `is`-based narrowing in `match` / `un check`
- Byte literals (`b"..."`), the `bytes` module, and `fs.read_bytes` / `fs.write_bytes` for binary file I/O (plain build requires `--embed` until NRT support lands)
- Data-carrying enum variants (`Test.a(42)`) with `match` destructuring
- Compile Mode (`un check`/`un native`) and standalone `un build`/`un obj` (libnrt.a, no VM)
- LSP language server (`un lsp`) and the `un-language` VS Code extension
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

Planned:

- Stronger static type checking
- Package manager

## Contributing

1. Fork and clone the repository.
2. Build with `rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml` and confirm zero warnings and errors.
3. Create a branch named `feat/my-feature`.
4. Add `.un` tests to the right group under `test/` and register a `group|path` row in
   `test/MANIFEST.txt`. Tests that check exit codes and stderr go in `test/negative/`
   with a `path|expected exit|expected substring` row in `test/negative/manifest.txt`.
5. Open a pull request. Commits are made by the contributor; automated tooling does not push to git.

Coding conventions: Rust 2021 edition. Modules carry a `//!` doc comment at the top, errors propagate with `?`, and pattern matching is used liberally. No external crate dependencies by default; the exception is the Cranelift JIT stack under `vendor/` (patched `region 3.0.2`, `cranelift-jit 0.137.1`, `wasmtime-internal-jit-icache-coherence 50.0.1`, raw kernel32 FFI because the GNU toolchain lacks `dlltool.exe`), used for the numeric-subset Cranelift JIT.

## License

MIT License. Copyright (c) 2024 Usejun.

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files, to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software.
