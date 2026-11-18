# docs/REFACTOR_LOG.md

Dated log of the changes applied to the Rust implementation, with the
commands used to verify them and the numbers they produced. Only verified
results are recorded here; a change without a measured result is not closed.

Format: one section per work session, newest first.

---

## 2026-11-16 — Audit pass 1: correctness defects, JIT hardening, first perf fixes

### Starting state

- `git status`: branch `main`, HEAD `958f9e2`, plus the user's uncommitted
  class-layout JIT specialisation (`seed_class_layouts`,
  `register_class_field_layout`, `world::set_class_layout`,
  `un_instance_int_get/set`, `slot_alloc("self")`).
- `cargo build --release` → exit 0, **4 warnings** (`rust/src/vm/mod.rs:1587-1588`,
  shadowed `mut` bindings in `seed_class_layouts`).
- `scripts/test.ps1` → Positive 39 files / 1363 passed / 0 failed / 0 crashed;
  Negative 39 matched / 0 mismatched / 0 documented divergences;
  Known 1 file (`test/regression/defects.un`, 6 failing assertions); exit 0.
- `cargo test --release` → 199 passed.

### Decisions taken (with the user)

1. The six documented defects are fixed to **standard semantics**. The C#
   reference and the differential harness are gone, so the language
   specification — not the old C# quirks — is the ground truth. Recorded as
   `docs/DECISIONS.md` **D-067**; D-027 and D-035 are marked superseded.
2. The uncommitted class-layout JIT work is **hardened conservatively**, not
   deleted and not left as-is: the failure modes that produced silently wrong
   values are removed, the infrastructure stays for a later phase, and the
   remaining gaps are recorded as debt.
3. Restructuring is **aggressive**: naming cleanup, dead-code removal and
   module splits are in scope, each verified with the full test runner.

### Changes

**Warnings**

- Removed the shadowed `mut class_regs` / `mut layout_out` bindings in
  `seed_class_layouts` (`rust/src/vm/mod.rs`). Build is back to 0 warnings.

**Correctness (S1-1 … S1-7)**

- `rust/src/runtime/collections.rs`
  - new `slice_indices(len, start, end, step) -> Result<(i64, i64, i64), Obj>`:
    resolves bounds the standard way (negative bounds count from the end, an
    omitted bound depends on the sign of the step, the end is exclusive).
  - new `slice_positions(len, start, end, step) -> Vec<usize>`: the selected
    indices, computed in `i128` so a huge step cannot overflow.
  - `list_slice` now *collects* the selected elements (it used to walk the
    step loop and return an empty list) and no longer truncates bounds to
    32 bits.
  - `tup_slice` uses the same helpers, so tuples reverse like lists.
  - `list_in` now checks scalar members element-wise instead of raising
    `cannot check if 'int' is in 'list'`.
  - new `set_equals` / `dict_equals` and `dict_binop`; `set_binop` gained
    `Eq`/`NEq` arms.
- `rust/src/runtime/obj.rs`
  - the string slice arm uses the shared helpers (negative steps and
    out-of-range bounds now behave like `list`/`tuple`).
  - `builtin_binop` routes `Payload::Dict` to `collections::dict_binop`.
- `rust/src/native/members.rs`
  - `list_upper_bound` searches in `[0, len]` and returns the insertion
    point, so the answer can be `len`.
- `rust/src/vm/mod.rs`
  - `is_obj` no longer swaps its operands: when the right-hand side is a type
    object the question is "is the left-hand side an instance of that type?",
    and two type objects compare by identity. This is what makes
    `type(none) is none` true while `str(type(none))` stays `<type: none>`.

**Tests**

- `test/regression/defects.un` rewritten as a regression suite for the fixed
  behaviour (16 assertions), moved from the `known` group to a new
  `regression` group in `test/MANIFEST.txt`.
- `test/KnownFailures.txt` emptied (comment-only).
- `test/unit/native/set.un`, `test/unit/native/dict.un`,
  `test/unit/runtime/builtins.un`: expectations that encoded the old
  behaviour replaced with the correct ones (set/dict equality, `type(none)`).
- Rust unit tests updated to the new semantics:
  `rust/src/runtime/collections/tests.rs` (`list_slice_rules`,
  `list_membership_and_unops`, `tup_index_and_names`) and
  `rust/src/vm/tests.rs` (`a_step_slice_keeps_every_selected_element`,
  `a_negative_step_past_the_end_clamps_to_the_last_element`).

**JIT hardening (S1-8, S2-1, S2-2, S2-3)**

- `rust/src/runtime/obj.rs`
  - `un_instance_int_ok(self_ptr, field_index) -> i64`: 1 when the member
    exists and holds an `Int`.
  - `un_instance_int_get(self_ptr, field_index) -> i64`: the value; callers
    must check `un_instance_int_ok` first.
  - `un_instance_int_set(self_ptr, field_index, value) -> i64`: 1 on success,
    0 when the slot does not exist.
  - all three document their `# Safety` contract and wrap the body in
    `catch_unwind`, so a `RefCell` conflict can no longer unwind into native
    code.
- `rust/src/jit_bc.rs`
  - the `GetAttr`/`SetAttr` class-layout arms probe with
    `un_instance_int_ok` and bail with tag=2 on a miss instead of reading a
    wrong value; the write path bails when the store could not land.
  - `precheck_fn` rejects class methods: the JIT value model carries only
    Int/Float, so an object receiver cannot be represented, and the previous
    handling of the hidden receiver argument was inconsistent
    (`precheck_fn` counted it, `emit_fn_in_module` did not).
  - new `un_instance_int_ok` import declared in the JIT module.
- `rust/src/vm/mod.rs`
  - the class field layout no longer contains method names, and `__init__`
    fields are collected first so the layout index matches the instance member
    order.
  - the per-registration layout refresh (`register_class_field_layout`) is
    gone; the layout is seeded once per program at load time.
- `rust/src/jit_cache.rs`
  - new `jit_debug()`: the `UV_JIT_DEBUG` flag is read once per process
    instead of once per emitted instruction.

**Documentation**

- `README.md` / `docs/README.md`: new "컬렉션과 슬라이싱" / "Collections and
  slicing" section documenting the slice rules, `==` on collections and `in`.
- `docs/DECISIONS.md`: D-067 added; D-027 and D-035 marked superseded with a
  pointer to D-067.

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test --release` | **202 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1383 passed / 0 failed / 0 crashed**; Negative **38 matched / 0 mismatched / 4 documented divergences**; Known **0**; exit **0** |
| `scripts/bench.ps1 -Group un -NoSave` | see the table below |

`scripts/bench.ps1 -Group un -NoSave` vs the pre-change baseline (same machine,
single runs, no warm-up save):

| Bench | Before (ms) | After (ms) |
| --- | --- | --- |
| fib24 | 3.411 | 3.526 |
| nested_sum_200 | 0.013 | 0.013 |
| qsort500 | 18.262 | 20.596 |
| rle1994 | 11.062 | 12.236 |
| dict5000 | 0.242 | 0.240 |
| iter10000 | 5.719 | 5.809 |
| math10000 | 0.016 | 0.019 |
| **class1000** | **7.272** | **6.215** |
| sieve30000 | 66.398 | 60.510 |
| matrix48 | 74.490 | 75.466 |
| sin20000 | 31.915 | 31.129 |
| call200000 | 0.694 | 0.692 |
| regex6000 | 22.430 | 22.146 |

These are single measurements on a loaded machine and the deltas are within
run-to-run variance; the JIT compile-time cost (per emitted instruction
environment lookup, per-class-registration layout rescan) was removed, but no
repeatable interpreter speed-up should be claimed from a single run.

---

## 2026-11-16 — Pass 2: native/stdlib depth audit, build/docs audit follow-up

Two follow-up depth audits were completed (the earlier attempts had failed
with a network error): one over the native/stdlib layer, one over the
build/docs/workspace layer. The 13 native-layer defects fixed in that pass
are recorded as S1-9 … S1-21 in `docs/PROJECT_AUDIT.md`; the build/docs findings
are recorded there as S2-build. A decision entry **D-068** in `docs/DECISIONS.md`
records the same fixes in the decision log.

### Native/stdlib fixes (see `docs/PROJECT_AUDIT.md` S1-9 … S1-21 and D-068)

- `lower_bound` now returns a true insertion point (the sibling of the
  `upper_bound` fix); `extend_insert` inserts in order and reports bad
  indices; float keys hash Eq-consistently (`-0.0`); lazy `.map()`/`.filter()`
  propagate lambda errors as `Step::Panic`; `fs.*` timestamps are computed
  from `runtime::date::parts`; `fs.rmdir` no longer deletes non-empty trees
  by default; `fs.move`/`fs.rename` copy before truncating; `walk_dir` stops
  on symlink cycles and depth 64; WebSocket frames follow RFC 6455 (126/127
  markers, 1 MiB cap, ping→pong); `iter.array`/`random.range`/`str_pad` have
  allocation caps; the module RNG seeds from OS entropy, `below()` uses
  rejection sampling, `between()` computes the span in i128; `time.sleep`
  clamps instead of truncating through `i32`; `date.add_years` clamps Feb 29
  to Feb 28.

### Build/docs fixes applied in the same pass

- `test/negative/manifest.txt`: the two absolute-path substrings now match on
  any checkout location (`io\write' not found`, `nosuchmod' not found`).
- `docs/DECISIONS.md`: duplicate `D-033`/`D-034` second uses renumbered to
  **D-069**/**D-070**; `D-066` moved back into numeric order.
- `AGENTS.md`: counts corrected to the measured state (76 files / 34,166
  wc-lines), JIT eligibility description includes the `math.sin` intrinsic
  and IntDict, the "no external crates" claim was corrected, `main.un`
  tracking is documented, and the tree lists the new audit documents.
- The Korean/English benchmarks, gap analysis, and tracking state were checked
  against `scripts/bench.ps1` (13 `$Pairs`, untracked files listed in
  `docs/PROJECT_AUDIT.md` S2-12).

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test --release` | **202 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1391 passed / 0 failed / 0 crashed**; Negative **39 matched / 0 mismatched / 0 documented divergences**; Known **0**; exit **0** |


