# Refactor Summary — 2026-11

One-page overview of the architecture audit, migration verification,
redundancy removal and performance work done on the Un implementation in
November 2026. It is the *index*, not the record — each fact here links to
the document that carries the detail.

Status: all verification gates green (see "Current baseline"). Language
semantics and the test baseline were preserved throughout; no test
expectation was weakened to make anything pass.

## How this started

The repository began as the Rust port of a C#/.NET 8 tree-walking interpreter
(byte-identical differential verification against the C# oracle, per the CI
history). The C# sources are gone (`git` history only). The work described
here started from commit `958f9e2` — the user's groundwork for class-intrinsic
JIT — and grew out of a full architecture audit of the Rust tree (76 files,
~34k lines at the time).

## What was done, in order

| Commit | Pass | What |
| --- | --- | --- |
| `3c9fc53` | 0–3 | Audit + baseline; correctness fixes (S1: 21 defects incl. slice syntax, `in` for lists, `upper_bound`/`lower_bound`, set/dict equality); 176 native-layer defects + audit build/docs layer (S2); build/toolchain CI wiring; dead-code removal (`jit.rs`, `TokenType::text()`, `NodeKind` variants). |
| `2df3a62` | 5 | Redundancy removal: `Scope` slot-name snapshots dropped from `Frame`; `runtime/global.rs` renamed to `runtime/modules.rs`. |
| `38fbe38` | 6 | Phase 8: class methods JIT-compiled via name-checked field intrinsics (`self.<Int>` read/write verifies `[name == field ∧ payload == Int]` at call time), Float compare in JIT, JIT module lifetime fixes, RC ABI fixes. |
| `a9849b9` | 7 | Lazy compile (first `try_call_jit` call compiles); IntDict `BuildDict` accepts inline Int-entry literals. |
| `5bc0b80` | 7 | JIT cache redesign: opaque monotone `Program.id` replaces all pointer-identity keys (PLAN_CACHE, JIT_FNS/MODULES/LAZY_ATTEMPTED); per-program `clear_program()`; stale-entry/ABA hazard eliminated. |
| `d580a8b` | 8 | `.unbc` round-trip harness (`scripts/unbc-roundtrip.ps1`, byte-compared, wired into CI); caught and fixed `Program.source_path` storing only the file stem — `.unbc` re-runs resolved modules cwd-relative instead of source-dir-relative (D-072). |
| `b8f1088` | 9 | Docs consolidation under `docs/` (git mv), `docs/project/REFACTOR_SUMMARY.md` index added. |
| `4048761` | 10 | Legacy removal: `REFACTOR_PLAN.md` and `docs/archive/` deleted; `TECHNICAL_DEBT.md` pruned to open items; stale counters/diagrams refreshed. |
| `dcb3ef9` | 11 | Benchmark-driven fix: `RegisterClass` no longer clobbers the seeded class field layout — class-method JIT actually engages (`norm()` compiles; `class1000` 5.59 → 4.73 ms). |
| (pass 12) | 12 | Debt closures: TD-46 (`iter.range` swap), TD-12 (dead `TryEntry.depth`), TD-42 (`rw` read_line UTF-8/EOF), TD-07 (IntDict arena: free-on-exit, poison-tolerant lock) + IntDict id escape fix. |
| `c2cc664` | 13 | Bug sweep: `a[]` no longer panics the process (clean `LangError`); probed string/UTF-8, try-remove semantics, unsafe surface, interner bounds — all clean. |
| (pass 14) | 14 | TD-36 (regex 10M-step budget; error instead of infinite hang, negative fixture) and TD-37 (HTTP/WS 10 s connect + 30 s IO timeouts, 64 MiB response cap with exact `Content-Length` validation, consistent scheme-guard messages, case-insensitive `set_header`). |
| `aad0f42` | 15 | Pass 14 follow-through for the playground WASM CI (52× red at `Test un`): native depth underflow in `transfer.rs` debug builds and a Cranelift prologue partial-block panic in `jit_bc.rs`; `RUSTUP_TOOLCHAIN=stable` in `un-playground/update-wasm.yml`. Update WASM CI is now green. |
| (pass 16) | 16 | Stopwatch `ms`/`ns` are now read-only native properties (no call syntax); writes raise `cannot assign to a read-only property`. Native property plumbing fixed (original-class table routing on read and write; `SetAttr` yields `LangError` like `Call`). |
| (pass 17) | 17 | Benchmark overhaul: Python lane added (winget CPython 3.13.17; PATH `python` is the broken WindowsApps stub, exit 9009) with 16 mirrored `bench/python/*.py`; two new 3-language benchmarks (`int_ops`, `mergesort1000`); `scripts/bench.ps1` rewritten (Un VM + Un JIT + Python + Node lanes, `-Group all|unvm|unjit|node|python`, `-Only <labels>`, auto Python/Node detection, header-indexed CSV compare vs the previous run, `-Report` regenerates root `BENCHMARK.md`, `-Compare <csv>` advisory regression detection, `results.json`); `BENCHMARK.md` generated at root; `docs/guides/BENCHMARK_GUIDE.md` rewritten. Measured: JIT ≈ VM (Int-subset JIT only), Un JIT/Python geomean **0.224x**, Un JIT/Node.js **0.005x**; Un wins Python at `nested_sum_200` (1.12x) and `dict5000` (1.37x). |
| (pass 18) | 18 | **The JIT was silently dead** (Pass 15 regression, `aad0f42`): prologue/block layout made the parameterless prologue the layout entry block, so Cranelift's verifier rejected every proto and `define_function` failed → silent interpreter fallback with all gates green. Fixed: prologue carries the signature parameters (it is the layout entry), body is parameterless, prologue jumps into it. Also added `Band`/`Bor`/`Bxor` to the Int subset and a `UV_JIT_DEBUG` log for `define_function` failures. **`int_ops`: 3226 ms → 0.08 s** (~400x); geomean Un JIT/Python **0.224x → 1.987x**; Un JIT now beats Python on Int/intrinsic rows and beats Node.js on 4 rows. All gates re-verified: 1406/41/205; differential 81/0; round-trip 40+34; `BENCHMARK.md` regenerated. |
| (pass 19) | 19 | User-facing semantics (`D-074`): `str.split()` now defaults the separator to `' '`; every `map` native accepts Func **or a TObj** (`["3","1"].map(int)` as converter) and dispatches arity-adaptively — 2-arg callee gets `(value, index)`, 1-arg callee gets just `(value)`. `io.read().split().map(int)` works. Positive assertions 1406→**1414**; all gates re-verified (41/81/205/40+34). |
| (pass 20) | 20 | Ergonomics sweep + fix: `str.trim()` now defaults `chars` to Unicode whitespace (Python `strip()` parity; `missing required argument` gone). Rest of the probe sweep found nothing else broken: `int(" 42")`/`float()`, `dict.get`, `list.pop()` (last element), `index_of` miss=-1, `sort`/`reverse`/`keys`/`values` all idiomatic already. Positive **1414→1415**; all gates re-verified. |
| (pass 21) | 21 | Global namespace shrink: `bin`/`oct`/`hex`/`ascii`/`chr`/`ord` moved verbatim to the new `codes` module (`use codes`); globals now exactly `len`/`exit`/`type`/`hash`/`throw`/`throw_if`/`throw_unless`/`repr` (+ constructors and `true`/`false`/`none`/`null`). Tests/fixtures updated; D-076; all gates re-verified (1415/41/205/81/40+34/109). |
| (pass 22) | 22 | fib benchmark: 150k recursive calls at ~3.4 ms (CPython parity), asking to catch up. Root cause: `un_jit_depth_enter` did an atomic RMW *and* a `WORLD.with(|w| w.borrow()...)` `max_recursion_depth()` lookup on every call. Fix: cached `AtomicUsize` (Relaxed) `JIT_MAX_DEPTH`, plain shared-memory `AtomicUsize` (Relaxed) `JIT_DEPTH`; `sync_max_jit_depth()` refreshes after `sys.setrecursionlimit`. **fib24: 3.39 → 1.32 ms** (~2.4x faster than Python); D-077; all gates re-verified. |

