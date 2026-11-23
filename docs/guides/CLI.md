# CLI Reference (`un`)

한국어 버전: `docs/guides/CLI.ko.md`

`un` is the command-line interface for the Un language: a bytecode-compiling,
register-based VM written in Rust (`unvm`, the library/toolchain name). This
page documents the `un` binary, not the language itself — for the language see
`README.md` (EN) / `docs/README.ko.md` (KO); for the bytecode format see `docs/spec/BYTECODE.md`.

> Naming history: the CLI binary was renamed from `unvm` to `un` on 2026-11-19.
> The library crate, the WASM playground (`unvm-playground`), test fixtures and
> some older documents still use the `unvm` name.

## Building

```powershell
rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml
```

The binary lands at `rust\target\release\un.exe` on Windows (GNU toolchain).
Add that directory to `PATH` if you want to run `un` from anywhere.

## Usage

    un <command> [args]

| Command | Purpose |
|---|---|
| `un run <file.un>` | compile and run a script |
| `un run <file.unbc>` | run a precompiled bytecode file (e.g. `un run hello.un.unbc`) |
| `un compile <file.un>` | compile a script to `<file.un>.unbc` |
| `un check <file.un>` | **experimental**: static type analysis only (compile mode) |
| `un native <file.un>` | **experimental**: static analysis + native execution (compile mode) |
| `un obj <file.un|file.unbc> [-o out.obj] [--entry name]` | **experimental**: translate source/bytecode to a relocatable object file (COFF .obj on Windows/GNU); `.unbc` is Int core only, source path also compiles Str/list/dict/classes/exceptions natively |
| `un build <file.un> [-o out.exe] [--embed] [--allow-dynamic] [--allow-dynamic] [--opt-level N]` | **experimental**: compile source to a relocatable object and link it into a standalone executable (via rustup's self-contained mingw gcc); `--embed` bakes the .unbc + VM runtime into a self-contained exe supporting any Un program; `N=0|1|2|3|s|z` controls rustc opt-level for the generated driver (default 0) |
| `un dis <file.un>` | dump stack bytecode with line numbers |
| `un dis-reg <file.un>` | dump the register-plan form of the bytecode |
| `un tokens <file.un>` | print the token stream |
| `un ast <file.un>` | print the AST |
| `un desugar <file.un>` | print the optimized (desugared) AST |
| `un opt <file.un>` | print the optimizer pass count and the optimized AST |
| `un help` | usage synopsis |
| `un lsp` | run the language server over stdio (used by the `un-language` VS Code extension; diagnostics, optional Compile Mode diagnostics, hover, go-to-definition, hierarchical symbols, references, rename, folding, IntelliSense completion + member completion after `.`, signature help, snippets, semantic tokens) |

If the file argument has no extension, `.un` is appended automatically.
Paths are resolved relative to the current working directory; imported modules
are resolved relative to the importing file first, then against `Global.PATH`.
Directory modules load either `mod.un` or every `.un` file in the directory.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | uncaught error (parse/compile error, runtime error, stack overflow) |
| 2 | usage error (wrong arguments, e.g. missing filename) |

## `un run <file.un>`

Parses, optimizes, compiles and runs the file. Output goes to stdout; errors
are printed to stdout (flushed before the interpreter dies).

    $ cat hello.un
    use io

    fn main()
        io.write("hello, un")

    main()

    $ un run hello.un
    hello, un

### Errors and the trace

An uncaught error prints the location of the raise site, an optional trace of
the frames the error crossed, and the message, then exits with code 1.

```text
fn inner()
    -> 1 / 0

fn outer()
    inner()

outer()
```

    $ un run example.un
    un: error trace
        -> <module>  <example>, line [9]
             outer()
        -> outer     <example>, line [7]
             inner()

    <example>, line [4], column [8]
        -> 1 / 0
           ^^^^^
    error: int division by zero

Rules:

- Frames are listed outermost first, with the source line under each frame.
  Frame positions use the same `<file>, line [N]` convention as the caret
  block. Module scope shows as `<module>`, anonymous `fn ... ->` as `<lambda>`.
- A single frame is not printed: the caret block already shows the raise site.
- A repeated frame collapses to `... (repeated N more times)`.
- When the trace grows past 10 frames it switches to a compact form: line
  information only, no source lines (e.g. alternating recursion).
- `catch` clears the trace; a caught error prints nothing by itself.
- Modules: an error raised inside an imported module displays against the
  module's own file/line, and the `use` line that triggered the import is
  recorded as a frame.

Common messages (see `test/negative/manifest.txt` for the exit/stderr contract
of error cases): `error: int division by zero`, `error: key '<key>' not found
in dictionary`, `circular import detected: '<path>'`,
`error: maximum recursion depth exceeded`, `error: undefined variable '<name>'`.

## `un run <file.unbc>` / `un compile`

`un compile <file.un>` writes `<file.un>.unbc` next to the source (it appends
`.unbc` to the full file name, so `hello.un` compiles to `hello.un.unbc`, and
`un run hello.un.unbc` executes it) and embeds the source for error display. `.unbc` is a self-contained bytecode container
(opcode stream, protos, constant pool, name table, source map, and the source
itself). Loading validates bounds — truncated files, out-of-range spans or
trailing garbage are rejected before the VM starts.

`un run file.un` and `un run file.unbc` produce byte-identical output; this is
verified in CI (`scripts/unbc-roundtrip.ps1`). The bytecode format is
documented in `docs/spec/BYTECODE.md`; `Op` values are fixed (`#[repr(u8)]`), so a
`.unbc` compiled by one build stays loadable by later builds of the VM.

## Diagnostics commands

These are the windows into the pipeline (`Source -> Lexer -> Parser ->
Optimizer -> Compiler -> VM`). All take one file argument.

### `un tokens`

Prints one line per token: `Type start..end<optional "span text">`. Blank
lines in the output correspond to empty spans (e.g. `Dedent 27..27`).

    $ un tokens add.un
    Func 0..2 "fn"
    Identifier 3..6 "add"
    LParen 6..7 "("
    Identifier 7..8 "a"
    Comma 8..9 ","
    Identifier 10..11 "b"
    RParen 11..12 ")"
    NewLine 12..13 "\n"
    Indent 13..17 "    "
    Return 17..19 "->"
    ...

### `un ast` / `un desugar` / `un opt`

Indented pretty-print of the AST. `un ast` prints the parsed tree; `un
desugar` prints the tree after desugaring; `un opt` prints the number of
optimizer passes and the folded tree. Sample (`un opt add.un`):

    passes: 1
    Block 0..36
      Function 0..25
        Identifier 3..6 "add"
        Tuple 6..12
          Parameter 7..8
            Identifier 7..8 "a"
          Parameter 10..11
            Identifier 10..11 "b"
        Block 17..25
          Return 17..25
            Binary 20..25
              Identifier 20..21 "a"
              Identifier 24..25 "b"
      Call 27..36
        Identifier 27..30 "add"
        Tuple 30..36
          Integer 31..32 1
          Integer 34..35 2

(For this input `un desugar` prints the same tree; the optimizer only folds
constant expressions, so simple programs are unaffected.)

### `un dis`

Dumps the stack bytecode of every proto (`names`, `consts`, per-proto code),
with per-instruction line numbers:

    $ un dis add.un
    names: ["<module>", "add", "a", "b", "a", "b", "add"]
    consts: [Int(1), Int(2)]
    proto 0 name "<module>" flags 0x0000 code 11
       0 MakeFunc flags 000001 op     0 depth   1 line 1
       1 Unwrap flags 000000 op     0 depth   1 line 1
       2 Pop flags 000000 op     0 depth   0 line 1
       3 LoadName flags 000000 op     0 depth   1 line 4
       ...
    proto 1 name "add" flags 0x0001 code 7
       0 LoadName flags 000000 op     0 depth   1 line 2
       ...

### `un dis-reg`

Same program translated to the register plan the VM actually executes
(scratch base, register count, per-instruction operands):

    $ un dis-reg add.un
    proto 0 name "<module>" scratch_base 0 reg_count 4 code 11
      params: []
      locals raw: [] names arena: ["add"]
       0 MakeFunc a 0 b 0 c 0 d 0 flags 000001
       ...

## Environment variables

| Variable | Meaning |
|---|---|
| (unset) | default: the embedded Cranelift JIT is enabled for the eligible Int/Float subset |
| `UV_JIT=0` | interpreter only (used by `scripts/differential.ps1` to compare JIT on/off byte-for-byte) |
| `UV_JIT=2` | strict canary: the process exits with code 2 if any JIT-eligible proto falls back to the interpreter (used in CI to catch JIT regressions) |

## Experimental (in development, may change)

These come from the in-progress compile mode; they require fully type-annotated
signatures and are not part of the stable CLI contract yet.

### `un check <file.un>`

Static type analysis only — no execution. Prints `compile error:` diagnostics
and exits 1 if the file does not type-check, or exits 0 otherwise.

    $ un check hello.un
    <hello.un>, line [1], column [8]
        fn add(a, b)
               ^
    compile error: parameter 'a' of 'add' requires a type annotation
    ...
    un: 3 compile-mode error(s)

### `un native <file.un>`

Static analysis + native execution (the compile-mode counterpart of `un run`).
Same annotation requirement as `un check`.

### `un obj <file.un|file.unbc> [-o out.obj] [--entry name]`

Translate source or bytecode into a relocatable native object file (`.obj`,
COFF on Windows/GNU). Reads a precompiled `.unbc` (Int core subset only) or,
for a `.un` source, runs `parse -> optimize -> typeck -> TIR` and lowers the
core subset to machine code via Cranelift. The source path also compiles
`if`/`while`/`for` loops. The entry proto (default `main`) is exported as
`un_main` with a C ABI (`() -> i64`) so it can be linked into an executable.
All un values are lowered as i64. Protos outside the subset are skipped with a
`skipped:` warning; protos that pass the static screen but fail to lower
become trap stubs (a call to one traps at runtime instead of returning
garbage). See `docs/compile/OBJ_MILESTONES.md` for milestones.

### `un build <file.un> [-o out.exe] [--embed] [--allow-dynamic] [--allow-dynamic] [--opt-level N]`

One command from source to a standalone native executable: emits the object
file as above, then links it against a generated driver and `libnrt.a` (the
standalone native runtime), using rustup's self-contained mingw gcc via
`rustc`. **Plain exe contains no VM** — it is ~5 MB vs ~17 MB for
`un build --embed`. `libnrt.a` implements the `unrt_*`/`un_math_*` C ABI used
by the emitted object, including Str/list/dict, classes (with inheritance),
exceptions, closures, native modules (`io`, `math`, `iter`, `os`, `sys`, `fs`,
`codes`, `re`, `time`, `types`), simple user-module imports/globals, and
byte-identical tracebacks. Default arguments and lambdas (including captured
and stored-in-list closures) are supported. Programs needing dynamic
`Any`/union types, time-seeded `random`, or module-level side effects beyond
simple assignments are rejected with an `un build --embed` hint.

**Entry semantics.** A plain native executable runs **only** the Un `main`
function (the entry proto, exported as `un_main`): the module's top-level
statements are **not** executed, and the process exit code is `main()`'s
return value. Write the entry logic inside `fn main()`. By contrast,
`un run` / `un native` / `--embed` use interpreter semantics — the module's
top-level statements execute once, and `main()` runs only if the file calls
it explicitly.

With `--embed`, the same command instead bakes the program's `.unbc` image
into the generated driver and links it against `libunvm.a` (the whole VM
runtime statically). Embed mode works for **any** Un program — classes,
collections, IO, string interpolation — and produces a single self-contained
executable. See `docs/compile/OBJ_MILESTONES.md`.

### `un lsp`

Run a minimal Language Server Protocol server over stdin/stdout (stdio).
This is what the `un-language` VS Code extension launches as `un lsp`; it is
not a debugging or execution path. Supported features:

- push diagnostics (`textDocument/publishDiagnostics`) from the lexer/parser —
  reported as you type, fail-fast (first error only)
- Compile Mode diagnostics (optional) — pass
  `initializationOptions: {"compileMode": true}` (or the `un/compileMode`
  notification with `{enabled: true}`) to also run
  `lex -> parse -> optimize -> typeck` and report up to 50 type-check errors
  with spans. This is what the VS Code setting `un.lsp.compileMode` does.
- `textDocument/completion` — keywords (plus snippets), primitive/language
  constants, native module names, and definitions collected from the current
  document. After a `.`, only members of the receiver are suggested: `self.`
  and instances of a class get that class's methods/properties; native module
  names (`io`, `math`, ...) get the module's native members; variables whose
  type is known from an annotation or a literal/constructor (`list[int]`,
  `"text"`, `[..]`, `{..}`, `set(..)`, `iter.range(..)`, ...) get the
  built-in type's methods (`str`, `list`, `dict`, `set`, `iter`, `date`,
  `stopwatch`)
