# TECHNICAL_DEBT.md

Register of the problems that are **known, accepted and not yet fixed**. Every
entry states the evidence, the risk it carries and what closing it would
cost. Items that were fixed during the 2026-11-16 audit pass are listed in
`REFACTOR_LOG.md`, not here.

Severity:

- **critical** — can produce a wrong result, a crash, or memory unsafety.
- **high** — leaks, unbounded growth, or a correctness risk under load.
- **medium** — measurable cost, missing verification, or duplicated logic.
- **low** — cosmetic or documentation-level.

| ID | Severity | Summary | Where |
| --- | --- | --- | --- |
| TD-01 | resolved | The tag contract now has a single home: `jit_cache::TAG_INT/TAG_FLOAT/TAG_BAIL/TAG_BOOL`. `vm/mod.rs` and `jit_fast.rs` reference the constants instead of repeating the numbers; an unknown tag in `jit_fast.rs` falls back to the interpreter instead of panicking | `rust/src/jit_cache.rs`, `rust/src/jit_fast.rs`, `rust/src/vm/mod.rs` — fixed 2026-11-16 |
| TD-02 | resolved | `jit_cache` is two-level (program_id → proto_index); there is no pointer-identity key, so a stale entry cannot be served to a program allocated at the same address. `clear_program(program_id)` removes one program's entries + native modules + lazy-compile marker; `clear()` removes everything (for tests / playground teardown). Growth is bounded by distinct program ids instead of pointer reuse | `rust/src/jit_cache.rs` — fixed 2026-11-16 |
| TD-03 | medium | `PLAN_CACHE` (thread-local) keeps every loaded `Rc<Program>` and `Rc<Plan>` alive for the process lifetime. Keyed by the same per-program id, which removes the stale-entry hazard — but the growth is unchanged; full spill/reload is a separate design problem | `rust/src/vm/mod.rs` |
| TD-04 | resolved | Compilation is now lazy: eligible protos are compiled on first call through `try_call_jit`, not eagerly at program load | `rust/src/vm/mod.rs` (plan_for), `rust/src/jit_fast.rs` (try_call_jit) — fixed 2026-11-16 |
| TD-05 | judged | Native code is never freed in normal runs (the vendored Cranelift backend `mem::forget`s allocations on drop). `clear_program(program_id)` and `clear()` now free it soundly and are exercised by tests (`vm::tests::a_cleared_jit_program_runs_identically`, `a_full_clear_lets_jit_run_again`). Whole-process boundary remains required | `rust/src/jit_cache.rs`, `rust/src/vm/tests.rs` |
| TD-06 | resolved | Class methods JIT-compile: the hidden `self` pointer occupies local slot 0, `self.<Int>` get/set is name-checked (member name at the layout index must match the expected field name AND be Int) before any read or write, and a mismatch bails to the interpreter (tag = 2). Float/Bool/None fields, constructors returning `none`, and methods building lists/dicts still bail as expected. No class-specific semantics were changed | `rust/src/jit_bc.rs`, `rust/src/runtime/obj.rs` — fixed 2026-11-16 |
| TD-07 | high | The `IntDict` arena only pushes (`alloc` never frees) and its `Mutex` can be poisoned by a panic inside JIT-executed code | `rust/src/runtime/int_dict.rs:35-39, 68, 73-95` |
| TD-08 | medium | `obj::invoke`/`obj::call` rebuild the argument vector with `args.to_vec()` on every call | `rust/src/runtime/obj.rs:515, 520` |
| TD-09 | medium | `scope::snapshot` deep-clones the whole scope chain for every `go` future | `rust/src/runtime/scope.rs:227-249`, `rust/src/vm/mod.rs:1225` |
| TD-10 | medium | `set_attr_from` clones the assigned value up to three times per assignment | `rust/src/runtime/obj.rs:743-770` |
| TD-11 | medium | `Source::get_line`/`get_column`/`get_line_start` scan the whole source from offset 0 | `rust/src/source.rs:38-62` |
| TD-12 | medium | `TryEntry.depth` is written but never read: the documented "registers are truncated on handler entry" invariant is not implemented | `rust/src/vm/mod.rs:91, 300-343` |
| TD-13 | resolved | The frontend now walks the AST once: `desugar` and `optimize` are merged into the optimizer's bottom-up pass, and the 64-pass fixed-point loop is gone — one pass reaches the fixpoint | `rust/src/optimize.rs`, `rust/src/desugar.rs` (deleted), `rust/src/runner.rs` — fixed 2026-11-16 |
| TD-14 | resolved (not a defect) | Verified by running a f-string with an interpolation error: the caret points at the correct column in the outer file. The compilation inspects sub-`Source` text only for the lexer; the expression parser receives the outer source and the byte ranges are relative to it, so text lookups read the correct buffer. `LangError::relocated` remains unused and is now deleted. | `rust/src/parser.rs:1098-1114`, `rust/src/error.rs:56-59` — re-classified 2026-11-16 |
| TD-15 | resolved | The `.unbc` round-trip behavioral check now exists — `vm::tests::an_unbc_round_trip_preserves_behavior` and `::an_unbc_round_trip_preserves_error_behavior` (source → `.unbc` → read → run must produce identical stdout, and identical error outcomes). The differential runner already covers JIT on/off | `rust/src/vm/tests.rs`, `scripts/differential.ps1` — fixed 2026-11-16 |
| TD-16 | medium | `plan_for` uses `.expect()`; it is safe only because every caller validates first, and the invariant is not enforced inside the function | `rust/src/vm/mod.rs:1670-1673` |
| TD-17 | medium | `transfer.rs` hard-codes opcode indices `744..=796` in the debug dump, which goes stale silently when the opcode table changes | `rust/src/vm/transfer.rs:240-244` |
| TD-18 | low | Five parallel representations of "type" (`Types`, `ObjData.{type_name,declared,types}`, `Payload::TObj`, three string vocabularies, the JIT's own layout keying) | `rust/src/runtime/types.rs`, `rust/src/runtime/obj.rs:153-163`, `rust/src/runtime/world/classes.rs` |
| TD-19 | low | `runtime/global.rs` was renamed to `runtime/modules.rs` (class seeding, module objects, import resolution); the `world/` split is in place | `rust/src/runtime/modules.rs` |
| TD-20 | low | Four "frame" types with the same word (`context::Frame`, `function::CallFrame`, `vm::ArgFrame`, `vm::TryEntry`) and two scope abstractions | `rust/src/runtime/context.rs:8-13`, `rust/src/runtime/function.rs:181-186`, `rust/src/vm/mod.rs:85-99` |
| TD-21 | low | Four independent equality implementations (`obj::key_eq`, `collections::element_eq`, `collections::stru_eq`, `obj::base_binop`) and two clone implementations (`obj::clone_obj`, `obj::copy_obj`) | `rust/src/runtime/obj.rs:278-289`, `rust/src/runtime/collections.rs:469-494, 1271-1296` |
| TD-22 | low | `Instance → member → self_ → instance` is a permanent `Rc` cycle (no `Weak` anywhere); `find_bound` writes `self_` on every lookup | `rust/src/runtime/obj.rs:445-456, 494-505` |
| TD-23 | low | The 18 dead `NodeKind` variants, the dead `TokenType::text()` table (116 arms) and the never-populated `Token.message` were removed (2026-11-16); the 22 never-lexed `TokenType` variants remain by deliberate choice (the `name()` display table references them) | `rust/src/ast.rs`, `rust/src/token.rs` |
| TD-24 | low | Dead `pub` items remaining: `scope::get_slot_name` stays (unreferenced, for the future slot-layout fast path). `Source::full_name`, `Source::len`, `Node::end`, `LangError::relocated`, `unbc::opcode_of`, `jit_cache::clear`, `world::current_program` (+ setter), `scope::declare`/`remove` (now `#[cfg(test)]`), `collections::is_single`, `iters::iter_copy`, `iters::next_of` were removed | `rust/src/runtime/scope.rs`, `rust/src/runtime/iters.rs`, `rust/src/bytecode/unbc.rs` |
| TD-25 | low | `rust/src/jit.rs` was deleted (2026-11-16); the Cranelift probe lives no more, and the public API exports `jit_bc`/`jit_cache`/`jit_fast` only | resolved; see REFACTOR_LOG pass 3 |
| TD-26 | low | C#-flavoured names survive in the Rust API (`collections::clr_to_string`, `collections::clr_type_name`) and the constructor family is inconsistent (`obj::obj_new`, `obj::plain`, `obj::int_obj`, `obj::bool_obj_new`, `obj::str_new`, `world::bool_obj`) | `rust/src/runtime/collections.rs:1400-1433`, `rust/src/runtime/obj.rs:179-217` |
| TD-27 | low | Error strings carry .NET exception names. **These are test-visible contract** (`test/negative/manifest.txt:19-21`) and must not be changed without updating the manifest and the docs | `rust/src/runtime/obj.rs`, `test/negative/manifest.txt` |
| TD-36 | high | The regex engine has no backtracking step/time budget: catastrophic patterns (`(a+)+$` on long runs) hang the process forever. Unlike the WS markers this is **deliberate .NET parity** (D-064) but unmitigated; every other input-driven native path is now capped | `rust/src/native/re/interp.rs` |
| TD-37 | medium | HTTP/WS have no connect or read timeout and no response-size cap; a hung or huge peer blocks the process or OOMs it; `https`/`wss` are accepted by the URI parser and then rejected with inconsistent messages; `post`/`put`/`patch`/`delete` never check the status code, and `set_header` compares names case-sensitively | `rust/src/native/net.rs:250-265`, `rust/src/native/http.rs`, `rust/src/native/ws.rs` |
| TD-38 | low | `http` `*_async` members are byte-identical aliases of the sync versions; there is no async anywhere (D-054/D-012) | `rust/src/native/http.rs:224-275` |
| TD-39 | medium | `atomic.*`/`pool.*`/`lock.*` are single-thread cooperative shims: `Rc<RefCell>` cells, a flag-not-a-mutex lock, a pool whose `worker_count` is stored but never used and whose `map` runs eagerly on the caller thread; atomic overflow wraps silently; the deterministic "concurrency" tests pass because of the scheduling model, not real parallelism (D-012/D-054/D-061) | `rust/src/native/atomic.rs`, `rust/src/native/flow.rs`, `rust/src/runtime/int_dict.rs` |
| TD-40 | medium | A Rust panic inside a native call is not caught by the VM and kills the worker thread with a raw Rust panic instead of an Un error. Largely capped now (allocation limits), but any remaining overflow site still escapes | `rust/src/main.rs`, `rust/src/vm/mod.rs` |
| TD-41 | low | `date.add_days` / `date.add_hours` / `time.from_epoch` / `time.from_epoch_ms` silently wrap i64 on huge inputs; `add_years` clamps Feb 29 correctly (D-068) but no test pins it | `rust/src/native/date_members.rs`, `rust/src/native/time.rs:89-104` |
| TD-42 | medium | Stream `"rw"` mode `read_line` reads bytes as Latin-1 chars (mangling UTF-8) and returns `None` on an empty line, conflating an empty line with EOF; the `"r"` path uses `BufReader` correctly | `rust/src/native/stream.rs:134-165` |
| TD-43 | low | `stopwatch.elapsed` returns a `date` holding the monotonic duration (not a timestamp); `start()` resets the timer instead of resuming | `rust/src/native/stopwatch.rs:174-180, 80-89` |
| TD-44 | low | `sys.version`/`os.version` return a fabricated `"Microsoft Windows NT 10.0.0"` regardless of the host | `rust/src/native/sys.rs:127-133`, `rust/src/native/os.rs:231-238` |
| TD-45 | low | `math.pow` saturates to `i64::MAX` on overflow for integer args; `math.wrap`/`abs` round-trip through f64; `trunc` always returns `int` (saturates); `round(1e30)` saturates; `hypot(1e200, 1e200)` → inf instead of using `f64::hypot`; `lcm` can overflow silently; `log`/`asin`/`acos` accept any base/domain | `rust/src/native/math.rs` |
| TD-46 | low | `iter.range(5, 0)` swaps the bounds instead of producing an empty (or erroring) result — silent non-Python, non-.NET semantics, undocumented | `rust/src/native/iter.rs:333-337` |
| TD-47 | low | `os.rs` and `sys.rs` are near-verbatim copies (env/pid/name/arch/version/cpuCount/is64bit/hostname/username/args/sep/pathsep/linesep/tickCount) that will drift; `io.write`/`io.read` declare unusable `stream`/`input`/`output` stub params | `rust/src/native/os.rs`, `rust/src/native/sys.rs`, `rust/src/native/io.rs` |
| TD-48 | low | `dict.add` on a duplicate key panics with the .NET `ArgumentException` — the message is byte-identical parity (D-024) but the path has no test | `rust/src/native/members.rs:1362-1370` |
| TD-49 | low | `exit()` with a non-int code silently exits 0; `ord` accepts multi-char strings and returns the first codepoint; `ascii` on an astral char errors while returning UTF-16 units elsewhere; `inspect.delattr` only removes own instance members | `rust/src/native/builtin.rs`, `rust/src/native/inspect.rs` |
| TD-50 | low | `hash` of a NaN key is unfindable (IEEE semantics, consistent with `.NET`); document, do not "fix" | `rust/src/runtime/obj.rs:299` |

**Resolved during this pass** (removed from the register): TD-28 (DECISIONS duplicate IDs `D-033`/`D-034` renumbered to D-069/D-070; `D-066` moved back into numeric order), TD-32 (the negative manifest no longer hard-codes the checkout path), TD-25 (`rust/src/jit.rs` deleted), TD-30 (`precedence.un` comments now match the implementation), TD-01 (JIT tag contract unified in `jit_cache`), TD-15 (.unbc round-trip tests added), TD-06 (class method JIT with name-checked layout intrinsic), TD-19/TD-24 partially (`runtime/world.rs` split into `runtime/world/`; the dead `pub` items removed), TD-33/TD-34/TD-35 (runner exit code, `docs/TEST_GUIDE.md`, README twins — now consistent with the current baseline), TD-29 (negative-suite vacuous checks repaired), TD-13 (single bottom-up optimizer pass), TD-14 (over-flagged by the audit; the interpolation parser reads the outer source and is correct), and the audit follow-ups recorded as D-067 through D-070.
| TD-29 | low | Known-divergent negative checks were stale: `depth_limit_crash.un` exited 0, `go_loop_var.un` exited 0 deterministically, and `order_crash.un` died on a missing `use io`. **Repaired 2026-11-16** — the vacuous files were deleted, coverage moved to `test/unit/native/list.un`, `test/unit/runtime/async.un`, `test/integration/recursion.un`, and `property_recursion.un` matches the current implementation (exit 1 + `maximum recur`) | `test/unit/native/list.un`, `test/unit/runtime/async.un`, `test/negative/manifest.txt` |
| TD-30 | resolved | `test/unit/parser/precedence.un` comments contradicted the assertions; the comments now match the implementation (power is left-associative, unary minus binds tighter than `**`) | `test/unit/parser/precedence.un:20-22` — fixed 2026-11-16 |
| TD-31 | low | Vendored Apache-2.0-WITH-LLVM-exception crates (`cranelift-jit`, `wasmtime-internal-jit-icache-coherence`) ship without their `LICENSE` files, and `vendor/README.md:17-18` claims they are present; no upstream revision is recorded for them | `vendor/README.md:17-18`, `vendor/cranelift-jit/`, `vendor/icache-coherence/` |
| TD-33 | resolved | `scripts/test.ps1` now exits 2 on an unknown `-Group` (prints the valid groups) and `-File` exits with the file's result code | `scripts/test.ps1` |
| TD-34 | resolved | `docs/TEST_GUIDE.md` now documents the current baseline (40 files / 1391 passed, negative 39 matched / 0 divergences, Known 0) | `docs/TEST_GUIDE.md` |
| TD-35 | resolved | `README.md` and `docs/README.md` both state the current baseline (40 files / 1389→1391 passed, 0 crashed, Known 0), the register-VM description (`vm/`, `transfer.rs`), the CLI list with `dis-reg`, `main.un` tracked, no "Val", and the current JIT description (sin + IntDict intrinsics) | `README.md`, `docs/README.md` |

## Notes on the highest-risk items

### TD-01 — the tag contract (resolved)

The tag contract has a single home: `jit_cache::TAG_INT`, `TAG_FLOAT`,
`TAG_BAIL`, `TAG_BOOL`. The old three-site description was unified on
2026-11-16: `vm/mod.rs` and `jit_fast.rs` name the constants rather than the
numbers, `jit_bc.rs` comments continue to describe the raw values, and an unknown
tag in `jit_fast.rs` falls back to the interpreter instead of `unreachable!`. The remaining step before classifying this phase done is the
producer-side check: `scripts/differential.ps1` already runs every manifest
file JIT on/off and would surface a producer emitting an undocumented tag.

### TD-06 — class methods in the JIT

The class-layout specialisation needs three things that do not exist yet:

1. A representation for an object receiver in the JIT value model (the
   receiver arrives as a raw `Rc` pointer, `jit_fast.rs:44`).
2. A proof that the pointer the JIT dereferences is the live receiver (the
   `Rc` must outlive the native call).
3. A layout whose index equals the instance member index — true only when
   `__init__` assigns each field exactly once, in order.

Until then `precheck_fn` rejects class methods, so the JIT never compiles
them and the interpreter always handles them.

### TD-15 — verification gaps

Two cheap, high-value additions:

- `scripts/differential.ps1`: run every manifest file twice (`UV_JIT=0` and
  the default) and diff stdout plus exit code.
- A `.unbc` round-trip test: compile → write → read → execute → compare with
  the directly executed program.
