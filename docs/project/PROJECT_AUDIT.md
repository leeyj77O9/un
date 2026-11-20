# docs/project/PROJECT_AUDIT.md

Audit of the Rust `unvm` implementation (branch `main`, base commit `958f9e2`).

This document is the **canonical audit record** for the current Rust port. The
C#→Rust migration record (including the measured C# differential results) lived
in `docs/archive/` until 2026-11-16; those files were removed in the docs
cleanup and remain in `git` history. This document describes *the state of the
code as it stands now* and what was found.

Related documents:

- `docs/project/REFACTOR_SUMMARY.md` — the one-page index (commit timeline, baseline).
- `docs/project/REFACTOR_LOG.md` — dated log of the changes that were actually applied.
- `docs/spec/ARCHITECTURE.md` — how the implementation is put together.
- `docs/project/TECHNICAL_DEBT.md` — the debt register (items deliberately left in place).
- `docs/project/DECISIONS.md` — language/runtime decisions (D-001 … D-092).

## 1. Scope and method

| Item | Value |
| --- | --- |
| Implementation | `rust/` (workspace: `unvm` lib + CLI + `unvm-playground`) |
| Source size | 76 `.rs` files, 34,166 lines (31,483 excluding `tests.rs` modules, wc) |
| Tests | 89 `.un` scripts under `test/`, 202 Rust unit tests |
| Toolchain | `stable-x86_64-pc-windows-gnu`, release profile only |
| Dependencies | standard library only, plus vendored `region` / `cranelift-jit` / `icache-coherence` patches |

Method: a read-only depth audit was run over six areas (frontend, runtime object
model, VM/bytecode, JIT, native/stdlib, build+docs+workspace) with independent
reviewers, every claim was re-checked against the working tree before it was
recorded here, and every behaviour change was validated with the full test
runner and `cargo test`.

Measured baseline before this audit (commit `958f9e2`, with the user's
uncommitted work in the tree):

- `cargo build --release` → exit 0, **4 warnings** (`rust/src/vm/mod.rs:1587-1588`).
- `scripts/test.ps1` → Positive 39 files / 1363 passed / 0 failed / 0 crashed;
  Negative 39 matched / 0 mismatched / 0 documented divergences; Known 1 file
  (6 failing assertions); exit 0.
- `scripts/bench.ps1 -Group un -NoSave` (UV_JIT=1) → `fib24` 3.411 ms,
  `nested_sum_200` 0.013 ms, `qsort500` 18.262 ms, `rle1994` 11.062 ms,
  `dict5000` 0.242 ms, `iter10000` 5.719 ms, `math10000` 0.016 ms,
  `class1000` 7.272 ms, `sieve30000` 66.398 ms, `matrix48` 74.490 ms,
  `sin20000` 31.915 ms, `call200000` 0.694 ms, `regex6000` 22.430 ms.

## 2. Findings by severity

### S1 — Correctness defects (fixed in this pass)

