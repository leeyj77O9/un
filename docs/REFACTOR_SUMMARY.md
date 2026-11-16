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

Pass-by-pass evidence is in `docs/REFACTOR_LOG.md`; per-phase checkboxes in
`docs/REFACTOR_PLAN.md`; the audit findings with fix mapping in
`docs/PROJECT_AUDIT.md`; the register of remaining debt in
`docs/TECHNICAL_DEBT.md` (all "high" items resolved or judged; what remains
is explicitly listed there).

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
  historical migration records under `docs/archive/`.

## Current baseline (all verified 2026-11-16)

| Gate | Result |
| --- | --- |
| `cargo build --release` | 0 errors, **0 warnings** |
| `cargo test --release` | **204 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1395 passed**; Negative **39 matched / 0 mismatched / 0 divergences**; Known **0**; exit **0** |
| `scripts/differential.ps1` | **79 files, 0 differences** (JIT on vs off, byte-identical) |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | Positive **40/40**; Negative compilable **33/33** identical |
| `test/test_suite.un` | **109 passed / 0 failed** |

## Intentionally not done / known limits

- Class methods with Float/Bool/None fields or constructors returning `none`
  or building lists stay on the interpreter (bail → fallback), by design.
- The JIT is an Int-subset accelerator, not a general native backend;
  `UV_JIT=0` disables it entirely.
- The vendored crates under `vendor/` carry raw-kernel32-FFI patches
  (cranelift-jit, region, icache-coherence) replacing windows-sys; new
  external crates remain off-limits.
- `bench/results/` is gitignored (history shows it was committed early,
  un-tracked later).
- CI (`.github/workflows/build.yml`) is defined but has never executed on
  GitHub; the local runner sequence above is the source of truth.
- `test/KnownDivergent.txt` documents the residual, deliberate
  implementation differences; `test/KnownFailures.txt` is empty.

## Document map

| File | Role |
| --- | --- |
| `docs/README.md` ↔ `README.md` | User docs (English / Korean), kept in sync |
| `docs/ARCHITECTURE.md` | Current architecture: pipeline, VM, JIT, module layout |
| `docs/PROJECT_AUDIT.md` | Audit findings (S1/S2/S3) with fix mapping |
| `docs/REFACTOR_PLAN.md` | Phase plan with checkbox tracking |
| `docs/REFACTOR_LOG.md` | Per-pass verification records |
| `docs/TECHNICAL_DEBT.md` | Debt register (open/resolved/judged) |
| `docs/DECISIONS.md` | D-001 … D-072, append-only |
| `docs/BYTECODE.md` | `.unbc` format specification |
| `docs/TEST_GUIDE.md` | Test authoring and runner guide |
| `docs/BENCHMARK_GUIDE.md` | Benchmark methodology and caveats |
| `docs/REFACTOR_SUMMARY.md` | This file |
| `docs/archive/` | Historical: C#→Rust migration records (superseded) |
