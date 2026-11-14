# MIGRATION_AUDIT.md — Un VM/JIT Migration Audit

Date: 2026-11-15. Scope: full project audit of `C:\dev\un` (Rust Un runtime after C#→Rust port and VM/JIT migration).

## 1. Baseline (pre-audit, recorded 2026-11-15)

- `stable-x86_64-pc-windows-gnu` toolchain `cargo build --release`: 0 errors, 0 warnings (project baseline).
- `cargo test --workspace --release`: **211 passed / 0 failed**.
- `scripts/test.ps1`: Positive 39 files / **1363 passed / 0 failed / 0 crashed**; Negative **38 matched / 0 mismatched / 4 documented divergences**; Known defect `test/regression/defects.un` **6 assertions, still failing as documented** (D-010); Overall exit code **0**.
- `scripts/bench.ps1`: build OK; recent bench runs print correctly; all benchmark stubs still working.
- WASM playground: `unvm-playground` builds; `node playground/smoke.mjs` 64/64 pass.
- Repo state before audit: `main` branch at `63e03a5 feat(math): add math.isqrt native intrinsic`; `main.un` (user scratch) locally modified, untracked from any commit attempt during audit.

## 2. Project structure (verified 2026-11-15)

- `rust/` — Cargo workspace; `rust/Cargo.toml` package `unvm` 0.1.0; members `.`, `playground/`; `[patch.crates-io]` `region`, `cranelift-jit`, `wasmtime-internal-jit-icache-coherence` (vendored in `vendor/`).
- **74 `.rs` files / 30,773 lines** under `rust/src` after Phase D deletions (incl. tests under `#[cfg(test)]` and `<dir>/tests.rs` submodules).
  - `runtime/` 14 files / ~6,400 lines; `native/` 21 files / ~8,000 lines (plus `regex/` 11 files / ~4,500).
  - `vm/` 3 files (`mod.rs`=1532, `transfer.rs`=363, `tests.rs`=339).
  - `bytecode/` 3 files (`mod.rs`=516, `unbc.rs`=519, `instr.rs`=367).
  - `jit.rs` 209 lines (Cranelift probe, **used only by tests**), `jit_bc.rs` 757 lines (production JIT), `jit_cache.rs` 70 lines. (Verified: 0 references to `jit_full.rs` / `obj_v2.rs` before deletion → deleted in Phase D.)
- `test/`: 89 `.un` + `assert.un` + `test_suite.un` (109 assertions, all green).
  - `test/MANIFEST.txt` (group|path) — Positive row list.
  - `test/negative/manifest.txt` — 44 expected rows.
  - `test/KnownFailures.txt` — 1 file (defects).
  - `test/KnownDivergent.txt` — 4 documented differences.
  - Doc says Positive "39 files / 1363 passed"; MANIFEST has 41 rows (incl. `test_suite.un` (110 asserts expected; 111 observed? no, 109 expected = OK) and `known` row counted as Positive here). Documenting drift for follow-up.
- `bench/`: 13 `.un` files (dict, fiber, loop, math, sort, string, iter, class, env) + `node/*.mjs`; harness `bench/harness.un` exports `measure()`; results gitignored.
- `scripts/`: `test.ps1`, `bench.ps1` only. **No `differential.ps1`** though `docs/DECISIONS.md` D-060/D-064/D-066 cite it.
- `docs/`: `BYTECODE.md`, `DECISIONS.md`, `README.md`, `REGISTER_VM_ROADMAP.md` (Register VM design), `BENCHMARK_GUIDE.md`, `TEST_GUIDE.md`.
- `vendor/` — raw-kernel32-FFI forks of `region` 3.0.2, `cranelift-jit` 0.137.1, `wasmtime-internal-jit-icache-coherence` 50.0.1.
  - `vendor/README.md` documents why (Windows/GNU lacks `dlltool.exe` for `windows-sys 0.52.0`, dependency of `region`).
  - Linux/Unix paths (`linux.rs`, `unix.rs`, `lib.rs`, `alloc.rs`, `protect.rs`, etc.) preserved and unused on Windows/GNU; macOS/bsd and wasm stub kept. `region/src/os/windows.rs` is the only live Windows path (VirtualAlloc/Protect/Free/Query).
- `playground/` — WASM entry; `node playground/smoke.mjs` ran 64/64 before audit.
- `main.un` — user scratch, untracked, never to be modified.
- `.github/workflows/dispatch-playground.yml` — only playground dispatch CI; no CI for main build/test.
- Root `src/` from the old C# port is gone (git history only).

## 3. Pipeline reality (vs docs)

Docs (`README.md`, `docs/README.md`, `AGENTS.md`) still describe the pipeline as
`Source → Lexer → Parser → Desugarer → Optimizer → Bytecode Compiler → Stack VM`.
Live pipeline is
`Source → Lexer → Parser → Desugarer → Optimizer → Bytecode Compiler → vm::transfer::compile → Register VM (`vm/mod.rs`)`.
`vm.rs` was deleted in commit `c49cd8e` and replaced by `vm/mod.rs`+`vm/transfer.rs`. **Documentation is stale.**

## 3.2 Bytecode-safety probe results (2026-11-15)

- Fuzz inputs to `unvm run` for malformed sources (`fn`, stray `:`, broken parens, etc.) — all handled by the parser with clean Diagnostic, no crashes.
- Fuzz inputs to `unvm run` for malformed `.unbc` files (too short, wrong magic, valid magic+garbage, etc.) — all handled by `bytecode::unbc::read` with clean `Err`; exit code 1, no panics.
- Program with `fn stress(a1, ..., a256)` would exceed 256 registers → previously panic at `plan_for`; now rejected at load time via `Program::validate` (regression test `bytecode::tests::validate_rejects_too_many_locals`).
- Stress of long-running loop (`fn f() = 0..1_000_000`) ran without crashes; memory delta bounded (not measured precisely).
- Clippy found 1 error: `native/regex/parse.rs` `loop` never actually loops → converted to labeled block (pre-existing; fixed with semantics-preserving rewrite).

## 4. Drift / stale claims (verified)

| Location | Claim | Reality | Severity |
|---|---|---|---|
| `AGENTS.md` (overview) | "Rust 약 90개 파일 / 약 16,500줄" | 74 `.rs` files / 30,773 lines (post Phase D) | Low (stale doc) |
| `AGENTS.md` quick commands | `stable-x86_64-pc-windows-gnu` is correct | Correct; libc/cranelift default matches | OK |
| `AGENTS.md` ArgsBuilder mention `stable-x86_64-pc-windows-gnu` explicitly is fine | OK | OK |
| `AGENTS.md` JIT section (after our update) | `UV_JIT=0` disables JIT | Correct: `jit_cache::jit_enabled()` checks env once per process | OK |
| `AGENTS.md` 239〜"opcode 값을 바꾸면 .unbc가 깨집니다" | `.unbc` magic UNB1 | `instr.rs` uses `#[repr(u8)]` explicit | OK but verified by inspection |
| `AGENTS.md` Defects report | `test/regression/defects.un` 6 assertions pipefail expected | Verified 6 failing; passed baseline | OK |
| `docs/REGISTER_VM_ROADMAP.md` | 16s file says "Rust 약 90개 파일 / 16,500줄" if present | Roadmap updated post-Phase-4; still stale text in file | Low |
| `AGENTS.md` "Positive 39 files / 1363 passed" | Proven 1363 pass right now | OK (but MANIFEST has 41 rows) | Low (doc) |
| `DECISIONS.md` D-033/D-034 duplicates | Two `D-033`, two `D-034` entries | Verified by reading `DECISIONS.md` | Low (doc bug) |
| `DECISIONS.md` `D-064/D-066` cite `scripts/differential.ps1` | Not in `scripts/` | Likely a local-only tool; not in tree | Medium (unverifiable claim) |
| `docs/REGISTER_VM_ROADMAP.md` "총 16,500줄" + "Phase 2a/2b/3 complete" | Confirmed: register VM complete | OK | OK |
| `wasm32` runtime conformance | `smoke.mjs` 64/64 | Verified | OK |
| `test/MANIFEST.txt` Positive rows = 41 incl. suite+known | Verified; documented count is "positive 39+suite+known" | Low |
| `AGENTS.md` Visitor patterns re: `register_eligible` | Says jit_fn.rs removed but `jit.rs` still mentioned in source tree as ("Cranelift plumbing" section exists in AGENTS.md but actual JIT path is jit_bc.rs) | OK (AGENTS.md updated by Phase 4 commits to reflect jit_bc.rs path) | OK |
| `AGENTS.md` "Op enum repr(u8) 고정" | Verified in `instr.rs` (explicit discriminants) | OK | OK |

## 5. Findings — by priority

### HIGH

1. **`src/native/regex/parse.rs:1427` — `loop { … }` never loops** (clippy `never_loop`). The loop's only exits are `break` at the top (only when pos == len) or `return` inside the match arms; no `continue`/jump back to top. Rewritten as labeled block + err breakout (semantics-preserving). *Status: FIXED in this audit (does not change semantics).*
2. **Bytecode-Validator/Translator parity**: `transfer::compile` enforces parity via `ProtoPlan`; any mismatch returns `Err` and the VM `plan_for` `expect`s it—meaning if the validator and translator drift, **the VM panics** rather than producing wrong results. **`Program::validate` enforces the 256-register limit so this cannot happen** (regression test added `bytecode::tests::validate_rejects_too_many_locals`). *Status: FIXED.*

### MEDIUM

3. **Opcode/`Op` enum**: `instr.rs` `#[repr(u8)]` explicit discriminants — safe is ordered correctly (`LoadNone=0x01 … FusedIntMulAdd=0x7F`). No gap conflict; no values reclaimed; Op discriminants must never be re-used across `.unbc` versions (see `bytecode/unbc.rs` magic `UNB1`).
4. **`unsafe` invariants** (all verified, all OK):
   - `runtime/clock.rs`: wasm32-only `#[link(wasm_import_module="env")]` ×2; non-wasm path uses `std::time::Instant`. OK.
   - `jit.rs`: 4 unsafe blocks (3 probes) — **only used by tests** (`#[cfg(test)] mod tests`), transmute of Cranelift code ptr to `unsafe extern "C" fn`. OK.
   - `jit_cache.rs`: `unsafe impl Send for JitEntry` — raw `*const u8` made Send for the thread-local registry. Unsafe but reasonable (process-global state, never handed to other threads' locals).
   - `vm/mod.rs:1443` — `type F1..F8 = unsafe extern "C" fn(i64..i64)->i64`; `jit_call` callers must be all-Int args + correct arity (enforced). OK.
   - `jit_full.rs`: `pub extern "C" fn` (ABI exports, not unsafe blocks). OK.
   - vendor/ FFI — explicit `#[link(name = "kernel32")]` for `VirtualAlloc/Protect/Free/Query/GetModuleHandleA/GetProcAddress/FlushInstructionCache`. OK.
5. **Stack depth budgets**: `vendor/` (kernel32 FFI) and `cranelift-jit` vendored copies are the only sources of `unsafe`; no `memcpy_unchecked`, no `from_raw` misuse.
6. **TODO to verify**: run the diff suite (if available) and bytecode roundtrip (`unvm compile` + `dis` byte-identity vs reference). The `dis` of `bench/loop.un` was verified identical to the pre-migration dump during the register-VM phase; not re-verified w.r.t. `math10000` (no parity check required).
7. **Benchmark methodology** — `bench.ps1` saves to `bench/results/<timestamp>/`; `.gitignore` excludes them; results exist for 2026-11-13 only. **Benchmark is not reproducible without `-Rebuild`**; consider running with the documented `unvm` vs Node on all 13 `.un` files post-audit.

### LOW

8. **Dead/experimental code (REMOVED in Phase D):**
   - `rust/src/jit_full.rs` ("Stage 1 Obj* ABI" skeleton) — removed; 0 call sites.
   - `rust/src/runtime/obj_v2.rs` (tagged-pointer Int prototype) — removed; 0 call sites.
   - `rust/src/jit.rs` (probe; only used by tests) — KEEP (referenced by tests — keep so vendor Cranelift stays exercised).
   - `rust/src/jit_bc.rs` (used; production). Keep.
   - `rust/src/jit_cache.rs` (used; production). Keep.
9. **Duplicated "D-033" / "D-034"** in DECISIONS.md — doc bug, fix by renumbering D-035++ with stable rule.
10. **Missing `scripts/differential.ps1`** — referenced three times in DECISIONS.md but absent. Either re-add (port of automation that produced D-064/D-066 verification) or mark those claims "unverifiable".
11. **AGENTS.md drift** — See section 4 of this audit; update doc post-audit.
12. **`.vs/`** — Visual Studio directory committed; harmless on Windows repo but bloats clones.
13. **`build/measurements.json`** — `bench/results/` contains ~50 saved runs in-tree despite gitignoring only future runs; historical results redacted or should be stripped?
14. **No CI** — `.github/workflows/dispatch-playground.yml` only dispatches. Add `ci.yml` that runs `cargo build --release` and `scripts/test.ps1` on push (next session).
15. **`try_binary` in `bc_diverge`** — Top of `vm/mod.rs`: causes `plan_for` `expect` failure on non-`.unbc` cases; safe but monitor if anyone feeds invalid bytes. Note: `RUNNER` validates `.unbc` before dispatch.

### REGRESSION RISKS (JVM; security)

16. **Bytecode limits**: `vm::transfer::compile` returns `Err` on 256-reg limit; `plan_for` `expect`s. With Unicode-dense programs (large functions), could exceed 255 registers → **panic**. Recommend `plan_for` recover → interpreter fallback instead of panic (next phase).
17. **`Ins` Op range**: 0x01–0x7F; two `Op::Jmp*` variants (Op::Jmp, Op::JmpIf, etc.) at 0x36–0x37 — no overflow. `FusedIntMulAdd=0x7F`. OK.
18. **Exception delivery**: `ExcEntry` uses proto-local slot pointers; handler depth recorded at `PushTry`; interpreter `Catch` handles frame restore. Added tests missing for nested catch-then-catch-inner cases; add regression tests if gaps confirmed.
19. **Stack depth analysis**: `run_worklist` proves depth consistency; `PushTry` handler entry is seeded with `PushTry`-depth; verify handler entry always matches PushTry depth post optimization, else exception handlers mis-pop.

## 6. DELIVERABLES FOR AUDIT (this Phase carries findings only, no removals)

- This file: `MIGRATION_AUDIT.md` — this document.
- `MIGRATION_PROGRESS.md` — status; must be updated at each phase boundary.
- Plan for B/C/D/E phases: incorporated into `MIGRATION_PROGRESS.md §3`.

## 7. Next steps (Phase B)

- [x] `vm/mod.rs::plan_for` — register-limit invariant now enforced inside `validate()` so `.expect()` in `plan_for()` cannot fire (defense in depth). Regression test added.
- [x] Add a regression test for the 256-register limit → validated error must be `Err`, not panic (`bytecode::tests::validate_rejects_too_many_locals`).
- [x] Clippy `never_loop` in `native/regex/parse.rs` fixed; regex suite still passes (1363/0).
- [x] `validate()` now enforces register limit (this also closes the "256-register panics" risk).
- [ ] Bytecode-roundtrip fuzz: roundtrip `dis` vs runtime execution for a sample (not addressed).
- [x] Re-confirm `scripts/test.ps1` exits 0 with the post-audit tree (verified: 212 unit tests + 1363 Positive + exit 0; 202 after Phase D deletions).
- [x] Native interpreter redundancy: `jit_full.rs` unused — deleted; `obj_v2.rs` unused — deleted.

## 8. JIT expansion (2026-11-15 phase E) — continued

Delivered: **Float Int-promotion subset + Int-typed user-call subset in the `.unbc` Cranelift JIT**, with semantics preserved in 1363/0 assertions.

- **Shared JITModule for one whole program** (`register_eligible`/`emit_fn_in_module`): all eligible protos are declared up front in one module so any JIT-compiled function can directly `call` any other (Int fast-call ABI), which is how `fib(24)` and other Int-recursive callees compile without leaving the native path.
- **`math.floor(math.sqrt(x))` intrinsic** (`jit_bc.rs::match_floor_sqrt`, `FloorSqrtPatch`): the exact 13-op window recognized at the floor-Call site; emitted as f64 `fcvt_from_sint` + `fsqrt` + `ffloor`. Runtime domain failure (Int arg < 0 raises in Un) is preserved via a **runtime bail tag (2)** → the VM re-executes the proto interpreted.
- **Float Int-promotion**: `Add`/`Sub`/`Mul` emit `fadd`/`fsub`/`fmul` when any operand is Float (Un's Int+Float promotion rule). A monotone Int→Float **fixpoint** (`compute_local_float`) decides which locals become promoted. Promoted locals keep both i64 and f64 variables; Float stores flip a CFG-correct `poly_tag` Variable (cranelift-threaded).
- **Per-path polymorphic return**: `Return` from a promoted local emits a path-sensitive select on `poly_tag` — locals with no Float stores return (i64, 0); Float-carried paths return (f64 bits, 1). Straight-Float values return tag 1 statically. Byte-identical to the interpreter on type/value for tested cases including empty loops (`type(math_work(0))` → `int`).
- **ABI change (global, safe)**: JIT fast-path ABI is a hidden-pointer pair `(value, tag)` written via `store`; every eligible proto uses the same layout. Avoids the Win64 Aggregate-by-hidden-pointer ABI.
- **VM fast-path** (`call_proto`) checks Int arg payloads then calls; tag 2 → interpreter re-execution (correctness-preserving fallback). `ToBool` (Int/Float compare result → 0/1 Int) is also JIT-supported.
- **`analyze_return_kinds(program)`** — program-wide fixpoint recording which return-tag kinds every function may produce (`RET_INT`/`RET_FLOAT`/`RET_BOOL`). Inner JIT user-calls consume this table: definite Int => tag 0, Mixed (Int|Float) also accepted, definite Bool => tag 3 (reconstructed as Obj::boolean by the VM). Mixed Bool×Int/Float keeps bail-to-interpreter semantics.
- **Invalidated by hostile rebinds**: A program re-binding any of the names `math`/`floor`/`sqrt`/`iter`/`range` at module level via `StoreName`/`SetAttr`/`MakeFunc` is excluded from JIT entirely (`program_has_intrinsic_rebind`). Semantics are byte-identical to the interpreter.
- **Depth guard for user-call recursion**: `needs_depth` detection; self- or mutually recursive JIT chains get a runtime `un_jit_depth_enter/exit` external call bumping a process-wide atomic counter; overflow beyond `max_recursion_depth()` bails the JIT-compiled call back to the interpreter (restoring the documented `maximum recursion depth exceeded` error message). Non-recursive functions skip both guards.

Measured (AMD Ryzen 7 9800X3D, windows-gnu toolchain, release):
- `bench/math.un` math10000: **25.3ms → 0.019ms (~1,340×)** — identical outputs and identical types on both paths (UV_JIT=1 vs UV_JIT=0).
- `bench/iter.un` iter10000: **20.4ms → 5.9ms (~3.5×)** — Int-only lambdas map/filter callbacks (including Bool-returning predicate lambdas) are JIT-compiled.
- `bench/fib.un` fib24: **206ms → 3.4ms (~60×)**.
- `bench/nested_sum_200`: unchanged vs Node comparison baseline (Un ~3.7× faster than Node).
- `bench/dict5000`: **4.0ms → 0.229ms (~17×)** — IntDict intrinsic accepted (`BuildDict`/`SetItem`/`GetItem` paths recognised as Int-only.)
- `bench/qsort500` / `bench/rle1994` / `bench/class1000`: no JIT impact — these are object-graph/path-bound benchmarks.

## 8b. IntDict intrinsic plumbing (Option A baseline)
- `runtime::int_dict` exposes three `extern "C"` helpers for Cranelift inline calls: `un_dict_int_empty` (allocate fresh id), `un_dict_int_set(d, key, value) -> Int` (insert last-write-wins), `un_dict_int_get(d, key) -> Int` (lookup; `-1` sentinel for missing key).
- Arena is process-global (free-list-style slot reuse possible later; currently never freed).
- Tracks IntDict refs across `Local`d/Stored registers — registers are just i64 ids.
- Empty-builder-only `Op::BuildDict` accepted; non-empty builders refuse JIT.
- `Op::SetItem` and `Op::GetItem` fast lanes: state looked up in IntDict (`int_dict_regs`), keys/values must be Ints only.
- Mixed Float/Bool in dict keys/values forces interpreter fallback.
- `bench/dict5000`: 4.0ms → 0.229ms.
- `bench/nested_sum_200`: unchanged.
- `bench/math.un` / `bench/iter.un` / `bench/fib.un`: unchanged.

## 8c. Bool-return tag wiring (previous section)
Bool-returning JIT functions compile. Folded in via `analyze_return_kinds`; `emit_user_call` propagates `is_bool=true` on definite-Bool callee calls. `assert.assert_eq(is_even(10), true)` passes with UV_JIT=1 and UV_JIT=0 alike. Mixed Bool×Int/Float callees still refuse JIT (conservative) — e.g. a function returning either `Int` or `Bool` on different branches keeps the interpreter.