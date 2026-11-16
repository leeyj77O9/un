# REFACTOR_PLAN.md

The phased plan that follows from `PROJECT_AUDIT.md`. Every phase has an
entry condition, a concrete change list and an exit condition that must be
verified with real commands — a phase is only closed when the exit condition
holds, and the numbers go into `REFACTOR_LOG.md`.

Ground rules for every phase:

1. `cargo build --release` must end with **0 errors and 0 new warnings**.
2. `cargo test --release` must stay green (202 tests at the time of writing).
3. `scripts/test.ps1` must stay green: Positive 40 files / 1391 passed /
   0 failed / 0 crashed, Negative 39 matched / 0 documented divergences,
   Known 0, exit 0.
4. `Op` discriminants never change (`.unbc` depends on them).
5. No new external crate; `vendor/` only when a raw-FFI workaround is needed.
6. The agent does not commit or push.

## Phase 0 — Audit and baseline (done)

- [x] Baseline measurements (build, tests, benchmarks) recorded.
- [x] Six independent depth audits (frontend, runtime, VM/bytecode, JIT,
      native, build/docs/workspace) consolidated into `PROJECT_AUDIT.md`.
- [x] `docs/DECISIONS.md` duplicate IDs `D-033`/`D-034` — see Phase 7.

Exit: baseline numbers recorded, audit document written. **Done.**

## Phase 1 — Correctness defects (done)

- [x] Standard slice semantics for `list`/`tuple`/`str` (shared
      `collections::slice_indices` + `slice_positions` helpers).
- [x] `upper_bound` returns a true insertion point.
- [x] `set`/`dict` equality.
- [x] `in` on lists/tuples with scalar operands.
- [x] `is`/`is not` semantics for type objects.
- [x] `test/regression/defects.un` moved to the `regression` group;
      `test/KnownFailures.txt` emptied.
- [x] `README.md` / `docs/README.md` document the new semantics.
- [x] `docs/DECISIONS.md` gains D-067; D-027/D-035 marked superseded.

Exit: 40 files / 1391 passed / 0 failed, 202 Rust tests green. **Done.**

## Phase 2 — JIT safety hardening (done, partial)

- [x] `un_instance_int_get` / `un_instance_int_set` report a status instead of
      silently returning `0` or dropping the write.
- [x] `un_instance_int_ok` added so a miss is distinguishable from a `0` value.
- [x] The JIT bails (tag=2) on a miss instead of continuing with a wrong value.
- [x] `unsafe extern "C"` bodies wrapped in `catch_unwind` so a `RefCell`
      panic cannot unwind into native code.
- [x] Class methods rejected by `precheck_fn` until the JIT value model can
      carry an object receiver (removes the arity mismatch and the
      receiver-identification bug).
- [x] Class field layout no longer contains method names, and `__init__`
      fields are ordered first so the layout index matches the instance
      member order.
- [x] The runtime layout refresh (`register_class_field_layout`) removed; the
      layout is seeded once per program.
- [ ] `int_dict` arena calls wrapped in `catch_unwind` (TD-07).
- [ ] Unified JIT tag contract documented in one place (TD-04).

Exit: no JIT path can produce a silently wrong value; every fallback is
explicit. **Partially done — remaining items are in `TECHNICAL_DEBT.md`.**

## Phase 3 — Performance: hot paths (partially done)

- [x] `UV_JIT_DEBUG` environment lookup cached (was per emitted instruction).
- [x] Class layout recomputation removed (was per class registration).
- [ ] `obj::invoke`/`obj::call`: stop rebuilding the argument vector.
- [ ] `scope::snapshot`: avoid deep-cloning the scope chain per `go` future.
- [ ] `set_attr_from`: drop the redundant value clones.
- [ ] `Source` line/column lookup: index the source once instead of scanning.

Each item needs a before/after benchmark number in `REFACTOR_LOG.md`.

## Phase 4 — Dead code removal and naming

- [x] Delete the 18 dead `NodeKind` variants (the 14 compound-assignment
      kinds, `Go`, `Argument`, `Alias`, `Target`). The 22 never-lexed
      `TokenType` variants are kept: they are still referenced by the
      `name()` display table and the opcode/lexer tables depend on the
      enum's shape.
- [x] Delete `TokenType::text()` (116 dead arms) and `Token::error`; drop
      `Token.message`.
- [x] Delete the dead `pub` items listed in `PROJECT_AUDIT.md` S4-5
      (`Source::full_name`, `Source::len`, `Node::end`, `LangError::relocated`,
      `unbc::opcode_of`, `jit_cache::clear`, `world::current_program` +
      `set_current_program`, `scope::remove`, `collections::is_single`,
      `iters::iter_copy`, `iters::next_of`, `scope::get_slot_name`).
      `scope::declare` had test-only callers; it stays as a
      `#[cfg(test)]` gated `pub(crate)` helper.
