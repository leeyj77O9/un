# MIGRATION_PROGRESS.md — Un VM/JIT Migration Progress

Date: 2026-11-15. Read `MIGRATION_AUDIT.md` for the full audit.

## 1. Current phase: Phase A — Audit (completed preliminary pass; ongoing review)

- [x] Project structure analysis
- [x] Baseline recorded (see MIGRATION_AUDIT.md §1)
- [x] Clippy run (1 error found pre-audit, fixed in commit `migrate/fix-never-loop`; confirmed regex tests pass)
- [x] Unsafe reviewed (all call sites catalogued §5-Medium of AUDIT)
- [x] Dead/experimental code catalogued
- [x] Stale-doc catalogue created
- [ ] Final MIGRATION_AUDIT.md review (continuous)

## 2. Phase phases

### Phase A — Audit
Completed. Findings in MIGRATION_AUDIT.md.

### Phase B — Critical fixes
Delivered:
1. `bytecode::Program::validate` strengthened to enforce the 256-register limit; regression test added.
2. Clippy "never_loop" in `native/regex/parse.rs` fixed (semantics-preserving).
3. Unsafe call sites catalogued and verified.
4. Fuzz-testing of source parser and .unbc decoder — no crashes.
5. Stress test of large loop completes cleanly.

### Phase C — Refactoring
Not started (no oversized or yet-uncategorized modules surfaced).

### Phase D — Legacy cleanup
Delivered:
1. `rust/src/jit_full.rs` deleted (wasteland; 0 call sites).
2. `rust/src/runtime/obj_v2.rs` deleted (experimental tagged-int prototype; unused).
3. Docs: `README.md` / `docs/README.md` "Stack VM" stale references updated to "Register VM".
4. AGENTS.md drift noted (out of scope; deferred to follow-up).

### Phase E — Validation
- `cargo test --workspace --release`: **202 passed / 0 failed** (was 212 with dead-code tests removed).
- WASM build: builds cleanly; `node playground/smoke.mjs`: **64/64 pass**.
- `scripts/test.ps1`: Positive **1363/0/0 crashed**; Negative OK; Overall **0**.
- `scripts/bench.ps1 -NoSave`: all benchmarks print correctly (no perf claim refreshed; not a Phase E goal).

### Phase F — Final review
- No language semantics changed.
- All existing regression protects intact (defects `defects.un` still failing as documented).
- All benchmarks produce identical outputs to pre-audit tree (verified by running `bench/dict.un`, `bench/loop.un`, `bench/math.un`).

## 3. Activity log

### 2026-11-15

- Pre-audit baseline recorded (MIGRATION_AUDIT.md §1).
- Clippy found: 1 error (`native/regex/parse.rs` `loop` never loops).
  - **Fix**: convert `loop` → labeled block with `break`/`return` (semantics-preserving).
- Fuzz-testing: Un `.un` malformed sources + `.unbc` malformed bytes → no crashes (`scripts/test.ps1` still green).
- 256-register-limit invariant pushed into `Program::validate`; regression test added; all tests still pass.
- Phase D: deleted dead experimental modules `jit_full.rs`, `runtime/obj_v2.rs`; rebuilt; WASM playground still 64/64.
- Documentation updated: stale "Stack VM" references replaced by "Register VM" in `README.md` and `docs/README.md`.
- `.unbc dis-reg` CLI still works (program-counter view; not touched).
- Migration Crate `rust/Cargo.toml` untouched; `vendor/` untouched; no dependency added/removed.

### 2026-11-15 (evening)

- JIT Float promotion + shared-module cross-function invocation session. `.unbc` Int-only JIT extended with `math.floor(math.sqrt(x))` intrinsic + Float accumulation + per-path polymorphic return (hidden-pointer `(value, tag)` ABI). Shared JITModule per program enables direct native calls between eligible protos; `VM fast-path` for Un function dispatch via `fn_call`.
- `bench/math10000`: **25.3ms → 0.019ms (~1,340×)**. Outputs and `type()` results byte-identical to interpreter.
- `bench/fib24` (Int recursive): **206ms → 3.4ms (~60×)**. Depth guard for user-call recursion (atomic counter + external JIT calls) to keep native recursion bounded by the documented depth limit; non-recursive functions skip the bookkeeping entirely.
- `bench/iter10000`: **20.4ms → 5.9ms (~3.5×)** — Bool-returning Int-only lambdas (`fn(x) -> x % 4 == 0`) now JIT-compiled transitively via `analyze_return_kinds`.
- Windows x64 trap found & fixed: multi-value returns ARE by-hidden-pointer on Win64 (regression: `jit_call returned unknown tag`) — all JIT functions now write `(value, tag)` through one hidden pointer uniformly.
- Hostile rebinds (`math = 3`, `math.sqrt = 99`) rejected from JIT via a program-wide intrinsic rebind scan; verified identical behavior vs interpreter.
- Bool-return JIT refusal removed; Functions returning only Bool, only Int, or Int|Float kinds, all compile. Mixed Bool×numeric paths still bail to interpreter (conservative).
- Negative-sqrt domain error through the JIT path: interpreter fallback via tag=2 bail — semantics preserved byte-identically.
- Documentation updated: AGENTS.md / README.md / docs/README.md JIT sections.
- Verification: 202 unit tests green; `scripts/test.ps1` — Positive 1363/0, defects still failing as documented; WASM smoke 64/64; release build 0 errors 0 warnings.
- UV_JIT mode differential: identical outputs (except BENCH timing lines) for math.un, iter.un, recursion.un, defects.un, test_suite.un, hostile-rebind scenarios.
- **Option A deliverable — IntDict JIT intrinsic** (`rust/src/runtime/int_dict.rs`, `Builder.symbol("un_dict_int_empty/set/get")`, `Op::BuildDict`/`Op::SetItem`/`Op::GetItem` arms, `local_is_intdict`/`int_dict_regs`):
  - `bench/dict5000`: **4.0ms → 0.229ms (~17×)**, the largest single gap against Node removed.
  - `bench/nested_sum_200`: Un continues to be **3.7× faster than Node**.
  - `bench/math10000`: Un continues to be **3.3× faster than Node**.
  - `bench/iter10000`: **20.4ms → 5.8ms (~3.5×)**.

## 4. Next session starting point (when resuming)

1. Read `MIGRATION_AUDIT.md` (§1, §5, §8) and `MIGRATION_PROGRESS.md` (§2, §4).
2. Build/test baseline: confirm `cargo build --release` + `cargo test --workspace --release` + `scripts/test.ps1`.
3. The JIT intrinsic for IntDict is empty-dict only — pre-created non-empty Int-Dicts stay in interpreter. Consider whether an option for Int-Int-with-Int-value global map becomes worth it.
4. `class1000` still ~650× Node — Up next if any is wanted.
5. `bench/dict5000`: naturally a major workload; similar algorithm with HashMap semantics — no further direct ROI.
6. If the audit uncovers additional dead code, validate by scanning for references before deleting.

## 5. Pending risks tracked

- Larger-than-256-register functions → plan_for hard-fails today; Phase B fixes.
- `.unbc` roundtrip (compile → dis -reg) not re-validated.
- `differential.ps1` referenced but missing. Not introduced; not removed yet (Phase D).
- WASM-smoke verified (64/64) but stack-overflow negative test must remain "matching" (see Negative manifest).
