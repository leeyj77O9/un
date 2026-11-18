# Code Normalization Report

Date: 2026-11-18
Scope: the whole `C:\dev\un` tree (VM, JIT, runtime, compiler, native modules,
benchmarks, test harnesses, docs). This document records what was found,
what was changed, what was deliberately kept, what it cost in benchmark
numbers, and what is still open. Nothing here weakens the language semantics,
the test suite, or the JIT/interpreter contract.

Baseline (before this work): Positive **1415 passed / 0 failed**, Negative
41 matched, Known 0, differential 81 files / 0 differences, `cargo test
--release` 202+3 = 205 passed, 0 errors / 0 warnings.

Final (after this work): Positive **1419 passed / 0 failed**, Negative
41 matched, Known 0, differential 81 files / 0 differences, `cargo test
--release` 205 passed, 0 errors / 0 warnings.

---

## 1. Found: benchmark-shaped special handling

These were implemented to move specific benchmark numbers, not because a real
program needs them. All were removed or re-scoped.

| # | Item | Where | What it was | Disposition |
|---|------|-------|-------------|-------------|
| 1 | `FloorSqrt` JIT intrinsic | `rust/src/jit_bc.rs` (`match_floor_sqrt`, 13-instruction window, `UnaryFloatKind::FloorSqrt`, `emit_unary_float`) | Recognised the exact bytecode sequence `math.floor(math.sqrt(<Int>))` by *name text* (`math`, `floor`, `sqrt`) — precisely the `bench/math.un` kernel (`math10000`) | **Removed** |
| 2 | `Sin` JIT intrinsic | `rust/src/jit_bc.rs` (`match_math_sin`, 7-instruction window, `UnaryFloatKind::Sin`), `native/math.rs` (`un_math_sin` FFI shim) | Recognised the exact call `math.sin(<Int>)` — precisely the `bench/floatmath.un` kernel (`sin20000`) | **Removed** (`un_math_sin` FFI shim removed too) |
| 3 | Intrinsic-related Float-promotion machinery | `rust/src/jit_bc.rs` (`IntArgSrc`, `UnaryFloatPatch`, `intrinsics`/`consumed` maps, the "reject a floor(sqrt()) whose Int argument is a promoted local" pre-check) | Existed solely to feed intrinsics 1–3 | **Removed** |
| 4 | `bench/string.un` rle_decode rewrite | committed in `ef61e07` ("exception-free digit check + explicit while loop replacing for-iter.range in rle_decode (twin-parity)") | Reshaped the Un benchmark to a `while` loop so `rle1994` would JIT; Node.js and Python twins both use a `for`-shaped/count-based decode. The "twin-parity" claim did not hold | **Reverted** to `for k in iter.range(0, count)`; `scripts/bench.ps1` report text updated to match the new reality |
| 5 | `math.floor(math.sqrt(x))` / `math.sin(x)` in docs & JIT description | `AGENTS.md`, `docs/ARCHITECTURE.md`, `docs/README.md`, `README.md` (+ a stale claim that class methods are *not* JIT targets — they are since 982d5f3) | Documented the bench-shaped intrinsics as JIT scope | **Updated** to describe the real eligibility set (Int loops/ops, `FusedIntMulAdd`, IntVec/IntDict/class intrinsics, Float poly-tag) |
| 6 | `scripts/bench.ps1` generated-report prose | mentioned `floor/sqrt` intrinsic as the reason `math10000` is fast | Stale after items 1–3 | **Updated**; `math10000`/`sin20000` now reported as interpreter-bound |

The rebind guard (`program_has_intrinsic_rebind`) applied a whole-program
JIT veto when a module store/definition touched `math`/`floor`/`sqrt`/`iter`/
`range`. Items 1–3 needed that guard; it now only covers `iter`/`range`
(which the range-loop JIT still matches by name text) **and `len`** (see
Finding 9). Semantics of affected programs: they fall back to the
interpreter, which was always the documented behaviour for rebound names.

## 2. Found and fixed: real correctness defects (not benchmark polish)