- [x] Delete `rust/src/jit.rs` (test-only probe in the public API).
- [ ] Rename the remaining C#-flavoured names: `collections::clr_to_string`,
      `collections::clr_type_name` → `display`/`type_name`; `obj::obj_new` /
      `obj::bool_obj_new` / `obj::str_new` → one consistent family.
- [ ] Unify the "type" vocabulary: one canonical `Types` representation, one
      `type_name` accessor, one registry.

## Phase 5 — Runtime restructuring (aggressive, as chosen)

- [x] Split `runtime/world.rs` (the god module) into `world/`:
      `world/mod.rs` (struct + `WORLD` + `reset`), `world/caches.rs`
      (interned type names, scalar caches), `world/classes.rs` (class
      registries + JIT layout seeds), `world/modules.rs` (import cache,
      loading set, path, global scope, native registry), `world/io.rs`
      (stdout/log/stdin, sandbox allow-list, exit gate, EOF), `world/depth.rs`
      (recursion limit), `world/executors.rs` (executor hooks). Callers
      unchanged — everything re-exported through `world::`.
- [x] Move `runtime/global.rs` into `runtime/modules.rs` (class seeding,
      module objects, import resolution). The module directory
      `runtime/modules/` holds the test module.
- [ ] Collapse the two scope abstractions (VM frame registers + lexical scope
      chain) and the four "frame" types (`context::Frame`, `function::CallFrame`,
      `vm::ArgFrame`, `vm::TryEntry`).
- [ ] Give the four duplicate equality implementations one owner
      (`obj::key_eq` / `collections::element_eq` / `collections::stru_eq`).
- [ ] Merge `obj::clone_obj` and `obj::copy_obj`.

## Phase 6 — Frontend streamlining

- [x] Fold `desugar` into the optimizer's bottom-up pass
      (`desugar_local`); the pipeline is now
      `parse → optimize (desugar + fold) → compile`.
      `desugar.rs` was deleted.
- [x] Replace the fixed-point loop in `optimize` with a single bottom-up
      pass: the fold is bottom-up, so one pass reaches the fixpoint; the
      64-pass loop and its `MAX_PASSES` panic are gone.
      (A subtly missing `changed` propagation after desugar substitution was
      found by this rewrite and fixed in the same commit.)
- [x] Fix the f-string interpolation source handling: verified by running
      interpolation-error probes — the caret points at the correct column in
      the outer file. (The audit over-flagged this; the nested parser
      already received the outer `Rc<Source>` and the byte ranges are
      relative to it.) Classified as **not a defect** (TD-14) and removed.
- [x] Fix `test/unit/parser/precedence.un` comments (the assertions are
      correct, the comments were not) — left-associative `**` and unary
      minus binds tighter.

## Phase 7 — Build, docs and workspace

- [x] Fix the duplicate `D-033`/`D-034` entries in `docs/DECISIONS.md`
      (renumbered the second pair to D-069/D-070; `D-066` reordered).
- [x] Add `rust-toolchain.toml` pinning `stable-x86_64-pc-windows-gnu`.
- [x] Add a CI workflow that builds and runs `scripts/test.ps1`
      (`.github/workflows/build.yml`) — added; runs on `windows-latest` but
      not yet executed by GitHub.
- [x] Add `scripts/differential.ps1`: run every manifest file twice
      (`UV_JIT=0` vs default) and diff stdout + exit code. Currently
      **79 files, 0 differences**.
- [x] Update `AGENTS.md`: file/line counts, test baseline, JIT description,
      tree listing, build/test toolchain and the new runner commands.
- [x] Move the historical VM/JIT documents to `docs/archive/`
      (`MIGRATION_AUDIT.md`, `MIGRATION_PROGRESS.md`, `REGISTER_VM_ROADMAP.md`).
- [ ] Add a `.unbc` round-trip test (compile → write → read → run → compare).
- [ ] Decide whether `bench/results/` stays committed.

## Phase 8 — JIT rework (deferred, tracked in `TECHNICAL_DEBT.md`)

- [ ] Teach the JIT value model about object receivers so class methods can
      be compiled again.
- [ ] Add a hotness counter and compile lazily instead of eagerly at load.
- [ ] Add eviction/invalidation for `jit_cache` and `PLAN_CACHE`.
- [ ] Free native code when a program is unloaded (or document the leak).
- [ ] Extend the intrinsic coverage (Float compare, `math.sin`, IntDict with
      entries, string ops).