| # | Finding | Evidence | Status |
| --- | --- | --- | --- |
| S1-1 | Stepped slicing dropped every element: `list_slice` walked the step loop and then returned an empty list, so `l[2:0:-1]`, `l[::2]` and every other non-unit step produced `[]` | `rust/src/runtime/collections.rs:386-447` (pre-fix) | fixed |
| S1-2 | Negative steps never reversed anything: the default start was `0` and the default end was `len` regardless of the step sign, so `l[::-1]`, `"abc"[::-1]` and `(1,2,3)[::-1]` were empty | `rust/src/runtime/collections.rs:387`, `rust/src/runtime/obj.rs:1156` (pre-fix) | fixed |
| S1-3 | Slice bounds were truncated to 32 bits (`i as i32 as i64`), so bounds outside the i32 range silently addressed the wrong element | `rust/src/runtime/collections.rs:393/405`, `rust/src/runtime/obj.rs:1162/1173` (pre-fix) | fixed |
| S1-4 | `upper_bound` could never return `len`: the search started at `len - 1`, so the insertion point after the last element was unreachable | `rust/src/native/members.rs:1157-1176` (pre-fix) | fixed |
| S1-5 | `set == set` and `dict == dict` raised `unsupported operand type(s) for ==` instead of comparing | `rust/src/runtime/collections.rs:692` (no `Eq` arm), `rust/src/runtime/obj.rs:1432` | fixed |
| S1-6 | `2 in [1, 2, 3]` raised `cannot check if 'int' is in 'list'`: list/tuple membership only accepted other lists/tuples | `rust/src/runtime/collections.rs:448-459` (pre-fix) | fixed |
| S1-7 | `type(none) is none` was `false`: the `is` operator swapped its operands whenever one side was a type object, so the question became "is the *type object* an instance of `none`?" | `rust/src/vm/mod.rs:1720-1726` (pre-fix) | fixed |
| S1-8 | The JIT's class-layout path could read the wrong member and silently produce `0` (or drop a write): the layout vector contained **method names**, so the layout index did not match the instance member index, and `un_instance_int_get` returned `0` for a miss | `rust/src/vm/mod.rs:1611-1621` (pre-fix), `rust/src/runtime/obj.rs:367-388` (pre-fix) | hardened (S2-1/S2-2) |
| S1-9 | `lower_bound` could never return `len(items)` and returned `-1` on an empty list | `rust/src/native/members.rs:1136-1158` (pre-fix) | fixed |
| S1-10 | `extend_insert` inserted the items of an iterable in **reverse** order and swallowed an invalid `index` argument silently | `rust/src/native/members.rs:993-1021` (pre-fix) | fixed |
| S1-11 | Float keys were not Hash/Eq-consistent: `-0.0 == 0.0` but `to_bits(-0.0) != to_bits(0.0)`, so `set([0.0]).contains(-0.0) == false` | `rust/src/runtime/obj.rs:299` (pre-fix), `rust/src/runtime/obj.rs:283` | fixed |
| S1-12 | Lazy `.map()` put a lambda error into the output list as a value and lazy `.filter()` silently dropped an item whose predicate errored | `rust/src/runtime/iters.rs:198-235` (pre-fix) | fixed |
| S1-13 | `fs.modified()`/`created()`/`accessed()` printed garbage dates (the time string used wrong tick divisors, ~86× off) | `rust/src/native/fs.rs:484-499` (pre-fix) | fixed |
| S1-14 | `fs.rmdir(dir)` deleted a non-empty directory by default (`recursive` defaulted to `true`) | `rust/src/native/fs.rs:352-354` (pre-fix) | fixed |
| S1-15 | `fs.move`/`fs.rename` deleted the destination **before** the rename, so a failed move lost the destination file | `rust/src/native/fs.rs:281-323` (pre-fix) | fixed |
| S1-16 | `walk_dir` had no symlink-cycle guard: `a -> a/x` recursion recursed forever until native stack overflow | `rust/src/native/fs.rs:476-487` (pre-fix) | fixed |
| S1-17 | WebSocket frames used non-RFC-6455 extended-length markers (100/101) and had no ping→pong handling — every conformant peer mis-decoded frames ≥ 100 bytes | `rust/src/native/net.rs` read/write frame paths (pre-fix), `rust/src/native/ws.rs` | fixed |
| S1-18 | `iter.array` could attempt an unbounded allocation from user input; negative dimensions silently produced an empty list | `rust/src/native/iter.rs:266-305` (pre-fix) | fixed |
| S1-19 | `random.range` materialised the whole range with no cap; the module RNG was deterministically seeded to 0 in every process; `below()` had modulo bias; `between()` overflowed i64 | `rust/src/native/random.rs` (pre-fix) | fixed |
| S1-20 | `date.add_years` rolled Feb 29 into Mar 1 instead of clamping to Feb 28 (inconsistent with `add_months`) | `rust/src/native/date_members.rs:192-210` (pre-fix) | fixed |
| S1-21 | `time.sleep` truncated the duration through `i32`/`f64→i32`, silently wrapping and sleeping the wrong amount | `rust/src/native/time.rs:55-74` (pre-fix) | fixed |

