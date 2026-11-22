# docs/spec/ARCHITECTURE.md

How the Rust implementation of Un is put together. This document describes
the code as it is today; `docs/project/PROJECT_AUDIT.md` records the audit findings
and `docs/project/REFACTOR_LOG.md` records what landed.

## 1. Overview

```
   .un source ──► Lexer ──► Parser ──► Optimizer (desugar+fold) ──► Bytecode compiler
                                                                          │
                            .unbc container ◄────────────────────────────┘
                                    │                                        │
                                    ▼                                        ▼
                          unbc::read (validate) ────────────► Register VM  ──►  Cranelift JIT
                                                                   │             (Int/Float subset)
                                                                   ▼
                                                          runtime::obj / collections / iters
                                                                   │
                                                                   ▼
                                                        native::* (io, fs, re, math, ...)
```

The runtime is single-threaded and cooperative: `go`/`wait`, `pool` and `lock`
all execute on the calling thread (see `docs/project/DECISIONS.md` D-012, D-054).

## 2. Pipeline

| Stage | Module | Notes |
| --- | --- | --- |
| lex | `rust/src/lexer.rs` | indentation-based tokens; `Source` is mutated to record ignored new lines |
| parse | `rust/src/parser.rs` | recursive descent with a precedence table (`get_binary_precedence`); `Rc<Source>` is the only shared state |
| optimize | `rust/src/optimize.rs` | constant folding + dead-branch elimination + desugaring (`desugar_local`) in a single bottom-up pass |
| compile | `rust/src/compiler.rs` | stack machine → `Proto` per function; slot table for locals; peephole fuses `a*b+c` into `FusedIntMulAdd` |
| translate | `rust/src/vm/transfer.rs` | stack → register translation; register count is `locals + max_stack_depth + 1`, capped at 256 |
| validate | `rust/src/bytecode/mod.rs` | opcode contract, register limit, source-map length invariant |
| execute | `rust/src/vm/mod.rs` | register VM; `match` dispatch on `Op` |

## 3. Object model

`rust/src/runtime/obj.rs`

```rust
pub type Obj = Rc<RefCell<ObjData>>;

pub struct ObjData {
    pub type_name: Rc<str>,     // runtime type ("int", "str", class name, ...)
    pub declared: Rc<str>,
    pub types: Types,            // ["obj", "int"] style union
    pub self_: Option<Obj>,      // bound receiver (class methods)
    pub super_: Option<Obj>,
    pub parents: Vec<Obj>,
    pub members: Vec<(String, Obj)>,   // instance fields, in assignment order
    pub annotations: Vec<(String, Obj)>,
    pub payload: Payload,        // 31 variants
}
```

- Scalars are **interned**: `obj::int` → `world::int_cached` (cache window
  `-1024..100000`), `obj::str` → `world::str_pooled` (pool of ≤10,000 strings
  of ≤1,000 chars), `obj::boolean` → `world::bool_obj`.
- `Payload` carries the value: `Int`, `Float`, `Str`, `Bool`, `Date`, `List`,
  `Dict`, `Set`, `Tup`, `Stru`, `Enu`, `Json`, `Func`, `Property`, `FieldRef`,
  `Iter`, `Future`, `Pool`, `Lock`, `AtomicInt`, `AtomicBool`, `Stream`,
  `Random`, `HttpClient`, `WebSocket`, `TObj`, `Spread`, `Stopwatch`.
  `none` and `null` have no payload variant of
  their own — they are `Plain` objects distinguished by their `type_name`.
- Attribute lookup: `get_attr_from` → instance members → bound method →
  super chain → parents; missing members raise
  `'{type}' object has no attribute '{name}'`.

### Collections

`rust/src/runtime/collections.rs` implements the `KeyMap` (insertion-ordered
bucket-chained hash table, `docs/project/DECISIONS.md` D-065) which backs `dict` and
`set`, plus `list`, `tuple`, `struct`, `enum`, `json` and `TObj` payloads.

## 4. VM

`rust/src/vm/mod.rs`

- **Locals live in the scope slot table**, not in registers. Registers hold
  expression temporaries only; the scratch area starts after the local slots
  (`transfer.rs:29`), so register pressure equals expression depth.
- **Arguments live in a separate `ArgFrame` stack** (`vm/mod.rs:96-99`).
- Closures capture the whole `Context`; there is no upvalue cell and
  `LoadUpv`/`StoreUpv` are ABI placeholders that are never emitted.
- Exception handling uses a `TryEntry` stack. The recorded handler depth
  (`TryEntry.acc_depth`) is read on handler entry to truncate the argument
  frame (`vm/mod.rs:346/354/371`) — TD-12 is resolved.