| # | Item | Where | Defect | Fix |
|---|------|-------|--------|-----|
| 7 | **List arguments were silently corrupted under the JIT** | `rust/src/jit_fast.rs`, `rust/src/jit_bc.rs` (cleanup), `rust/src/runtime/int_list.rs` | `jit_fast` deep-copied every `Payload::List` argument into a JIT-only IntVec arena copy; the callee mutated the copy; the caller's list was never updated. `fn f(xs) xs[i] = xs[i] * 2 — produced `1,2,3` under JIT and `2,4,6` under the interpreter. Arg handles were even *freed* by the callee's teardown | Argument handles are now marked **borrowed** in the emitter (provenance tracked through `LoadName param` / `Dup` / `StoreLocal` / `LoadLocal` round-trips; the callee never frees them). After a successful JIT call, `jit_fast` writes the arena tree back into the caller's Un list objects in place (flat lists wholesale, `KIND_OUTER` row-by-row with identity preserved for rows that remain lists). Two-phase validation: if **any** argument tree is poisoned/dead, the whole JIT result is discarded and the interpreter re-runs the function on the pristine originals — never a partial write. A callee that returns an argument handle itself (`-> xs`) is mapped back onto the original object identity (`is` works) |
| 8 | **IntDict missing key returned `Int(-1)` under the JIT** | `rust/src/jit_bc.rs` (`Op::GetItem` IntDict fast path), `rust/src/runtime/int_dict.rs` | The JIT read A without a presence check and got `-1` for a missing key; the interpreter raises `key not found in dictionary`. Only masked because `dict5000` never reads a missing key | New `un_dict_int_has(d, key)` FFI (1/0); the JIT probe bails (tag=2) on a miss and the interpreter re-runs, producing the exact error. `un_dict_int_get` keeps the sentinel but the docs now state the protocol |
| 9 | **`len` is a name-text marker the rebind guard did not cover** | `rust/src/jit_bc.rs` (`len_regs`, `name == "len"`), `program_has_intrinsic_rebind` | A module that rebinds `len` (`len = 42`) could hit the JIT's `len(xs)` intrinsic instead of the interpreter's "not callable" error. It bailed only by accident of an incidental codegen panic | `len` added to `program_has_intrinsic_rebind` (whole-program JIT veto), like `iter`/`range`. Removes the reliance on incidental bailouts |
| 10 | **Crafted Ints ≥ 2^62 aliased live arena handles** | `rust/src/jit_fast.rs` (FFI argument marshalling) | Un Ints are full i64; arena handles are `idx + 2^62`. A user `Int(2^62 + k)` could alias a live arena handle and be read/written as a list | `jit_fast` refuses to dispatch any Int argument ≥ `ID_BASE` (JIT→interpreter fallback). Note: a *computed* Int ≥ 2^62 produced inside a callee is a different, much narrower aliasing case — see §6 |
| 11 | **Regex DFA fast path returned wrong captures** | `rust/src/native/regex/mod.rs` (`find_all_direct`) | Slot 0 was stored as `(start, length)` while every other code path stores `(start, absolute_end)`; `group_end(0)` returned a length. With a fast-path-eligible pattern and a `$0`/`${0}` backreference in `re.replace_all`, this mis-sliced or panicked (`slice index starts at 9 but ends at 2`) — e.g. `re.replace_all("[0-9]+", "abc123def45", "[$0]")` | Slot 0 now stores `(start, absolute_end)` like every other path; regression tests added to `test/unit/native/re.un` (26 passed) |
| 12 | **List/tuple indices truncated to i32** | `rust/src/runtime/collections.rs` (`list_get_item`, `list_set_item`, `tup_get_item`, `list_insert`, `list_remove_at`) | `i as i32 as i64` mis-indexed for |i| ≥ 2^31; `(i + count) as i32` also overflowed i64 near `i64::MAX` | i32 casts removed (saturated add for the negative-index wrap). `enu_init` keeps its `"as i32 as i64"` — deliberate .NET Int32 enum parity |
| 13 | **`math.hypot(1e200, 1e200)` returned `inf`** | `rust/src/native/math.rs` | Manual `(x*x + y*y).sqrt()` overflows where `f64::hypot` is exact (documented as TD-45) | Now uses `f64::hypot` |
| 14 | **JIT depth guard disabled for embedders that skip `world::reset`** | `rust/src/jit_cache.rs` (`JIT_MAX_DEPTH` initialised to 0 = "unlimited") | A host that never calls `world::reset()` had an unlimited native recursion guard (cli always calls it; the playground calls it; a bare embedder might not) | `JIT_MAX_DEPTH` now initialises to `MAX_RECURSION_DEPTH` (1000); `sys.setrecursionlimit(0)` can no longer select "unlimited" from the public API (it never could), so 0 means exactly "never synced", which is now unreachable by accident |

## 3. Kept, with reasons

These all appeared benchmark-motivated; on inspection they are general,
semantics-preserving, and bail to the interpreter on any unsupported shape.
They stay:

