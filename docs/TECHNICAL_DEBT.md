# docs/TECHNICAL_DEBT.md

Register of the problems that are **known, accepted and not yet fixed**. Every
entry states the risk it carries and where it lives.

Severity:

- **high** — leaks, unbounded growth, or a correctness risk under load.
- **medium** — measurable cost, missing verification, or duplicated logic.
- **low** — cosmetic or documentation-level.

| ID | Severity | Summary | Where |
| --- | --- | --- | --- |
| TD-03 | medium | `PLAN_CACHE` (thread-local) keeps every loaded `Rc<Program>` and `Rc<Plan>` alive for the process lifetime. Keyed by the opaque `Program.id`, so a pointer collision cannot serve a stale plan — but the growth is unchanged; full spill/reload is a separate design problem | `rust/src/vm/mod.rs` |
| TD-05 | high | Native code is never freed in normal runs (the vendored Cranelift backend `mem::forget`s allocations on drop). `clear_program(program_id)` / `clear()` now free it soundly and are exercised by tests — but a whole-process boundary is required | `rust/src/jit_cache.rs`, `rust/src/vm/tests.rs` |
| TD-08 | medium | `obj::invoke`/`obj::call` rebuild the argument vector with `args.to_vec()` on every call | `rust/src/runtime/obj.rs:515, 520` |
| TD-09 | medium | `scope::snapshot` deep-clones the whole scope chain for every `go` future | `rust/src/runtime/scope.rs:227-249`, `rust/src/vm/mod.rs:1225` |
| TD-10 | medium | `set_attr_from` clones the assigned value up to three times per assignment | `rust/src/runtime/obj.rs:743-770` |
| TD-11 | medium | `Source::get_line`/`get_column`/`get_line_start` scan the whole source from offset 0 | `rust/src/source.rs:38-62` |
| TD-16 | medium | `plan_for` uses `.expect()`; it is safe only because every caller validates first, and the invariant is not enforced inside the function | `rust/src/vm/mod.rs:1670-1673` |
| TD-17 | medium | `transfer.rs` hard-codes opcode indices `744..=796` in the debug dump, which goes stale silently when the opcode table changes | `rust/src/vm/transfer.rs:240-244` |
| TD-39 | medium | `atomic.*`/`pool.*`/`lock.*` are single-thread cooperative shims: `Rc<RefCell>` cells, a flag-not-a-mutex lock, a pool whose `worker_count` is stored but never used and whose `map` runs eagerly on the caller thread; atomic overflow wraps silently (D-012/D-054/D-061) | `rust/src/native/atomic.rs`, `rust/src/native/flow.rs`, `rust/src/runtime/int_dict.rs` |
| TD-40 | medium | A Rust panic inside a native call is not caught by the VM and kills the worker thread with a raw Rust panic instead of an Un error. Largely capped now (allocation limits), but any remaining overflow site still escapes | `rust/src/main.rs`, `rust/src/vm/mod.rs` |
| TD-18 | low | Five parallel representations of "type" (`Types`, `ObjData.{type_name,declared,types}`, `Payload::TObj`, three string vocabularies, the JIT's own layout keying) | `rust/src/runtime/types.rs`, `rust/src/runtime/obj.rs:153-163`, `rust/src/runtime/world/classes.rs` |
| TD-20 | low | Four "frame" types with the same word (`context::Frame`, `function::CallFrame`, `vm::ArgFrame`, `vm::TryEntry`) and two scope abstractions | `rust/src/runtime/context.rs:8-13`, `rust/src/runtime/function.rs:181-186`, `rust/src/vm/mod.rs:85-99` |
| TD-21 | low | Four independent equality implementations (`obj::key_eq`, `collections::element_eq`, `collections::stru_eq`, `obj::base_binop`) and two clone implementations (`obj::clone_obj`, `obj::copy_obj`) | `rust/src/runtime/obj.rs:278-289`, `rust/src/runtime/collections.rs:469-494, 1271-1296` |
| TD-22 | low | `Instance → member → self_ → instance` is a permanent `Rc` cycle (no `Weak` anywhere); `find_bound` writes `self_` on every lookup | `rust/src/runtime/obj.rs:445-456, 494-505` |
| TD-24 | low | `scope::get_slot_name` stays (unreferenced, reserved for the future slot-layout fast path). The other dead `pub` items were removed 2026-11-16 | `rust/src/runtime/scope.rs` |
| TD-26 | low | C#-flavoured names survive in the Rust API (`collections::clr_to_string`, `collections::clr_type_name`) and the constructor family is inconsistent (`obj::obj_new`, `obj::plain`, `obj::int_obj`, `obj::bool_obj_new`, `obj::str_new`, `world::bool_obj`) | `rust/src/runtime/collections.rs:1400-1433`, `rust/src/runtime/obj.rs:179-217` |
| TD-27 | low | Error strings carry .NET exception names. **These are test-visible contract** (`test/negative/manifest.txt:19-21`) and must not be changed without updating the manifest and the docs | `rust/src/runtime/obj.rs`, `test/negative/manifest.txt` |
| TD-31 | low | Vendored Apache-2.0-WITH-LLVM-exception crates (`cranelift-jit`, `wasmtime-internal-jit-icache-coherence`) ship without their `LICENSE` files, and `vendor/README.md:17-18` claims they are present; no upstream revision is recorded for them | `vendor/README.md:17-18`, `vendor/cranelift-jit/`, `vendor/icache-coherence/` |
| TD-38 | low | `http` `*_async` members are byte-identical aliases of the sync versions; there is no async anywhere (D-054/D-012) | `rust/src/native/http.rs:224-275` |
| TD-41 | low | `date.add_days` / `date.add_hours` / `time.from_epoch` / `time.from_epoch_ms` silently wrap i64 on huge inputs; `add_years` clamps Feb 29 correctly (D-068) but no test pins it | `rust/src/native/date_members.rs`, `rust/src/native/time.rs:89-104` |
| TD-43 | low | `stopwatch.elapsed` returns a `date` holding the monotonic duration (not a timestamp); `start()` resets the timer instead of resuming | `rust/src/native/stopwatch.rs:174-180, 80-89` |
| TD-44 | low | `sys.version`/`os.version` return a fabricated `"Microsoft Windows NT 10.0.0"` regardless of the host | `rust/src/native/sys.rs:127-133`, `rust/src/native/os.rs:231-238` |
| TD-45 | low | `math.pow` saturates to `i64::MAX` on overflow for integer args; `math.wrap`/`abs` round-trip through f64; `trunc` always returns `int` (saturates); `round(1e30)` saturates; `hypot(1e200, 1e200)` → inf instead of using `f64::hypot`; `lcm` can overflow silently; `log`/`asin`/`acos` accept any base/domain | `rust/src/native/math.rs` |
| TD-47 | low | `os.rs` and `sys.rs` are near-verbatim copies (env/pid/name/arch/version/cpuCount/is64bit/hostname/username/args/sep/pathsep/linesep/tickCount) that will drift; `io.write`/`io.read` declare unusable `stream`/`input`/`output` stub params | `rust/src/native/os.rs`, `rust/src/native/sys.rs`, `rust/src/native/io.rs` |
| TD-48 | low | `dict.add` on a duplicate key panics with the .NET `ArgumentException` — the message is byte-identical parity (D-024) and has no test | `rust/src/native/members.rs:1362-1370` |
| TD-49 | low | `exit()` with a non-int code silently exits 0; `ord` accepts multi-char strings and returns the first codepoint; `ascii` on an astral char errors while returning UTF-16 units elsewhere; `inspect.delattr` only removes own instance members | `rust/src/native/builtin.rs`, `rust/src/native/inspect.rs` |
| TD-50 | low | `hash` of a NaN key is unfindable (IEEE semantics, consistent with `.NET`); document, do not "fix" | `rust/src/runtime/obj.rs:299` |

## Resolved in 2026-11

Closed during the audit/refactor passes (see `docs/REFACTOR_LOG.md` for the
evidence and `docs/DECISIONS.md` for the semantics decisions):
TD-01, TD-02, TD-04, TD-06, TD-07, TD-12, TD-13, TD-14, TD-15, TD-19, TD-23,
TD-25, TD-28, TD-29, TD-30, TD-32, TD-33, TD-34, TD-35, TD-36, TD-37, TD-42, TD-46.

Closed-out notes: TD-37 kept the deliberate API shape (4xx/5xx responses
return the body, matching the .NET reference — the Un http client does not
throw on unsuccessful status); the connect/read/write timeouts, the
64 MiB response cap with Content-Length validation, the `http` scheme-guard
message, and case-insensitive `set_header` names were the defects and are
fixed. TD-36's budget is per engine attempt (10M dispatch steps) and never
reports a negative match.