Pass-by-pass evidence is in `docs/project/REFACTOR_LOG.md`; the audit findings with
fix mapping in `docs/project/PROJECT_AUDIT.md`; the register of remaining debt in
`docs/project/TECHNICAL_DEBT.md`.

## What changed structurally

- **Semantics pinned to the C# reference where it differed sanely**; where
  the reference was itself arbitrary (e.g. `type(none) is none`), the Rust
  behavior is now the documented standard semantics (D-067 supersedes
  D-027/D-035; slice and `upper_bound`/`lower_bound` were fixed, not
  documented around).
- **JIT**: Int-subset protos + `floor(sqrt)`/`sin` intrinsics + IntDict +
  class methods with name-checked layout intrinsics. Receiver is a hidden
  raw `Rc<ObjData>` pointer; bail (tag 2) falls back to the interpreter with
  semantics preserved. Compilation is lazy. All per-program JIT state is
  keyed by `Program.id`; `clear_program()` and `clear()` free native code
  soundly after teardown. `.unbc` format and `Op` discriminants unchanged
  throughout.
- **Frontend**: desugaring folded into the single-pass bottom-up optimizer
  (`optimize.rs`); unused AST machinery removed.
- **Runtime**: `world.rs` split into `runtime/world/` modules; `runtime/global.rs`
  renamed to `runtime/modules.rs`; per-scope slot-name snapshots removed.