- **The Int JIT as a whole** (`jit_bc.rs`, `jit_fast.rs`, `jit_cache.rs`):
  a register-plan translator with a rejection-driven design (anything it
  cannot model becomes an all-bail stub or falls back at the call site).
  `iter.range` peephole, `FusedIntMulAdd`, Int/Float poly-tag returns,
  user-call direct native calls with a cached recursion-depth bound — general mechanisms, no input values or iteration counts baked in.
- **The IntVec arena + `KIND_OUTER` nested lists** (`runtime/int_list.rs`):
  only used by JIT-emitted code, but it models a general domain
  (homogeneous `List[List[Int|Bool]]`, any length), has uniform-kind
  poisoning with interpreter fallback, and handles the FFI boundary. Its
  costs are documented (§6). Removing it would gut the JIT for sorting/
  sieve/matrix workloads — the kind of change the task warns against
  ("do not delete code without evidence" — the task's constraint).
- **IntDict arena** (`runtime/int_dict.rs`): same shape, Int→Int only; now
  present-guarded at the single read site (item 8).
- **Class-method JIT, Int-field name-checked intrinsics** (`obj.rs`,
  `jit_bc.rs`): name-checked at call time, bail on mismatch; general for
  Int-field classes.
- **Native fast paths** (`native/iter.rs` `sum_list`/`iter_values`,
  `obj.rs` `key_eq`/`hash_code`/ASCII string index, string concat bypassing
  the pool, `vm` ForNext lane, `SetItem` dict lane): all general, all keep
  their generic fallbacks.
- **Regex weak-DFA fast path** (`native/regex/`): narrow pattern class,
  slot-0 capturemarks only, conservative rejection, engine fallback. Fixed
  (item 11) but retained: it is a legitimate optimization of a general
  facility.
- **FusedIntMulAdd operand packing** (compiler `(dst<<20)|(sx<<10)|sy`,
  10-bit slots): structural, general, documented.
- `iter_values`/`sum` fast paths reference clone-avoidance of whole lists — general; the `members.rs` iterator-method twin was left untouched
  (documented divergence, see §5).

## 4. Removed / merged

- `match_floor_sqrt`, `match_math_sin`, `IntArgSrc`, `UnaryFloatKind`,
  `UnaryFloatPatch`, `emit_unary_float`, `intrinsics`/`consumed`
  bookkeeping, the per-proto Float-promoted-arg reject arm, the
  whole-program `floor`/`sqrt`/`math` rebind entries. Net **≈540 lines
  removed** from `jit_bc.rs`, plus `un_math_sin` from `native/math.rs` and
  all supporting declarations (`fid_sin` plumbing across two emit passes).
- The `Float` header claim in the JIT docs; stale `floor(sqrt)` references
  in `AGENTS.md`, `docs/ARCHITECTURE.md`, `docs/README.md`, `README.md`,
  `scripts/bench.ps1` (report prose), and the `jit_cache::clear` doc comment
  (benchmark example kept, it was accurate).
- Nothing else was deleted; the "dead code" sweep found none beyond the
  above (ObjV2 prototype was already removed in an earlier pass).

## 5. Verification results

Build & unit gates (all after the final edit):

| Gate | Result |
|---|---|
| `cargo build --release` | 0 errors, 0 warnings |
| `cargo test --release` | **205 passed**, 0 failed |
| `scripts/test.ps1` | Positive 40 files, **1419 passed / 0 failed / 0 crashed**; Negative **41 matched / 0 mismatched / 0 divergences**; Known 0; exit 0 |
| `scripts/differential.ps1` | **81 files, 0 differences** |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | Positive 40/40 identical; Negative 34/34 identical (7 compile-rejected, as before) |
| `test/unit/native/re.un` | 26 passed (was 23; +3 `$0` regression tests) |

Benchmark suite (`unvm`+`unjit` lanes, `scripts/bench.ps1 -Group unvm,unjit -NoSave`,
three runs, AMD Ryzen 7 9800X3D; baseline = commit `2f67935` figures from the
same session):

| Bench | JIT before | JIT after | Notes |
|---|---|---|---|
| int_ops | 2.97 ms | 2.97 – 3.00 ms | unchanged |
| fib24 | 1.32 ms | 1.32 ms | unchanged |
| nested_sum_200 | 0.013 ms | 0.013 ms | unchanged |
| qsort500 | 0.85 ms | 0.86 ms | unchanged |
| mergesort1000 | 1.47 ms | 1.48 – 1.50 ms | unchanged |
| dict5000 | 4.05 ms | 3.88 – 3.90 ms | slightly faster (presence probe is noise-level) |
| iter10000 | 4.79 ms | 4.85 – 4.89 ms | ~ noise |
| class1000 | 0.71 ms | 0.72 – 0.72 ms | unchanged |
| sieve30000 | 4.91 ms | 4.92 – 4.95 ms | unchanged |
| matrix48 | 10.97 ms | 11.34 – 11.37 ms | ≈ noise; pays the new arg write-back (nesting) |
| call200000 | 0.689 ms | 0.687 – 0.694 ms | unchanged |
| regex6000 | 1.21 ms | 1.20 – 1.22 ms | unchanged |
| rle1994 | 6.83 ms | 7.46 – 7.65 ms | **8–12% slower** — restored twin-parity `for` loop (see §2 #4) |
| math10000 | **0.016 ms** | **≈21.97 ms** | **JIT no longer special-cases `math.floor(math.sqrt(i))`; falls back to the interpreter** (identical to the VM lane) — the intended honest cost of removing the benchmark kernel |
| sin20000 | **0.54 ms** | **≈25.9 ms** | **Same** — `math.sin` is now interpreted |

Also verified by hand (ad-hoc scripts, both lanes byte-identical):
mutating a list argument (`xs[i] = xs[i] * 2`), nested list mutation
(`xs[i][0] = 1`), `xs.add(...)`, `-> xs` identity (`out is xs`), building and
returning a fresh list, `len(xs)` with a rebound `len`, missing Int key in an
IntDict (error message now identical to the interpreter).

Stability: three consecutive unvm/unjit bench runs agree within run-to-run
noise except where the table says otherwise; no flaky tests observed; negative
manifest unchanged.

## 6. Not resolved / residual risks

- **`math10000` and `sin20000` lost their JIT speed.** This is the deliberate,
  honest consequence of removing the benchmark-shaped intrinsics. The right
  long-term fix is a *general* Float-math intrinsic set (`math.sqrt`/`floor`/
  `sin`/… for Float args, with proper error semantics), not a restored
  call-sequence window matcher. Not implemented here: it changes the Float
  value model's error path and is a design decision, not a cleanup.
- **Handle-namespace aliasing (remnants of item 10).** A *computed* Int
  ≥ 2^62 produced inside a JIT callee could, in principle, alias a live
  arena handle. Arguments are now rejected at the FFI; computed values are
  not rejected (would require reserving a tag bit or a JIT-side range
  check). Narrow, pre-existing, documented; fixing it properly means an
  in-band tag redesign (the ObjV2 experiments pointed the same way).
- **Arenas grow monotonically.** `INT_LIST_ARENA`/`INT_DICT_ARENA`/
  `INT_CLASSES` never reuse freed slots; only whole-process
  `jit_cache::clear()` releases them. Documented; not a correctness issue;
  `PLAN_CACHE` keeps `Rc<Program>`/`Rc<Plan>` alive forever (TD-03, by
  design, see `docs/TECHNICAL_DEBT.md`).
- **`class_layouts` keyed by class name, append-deduplicated, never
  reordered** (TD-51). A second, same-named class with different field order
  silently degrades that class to interpreter speed. Known, documented,
  low risk; the JIT-name-check intrinsic bails correctly.
- **`JIT_LAZY_ATTEMPTED` is one-shot per program.** A transient miscompile
  (or a module loaded before its classes are seeded) permanently disables
  the JIT for that program until process end (TD-51). Mitigated 2026-11-19:
  `UV_JIT=2` aborts the process with exit code 2 naming the failing protos,
  and CI runs the canary `test/jit/strict.un` under `UV_JIT=2` (see
  docs/TECHNICAL_DEBT.md TD-51). Remaining subclasses still print via
  `UV_JIT_DEBUG=1` only.
- **Pre-existing divergences noted, not fixed** (out of scope, recorded so
  they are not mistaken for work of this pass): `native/members.rs`
  `iter_sum` returns `none` for an empty iterator while
  `native/iter.rs` `sum_list`/`normalize_to_tuple` errors on an empty list;
  `sorted_impl` treats elements whose comparison raises as equal;
  `lcm` can wrap silently; `normalize_to_tuple` expects a tuple-wrapped list
  in the 1-element case; `int_dict_get` sentinel `-1` is by design;
  `enu_init` keeps the `i32` cast (.NET enum parity); `PLAN_CACHE` is
  unbounded (TD-03).
- **`BENCHMARK.md` was stale after the pass** (pre-pass numbers; local,
  deliberately not touched mid-work). It has since been regenerated from the
  committed tree (`un_commit 4088322`, 2026-11-18) with `scripts/bench.ps1
  -Group all -Report`; that file is now authoritative. §5 keeps the
  session-time before/after for the removed intrinsics. When regenerating,
  always measure a clean, committed tree and keep the reported `un_commit`
  hash — never hand-edit the tables.
- **`docs/BENCHMARK_GUIDE.md`** documents the benchmarks themselves; it is
  unaffected by the intrinsic removal (it describes what the benchmarks
  measure, not how they run).
- **WASM build breakage, found after the original audit.** The normalisation
  introduced `const PARAM_SENTINEL: usize = 1usize << 48` in
  `jit_bc.rs`'s `name_to_slot` wiring to tag parameter-source `reg_slot`
  entries; on wasm32-unknown-unknown (32-bit `usize`) the shift overflows
  and the target fails to compile. Fixed by replacing the sentinel with
  `usize::MAX - ix` (always valid as a "not a real slot" marker because
  every local slot index is `< proto.locals.len() ≪ usize::MAX`).
  Verification: `cargo build --release --target wasm32-unknown-unknown -p unvm-playground`
  succeeds, and all native gates unchanged.
- **`-> xs` parameter round-trip bug, found while auditing.** `analyze_one_proto`
  claimed `RET_INTLIST` only when the proto contained `Op::BuildList`. A
  pure `-> xs` function (no BuildList) therefore compiled to JIT passing a
  raw arena handle as Int through its return slot; the caller received an
  `Int(handle)` instead of a list (`len(ident(xs))` read 1 instead of 3).
  Fixed in `claims_intlist` (`jit_bc.rs`) to also claim list-capability for
  any proto with at least one parameter — statically undecidable whether a
  parameter is returned. The existing `fid_list_is_handle` probe in the
  `Op::Return` arm decides at runtime, so this is sound (Int returns take
  the Int path). Added a regression test in
  `test/unit/runtime/functions.un` (`ident_list`) covering the identity
  round-trip through both lanes; test count is now **1419 passed**.

## 7. Recommended next steps

1. ~~Regenerate `BENCHMARK.md` with `scripts/bench.ps1 -Group all -Report` and
   re-pin the documented numbers; hand-edit nothing.~~ **Done** (two passes,
   2026-11-18). The committed tree (`un_commit 4088322`) now measures
   `math10000` at ≈ 23.4 ms and `sin20000` at ≈ 31.5 ms — both at interpreter
   speed, matching the VM lane, as intended after the intrinsic removal — with
   an honest **3.336×** JIT/Python geomean (JIT/Node 0.063×). The first
   regenerated file had measured a dirty, pre-normalization tree (recorded
   `2f67935`) and showed all three languages regressing together on some
   benchmarks (fib24 +80%, qsort500 +1910%, mergesort1000 +923%) — a
   loaded-machine artifact, superseded by the clean re-measure. The clean
   run's advisory check flags `qsort500 unjit +12.9%` vs the previous saved
   run (same code, 0.83 → 0.94 ms) — noise-level; re-check if it persists.
2. Design a general Float-math intrinsic tier (sqrt/floor/sin/cos/… for
   Float args, with the interpreter's error behaviour preserved) to recover
   `math10000`/`sin20000`-class workloads without benchmark-specific
   matching. Verify with differential + roundtrip gates before merging.
3. Decide the handle-namespace question for JIT-produced large Ints
   (reserved tag bit vs. runtime range check at handle-probes).
4. ~~Implement the `UV_JIT=2` strict canary (TD-51) so a silently-dead JIT is
   CI-detectable.~~ **Done** (`b354f60`): any JIT candidate that fails to
   compile aborts the process with exit code 2 and a named proto list; the
   `JIT strict canary` CI step runs `test/jit/strict.un` under `UV_JIT=2`
   (see docs/TECHNICAL_DEBT.md TD-51).
5. Consider unifying the three near-identical `iter_values` implementations
   (`native/iter.rs`, `native/members.rs`) into one helper — behaviour
   differences are in error messages only; needs a design pass on the
   error contract first.
6. `sys.setrecursionlimit` / depth accounting and the arenas would benefit
   from a single documented "JIT resource lifecycle" note in
   `docs/ARCHITECTURE.md` (module unload/free ordering, `clear()` hazards).
7. Keep the git history aware of this pass's principle: **the JIT is a
   semantics-preserving accelerator; anything it cannot model must fall back
   to the interpreter, byte-identically.** Every change here moves toward
   that invariant; none of it is benchmark-shaped.
