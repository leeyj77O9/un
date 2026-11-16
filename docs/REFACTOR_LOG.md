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
median: **5.59 → 4.73 ms** (audit baseline 7.272 ms). Gates unchanged:
cargo test 204, test.ps1 1395/exit 0, differential 79/0, round-trip green.
`bench/class.un` untouched (user WIP).

### Verification

No code changes; documentation only. Gates assumed green from pass 8.