- **Docs**: all engineering docs consolidated under `docs/` (this folder);
  `REFACTOR_PLAN.md` and the migration records under `docs/archive/` removed.
- **Later additions (present as of the 2026-11-23 refresh)**: generics and
  data-carrying enum variants; plain no-VM `un build` (links `libnrt.a`, the
  exe contains no VM; `--embed` is the escape hatch for dynamic cases);
  `dict[int,int]`, `un lsp`, and the 6-lane benchmark suite.

## Current baseline (verified 2026-11-23 at HEAD 660559b; repository HEAD has since advanced to f161f11)

| Gate | Result |
| --- | --- |
| `cargo build --release` | 0 errors, **0 warnings** |
| `cargo test --release --workspace` | **369 passed / 0 failed** (nrt 25 + lib 296 + un bin/LSP 24 + playground 8; includes 2 JIT + 3 sandbox regression guards added 2026-11-20) |
| `scripts/test.ps1` | Positive **51 files / 1638 passed** / 0 failed / 0 crashed; Negative **51 matched / 0 divergences**; Known **0**; exit **0** |
| `scripts/differential.ps1` | **102 files, 0 differences** (JIT on vs off, byte-identical) |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | Positive **51/51**; Negative compilable **42/42** identical (+ 9 compile-rejected skip) |
| `test/test_suite.un` | **109 passed / 0 failed** |
| `scripts/compile-check.ps1` | **36 passed / 0 failed** |
| `scripts/exe-diff.ps1` | **42 passed / 0 failed** |
| `scripts/bench.ps1 -Group all -Report` | 6 lanes (unvm/unjit/python/node/unnative/unbuild) self-validate; `BENCHMARK.md` regenerated per run (생성 시점 커밋 명시); Un JIT/Python geomean **3.688x** (`BENCHMARK.md`, 재생성 2026-11-25 18:51 +09:00, 커밋 180d3db 기준) |

## Intentionally not done / known limits

- In the bytecode JIT (Int subset), class methods with Float/Bool/None fields
  or constructors returning `none`/building lists bail to the interpreter.
  This is separate from plain `un build`, whose NRT runs those cases natively.
- The bytecode JIT is an Int-subset accelerator, not a general native backend;
  `UV_JIT=0` disables it entirely.
- The vendored crates under `vendor/` carry raw-kernel32-FFI patches
  (cranelift-jit, region, icache-coherence) replacing windows-sys; new
  external crates remain off-limits.
- `bench/results/` is gitignored (history shows it was committed early,
  un-tracked later).
- CI (`.github/workflows/build.yml`) runs on every push/PR to `main` (12 steps):
  checkout, install GNU toolchain, `cargo build --release`, `scripts/test.ps1`,
  `cargo test`, `scripts/compile-check.ps1`, `scripts/exe-diff.ps1`,
  `scripts/edge-fuzz.ps1`, `scripts/differential.ps1`,
  `scripts/unbc-roundtrip.ps1 -IncludeNegative`, and the two strict-canary
  steps (`UV_JIT=2`, `test/jit/strict.un` Int + `test/jit/strict_float.un`
  Float). The local runner sequence above remains the fast feedback loop.
- `test/KnownDivergent.txt` documents deliberate implementation differences
  (currently none registered); `test/KnownFailures.txt` is empty.

## Document map

| File | Role |
| --- | --- |
| `README.md` ↔ `docs/README.ko.md` | User docs (English / Korean), kept in sync |
| `docs/spec/ARCHITECTURE.md` | Current architecture: pipeline, VM, JIT, module layout |
| `docs/project/PROJECT_AUDIT.md` | Audit findings (S1/S2/S3) with fix mapping |
| `docs/project/REFACTOR_LOG.md` | Per-pass verification records |
| `docs/project/TECHNICAL_DEBT.md` | Debt register (open items + closed summary) |
| `docs/project/DECISIONS.md` | D-001 … D-092, append-only |
| `docs/spec/BYTECODE.md` | `.unbc` format specification |
| `docs/guides/TEST_GUIDE.md` | Test authoring and runner guide |
| `docs/guides/BENCHMARK_GUIDE.md` | Benchmark methodology and caveats |
| `BENCHMARK.md` | Generated benchmark report at the repository root; regenerate with `scripts\bench.ps1 -Group all -Report` |
| `docs/project/REFACTOR_SUMMARY.md` | This file |