- `PLAN_CACHE` (thread-local) maps `program.id` → `(Rc<Program>, Rc<Plan>)`.
  It is bounded to 64 entries with FIFO eviction by first insertion
  (`vm/mod.rs:1672`, `PLAN_CACHE_LIMIT = 64`); insert-beyond-limit drops the
  oldest entry's `Rc<Program>`/`Rc<Plan>`, and a later hit recompiles from
  the program passed to `plan_for`. Keys are opaque monotone `Program.id`s,
  so a pointer collision cannot serve a stale plan (TD-03 resolved).

## 5. JIT

`rust/src/jit_bc.rs`, `rust/src/jit_cache.rs`, `rust/src/jit_fast.rs`

- The JIT translates the **register plan** of a proto into Cranelift IR.
  It supports the Int subset: parameters, locals, `iter.range` loops,
  integer arithmetic (with constant divisors for `//` and `%`), comparisons,
  `FusedIntMulAdd`, plus the IntVec/IntDict and class-method intrinsics and
  the `math.<fn>` Float intrinsics (sin/sqrt/floor/… — sqrt of a negative
  number and log of a non-positive number bail to the interpreter, which
  raises the exact Un error).
- Values are tagged: `0` = Int, `1` = Float (IEEE-754 bits), `2` = runtime
  bail (the interpreter takes over), `3` = Bool, `5` = None, `6` = IntList.
  The tag is written through a hidden pointer to a `(value, tag)` pair
  (`rust/src/jit_cache.rs`: `TAG_NONE`, `TAG_INTLIST`; `4` is unused).
- Float compares (`Lt/Le/Gt/Ge/Eq/Ne`) are native; mixed Int/Float operands
  promote with `fcvt_from_sint`; NaN semantics match Rust.
- Eligibility is decided by `precheck_fn`. Compilation is **lazy**: eligible
  protos are compiled on the first call through `try_call_jit` (one attempt
  per program). Protos that cannot be compiled are not registered at all;
  the caller falls back to the interpreter.
- Class methods are eligible: the hidden `self` pointer arrives as a raw
  `Rc<ObjData>` at ABI position 2 (after the hidden result pointer and the
  `param_tags` hidden argument) and is pinned to local slot 0 in the entry
  block. `self.<Int>` get/set is a **name-checked** intrinsic that verifies
  `[member name == field name ∧ payload == Int]` at call time; a mismatch
  bails to the interpreter (tag = 2). Constructors and non-Int-field methods
  still bail (by design), as do anything that cannot lift into the Int/Float
  value model.
- The JIT FFI ABI is `(hidden result ptr, param_tags: i64, args...)`.
  `param_tags` bit `p` is set iff argument `p` is a borrowed IntVec arena
  handle rather than a plain Int (plain Ints ≥ `ID_BASE` never reach the
  JIT). A per-proto entry guard reads `unsafe_param_mask` — the set of
  parameter positions that can reach an Int-consuming op — and bails the
  whole frame to the interpreter when a handle lands in one of them.
  This is what stops `xs + 1` from silently returning a handle value.
- Nested JIT calls carry list handles: `emit_user_call` accepts them
  (`int_list_foreign_seen`), and the callee prologue tags its args on entry
  "borrowed, not for free'd".
- All per-program JIT state (`JIT_FNS`, `JIT_MODULES`, `JIT_LAZY_ATTEMPTED`,
  and the `PLAN_CACHE` key) is keyed by the opaque monotone `Program.id`,
  never by pointer identity — a program dropped and re-allocated at the same
  address cannot be served stale entries.
- JIT modules are stashed in `jit_cache::JIT_MODULES` per program;
  `jit_cache::clear_program(id)` frees exactly one program's native code,
  `jit_cache::clear()` frees everything (callers must never enter JIT code
  afterwards).
- `UV_JIT=0` disables the JIT at runtime; `UV_JIT_DEBUG=1` prints the
  compile decisions.