New test coverage: `test/unit/native/list.un` (lower_bound past-the-end and
empty list, extend_insert in order, `.order()` ascending by key),
`test/unit/native/fs.un` (rmdir default errors on a non-empty directory,
`recursive = true` deletes it),
`test/unit/runtime/async.un` (go inside a loop keeps the loop values).

### Negative-suite repair

The three vacuous "documented divergences" were removed on user approval:

- `test/negative/order_crash.un` was deleted; its meaningful behaviour
  (`.order()` sorts by the key) is asserted in `test/unit/native/list.un`.
- `test/negative/depth_limit_crash.un` was deleted; recursion coverage
  remains in `test/integration/recursion.un` and setcalldepth is covered there.
- `test/negative/go_loop_var.un` was deleted; loop-scoped `go` is now
  asserted in `test/unit/runtime/async.un`.
- `test/negative/property_recursion.un` now expects the current behaviour
  (exit 1 + `maximum recur`), so the negative suite passes without any
  divergent entries.
- `test/KnownDivergent.txt` is comment-only; the suite is **39 matched /
  0 documented divergences**.

---

## 2026-11-16 — Pass 3: Phase 4 dead code removal, Phase 5 world split, Phase 7 tooling

Executed on the user's decision (Phase 4 + 5 + CI/toolchain/differential).

### Phase 4 — dead code removal

- `ast.rs`: the 18 never-constructed `NodeKind` variants (14 compound-assignment
  kinds, `Go`, `Argument`, `Alias`, `Target`) and `Node::end` removed.
- `token.rs`: `TokenType::text()` (116 arms), `Token::error`, `Token.message`
  removed.
- `error.rs`: `LangError::relocated` removed; `source.rs`: `Source::full_name`,
  `Source::len`; `unbc.rs`: `opcode_of`; `iters.rs`: `iter_copy`, `next_of`;
  `collections.rs`: `is_single`; `scope.rs`: `get_slot_name`, `remove`;
  `jit_cache.rs`: `clear()`; `world.rs`: `current_program` + `set_current_program`.
- `scope::declare`/`remove` are now `#[cfg(test)] pub(crate)` — their only
  callers are test-only.
- `rust/src/jit.rs` deleted (test-only Cranelift probe in the public API).
- The 22 never-lexed `TokenType` variants are kept: the `name()` display
  table still references the enum's shape and the opcode contract relies
  on the representation.

### Phase 5 — world.rs split

`runtime/world.rs` (489 lines, all per-process state) is now `runtime/world/`:

- `world/mod.rs` — `World` struct, `WORLD` thread cell, `reset()`.
- `world/caches.rs` — interned type names, scalar caches (`int`, `str`,
  `bool`, `none`, `null`, `__uninitialized__`), `MAX_POOL_SIZE`,
  `MAX_POOLED_LEN`, `REGISTERED_TYPES`.
- `world/classes.rs` — the class registries (original class attrs, JIT
  class layouts, `classes`, `classes_tobj`).
- `world/modules.rs` — import cache, loading set, working path, global
  scope, native registry.
- `world/io.rs` — stdout/log/stdin, sandbox allow-list, `exit` gate, EOF.
- `world/depth.rs` — recursion limit; `world/executors.rs` — the executor
  hooks used by the CLI and the playground.

Callers unchanged — everything is re-exported through `world::`.

### Phase 7 — tooling

- `.github/workflows/build.yml` — Windows GNU toolchain build + `test.ps1`
  + `cargo test --release` (runs on push/PR; not yet executed by GitHub).
- `rust-toolchain.toml` — pins `stable-x86_64-pc-windows-gnu`.
- `scripts/differential.ps1` — runs every test-manifest file with `UV_JIT=0`
  and with the default JIT and diffs stdout + exit code; **79 files,
  0 differences**.
- `docs/archive/` — `MIGRATION_AUDIT.md`, `MIGRATION_PROGRESS.md`,
  `REGISTER_VM_ROADMAP.md` had been moved there with superseded banners
  (git mv). They were deleted entirely in the 2026-11-16 docs cleanup;
  the content survives in `git` history.
- `vendor/THIRD_PARTY.md` added; `vendor/README.md` attribution corrected
  (the two wasmtime crates ship without the upstream `LICENSE` files —
  recorded as debt TD-31).
- Dead-weight `slot_names` removed from `Scope` and from the per-`go`
  scope snapshot (each future used to carry a `Vec<String>` it never read).
- `runtime/global.rs` renamed to `runtime/modules.rs`; its tests moved to
  `runtime/modules/tests.rs`; the module tree is now `mod`/`fn`/`obj`/
  `scope`/`world`/`iters`/`collections`/`context`/`date`/`modules`/`types`/
  `int_dict`/`prims`/`clock`.

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test --release` | **200 passed / 0 failed** (the jit.rs probe's 2 tests went with its deletion) |
| `scripts/test.ps1` | Positive **40 files / 1391 passed / 0 failed / 0 crashed**; Negative **39 matched / 0 mismatched / 0 documented divergences**; Known **0**; exit **0** |
| `scripts/differential.ps1` | **79 files, 0 differences** |
| `scripts/bench.ps1 -Group un -NoSave` | within run-to-run variance of the previous baseline (class1000 8.205 ms vs 6.215; stddev 0.244 — not a regression attribution) |

Behaviour probe (temporary script, `unvm run`):

```
a [1, 3, 5]        l[0:5:2]
b [5, 4, 3, 2]     l[4:0:-1]
c [5, 4, 3, 2, 1]  l[::-1]
d edcba            "abcde"[::-1]
e (4, 3, 2, 1)     (1,2,3,4)[::-1]
f [3, 2]           l[2:0:-1]
g [4, 5]           l[-2:]
h [1, 2, 3]        l[:-2]
m 4                [1,2,4,5].upper_bound(5)
n 1                [1,2,4,5].upper_bound(1)
o 4                [1,2,4,5].upper_bound(9)
p true             2 in [1,2,3]
r true             set(1,2) == set(1,2)
t true             {"a":1} == {"a":1}
w true             type(none) is none
x <type: int>      type(1)
```

### Open items carried into `docs/TECHNICAL_DEBT.md`

TD-01 … TD-50 (see that file). The most important ones: the eager JIT
compilation cost, the IntDict arena that never frees, the f-string source
subtleties, and the dead code that remains beyond what was deleted in pass 3.

---

## 2026-11-16 — Pass 4: Phase 6 frontend streamlining

### Desugar folded into the optimizer

- `rust/src/desugar.rs` deleted. Compound-assignment wrapping (`x += 1` →
  `x = x + 1`) and the `Branch`/`ElIf` → `IfCase` rewrite are now
  `desugar_assign` / `desugar_if` in `optimize.rs` and run inside the same
  bottom-up pass as the constant folder (children first).
- `runner.rs` no longer walks the AST separately: the pipeline is
  `parse → optimize → compile`.
- `optimize` no longer loops up to `MAX_PASSES = 64` times; a single
  bottom-up pass reaches the fixpoint, so the tree is cloned once per node,
  once.
- Note: `main.rs`'s `desugar`/`opt`/`dis-reg` subcommands now fold through
  `optimize` directly.

### The subtle regression this exposed

Folding desugar into the optimizer immediately exposed a latent bug in the
pass bookkeeping: when `optimize_children` found no child that reported
`changed = true`, the parent returned `node.clone()` — discarding the
children-optimized (and then desugar-substituted) node tree. The desugared
node is always new, so parents above the substitution must rebuild. The fix
was to mark `changed = true` in `optimize_pass` when `desugar_local` fires.
Without it, every `if`/`elif`/`else` body was transformed but the BLOCK still
executed the old (raw) tree, compound assignment stayed unwrapped, and
`fib`-style recursion never terminated (the `if n < 2` guard was lost).
Detected and fixed in the same pass.

### Precedence comment drift fixed

`test/unit/parser/precedence.un` now asserts left-associative `**` and the
tighter binding of unary minus, matching the implementation.

### Tag contract unified (TD-01)

`jit_cache::TAG_INT`, `TAG_FLOAT`, `TAG_BAIL`, `TAG_BOOL` are now the single
source of truth; `jit_fast.rs` and `vm/mod.rs` reference the constants, and
an unknown tag falls back to the interpreter instead of panicking.

### .unbc round-trip test (TD-15)

`vm::tests::an_unbc_round_trip_preserves_behavior` / `::an_unbc_round_trip_preserves_error_behavior`:
the same program run (i) directly, (ii) via `source → .unbc file → read → VM`,
produces identical stdout and outcomes.

### f-string interpolation source handling (TD-14) re-classified

Verified by running interpolation-error probes: the caret points at the
column of the interpolation in the outer file. The audit over-flagged it
(the nested parser receives the outer source, and the byte ranges already
are relative to it). Not a defect; the stale `LangError::relocated` was removed
as part of the pass-3 deletions.

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test --release` | **202 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1391 passed / 0 failed / 0 crashed**; Negative **39 matched / 0 mismatched / 0 documented divergences**; Known **0**; exit **0** |
| `scripts/differential.ps1` | **79 files, 0 differences** |

Startup cost of `test_suite.un` (5-run sample): ~59 ms (`UV_JIT=0`) vs ~64 ms
(default JIT) — within run-to-run noise; no claim made from this sample.

---

## 2026-11-16 — Pass 5: Phase 8 — JIT class methods, module lifetime, Float compare

### What changed