### S2 — Memory safety and JIT soundness

| # | Finding | Evidence | Status |
| --- | --- | --- | --- |
| S2-1 | The JIT runtime helpers had no failure channel: a missing member, a non-Int member or a `RefCell` borrow conflict all produced `0`, and a write to a missing slot was dropped | `rust/src/runtime/obj.rs:367-403` (pre-fix) | fixed — helpers now report a status and the JIT bails (tag=2) |
| S2-2 | Class methods were compiled with an inconsistent arity: `precheck_fn` counted the hidden receiver argument, `emit_fn_in_module` did not, so the first real parameter was never bound | `rust/src/jit_bc.rs` (pre-fix) | resolved in Phase 8 — arity is consistent (params + receiver), and the receiver is bound to local slot 0; the name-checked intrinsic verifies the layout at call time |
| S2-3 | `unsafe extern "C"` bodies could unwind into native code (a `RefCell` panic crossing the Cranelift frame is undefined behaviour) | `rust/src/runtime/obj.rs:376/398`, `rust/src/runtime/int_dict.rs:73-95` | partially fixed — the instance helpers are wrapped in `catch_unwind`; the IntDict arena still needs it |
| S2-4 | The JIT tag contract was documented in three different places with three different legal tag sets (`0` Int, `1` Float, `2` bail, `3` Bool); `jit_fast.rs` panicked on any tag it did not know | `rust/src/jit_cache.rs:22`, `rust/src/vm/mod.rs:1532-1537`, `rust/src/jit_fast.rs:58-62` | open — see `docs/project/TECHNICAL_DEBT.md` TD-04 |
| S2-5 | `jit_cache` keys entries by `Rc::as_ptr(program)`. The cache is never cleared, so the program is kept alive for the process lifetime and the pointer key can never be reused; but `jit_cache::clear()` exists and is never called, and the module-level header still documents a removed AST-based `"fn:<name>"` key format | `rust/src/jit_cache.rs:1-7, 100` | open — TD-02 |
| S2-6 | `PLAN_CACHE` (thread-local) stores `(Rc<Program>, Rc<Plan>)` keyed by the same pointer and never evicts, so every loaded program is retained for the process lifetime | `rust/src/vm/mod.rs:1571-1573` | open — TD-03 |
| S2-7 | `unsafe impl Send for JitEntry` — sound only because the vendored Cranelift backend `mem::forget`s every allocation, so native code is never freed | `rust/src/jit_cache.rs:57`, `vendor/cranelift-jit/src/memory/system.rs:146-151` | open — TD-05 |

### S3 — Performance

| # | Finding | Evidence | Status |
| --- | --- | --- | --- |
| S3-1 | `std::env::var("UV_JIT_DEBUG")` was read **per emitted instruction** and per `Op::AddMethod` — an environment lookup inside the JIT emitter hot path | `rust/src/jit_bc.rs:241-1072`, `rust/src/vm/mod.rs:1551-1610` (pre-fix) | fixed — cached in `jit_cache::jit_debug()` |
| S3-2 | The class field layout was recomputed on every `Op::AddMember`/`Op::AddMethod`, scanning every proto — O(classes × methods × protos × code) | `rust/src/vm/mod.rs:205-233` (pre-fix) | fixed — layout is seeded once per program at load time |
| S3-3 | The JIT compiles every eligible proto eagerly at program load, including protos that end up as bail-stubs; measured cost on `test/test_suite.un` was 96 ms with the JIT vs 55 ms without | `rust/src/jit_bc.rs:114-127`, `rust/src/vm/mod.rs:1679` | open — TD-06 (no tiering / hotness counter) |
| S3-4 | `obj::invoke`/`obj::call` rebuild the argument vector with `args.to_vec()` on every call | `rust/src/runtime/obj.rs:515, 520` | open — TD-08 |
| S3-5 | `scope::snapshot` deep-clones the whole scope chain (symbols, order, slots, slot names, parents) and is called for every `go` future | `rust/src/runtime/scope.rs:227-249`, `rust/src/vm/mod.rs:1225` | open — TD-09 |
| S3-6 | `set_attr_from` clones the assigned value up to three times per assignment | `rust/src/runtime/obj.rs:743-770` | open — TD-10 |
| S3-7 | `Source::get_line` / `get_column` / `get_line_start` scan the whole source from offset 0 on every error render | `rust/src/source.rs:38-62` | open — TD-11 |