- `textDocument/signatureHelp` — UDF functions, constructors (`Point(...)`
  shows `__init__`), native module functions: triggers on `(` and `,`,
  re-triggers on `,` and `)`
- `textDocument/definition` — jump to the definition of the identifier under
  the cursor (functions, extensions, classes, enums, variables)
- `textDocument/hover` — the declaration line of the symbol under the cursor
- `textDocument/documentSymbol` — hierarchical outline (class > methods and
  fields, enum members, module-level symbols; locals inside functions are
  hidden)
- `textDocument/references` — all sites of the symbol visible from the cursor,
  scope-aware (a same-named local binding wins over the module binding;
  `self` and `super` are excluded)
- `textDocument/rename` — `WorkspaceEdit` for every site of the symbol
  visible from the cursor, scope-aware; `self` and `super` cannot be renamed;
  the new name must be a valid Un identifier
- `textDocument/foldingRange` — indentation (`Indent`/`Dedent`) fold ranges
- `textDocument/semanticTokens/full` — AST-based semantic highlighting:
  classes, enums and their members, functions and methods, parameters,
  variables, properties (class fields and member accesses after `.`),
  namespaces (`use` paths and aliases) and type annotations are tagged with
  the standard `namespace`/`class`/`enum`/`enumMember`/`parameter`/`variable`/
  `property`/`function`/`type` legend. A member access directly followed by
  `(` is a call and is tagged `function` (`io.write(...)`, `list.map(...)`,
  `obj.method(...)`); keywords and punctuation stay with the TextMate grammar

Compile Mode diagnostics are opt-in because compile mode requires fully
annotated signatures and would produce noise on interpreter-mode programs.
References and renames only consider the current document; cross-file
references are not resolved. The implementation has no external dependencies.
Logs go to stderr, which the VS Code client surfaces in the
"Un Language Server" output channel.

## Implementation notes

- Rust 2021, single external dependency tree (Cranelift for the JIT); no
  runtime dependencies are needed to run scripts.
- The VM executes in a worker thread with a 64 MiB stack, so deep recursion
  terminates with `error: maximum recursion depth exceeded` instead of
  overflowing the process stack.
- `main.un` in the repo root is a user scratch file; it is not a product
  entry point and is excluded from the documented CLI behavior.