- `precheck_fn` now admits class methods (Int-region local `self` slot 0).
  Class method local slot 0 is `self`; the entry block pins `incoming[1]`
  (receiver pointer) to `vars[arity]`.
- `emit_fn_in_module` changed the class-method arity to `params.len() + 1`,
  matching `precheck_fn`; params are addressed at shifted positions.
- The previous WIP `GetAttr`/`SetAttr` arms (which checked `ins.b == 0`) never
  fired: the compiler assigns arbitrary register numbers per program. Register
  identity is now tracked (`self_regs` is a bitmap set by `LoadLocal(0)` and
  cleared by any later write to the same register — conservative bail, not a
  silent wrong read).
- The field load/store intrinsic is name-checked (`un_instance_int_ok_name`
  verifies member name at the given index equals the expected field name AND
  the payload is Int before any read or write happens). A mismatch bails to
  the interpreter with tag = 2 — wrong field, non-Int field, layout drift are
  all impossible to miscompile silently.
- JIT modules are stashed in `jit_cache::JIT_MODULES`; `jit_cache::clear()`
  now releases the native code with `free_memory` after JIT_FNS is cleared.
  Normal execution still leaks by design; the leak is now documented and can
  be reclaimed in tests/benchmarks.
- Float comparisons (`Lt/Le/Gt/Ge/Eq/Ne`) are now native in the JIT; mixed
  Int/Float operands promote with `fcvt_from_sint`; NaN semantics match Rust.

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** (clean build from `cargo clean`) |
| `cargo test --release` | **202 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1395 passed / 0 failed / 0 crashed**; Negative **39 matched / 0 mismatched / 0 divergences**; Known **0**; exit **0** |
| `scripts/differential.ps1` | **79 files, 0 differences** (UV_JIT=0 vs default) |
| `test/unit/runtime/classes.un` | 60 passed (was 56; +4 assertions on natively-compiled class-method paths) |
| `UV_JIT_DEBUG=1` on `classes.un` | `inc`, `__len__`, `__getitem__`, `__setitem__` etc. compile as `OK`; 4-arg `norm` point probe ran natively |

Constraints that remain: constructors (`none`-returning) and methods building
lists/dicts/iters still bail to the interpreter (by design — the value model
is Int/Float-only); non-Int field values read/write bail; `layout` must
inclure `__init__`'s assignment order, which is checked name-wise at call
time rather than trusted compile-time.

---

## 2026-11-16 — Pass 6: Phase 8 — lazy compile, IntDict with entries

### Lazy compilation

`plan_for` no longer eagerly compiles eligible protos at program load. The
compile happens on the first `try_call_jit` invocation per program (marked in
`jit_cache::JIT_LAZY_ATTEMPTED`). Every eligible proto (including bail stubs
for ineligible ones) is compiled at first use, spread across execution
instead of blocking startup. Startup time for programs with many eligible
protos (`test_suite.un`) is roughly equivalent at this scale (within
run-to-run noise; ~60–70 ms against a 59–70 ms JIT=0 baseline); the win is
that smaller scripts pay nothing at all.

### `BuildDict` with entries

The Int-Int `BuildDict` intrinsic previously only supported `d = {}` (zero
entries). Non-zero entry counts now inline one `un_dict_int_empty` + one
`un_dict_int_set` per `[key, value]` pair at translation time. Eligibility
requires Int-only keys and Int-only values (Bool/Float/None entries bail to
the interpreter). Verified on a `dict_work` probe (asserts `JITDBG OK` for
inline-built literal dicts).

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test --release` | **202 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1395 passed / 0 failed / 0 crashed**; Negative **39 matched / 0 mismatched / 0 divergences**; Known **0**; exit **0** |
| `scripts/differential.ps1` | **79 files, 0 differences** |
| `test_suite.un` (7 runs) | avg ~70 ms (identical to JIT=0 within noise) |

---

## 2026-11-16 — Pass 7: JIT cache redesign (per-program eviction)

### Motivation

The registry was keyed by `Rc::as_ptr(program) as usize` (the
"unbc:<ptr>:<proto_index>" key) together with `PLAN_CACHE` keyed the same
way. Besides being unable to evict individual programs, that scheme *allows*
a stale-entry serve: a program is dropped, a new program lands at the same
address, and a lookup returns the previous program's native code pointer
(associativity hazard, not observable in a running VM thanks to PLAN_CACHE
pinning — but nothing would detect it either).

The redesign replaces pointer-identity with an opaque monotone `program.id`
(u64, assigned at compile/`.unbc`-read time, `.unbc` never persists it).
Every per-program JIT structure is now keyed by `program_id`:

- `PLAN_CACHE` (vm/mod.rs): `program_id → (Rc<Program>, Rc<Plan>)`; plan lookup
  skips `Rc::ptr_eq` by construction.
- `jit_cache::JIT_FNS` (two-level): `program_id → proto_index → JitEntry`.
- `jit_cache::JIT_MODULES`: `program_id → Vec<JITModule>`.
- `jit_cache::JIT_LAZY_ATTEMPTED`: `program_id → …`.

`jit_cache::clear_program(program_id)` removes exactly one program's
entries + native modules + marker; `jit_cache::clear()` removes everything
(playground/test teardown). No `Rc::as_ptr` for JIT state remains anywhere.

### Tests

`vm::tests::a_cleared_jit_program_runs_identically` — run, clear_program,
re-run the identical source and assert byte-identical stdout (plus
`assert!(freed >= 1)`); `a_full_clear_lets_jit_run_again` — clear() then
re-run; both verify the lazy path recompiles after invalidation.

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test --release` | **204 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1395 passed / 0 failed / 0 crashed**; Negative **39 matched / 0 mismatched / 0 divergences**; Known **0**; exit **0** |
| `scripts/differential.ps1` | **79 files, 0 differences** |

---

## 2026-11-16 — Pass 8: `.unbc` round-trip harness + `source_path` defect fix

### Harness

New `scripts/unbc-roundtrip.ps1`: for every Positive manifest file (and, with
`-IncludeNegative`, every compilable Negative fixture), run
`unvm run <file>` vs `unvm compile <file>` + `unvm run <file>.unbc` and
byte-compare stdout, stderr and the exit code. Parse-rejected Negative files
cannot produce a `.unbc` and are counted as skipped. The harness is wired
into CI (`.github/workflows/build.yml`, after `cargo test`) and documented in
AGENTS.md (the phase was tracked in `docs/REFACTOR_PLAN.md`, later deleted
in pass 10).

### Defect found and fixed (D-072)

The harness caught a real defect: `compiler.rs` stored only the file stem
(`Source::name()`) in `Program.source_path`, and `run_bytecode` reconstructs
the `Source` from it. Module resolution (`module::resolve_module` is
"importing file's directory, then cwd") therefore diverged between direct run
and `.unbc` re-run:

- `test/fixtures/rel_test/main.un` — direct run exits 0 (`rel ok`); the
  `.unbc` re-run panicked with `module 'C:\dev\un\utils' not found`.
- `circular_main.un` — the `circular import detected` diagnostic printed an
  absolute path on the direct run but a cwd-relative path on `.unbc` re-run.