### S2-build — Build, packaging and verification integrity

| # | Finding | Evidence | Status |
| --- | --- | --- | --- |
| S2-8 | The negative manifest hard-codes the absolute checkout path `C:\dev\un` in two expected substrings, so the suite fails on any other checkout location | `test/negative/manifest.txt:15-16` (pre-fix) | fixed — the substrings no longer name the checkout root |
| S2-9 | `scripts/test.ps1` silently exits 0 on an unknown `-Group` and `-File` always exits 0; no per-test timeout | `scripts/test.ps1:194-205` | fixed — unknown `-Group` exits 2 (prints valid groups); `-File` exits with the file's result code (per-test timeout still open — TD-33) |
| S2-10 | Three of the four "documented divergences" in the negative suite were vacuous: `depth_limit_crash.un` exits 0, `go_loop_var.un` exits 0 deterministically, and `order_crash.un` died on a missing `use io` before reaching the code its note documented | `test/negative/depth_limit_crash.un`, `test/negative/go_loop_var.un`, `test/negative/order_crash.un`, `test/KnownDivergent.txt:8-18` | fixed — the checks were repaired into meaningful coverage: `.order()` assertions in `test/unit/native/list.un`, go-in-a-loop in `test/unit/runtime/async.un`, recursion in `test/integration/recursion.un`; the vacuous files were deleted; `property_recursion.un` now matches the current implementation (exit 1 + `maximum recur`); negative suite is **39 matched / 0 divergences** |
| S2-11 | The vendored Apache-2.0-WITH-LLVM-exception crates (`cranelift-jit`, `wasmtime-internal-jit-icache-coherence`) ship without their `LICENSE` files; `vendor/README.md` claimed they are present | `vendor/README.md:17-18`, `vendor/cranelift-jit/`, `vendor/icache-coherence/` | fixed — `vendor/README.md` now states the situation honestly and `vendor/THIRD_PARTY.md` records the attribution; the license texts themselves are still missing (user decision — TD-31) |
| S2-12 | The five new benchmarks live in untracked files; `scripts/bench.ps1` is modified to use them — committing the script without the files breaks `-Group all` on fresh clones | `scripts/bench.ps1`, `bench/{sieve,matrix,floatmath,funccall,regex}.un`, `bench/node/*` | user decision — keep them together (the user chose "같이 커밋") |
| S2-13 | No CI builds or tests the repo; the only tracked workflow notifies an external `un-playground` repository and does nothing without a secret | `.github/workflows/dispatch-playground.yml` | fixed — `.github/workflows/build.yml` builds with the GNU toolchain and runs `scripts/test.ps1` + `cargo test --release`; plus `rust-toolchain.toml` pins the toolchain |

### S4 — Maintainability and dead code