- **Resource lifecycle** (the part that is not obvious from the code):
  - Every program that enters the JIT goes through `jit_cache::JIT_MODULES`,
    keyed by the opaque monotone `Program.id` (never by pointer identity).
    The arenas — `INT_LIST_ARENA`, `INT_DICT_ARENA`, `INT_CLASSES` — are
    **process-lifetime and grow monotonically**; they never recycle freed
    slots. Only `jit_cache::clear()` releases them, and after that no JIT
    code may be entered: calling into a cleared module is out of contract
    (that is also what makes `unsafe impl Send for JitEntry` hold today —
    the vendored Cranelift backend never frees native code inside a running
    process, see TD-05). `clear_program(id)` frees exactly one program's
    native code and is what tests use.
  - `PLAN_CACHE` (thread-local) is bounded to `PLAN_CACHE_LIMIT = 64` entries
    with FIFO eviction by first insertion (`vm/mod.rs:1672`); eviction drops
    the oldest entry's `Rc<Program>`/`Rc<Plan>` and a later hit recompiles
    from the program passed to `plan_for`. Keys are opaque monotone
    `Program.id`s, so a mis-keyed cache is impossible (TD-03 resolved). This
    bounds process-lifetime growth; it is not reclamation inside a running
    program (module unload is still not designed).
  - The recursion-depth guard is an atomic counter (`JIT_DEPTH`); its bound
    (`max_recursion_depth`) is a `Relaxed` `AtomicUsize` cache that is
    refreshed by `world::set_max_recursion_depth()` (i.e. by
    `sys.setrecursionlimit`). `world::reset()` seeds it; an embedder that
    never calls `world::reset()` still starts at the default 1000, so no
    path can accidentally get "unlimited" from the API.
  - Tag = 2 (bail) is the safety valve for anything the JIT cannot prove:
    the whole proto frame re-runs on pristine arguments in the interpreter,
    producing byte-identical output — which is why "correct everywhere" is
    verifiable with a differential gate instead of per-opcode proofs.
  - `class_layouts` is keyed by class name, appended-deduplicated, never
    reordered; a second same-named class with a different field order
    silently degrades JIT to interpreter speed (the name-checked intrinsic
    still bails correctly — speed, not correctness; TD-51).
  - Wholesale native memory is only ever reclaimed at process exit. Any
    future hot-reload/module-unload story must keep the arena/id namespace
    coherent across `clear()` boundaries; today nothing unloads a module
    at runtime.

## 6. Native modules

`rust/src/native/` — `io`, `fs`, `re`, `math`, `iter`, `time`, `os`, `sys`,
`random`, `http`, `ws`, `atomic`, `flow`, `inspect`, `builtin`, `members`.

Registration is explicit in `native::init`; there is no attribute-based
discovery. `native/members.rs` is a large member table (the `x.m(...)`
dispatch); it delegates all semantics to `runtime::obj` and never re-implements
them.

## 7. Tests and tooling

| Tool | Purpose |
| --- | --- |
| `scripts/test.ps1` | runs `test/MANIFEST.txt` groups (`suite`, `unit`, `integration`, `regression`, `known`, `negative`), aggregates `N passed, M failed`, exit 1 on any `[FAIL]` |
| `test/negative/manifest.txt` | `path|expected exit|expected substring` — exit code and stderr matching for error cases |
| `test/KnownFailures.txt` | scripts that are expected to fail (currently empty) |
| `test/KnownDivergent.txt` | documented divergences (currently none registered) |
| `scripts/differential.ps1` | runs every manifest file with `UV_JIT=0` and default JIT, diffs stdout + exit code (101 files, 0 differences) |
| `scripts/unbc-roundtrip.ps1` | `.unbc` round-trip byte comparison for Positive and compilable Negative files |
| `scripts/bench.ps1` | warm-up + repetitions + statistics (min/median/mean/max/stddev/ops) |
| `cargo test --release --workspace` | 234 tests (nrt 3 + lib 219 + un bin/LSP 4 + playground 8; lexer, parser, compiler, runtime, VM, native, net/redirects + JIT/sandbox regression guards) |

The build is release-only and requires the GNU toolchain:

```bash
rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml
```

## 8. Global state

`rust/src/runtime/world/` holds the process-wide state in a thread-local
`World`, split by responsibility into submodules (`world/mod.rs` owns the
struct and the `WORLD` cell; `caches` owns interned type names and the
`int`/`str`/`bool`/`none` scalar caches; `classes` owns the class
registries — `classes`, `classes_tobj`, `original_classes`,
`original_base_types`, `class_layouts`; `modules` owns the import cache,
the working path, the global scope and the native registry; `io` owns
stdout/log/stdin and the sandbox allow-list; `depth` owns the recursion
limit; `executors` owns the hooks the CLI and the playground install to
run compiled protos). Everything the runtime mutates at runtime lives
here, so a test or the WASM playground must restore what it changes.

## 9. Unsafe surface

| Location | Purpose | Guard |
| --- | --- | --- |
| `rust/src/runtime/obj.rs` (`un_instance_int_get` / `un_instance_int_ok` / `un_instance_int_ok_name` / `un_instance_int_set`) | direct member access for the JIT | documented `# Safety` contract, `catch_unwind`, name-checked for the class-layout intrinsic, bails to the interpreter on any mismatch |
| `rust/src/runtime/int_dict.rs` | integer-keyed dictionaries for JIT-emitted code | `Mutex` arena (poison-tolerant lock); tables are freed at JIT exits via `un_dict_int_free`; the arena itself is process-lifetime, wiped by `jit_cache::clear()` |
| `rust/src/jit_cache.rs:57` | `unsafe impl Send for JitEntry` | sound only because the vendored Cranelift backend leaks its allocations |
| `rust/src/vm/mod.rs` (`F1..F10`) | call into JIT-generated code | hidden result pointer + `param_tags` bitmask contract, arity asserted |
| `rust/src/runtime/clock.rs` | WASM host imports | WASM-only |