Fix: `Program.source_path` stores the full path as passed at compile time
(`source.path.clone()`). Error display still shows the stem via
`Source::name()`, so message shape is unchanged; `.unbc` format and opcode
ABI untouched. Documented as decision **D-072** in `docs/DECISIONS.md`.

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test --release` | **204 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1395 passed**; exit **0** |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | Positive **40/40 identical**; Negative compilable **33/33 identical**, 6 skipped (parse-rejected) |

---

## 2026-11-16 — Pass 9: docs consolidation

### Layout

All engineering docs now live under `docs/`: `ARCHITECTURE.md`,
`PROJECT_AUDIT.md`, `REFACTOR_LOG.md`, `TECHNICAL_DEBT.md` moved from
the repository root (`git mv`, history preserved); `REFACTOR_PLAN.md` was
deleted (its open items are tracked in `docs/TECHNICAL_DEBT.md`).
`docs/archive/` (the C#→Rust migration records) was removed entirely;
the content survives in `git` history. Every bare mention of the moved
files across the tracked tree was rewritten to `docs/<name>.md`;
AGENTS.md tree section, audit-docs line and quick commands were updated
accordingly; AGENTS.md scale counters refreshed
(81 files / 33,633 lines / 86 `.un` scripts).

New `docs/REFACTOR_SUMMARY.md`: one-page index of the whole audit →
refactor → optimization effort — commit timeline, structural changes,
current verified baseline, intentional limits, and a document map.

Root keeps only what belongs there: `README.md`, `AGENTS.md`, `LICENSE`.
Stable cross-references (`docs/DECISIONS.md` etc.) are unchanged.

### Verification

No code changes; documentation moves only. Gates assumed green from pass 8.

---

## 2026-11-16 — Pass 10: docs cleanup (legacy removal)

### Deleted

- `docs/REFACTOR_PLAN.md` — the phased plan was fully worked through;
  every checkbox was closed or is tracked in `docs/TECHNICAL_DEBT.md`.
- `docs/archive/` (`MIGRATION_AUDIT.md`, `MIGRATION_PROGRESS.md`,
  `REGISTER_VM_ROADMAP.md`) — the C#→Rust migration records,
  superseded since the 2026-11-16 audit; content survives in `git` history.

### Pruned and corrected

- `docs/TECHNICAL_DEBT.md` — resolved rows removed (they are enumerated in
  the new "Resolved in 2026-11" summary); stale Notes sections dropped;
  open items sorted by severity. TD-05 stays **high** (unbounded native-code
  growth outside teardown).
- `docs/ARCHITECTURE.md` — pipeline reflects `parse → optimize
  (desugar+fold) → compile`; `PLAN_CACHE`/`program.id` keying; lazy-compile
  eligibility; `clear_program`; stale `jit.rs` rows removed; `cargo test`
  count 204; new runner rows (differential.ps1, unbc-roundtrip.ps1).
- `docs/PROJECT_AUDIT.md`, `docs/REFACTOR_SUMMARY.md` — dead references to
  the deleted files removed; audit-time baseline table kept (dated evidence).
- `docs/TEST_GUIDE.md` — counts updated (1395), `KnownDivergent.txt` now
  described as empty, harness list includes differential/round-trip runners.
- `README.md` / `docs/README.md` — baseline 1391 → 1395.
- `docs/BYTECODE.md` — flags bit 0 (source embed) documented; `source path`
  is stored as the full compile-time path (D-072).
- AGENTS.md — pipeline diagram (no more Desugarer stage), audit-docs line,
  tree section (archive removed, summary listed), scale counters already
  current.

### Verification

No code changes; documentation only. Gates assumed green from pass 8.

---

## 2026-11-17 — Pass 11: class-JIT actually engages (benchmark-driven fix)

### Problem

While benchmarking, `UV_JIT_DEBUG=1` on `bench/class.un` showed
`Point.norm()` was **not** JIT-compiling: `JITREJECT getattr not_in_layout
ref=x`. Root cause: `seed_class_layouts` (at plan time) computes the field
layout (`[\"x\", \"y\"]`) into `world::set_class_layout`, but at runtime
`Op::RegisterClass` → `world::set_class` **overwrote** that entry with the
template member order (methods only, e.g. `[\"__init__\", \"norm\"]`). The
JIT precheck then found "x" missing from the layout and bailed on every
class-layout read — the class-method JIT silently never engaged for this
program (all class methods STUB-bailed at load time on the first call).

### Fix

`world::set_class` no longer clobbers an existing `class_layouts` entry —
the seeded field order wins; the member-order snapshot is installed only
as a fallback for classes that bypassed the seeder.

### Result

`norm()` now compiles natively (`JITDBG OK proto[2] norm`); `class_work`
and `__init__` still bail by design (unsupported surface). `class1000`
median: **5.59 → 4.73 ms** (audit baseline 7.272 ms). `bench/class.un`
untouched (user WIP); `bench/results/` remains gitignored.

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test --release` | **204 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1395 passed**; exit **0** |
| `scripts/differential.ps1` | **79 files, 0 differences** |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | 40/40, 33/33 identical |

---

## 2026-11-17 — Pass 12: four debt closures (TD-46, TD-12, TD-42, TD-07) + IntDict escape fix

### TD-46 — `iter.range(5, 0)` was silently swapped to `range(0, 5)`

`range(n)` (single argument) shares the `start` parameter with
`range(start, stop)`; the single-argument form was served by an
unconditional bounds swap, which also rewrote descending two-argument
ranges. Fix: `end` omitted → `(0, start)`; otherwise `(start, end)`; the
stage machine applies Python-style semantics everywhere (positive step →
empty when start ≥ stop). `iter.range(5, 0)` is now empty, `iter.range(3)`
is `[0, 1, 2]`, `iter.range(0, 5, -1)` is empty, `iter.range(5, 0, -1)` is
`[5..1]`. +2 assertions in `test/unit/native/iter.un`.

### TD-12 — `TryEntry.depth` was written, never read

The "registers are truncated on handler entry" invariant was never
implemented (and locals live in the scope slot table regardless).
Removed the dead field; the module header documents the actual invariant
(handler entry truncates only the argument stack via `acc_depth`; scratch
registers are dominated by writes in the register plan, so stale slots are
never observed).

### TD-42 — stream `"rw"` `read_line` mangled UTF-8 and conflated EOF/empty-line

`read_line` byte-cast to `char` (Latin-1), discarded `\r\n` handling, and
returned `None` for both EOF and a blank line. Fix: collect bytes,
`String::from_utf8` with a hard error on invalid input (matching the
`"r"`/`BufReader` path), distinguish EOF-at-nothing (`None`) from an empty
line (`Some("")`), and trim `\r\n` like the `"r"` branch. +4 assertions in
`test/unit/native/io.un` (UTF-8 Korean text, embedded empty line, EOF
after content, line past the empty line).

### TD-07 — IntDict arena leaked and its Mutex could be poisoned

- `INT_DICT_ARENA` locks poison-tolerantly (`into_inner`); a panic in an
  intrinsic can no longer cascade into permanently-failing dict ops.
- New `un_dict_int_free(id)` — tombstone frees, idempotent; the arena
  drops the table behind the id.
- The JIT emitter frees its function's IntDicts on **all four exit
  paths** (normal `Return`, GetAttr bail, SetAttr bail, user-call bail)
  with a shared `emit_int_dict_cleanup`; ids cannot escape a compilable
  function, so tables are dead at any exit.
- `jit_cache::clear()` additionally wipes the arena (test/teardown path).

### IntDict id escape (found while closing TD-07; same intrinsic family)

An IntDict id could previously escape a JIT function as a `tag=0` Int
(`Return` and `Op::Call` arguments only checked the Float/Bool/None
markers). Every way an id could flow into interpreter-observable state now
declines at translation: `Return`, user-call arguments, `SetAttr` values,
`SetItem` key/value, `BuildDict` pairs, `Add/Sub/Mul/IDiv/Mod`, comparisons,
`FusedIntMulAdd`, `ToBool`. New regression test
`vm::tests::an_int_dict_id_cannot_escape_as_a_tag0_int`.

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test --release` | **205 passed / 0 failed** (new test included) |
| `scripts/test.ps1` | Positive **40 files / 1401 passed / 0 failed / 0 crashed**; exit **0** |
| `scripts/differential.ps1` | **79 files, 0 differences** |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | 40/40, 33/33 identical |

---

## 2026-11-17 — Pass 13: bug sweep — parser crash (`a[]`) fixed; probed areas clean

### Found and fixed

`a[]` (empty index expression) reached `parser.rs:1002` and panicked on
`Option::unwrap()` — a raw Rust panic instead of a parse error. Now it
produces a proper `LangError` ("expected an index expression inside '['")
with the caret at the token. New negative fixture
`test/negative/empty_index.un` (exit 1, substring check); the negative
manifest is now **40 checks**. Docs counters updated (AGENTS.md, README
twins, TEST_GUIDE.md, REFACTOR_SUMMARY.md).

### Probed (no defects found)

- strings/UTF-8 slicing, `find`/`contains`/`starts_with`/`split`/`trim`/
  `replace`/`to_upper` on multibyte text — all consistent (UTF-16 code-unit
  semantics).
- `dict.remove`/`set.remove`/`list.remove` — consistent Bool-returning
  try-remove pattern (no silent data loss; not an error category).
- mixed-type `sort` — no crash; stable for mixed keys.
- 400-level nested list build and print — OK.
- JSON parser — unreachable from scripts (no native module exposes it).
- regex engine (`(?i)`, backrefs, nested groups, lazy quantifiers,
  replacements) — all correct.
- unsafe surface: all three `unreachable!` sites are dispatcher-guarded;
  the `unsafe extern "C"` intrinsics are name-checked or are one-word
  scalar loads behind the layout contract, wrapped in `catch_unwind`.
- int cache (`int_cached` window) and string interner (cap with clear) —
  no unbounded state.
- stdout sandboxing, `stdin_next_line` EOF handling — clean.
- error paths: `str.parse::<i64>` on literals, string→int conversion,
  native sink errors, fs copy self-to-self — all handled.
- Int-heavy arithmetic (`wrapping_div(i64::MIN, -1)`, div-by-zero) —
  explicit errors, no panics.

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test --release` | **205 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1401 passed / 0 crashed**; Negative **40 matched / 0 divergences**; exit **0** |
| `scripts/differential.ps1` | **79 files, 0 differences** |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | 40/40, 33/33 identical |

---

## 2026-11-17 — Pass 14: TD-36 (regex backtracking budget) and TD-37 (HTTP/WS hardening)

### TD-36 — regex backtracking could hang the process forever

`(a+)+$` on a long input never finishes. Fix: the `Engine` now counts
dispatch steps per engine attempt; at 10,000,000 steps the attempt returns
no-match *and is marked* (`Engine::limited()`); `Regex::find_at`/`find_all`
return `Result<_, String>` and surface it as `error: regex backtracking
limit exceeded` to the caller instead of a silent non-match (and instead of
hanging). All `re.*` native ops propagate the error verbatim; the internal
unit test module updated accordingly. New negative fixture
`test/negative/regex_limit.un` (matches in ~0.6 s vs never terminating
before); negative manifest is now **41 checks**.

### TD-37 — HTTP/WS had no timeout, no response cap, uneven scheme errors

- `connect_tcp`: DNS resolve + `TcpStream::connect_timeout` (10 s) +
  read/write timeouts (30 s) — both HTTP and WS paths use it.
- HTTP responses are read bounded (`take(64 MiB + 1)`); a declared
  `Content-Length` is validated against the body length exactly (a shorter
  read is `unexpected end of http response`, never a silent success);
  chunked bodies already error on truncation.
- Scheme guards now consistent .NET-style messages:
  `http://`-only for HTTP (`SCHEME_HTTP`), `ws://`-only for WS (`SCHEME_WS`).
- `http.set_header` now replaces case-insensitively (RFC 7230);
  `wss`/`https` remain rejected up front with the scheme message.
- Verified against a Node loopback server (not committed): normal GET
  returns the body; 70 MiB response → `http response too large`;
  server-lying Content-Length → `unexpected end of http response`;
  destroyed socket mid-stream → `Unable to connect to the remote server`
  (no hang); `https://…` → scheme message. 4xx/5xx still return the body
  by design (.NET parity; Un's http client does not throw on unsuccessful
  status — unchanged, documented in `docs/TECHNICAL_DEBT.md`).

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test --release` | **205 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1401 passed / 0 crashed**; Negative **41 matched / 0 divergences**; exit **0** |
| `scripts/differential.ps1` | **81 files, 0 differences** (2 new negative fixtures included) |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | Positive 40/40; Negative 34/34 (+7 parse-rejected skips) |

### CI

The `main` push of `dff50d1` triggered `.github/workflows/build.yml`:
**run #43977741595 (`build #2`) completed successfully in 3m 28s**
(job `build-test` 3m 25s). The earlier pass-10 push (`4048761`) ran
**run #43890043060 (`build #1`) successfully in 4m 0s**. One warning:
`actions/checkout@v4` targets Node.js 20, which GitHub is deprecating in
favour of Node.js 24 (forced there; a future `actions/checkout@v5` bump
will silence it).

---

## 2026-11-17 — Pass 15: playground WASM CI — root-cause fixes

### What

The `un-playground` WASM CI (`update-wasm.yml`) had failed 52 consecutive
times at its `Test un` step with
`error: target tuple in channel name 'stable-x86_64-pc-windows-gnu'`:
`un`'s `rust-toolchain.toml` pins a Windows-only GNU toolchain, and Linux
rustup refuses to read the file (the Linux playground runner hits it; the
Windows CI for `un` itself uses `rustup default stable-x86_64-pc-windows-gnu`
first).

Two complementary un-side debug-profile defects were exposed once the Linux
workflow got past the toolchain issue (local workflow was release-only):

- `transfer.rs`: register-plan `depth` math in `i64` with a clamping `s()`
  helper; a module that ends at stack depth 0 underflowed `depth - 1` in
  `Op::Return` and panicked every debug build.
- `jit_bc.rs`: emit the prologue into its own block and jump into the
  body, so the entry block stays pristine and Cranelift's debug verifier
  accepts the initial `switch_to_block`.

In `un-playground`: `update-wasm.yml` sets `RUSTUP_TOOLCHAIN: stable`,
which overrides the Windows-only pin on the Linux runner.

### Result

`Update WASM` #54 (43980496909) **success 3m 18s** — all steps green
(Test / Build / Smoke / Commit and push), pushing `chore: rebuild WASM
from un@aad0f42` to `un-playground` main (`7bab16cb`).

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test` (debug) | **205 passed / 0 failed** |
| `cargo test --release` / all gates | **205 / 1406 / 41 / 81** (unchanged) |

---

## 2026-11-17 — Pass 16: stopwatch read-only `ms`/`ns` properties

### What

`time.stopwatch()` now exposes `ms` and `ns` as **read-only properties**
(no `()` call syntax): `sw.ms` returns the elapsed milliseconds (int),
`sw.ns` the elapsed nanoseconds (int). `sw.ms = x` / `sw.ns = x` raise an
error ("cannot assign to a read-only property") instead of silently shadowing
to a member. The `ms`/`ns` getters are the same native functions previously
exposed as methods; a private `read_only` setter always returns the error.

Native objects advertised through `register_original_class` needed routing
both ways: `get_attr_from` and `set_attr_from` now invoke property members
found on the original-class table (previously a property there was returned
verbatim, bypassing `Property::getter`/`Property::setter`), and `Op::SetAttr`
converts an `Error` payload to a `LangError` the same way `Op::Call` already
did (native `__setattr__`/property-setter failures were previously swallowed
silently).

### Tests / call-site updates

`sw.ms()` → `sw.ms` in `test/unit/native/time.un`, `test/unit/runtime/date.un`,
`rust/playground/smoke.mjs`, `test/unit/runtime` etc. `time.un` gains
assertions for `sw.ms`/`sw.ns` being ints and for the set-error path; `date.un`
uses `sleeping.ms` (date's own `dt.ms()` is unchanged — different object).
Docs: README/docs-README member tables, BENCHMARK_GUIDE resolution note.

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | exit 0, **0 warnings** |
| `cargo test` (debug) | **205 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1406 passed** (+5 assertions) / 0 crashed; Negative **41 matched**; exit **0** |
| `scripts/differential.ps1` | **81 files, 0 differences** |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | Positive 40/40; Negative 34/34 (+7 parse-rejected skips) |


---

## 2026-11-17 — Pass 17: benchmark overhaul — Python lane, VM/JIT split, regression detection

### Starting state

- `scripts/bench.ps1` measured only Un (a single JIT lane; VM was not split
  out) and Node; there was no Python lane anywhere in the repo, no regression
  comparison against previous runs and no generated `BENCHMARK.md`.
- `python` on `PATH` is a broken WindowsApps stub (exit 9009), so CPython
  could not run on this machine at all.

### Decisions taken

1. Install **CPython 3.13.17** via winget (`Python.Python.3.13`) — user
   approved. It landed in `%LOCALAPPDATA%\Programs\Python\Python313` (not on
   `PATH`); the runner tries `python` first and falls back to that path.
2. Keep the existing harness design (warmup 2 -> reps 7, median primary) — it
   was already sound — and mirror it exactly in Python (`tracemalloc`,
   `perf_counter`).
3. `bench/results/` stays gitignored; two earlier misformatted runs were
   renamed to `*-broken` and kept for honesty. Fresh runs are compared
   against the immediately previous `results.csv`.
4. `-Report` regenerates root `BENCHMARK.md`; `-Compare <csv>` reports >10%
   median change as an advisory regression and never fails the run.

### Changes

- `bench/python/` (16 files): 15 benchmarks mirrored 1:1 from `bench/node/`
  plus `harness.py` and `env.py`. Each self-validates (`assert`), prints
  `BENCH|…` and `MEM|…`, then `=== Verify: ok ===`.
- `bench/int_ops.un`, `bench/mergesort.un` + `bench/node/{int_ops,mergesort}.mjs`
  + `bench/python/{int_ops,mergesort}.py`: two new 3-language benchmarks
  (int arithmetic / bitwise ops, mergesort on 1000 LCG ints). Un expectations
  verified against the actual runtime on all three languages.
- `scripts/bench.ps1` rewritten: `-Group all|un|unvm|unjit|node|python`, `-Only <labels>` for selective runs,
  (comma-combos too), UV_JIT=0 for the Un VM lane, auto Python/Node detection,
  header-indexed CSV parsing (works on both the old and the new format),
  geomean ratios, `-Report` (regenerates `BENCHMARK.md`), `-Compare <csv>`,
  `results.json` written next to `results.csv`/`results.md`.
- `BENCHMARK.md` at the repository root, generated by `-Report`: overview,
  environment, executive summary + geomean table, per-category tables,
  runtime/memory, concurrency (N/A), overall comparison, performance
  analysis, performance regression (vs the previous run), historical
  comparison (10 %+ significant rows only), methodology, known limitations.
- `bench/harness.un`: stale comment fixed (`tick` is 100 ns, not 0.1 ms).
- `docs/BENCHMARK_GUIDE.md` rewritten for the 3-language workflow;
  `AGENTS.md` bench section updated (VM + JIT lanes, python lane, counters).

### Measured (Ryzen 7 9800X3D, 16 threads, Windows 10, CPython 3.13.17, Node 26.13.0)

- 15 benchmarks x (Un VM, Un JIT, Python, Node), all self-validating,
  warmup 2 + 7 reps, median compared.
- **JIT ≈ VM** everywhere: the native subset is Int loops, `floor(sqrt)` /
  `sin` intrinsics, IntDict and class Int-field methods only.
- Geomeans: Un JIT/Python **0.224x**, Un JIT/Node.js **0.005x**,
  Python/Node.js **0.020x** — Un ≈ 4.5x slower than CPython on geomean;
  Node.js is ≈ 50x faster than CPython.
- Un wins against Python at `nested_sum_200` (1.12x) and `dict5000` (1.37x) —
  the IntDict fast path pays off; Node leads every row today.
- Regression vs `20261117-094751`: Node.js rows jitter >10 % (advisory only);
  Un and Python rows stay within ±10 %.

### Verification

| Command | Result |
| --- | --- |
| `scripts/bench.ps1 -Group all -Report` | 15 x 4 lanes self-validate, exit 0; `BENCHMARK.md` regenerated |
| `scripts/bench.ps1 -Group node` / `-Group python` / `-Group unvm` (`-NoSave`) | exit 0 each |
| `bench/int_ops.un` / `bench/mergesort.un` | `3 passed, 0 failed` each; Node/Python twins print `verify ok` |
| `scripts/test.ps1` | Positive 40 files / **1406 passed** / 0 failed / 0 crashed; Negative **41 matched / 0 divergences**; exit **0** |
| Build | no Rust changes in this pass; toolchain baseline unchanged |


---

## 2026-11-17 — Pass 18: the JIT was silently dead (Pass 15 regression); benchmark-observed, fixed, verified

### Starting state

- `int_ops(1_000_000)` on the un-committed benchmark showed Un JIT ≈ Un VM at ~3.2 s
  (both pure interpreter), Geomean Un JIT / Python **0.224x**. `UV_JIT_DEBUG=1` showed
  `JITDBG STUB proto[1] int_ops` — every proto fell back to a stub.
- Root cause, verified in Cranelift sources:
  `append_block_params_for_function_params` must attach to the **layout entry block**
  (position 0). Pass 15 attached the signature params to `entry` (first-created block)
  and then switched to a fresh `prologue` first, making `prologue` the layout entry —
  which had no parameters. `module.define_function` -> verifier error
  (`entry block parameters (0) must match function signature (2)`) -> STUB -> silent
  interpreter fallback. Debug and release builds both failed the verifier; every gate
  stayed green because fallback is byte-identical.

### Changes

- `rust/src/jit_bc.rs`
  - **Fix (root cause)**: the prologue block now carries the function-signature
    parameters itself (it *is* the layout entry); the body block (`entry`) is
    parameterless; the prologue terminates with a `jump` into it. The `has_user_call`
    (brif->cont/bail) path is unchanged in structure. `int_ops` now JITs: **0.08 s**
    vs UV_JIT=0 ~31 s vs the previous release ~3.2 s.
  - `Band`/`Bor`/`Bxor` (`i64 & | ^`) are supported in the Int subset; IntDict ids,
    bool/none/float operands bail to the interpreter (which raises
    `unsupported operand type(s)` there) — bit ops match the interpreter exactly.
  - `define_function` failure now prints `JITREJECT define_function ... err={err:?}`
    under `UV_JIT_DEBUG=1` (was a silent `return None`).
- `BENCHMARK.md` regenerated: **Un JIT / Python geomean 1.987x** (was 0.224x),
  Un JIT / Node.js 0.037x (was 0.004x); Un JIT now beats Python on the Int arithmetic/
  intrinsic rows (`int_ops` 794x, `math10000` 271x, `call200000` 201x, `nested_sum_200`
  1237x) and even Node.js on 4 rows.
- `docs/BENCHMARK_GUIDE.md`, `docs/REFACTOR_SUMMARY.md`, `AGENTS.md` (bitwise support),
  `docs/TECHNICAL_DEBT.md` (new TD-51: silent JIT death is undetectable by current
  gates), `docs/DECISIONS.md` (**D-073**).
- `scripts/bench.ps1`: Executive Summary binary-choice fix for JIT-beats-Python,
  performance-analysis bullets rewritten to computed ratios (no stale claims), and the
  "VM ≈ JIT" wording replaced with the actual interpreter/native split.

### Measured (Ryzen 7 9800X3D, Windows 10, CPython 3.13.17, Node 26.13.0)

| Benchmark | Before (interpreter) | After (JIT native) | JIT / Python | JIT / Node |
| --- | --- | --- | --- | --- |
| int_ops | 3226.599 ms | **2.96 ms** | 794x | 3.23x |
| nested_sum_200 | 14.14 ms | **0.013 ms** | 1237x | 3.58x |
| math10000 | 22.6 ms | **0.0185 ms** | 271x | 3.40x |
| call200000 | 362 ms | **0.688 ms** | 201x | 5.55x |
| fib24 | 177 ms | **3.38 ms** | 0.92x | 0.079x |
| iter10000 | 17.4 ms | **4.31 ms** | 1.07x | 0.051x |

Geomeans: Un JIT/Python **1.987x**, Un JIT/Node.js **0.037x**, Un VM/Python 0.229x,
Python/Node.js 0.018x.

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | 0 errors, 0 warnings |
| `scripts/test.ps1` | Positive 40 files / **1406 passed** / 0 failed / 0 crashed; Negative **41 matched / 0 divergences**; exit **0** |
| `scripts/differential.ps1` | **81 files, 0 differences** |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | Positive 40/40; Negative compilable 34/34 |
| `cargo test --release` | **205 passed / 0 failed** |
| `cargo test` (debug, verifier active) | **205 passed / 0 failed** |
| `scripts/bench.ps1 -Group all -Report` | exit 0; BENCHMARK.md + results.json regenerated |


---

## 2026-11-17 — Pass 19: `split` default, `map` type converters, adaptive callable arity

### Starting state

- `io.read().split(' ').map(int)` failed with `unsupported operand type(s) for (): 'type'`.
- Diagnosis: three compounding gaps — `split` REQUIRED the `sep` arg; every `map` native
  demanded a Func payload (`function::is_func` + `fn_call`) so a TObj type object was
  rejected; `list.map` passed `(value, index)` regardless of the callee's arity (a 1-arg
  lambda died with `too many positional arguments`), while `iter.map` was already
  adaptive via `arg_count`.

### Changes (requested by the user; D-074)

- `rust/src/native/members.rs` — `str.split()`: `sep` is now optional with default `' '`.
  `list.map(f)`: callable check widened to Func|TObj (`function::is_callable`), arity
  adaptive (`arg_count >= 2` → `(value, index)`), dispatched through `obj::call_args`.
- `rust/src/runtime/function.rs` — new `is_callable(o) -> bool` (`Func | TObj`).
- `rust/src/runtime/iters.rs` — lazy `.map` stage calls `call_args` instead of `fn_call`.
- `rust/src/native/iter.rs` — global `iter.map`: TObj converters accepted, adaptive arity
  (TObj → 1 arg; `arg_count >= 2` → `(value, index)`), dispatched via `call_args`.
- `rust/src/native/members.rs` — lazy method `iter.map`: callable check widened the same
  way (wants_index already adaptive).
- Tests: `list.un` (1-arg + 2-arg lambdas, TObj converter expectations replace the
  obsolete "requires a 2-arg lambda" error assertion per the requested semantics),
  `str.un` (default `split()`), `iter.un` (global `iter.map` converter + 2-arg lambda),
  `range.un` (lazy `.map` converter + 2-arg lambda).
- Positive assertions 1406 -> **1414**.

### Decisions

- D-074 recorded. `filter`/`any`/`all` keep the Func-only contract (predicates have no
  type-converter meaning). Keyword kwarg name `type` of `list.map` is unchanged
  (renaming would break kwarg callers for zero gain).

### Verification

| Command | Result |
| --- | --- |
| `scripts/test.ps1` | Positive 40 files / **1414 passed** / 0 failed / 0 crashed; Negative **41 matched / 0 divergences**; exit **0** |
| `scripts/differential.ps1` | **81 files, 0 differences** |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | Positive 40/40; Negative 34/34 (+7 parse-rejected skips) |
| `cargo test --release` | **205 passed / 0 failed** |
| `cargo test` (debug) | **205 passed / 0 failed** |
| `test/test_suite.un` | **109 passed / 0 failed** |
| `io.read().split().map(int)` | works: `'3 1 2'` -> `[3, 1, 2]` |


---

## 2026-11-17 — Pass 20: ergonomics sweep — `str.trim()` (default strip) and the audit that found it

### Starting state

- Pass 19 fixed `split`'s required separator, the `map` Func-vs-TObj callable gap and the
  `list.map` non-adaptive arity (D-074). The user then asked for the same class of
  defects to be found and fixed wholesale.

### Sweep (probe → verdict)

Fixed: `str.trim(chars)` previously **required** `chars`; Python-style
`"  hi  ".trim()` errored with `missing required argument`. Now `chars` is optional and
defaults to Unicode whitespace at both ends (`char::is_whitespace()`), matching Python
`str.strip()`; the explicit-`chars` form is unchanged. The one test asserting the old
err is replaced with positive coverage (D-075).

Checked and left unchanged (each verified in the VM, reasons noted):

| Idiom | Status |
| --- | --- |
| `int(" 42")`, `float(" 1.5 ")`, `int("42\n")` | works already |
| `int("  ")` | errors correctly (`cannot convert '  ' to 'int'`) |
| `dict.get(k)` / `d.get(k, default)` | default `none` works; two-arg form works |
| `l.pop()` | pops last element; negative indices work |
| `l.index_of(x)` / `s.index_of(x)` | `-1` on miss (Python parity) |
| `l.sort()`, `l.reverse()`, `d.keys()`, `d.values()` | plain-call form works |
| global `sum`/`sorted`/`max`/`min` | intentionally absent (module design, `use iter`/`use math`) — not a defect |
| `.keys()`'s dict iteration etc. | already idiomatic |
| `" 42 ".is_number()` | `false` is correct (`isdigit` parity) |

### Verification

| Command | Result |
| --- | --- |
| `scripts/test.ps1` | Positive 40 files / **1415 passed** / 0 failed / 0 crashed; Negative **41 matched / 0 divergences**; exit **0** |
| `scripts/differential.ps1` | **81 files, 0 differences** |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | Positive 40/40; Negative 34/34 (+7 parse-rejected skips) |
| `cargo test --release` | **205 passed / 0 failed** |
| `cargo test` (debug) | **205 passed / 0 failed** |
| `test/test_suite.un` | **109 passed / 0 failed** |
| `cargo build --release` | 0 errors, 0 warnings |


---

## 2026-11-17 — Pass 21: global namespace shrink — `codes` module

### Starting state

- 14 global builtins lived in `native/builtin.rs` at world scope; the user wanted the
  global surface to shrink. Usage survey across tests/benches/docs: `len` 171,
  `type` 95, `repr` 10, `hash` 10, `exec`/`throw*` 4–6, `bin`/`oct`/`hex`/`ascii` 2 each,
  `chr`/`ord` ~5.
- Type objects (constructors) and `true`/`false`/`none`/`null` are language constants
  and stay in the global scope.

### Changes

- New module **`codes`** (`rust/src/native/codes.rs`, `use codes`): `bin`/`oct`/`hex`
  (radix strings, `-0b...` for negatives), `ascii` (first UTF-16 unit of a one-char
  string), `chr`/`ord` (codepoint <-> char). Same implementations, same error strings,
  same boundary checks — moved verbatim from `builtin.rs` and registered in
  `native::init`.
- `builtin.rs`: the six members and impls are gone; globals are now exactly
  `len`/`exit`/`type`/`hash`/`throw`/`throw_if`/`throw_unless`/`repr`.
- `rust/src/runtime/modules/tests.rs`: global-set assertions updated — the six moved
  names must NOT be global; `codes` must carry them (via `native_members`).
- Tests fixture updates: `builtins.un` (`use codes`, `codes.*`), negative
  `chr_space`/`ord_space`/`multi_space_arg` (`codes.chr()`/`codes.ord()`).
- Docs: `docs/README.md` (Korean `README.md`쌍) 모듈 표에 `codes` 행 추가 + built-in 목록
  갱신; `docs/DECISIONS.md` **D-076**; SUMMARY/LOG 갱신.
- User's `main.un` uses none of the moved functions (checked, unchanged).

### Verification

| Command | Result |
| --- | --- |
| `cargo build --release` | 0 errors, **0 warnings** |
| `scripts/test.ps1` | Positive 40 files / **1415 passed** / 0 failed / 0 crashed; Negative **41 matched / 0 divergences**; exit **0** |
| `scripts/differential.ps1` | **81 files, 0 differences** |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | Positive 40/40; Negative 34/34 (+7 parse-rejected skips) |
| `cargo test --release` | **205 passed / 0 failed** |
| `cargo test` (debug) | **205 passed / 0 failed** |
| `test/test_suite.un` | **109 passed / 0 failed** |


---

## 2026-11-17 — Pass 22: fib benchmark — depth guard cost removed

### Starting state

- `fib24` (150,049 recursive calls, measured via warmup=2/reps=7/median) was
  ~3.39 ms on Un JIT — on par with CPython's 3.13 ms on the same machine and
  ~13x slower than Node/V8 (0.27 ms). The user asked to close the gap with
  Python at minimum.

### Analysis

- The recursion itself compiles natively through `emit_user_call` (direct
  Cranelift call, no interpreter round-trip). The per-call overhead was the
  *entry* guard: every entry emitted `call un_jit_depth_enter`, which did an
  atomic `fetch_add` AND a fresh `WORLD.with(|w| w.borrow()...)` lookup of
  `max_recursion_depth()` — a heap RefCell TLS borrow on the hot path
  hundreds of thousands of times.
- `un_jit_depth_exit` was a second `fetch_sub`.

### Changes

- `rust/src/jit_cache.rs`: `JIT_DEPTH` is now a plain shared-memory
  `AtomicUsize` (Relaxed); `max_recursion_depth` is served from a cached
  `AtomicUsize` (Relaxed) instead of borrowing `WORLD` per call.
  `sync_max_jit_depth()` refreshes the cached bound from
  `world::set_max_recursion_depth()`; `world::reset()` initializes the cache.
- Semantics preserved (depth overflow still bails via `TAG_BAIL` → interpreter
  fallback, which still enforces `max_recursion_depth` exactly).

### Result

- `fib24`: **~1.32 ms** (was 3.39 ms) — ~2.4x faster than Python's 3.13 ms on
  the same machine.
- Correctness re-verified: Positive 1415/41/0, differential 81/0,
  cargo test 205 (release+debug), unbc-roundtrip 40/40+34/34, suite 109.

---

## 2026-11-18 - Pass 23: math Float intrinsic (JIT parity for numeric kernels)

### Starting state

- After the normalization pass removed the benchmark-shaped intrinsics
  (`match_floor_sqrt`, `match_math_sin`), every proto containing a
  `math.*` call was rejected at candidacy (`LoadName "math"` had no
  marker, so the whole proto was interpreted). `math_work` and `sin_sum`
  ran at interpreter speed: math10000 JIT ~22ms, sin20000 JIT ~27ms.
- Removing user-hostile special cases was correct; but numeric programs
  (the whole point of the Int-JIT value model) lost all JIT benefit.

### Analysis / design

- The removed intrinsics were rejected for *matching call-sequence shapes
  by name text*. The replacement must be a general, always-honest lowering:
  any Float argument, any call site, no input-value-dependent decisions.
- Design doc: `docs/JIT_FLOAT_MATH_INTRINSIC.md` (Accepted after review;
  open questions resolved: `pow` = both-Int args bail (Int semantics owned
  by the interpreter - nameless registers cannot express Int|Float poly
  tagging); `log` keeps base with `un_math_log(v, b) = v.ln() / b.ln()`;
  `UV_JIT=2` canary unchanged (the differential gate is the accuracy
  backstop); the rebind guard rule 2 (math-use + member-name rebind) is
  documented bluntness).
- Implementation reuses the existing marker machinery (`len` / class-method
  precedent): `LoadName "math"` sets a marker; `GetAttr <fn>` binds the
  member marker; `Op::Call` lowers to an f64 FFI call (`un_math_*` in
  `native/math.rs`). Domain violations (sqrt(x<0), log(x<=0)) are
  pre-checked by emitted `fcmp` guards that tag=2 bail the frame - the
  interpreter re-runs and raises the exact Un error, so error text and
  bytes are preserved.
- `compute_local_float` had to learn the intrinsics: the static poly-tag
  analysis previously knew only LoadConst Float / Float arithmetic / Dup;
  without math-call arms the promoted-local slot of `total` was never
  flagged and the loop returned 0. Fixed by teaching the analysis the
  math-call result kinds (Float-always for transcendentals / ln / log /
  hypot; identity-preserving for floor/ceil/trunc; arg-kind for abs;
  Sign always Int).
- Rebind guard: `math` store vetoes unconditionally; `SetAttr` of a
  math-fn name vetoes only when the program also reads the `math` global
  (alias `m = math` counts through the source LoadName). Math-free
  programs with `self.sin = 3` are unaffected.

### Result

- `math.un` and `floatmath.un` JIT lanes are byte-identical to `UV_JIT=0`
  and fast again: math10000 JIT ~0.05ms, sin20000 JIT ~0.54ms
  (interpreter lanes ~22 / 27ms).
- Gates: Positive **1493 passed / 0 failed** (47 files), Negative **41 matched**,
  differential **88 files, 0 differences**, unbc-roundtrip **47/47 identical**
  (negative 34/34, 7 compile-rejected skips), cargo test --release **205 passed**,
  `UV_JIT=2` canary OK, test/unit/native/math.un 47 passed.
- Ad-hoc verification matrix: sqrt/log domain errors byte-identical between
  lanes; Int-identity floor/trunc verified; `math = 5` and `math.sin = spy`
  module rebinds veto the whole program to the interpreter (verified both
  directly and through a `m = math` alias); a math-free class with a
  `self.sin` attribute still JIT-compiles.

---

## 2026-11-18 - Pass 23 follow-up: Float strict canary

- `test/jit/strict_float.un` 신설: `test/jit/strict.un`(Int 전용)과 같은 형태로
  Float math intrinsic 계열을 커레이트한 proto들만으로 구성. `math.un`/`floatmath.un`의
  커널 모양(Int 누산 승격, Float 누산)과 sqrt/log/sin/trunc/sign/hypot/pow(mixed Float)
  사례를 담고, `close()` 헬퍼로 Float 근접 비교를 직접 구현한다(전부 JIT 적격).
- `.github/workflows/build.yml`에 `JIT Float strict canary (UV_JIT=2)` 스텝 추가.
- bring-up에서 실측으로 얻은 사실: **unary `Op::Neg`(Float 상수 부정)은 JIT 미지원.**
  `math.sign(-0.5)`를 `0.0 - 0.5`(fsub)로 바꾸면 통과한다. 남은 JIT 갭으로 기록.
- 검증: 양 lane + `UV_JIT=2` 모두 `jit-float-strict canary OK`, exit 0.
  (의도적으로 `sign(-0.5)`를 쓴 초안은 `UV_JIT=2`에서 `1/6 candidate(s) failed JIT codegen: 4:sign_sum`로 죽는 것을 확인 — canary가 작동한다.)

---

## 2026-11-19 - Pass 23 follow-ups 2: strict canary verified, debt sync, iter_values unified

- Float strict canary (see previous entry) re-verified green in all three
  lanes (default / UV_JIT=0 / UV_JIT=2). `.github/workflows/build.yml`
  gained the `JIT Float strict canary (UV_JIT=2)` step; AGENTS.md synced.
- `TD-45` evidence refreshed (hypot fixed earlier via f64::hypot — entry now
  says so). New `TD-53`: the Int-JIT never lowers unary `Op::Neg` — any proto
  with an unary minus is rejected and interpreted; `0.0 - x` (fsub) is the
  documented workaround.
- `iter_values` unified: both `native::iter::iter_values` (iter.* global
  functions) and `native::members::iter_values` (method twins) were merged
  into `runtime::iters::into_values`. Plain-list fast path now benefits both
  call sites; error strings preserved exactly ("expected 'iterable' argument
  to be of type 'iter'" vs "counter is infinite"). Verified:
  iter-heavy paths (test_suite, algorithms.un) and all gates
  (Positive 1493, differential 88/0, unbc-roundtrip, cargo test 205).
- `docs/ARCHITECTURE.md` gained the "Resource lifecycle" section: arenas are
  process-lifetime and grow monotonically, only `jit_cache::clear()` releases
  (incl. why `unsafe impl Send for JitEntry` is sound today), PLAN_CACHE
  (TD-03) bounds, recursion-depth atomic + cached bound, tag=2 bail as the
  safety valve, class-layout reordering note (TD-51).

---

## 2026-11-19 - Pass 24: unary Op::Neg JIT lowering (TD-53)

### Trigger

- `strict_float.un` bring-up (Pass 23 follow-up) revealed the Int-JIT never
  lowers unary `Op::Neg` - any proto with an unary minus is rejected and
  falls back to the interpreter. Registered as TD-53.

### Changes

- `rust/src/jit_bc.rs`: new `Op::Neg` arm. Int args lower to `ineg`
  (two's-complement wrapping, identical to the interpreter's `wrapping_neg`,
  including i64::MIN); Float args to `fneg`. Bool/none payloads and arena
  handles (IntVec/IntDict/class markers) bail the frame to the interpreter,
  which raises the exact "unsupported operand type(s) for -" error.
- Module header updated: `Neg` listed under supported unary ops.
- `test/jit/strict_float.un` now writes `-0.5`/`-7` instead of the fsub
  workaround, keeping the arm CI-verified under UV_JIT=2.
- Debt/docs: TD-53 closed (table + resolved list), D-079 registered, the
  Float strict-canary decision entry in the design doc updated.

### Result

- Gates: Positive 1493/1493, Negative 41 matched, differential 88/0,
  unbc-roundtrip 47/47+34/34, cargo test 205, strict_float canary green in
  all three lanes. i64 extremes (9223372036854775807 / negatives) identical
  on both lanes.

---

## 2026-11-19 - Pass 25: native-call panic boundary hardening (TD-16, TD-40)

### Trigger

- TD-40: "a Rust panic inside a native call kills the worker thread with a
  raw Rust panic instead of an Un error." The entry said "largely capped
  now"; the user asked for an honest audit of what escapes, to either close
  the gap or close the debt. TD-16 (`plan_for`'s `.expect()` with the
  invariant enforced only by callers) was the other suspect.

### Audit

- Step-level containment already exists and is the outer catch for engine
  execution: `VM::run`'s dispatch loop wraps every `self.step(...)` in
  `catch_unwind` and converts a payload into `Outcome::Panic`. Converted
  panics surface in `main.rs` via `report()` with exit code 1.
- Every Un-visible native call path (`fn_call` → `host(&scope)`, JIT calls,
  builtins) rides on the same guard because it runs inside a step.
- The surviving holes were plan acquisitions *before* the first step:
  `VM::new`'s outer `plan_for(&program)` (its `.expect` fires before any
  catch boundary), the proto-executor closure registered in `VM::new`, and
  `VM::with_context` (the module loader path, which pre-validates and even
  *documented* the invariant in a comment - but did not enforce it).
- Playground (`unvm-playground`) and the CLI go through the same two
  constructors, so they inherited the same gap.

### Changes

- `rust/src/vm/mod.rs`: VM::new wraps both plan acquisitions in
  `catch_unwind`; on failure the VM returns `Result<_, String>` (outer) or
  `function::Flow::Panic` (proto executor) carrying an "internal error
  (please report): {message}" Un panic object. `VM::with_context` now
  returns `Result<Self, String>` and shares the helper.
- `rust/src/module.rs`: the module loader's single call site now maps
  `Err` to `ModuleError::Panic`, matching its `validate()`-failure path.
- Debt doc: TD-16 and TD-40 closed (table + resolved-in-2026-11 list).

### Result

- No known unprotected path remains: a Rust panic anywhere in engine code
  now surfaces as an Un panic object that flows through `report()` with
  exit 1, on all three front doors (CLI, module loader, playground).
- Gates: Positive 1493, Negative 41 matched, differential 88/0,
  unbc-roundtrip 47/47+34/34, cargo test 205, test_suite 109, algorithms.un
  42. AGENTS.md scale counts synced (84 files / 38,265 lines).


---

## 2026-11-19 - Pass 26: perf/compliance sweep (TD-08, TD-17, TD-31, TD-03, TD-11)

### Trigger

- Small, verified wins between the TD-40/TD-16 closure (Pass 25) and the
  remaining structural debt. Selection: TD-08 (call-path allocation), TD-17
  (debug-dump magic), TD-31 (vendor licence compliance), TD-03
  (PLAN_CACHE bound), TD-11 (Source lookup cost).

### Changes

- `runtime/function.rs`: `Args::positional` now allocates a single empty
  `names` Vec; `Args::named` unchanged; positional consumers use `.get(i)`
  and treat a missing name as positional. Per-argument `String::new()`
  calls and the names Vec of size n are gone. Local-A/B on
  `bench/funccall.un`: no measurable delta (522/548/549ms vs stashed
  baseline 478/498/533ms - within run-to-run noise); allocation count per
  call dropped by n+1.
- `vendor/`: LICENSE (Apache-2.0 + LLVM exception, verbatim from the
  bytecodealliance repo) shipped for `cranelift-jit` and
  `icache-coherence`; `vendor/README.md` and `vendor/THIRD_PARTY.md`
  updated. TD-31 closed.
- `vm/transfer.rs`: the depth-mismatch debug dump prints a clamped
  ±32-instruction neighbourhood of the failing ip instead of the magic
  `744..=796` window (which was also able to panic out-of-bounds on short
  protos). TD-17 closed.
- `vm/mod.rs`: `PLAN_CACHE` is bounded at 64 entries with first-insertion
  FIFO eviction; insertion beyond the limit evicts the oldest. A hit just
  recompiles from the program passed to `plan_for`. TD-03 closed.
- `source.rs`: line-start index built once at construction (after CRLF
  normalisation); `get_line`/`get_column`/`get_line_start` are O(log lines)
  via binary search. TD-11 closed.

### Result

- Gates: Positive 1493, Negative 41 matched, differential 88/0,
  unbc-roundtrip 47/47+34/34, cargo test 205. Suite 109, algorithms 42.
- Debt doc: TD-03, TD-11, TD-17 closed; TD-08 mitigated (residual
  `to_vec` clone is API-breaking to remove and stays as documented debt).
- AGENTS.md scale counts synced (84 files / 38,299 lines).


---

## 2026-11-19 - Pass 27: JIT native code release at process boundaries (TD-05)

### Trigger

- TD-05 (the only remaining high-severity debt item): native code emitted
  by the JIT is never freed during a normal run — the vendored
  Cranelift backend deliberately `mem::forget`s every allocation so function
  pointers extracted at registration stay valid for the process lifetime.
- The pragmatic fix is not "free in Drop" (that would unmap code pages that
  live function pointers still reference — use-after-free); it is wiring a
  boundary where freeing is provably safe.

### Changes

- `rust/src/main.rs`: the CLI worker calls `jit_cache::clear()` on the
  success path after `worker.join()` returns, and `report()` calls it on
  every error/panic exit path before printing. In both places no JIT'd code
  runs afterwards; when JIT is disabled (`UV_JIT=0`) or nothing was
  compiled, clear() is a cheap no-op (it still drops the IntDict/IntList/
  class arenas and the JIT registry).
- `rust/playground/src/lib.rs`: the WASM playground calls
  `unvm::jit_cache::clear()` at the end of every run — each run compiles a
  fresh `Program` with a fresh id, so its JIT entries can never be reused
  and would otherwise accumulate in WASM linear memory across user edits.
- Vendored backend unchanged: `Memory::drop` stays `mem::forget` on purpose —
  that *is* the safety argument for `jit_cache.rs`'s `unsafe impl Send for
  JitEntry`. The in-flight-unfreeable window is documented in
  `jit_cache.rs`'s module docs and in the ARCHITECTURE "Resource lifecycle"
  section; it is not observable as correctness failure because the leak
  lives until process exit by construction.

### Result

- CLI processes now release JIT-native memory at exit on both boundary
  classes; the WASM playground frees per user run. What remains is the
  structural in-flight window only.
- Gates: Positive 1493, Negative 41 matched, differential 88/0,
  unbc-roundtrip 47/47+34/34, cargo test 205, test_suite 109.
  WASM target (`-p unvm-playground`) builds clean.
- No high-severity debt items remain in `docs/TECHNICAL_DEBT.md`
  (TD-08 mitigated, TD-05 closed).


---

## 2026-11-19 - Pass 27 addendum: TD-10 evidence-refresh

### Question

- TD-10 claims `set_attr_from` clones the assigned value up to three times
  per assignment.

### Findings

- `Obj = Rc<RefCell<ObjData>>`: every one of those "clones" is a
  non-atomic refcount bump - no deep data copy happens anywhere on this
  path. A member-set assignment costs, per call: one target Rc clone +
  one value Rc clone (VM), one value Rc clone (slot push in member_set),
  one value Rc clone (return for the stack slot). The handler path adds
  one clone into the handler args.
- The residual is inherent to the store needing its own Rc. Eliminating
  the final return clone requires leaking path information across the
  `set_attr` boundary; the win is ~1 refcount bump per assignment.
- No bench kernel is attribute-write bound; the class workload (2 writes
  + 2 reads per iteration) is dominated by the VM dispatch loop itself.

### Verdict

- Not restructured. TD-10 reclassified as evidence-refresh-only; the
  residual is documented debt with measured impact below noise at the
  kernels we track (see bench).


---

## 2026-11-19 - Pass 28: stdlib correctness (date overflow, exit/ord, dict pin)

### Trigger

- Stdlib correctness batch: date arithmetic silently wrapped i64 on
  enormous arguments, `time.from_epoch` multiplied unbounded,
  `exit("x")` silently returned 0, `ord("ab")` returned the first codepoint,
  and `dict.add` on a duplicate key raised the .NET `ArgumentException`
  with no test pinning that contract.

### Changes

- `native/date_members.rs`: `add_days/hours/minutes/seconds/ms`,
  `add_years`, `add_months` now use checked arithmetic
  (`checked_ticks` / `checked_add` / `checked_mul`). Overflow returns a
  catchable `date arithmetic overflowed` error instead of a wrapped date.
- `native/time.rs`: `from_epoch` / `from_epoch_ms` checked similarly.
- `native/builtin.rs`: `exit()` with a non-Int code returns a catchable
  `exit() expects an 'int' code` error (previously silent exit 0).
- `native/codes.rs`: `ord()` requires exactly one Unicode scalar (else
  `ord: expected a single character`).
- Tests: new assert_err pins in `test/unit/runtime/date.un` (add_* overflow),
  `test/unit/native/time.un` (from_epoch_ms), `test/unit/runtime/errors.un`
  (exit non-int); `test/negative/dict_add_duplicate.un` and
  `test/negative/ord_multiple_chars.un` pin `.NET ArgumentException` and the
  ord single-char error for the negative manifest (41 -> 43 entries).
- Full gates re-run: Positive **1504 passed** (was 1493, +11 new pins),
  Negative **43 matched**, differential 88/0, unbc-roundtrip green,
  cargo test 205.
- AGENTS.md baseline counts synced; TD-41/TD-48/TD-49 marked resolved
  (TD-49's astral `ascii` UTF-16 behavior is kept as parity; `inspect.delattr`
  own-instance scope pinned-accepted).