| # | Finding | Evidence |
| --- | --- | --- |
| S4-1 | 18 `NodeKind` variants were never constructed: the 14 compound-assignment kinds, `Go`, `Argument`, `Alias`, `Target` | `rust/src/ast.rs:25-38, 57, 64-65, 68` — **deleted** (2026-11-16) |
| S4-2 | 22 `TokenType` variants are never emitted by the lexer; they remain only in the `name()` display table (the `text()` copy was deleted). They are kept deliberately: nothing constructs them, but removing enum variants is an unneeded ABI risk here | `rust/src/token.rs` |
| S4-3 | `TokenType::text()` (116 match arms) had **zero** call sites — **deleted** (2026-11-16) | `rust/src/token.rs:118` |
| S4-4 | `Token::error` was never called and `Token.message` was therefore never populated — both **deleted** (2026-11-16) | `rust/src/token.rs:363, 374-381` |
| S4-5 | Dead `pub` items: `Source::full_name`, `Source::len`, `Node::end`, `LangError::relocated`, `unbc::opcode_of`, `jit_cache::clear`, `world::current_program` (+ setter), `scope::get_slot_name`, `collections::is_single`, `iters::iter_copy`, `iters::next_of`, `scope::declare`/`remove` (now `#[cfg(test)]`) — all **removed** (2026-11-16) | see §4 of this document for the verification method |
| S4-6 | `rust/src/jit.rs` was a test-only Cranelift probe that shipped in the public library API (`pub mod jit;`) with no production caller — **deleted** (2026-11-16) | `rust/src/lib.rs:6`, `rust/src/jit.rs:186-205` |
| S4-7 | The frontend walked the AST three times before bytecode: `desugar` (full rebuild), `optimize` (fixed-point loop, each pass cloning the tree), `compile` — **fixed**: desugar is now folded into the optimizer's bottom-up pass (`desugar_local`), and the fixed-point loop is gone (a single bottom-up pass reaches the fixpoint). The pipeline is `parse → optimize (desugar+fold) → compile` | `rust/src/desugar.rs` (deleted), `rust/src/optimize.rs`, `rust/src/runner.rs` |
| S4-8 | Five parallel representations of "type" (`Types`, `ObjData.type_name/declared/types`, `Payload::TObj`, three string vocabularies, the JIT's own `class_layouts` keying) | `rust/src/runtime/types.rs`, `rust/src/runtime/obj.rs:153-163`, `rust/src/runtime/world/classes.rs` |
| S4-9 | `runtime/world.rs` was a god module: type cache, scalar caches, class registries, JIT class layouts, module cache, stdout/log/stdin, recursion depth, executor hooks — **split** into `runtime/world/` (`caches`, `classes`, `modules`, `io`, `depth`, `executors`) with unchanged caller-facing API | `rust/src/runtime/world/` |
| S4-10 | Error strings still carry .NET exception names (`System.DivideByZeroException`, `System.IndexOutOfRangeException`, `System.NullReferenceException`). These are **test-visible contract** (`test/negative/manifest.txt`) and must not be changed casually | `test/negative/manifest.txt:19-21` | **Superseded (2026-11-20, D-092)**: `System.*` naming removed; every built-in panic prints `error: <msg>`; the negative manifest substrings were rewritten to the message bodies |
| S4-11 | The `is`/`is not` operators and the `type()` builtin have four different spellings of the same concept (`Types`, `TObj`, `type_name`, `class_name`, `clr_type_name`, `Kind::name`) | `rust/src/runtime/obj.rs:989-1023`, `rust/src/runtime/collections.rs:1410-1433`, `rust/src/runtime/iters.rs:331-339` |
| S4-12 | `test/unit/parser/precedence.un` comments contradicted the implementation; the comments now match (power is left-associative, unary minus binds tighter than `**`) — **fixed** | `test/unit/parser/precedence.un:20-22` |

## 3. What the audit explicitly did **not** change

- **Opcode values.** `Op` is `#[repr(u8)]` and `.unbc` files depend on the
  values. The frozen contract test `opcode_values_are_the_contract`
  (`rust/src/bytecode/instr.rs:300-338`) hard-codes every discriminant.
  Eight opcodes (`LoadUpv`, `StoreUpv`, `Dup2`, `Rot`, `PopN`, `Tag`,
  `FlushDefers`, `FlushUsings`) are contract-present but never emitted; they
  were left in place because removing them would break the ABI.
- **.NET-derived error strings.** They are asserted by the negative-test
  manifest; changing them would be a user-visible change, not a cleanup.
- **`docs/project/DECISIONS.md` history.** Decisions are marked superseded, never
  deleted.
- **The user's uncommitted class-layout work.** It was kept, hardened and
  documented rather than deleted (see `docs/project/REFACTOR_LOG.md`).

## 4. Verification method used for the dead-code census

Every "dead" claim was checked with a repository-wide grep for the symbol and
its call syntax, restricted to `rust/src` and `rust/playground`, and the count
was re-verified from the working tree:

```powershell
cd rust\src
(Select-String -Path *.rs,*/*.rs -Pattern '\.text\(\)' | Measure-Object).Count   # 0
(Select-String -Path *.rs,*/*.rs -Pattern 'Token::error' | Measure-Object).Count # 0
(Select-String -Path *.rs,*/*.rs -Pattern 'opcode_of' | Measure-Object).Count    # 1 (definition)
(Select-String -Path *.rs,*/*.rs -Pattern 'NodeKind::Go' | Measure-Object).Count # 0
```

A symbol with only its definition and no call site is reported as dead; a
symbol with exactly one occurrence in its own definition file is reported as
"definition only".

## 5. Test and verification tooling gaps

| Gap | Detail | Where |
| --- | --- | --- |
| `core` round-trip | `vm/tests.rs::an_unbc_round_trip_preserves_behavior` runs source directly and then `source → .unbc file → read → run`, and asserts identical stdout and identical outcome kinds (success and error). **79 files, 0 differences** reported by the differential runner for JIT on/off | `rust/src/vm/tests.rs`, `scripts/differential.ps1` |
| No JIT differential runner | `scripts/differential.ps1` now runs every test-manifest file with `UV_JIT=0` and with the default JIT and diffs stdout + exit code; **79 files, 0 differences** | `scripts/differential.ps1` |
| No JIT test corpus | no `.un` file under `test/` mentions the JIT, so JIT-compiled shapes are only covered incidentally | `test/**` |
| No CI | `.github/workflows/build.yml` builds with the GNU toolchain and runs `scripts/test.ps1` + `cargo test --release`; plus `rust-toolchain.toml` pins the toolchain | `.github/workflows/build.yml`, `rust-toolchain.toml` |
| `AGENTS.md` drift | counts and paths were stale or wrong (file/line numbers, the `MIGRATION_*.md` location, the "no external crates" claim, the `main.un` tracking claim, the missing JIT intrinsics). All were corrected to the measured state during this pass | `AGENTS.md` |
| `docs/project/DECISIONS.md` has duplicate IDs | `D-033` and `D-034` are each used twice | `docs/project/DECISIONS.md` — fixed in this pass: the second pair are now D-069/D-070 and `D-066` is reordered |

## 6. Result of this audit pass

(as of 2026-11-16/17. Several counts have since moved — the authoritative current figures are
maintained in `docs/guides/TEST_GUIDE.md` (Positive 48 files / 1570 passed, Negative 51 matched,
differential 99 files, roundtrip 48/48 + 42/42). The numbers below are kept as the historical
record of the audit run.)

- Build: `cargo build --release` → **exit 0, 0 warnings**.
- Tests: `scripts/test.ps1` → **Positive 40 files / 1391 passed / 0 failed /
  0 crashed**, Negative **39 matched / 0 mismatched / 0 documented divergences**,
  Known **0**, exit **0**.
- Rust unit tests: `cargo test --release` → **200 passed / 0 failed** (the
  jit.rs probe's 2 tests were deleted with the file).
- `scripts/differential.ps1` → **79 files, 0 differences** between `UV_JIT=0`
  and the default JIT.
- All six documented defects in `test/regression/defects.un` are fixed; the
  file moved from the `known` group to the `regression` group and now guards
  the fixed behaviour.
- Dead code removal and the `runtime/world.rs` split are done; tooling
  (CI workflow, toolchain pin, differential runner, archive move) is done.
  Nothing in the audit remains open; the residual design limitations are in
  `docs/project/TECHNICAL_DEBT.md` and the phases are in `docs/project/REFACTOR_LOG.md`.
- This document also records the follow-up depth audits of the **native/stdlib**
  layer and the **build/docs/workspace** layer (see S1-9…S1-21, S2-build):
  13 native-layer defects were fixed, the native-layer remaining issues are
  recorded in `docs/project/TECHNICAL_DEBT.md`, and the build/docs tree findings are
  recorded as S2-build and in the decision log (D-067 through D-070).
