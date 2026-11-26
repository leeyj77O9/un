# docs/project/TECHNICAL_DEBT.md

Register of known technical debt. Most entries were resolved or mitigated in
2026-11; the **Status** column is authoritative. Historical summaries are kept
so the reason for each decision remains auditable.

Severity:

- **high** — leaks, unbounded growth, or a correctness risk under load.
- **medium** — measurable cost, missing verification, or duplicated logic.
- **low** — cosmetic or documentation-level.

| ID | Severity | Summary | Where | Status |
| --- | --- | --- | --- | --- |
| TD-03 | medium | `PLAN_CACHE` (thread-local) keeps every loaded `Rc<Program>` and `Rc<Plan>` alive for the process lifetime. Keyed by the opaque `Program.id`, so a pointer collision cannot serve a stale plan — but the growth is unchanged; full spill/reload is a separate design problem | `rust/src/vm/mod.rs` | **Closed by contract (D-093 + D-094, 2026-11-24)**: bounded to 64 FIFO; no in-run reclamation by default. Opt-in `module_cache_remove`/`clear` now calls `vm::clear_plan_cache(program_id)` alongside JIT cleanup and unregisters module-registered classes (before/after diff tracking); object memory held by intentional cycles stays a bounded leak. API is opt-in and not exposed to the language/C ABI. |
| TD-05 | high | Native code is never freed in normal runs (the vendored Cranelift backend `mem::forget`s allocations on drop). `clear_program(program_id)` / `clear()` now free it soundly and are exercised by tests — but a whole-process boundary is required | `rust/src/jit_cache.rs`, `rust/src/vm/tests.rs` | **Resolved (2026-11-19)**: `clear()` is now wired at both CLI process-exit boundaries (`main()` on success, `report()` on any error/panic path) and at the end of every WASM playground run (each run compiles a fresh `Program` with a fresh id, so its JIT entries can never be reused — they are freed, not leaked). The residual in-flight window (raw native pointers may sit on the stack of a running JIT frame) is inherent to the vendored backend's deliberate leak-safety and is documented in `jit_cache.rs` and the ARCHITECTURE lifecycle section; no correctness exposure |
| TD-08 | medium | `obj::invoke`/`obj::call` rebuild the argument vector with `args.to_vec()` on every call | `rust/src/runtime/obj.rs:515, 520` | **Mitigated (2026-11-19)**: positional calls no longer allocate the per-argument names bookkeeping (`Args::positional` allocates a single empty `names` Vec instead of `n` empty Strings); the remaining clone of `args.values` (the `to_vec`) is intrinsic to `fn_call` taking ownership, and eliminating it is API-breaking across `invoke`/`call_args` — stays as documented debt. A/B against the pre-change baseline on `bench/funccall.un` shows no measurable regression |
| TD-09 | medium | scope::snapshot deep-clones the whole scope chain for every go future | rust/src/runtime/scope.rs:227-249, rust/src/vm/mod.rs:1225 | **Closed by design (D-080, 2026-11-19)**: fork-time capture-by-value is Un's contract; measured 200k forks ~1,058 ms with snapshot cost ~1-3 us, lost in dispatch. fork-on-write / compile-time capture-set / shallow-share rejected (semantics vs. ROI). |
| TD-10 | medium | `set_attr_from` clones the assigned value up to three times per assignment | `rust/src/runtime/obj.rs:743-770` | **Evidence-refresh (2026-11-19)**: `Obj = Rc<RefCell<ObjData>>`, so every clone here is a refcount bump, no deep data copy. The residual is inherent to the store needing its own `Rc`; the last return clone can only be eliminated with path-leaking API changes for ~1 refcount bump. Not restructured. |
| TD-11 | medium | `Source::get_line`/`get_column`/`get_line_start` scan the whole source from offset 0 | `rust/src/source.rs:38-62` | **Resolved (2026-11-19)**: a char-offset line-start index is built once at construction (post CRLF normalisation); all three accessors are O(log lines) via `binary_search`. Verified with the full gate battery (Positive 1493, Negative 41 matched, differential 88/0, cargo test 205) |
| TD-16 | medium | `plan_for` uses `.expect()`; it is safe only because every caller validates first, and the invariant is not enforced inside the function | `rust/src/vm/mod.rs:1670-1673` | **Resolved (2026-11-19)**: every acquisition of a plan is now inside `catch_unwind` — `VM::new` (both the proto-executor closure and the outer `plan_for`), and `VM::with_context` (module path, returning `Result<Self, String>`; the module loader maps it to `ModuleError::Panic`). Any invariant violation now surfaces as an Un panic object that reaches the normal error channels instead of a raw process abort |
| TD-17 | medium | `transfer.rs` hard-codes opcode indices `744..=796` in the debug dump, which goes stale silently when the opcode table changes — and could panic with an out-of-bounds index on short protos | `rust/src/vm/transfer.rs:240-244` | **Resolved (2026-11-19)**: the dump now prints a bounded ±32-instruction neighbourhood of the offending `ip`, clamped to `proto.code.len()` — always in bounds, always local to the mismatch |
| TD-39 | medium | atomic.*/pool.*/lock.* are single-thread cooperative shims (Rc<RefCell> cells, flag-not-a-mutex, inline pool.map). Documented truthfully and hardened to wrapping arithmetic (matches Un Int); atomic backed by RefCell/documented instead of Interlocked claim | rust/src/native/atomic.rs, rust/src/native/flow.rs | **Resolved (2026-11-19)**: dd/inc/dec wrapping_add/sub (was debug-panic); doc syncs to RefCell/cooperative-single-thread semantics. pool/lock remain cooperative shims by design (D-054/D-061). **(2026-11-27 각주, D-099)** 기본 경로는 deferred cooperative 유지, **`un build --parallel`(plain exe 전용)에서만 OS 스레드** 사용 — `--embed`와 상호배제. opt-in documented divergence/잠재 이슈는 TD-54/TD-55/TD-56 |
| TD-40 | medium | A Rust panic inside a native call is not caught by the VM and kills the worker thread with a raw Rust panic instead of an Un error. Largely capped now (allocation limits), but any remaining overflow site still escapes | `rust/src/main.rs`, `rust/src/vm/mod.rs` | **Resolved (2026-11-19)**: audit found the surviving escapes — plan acquisition before the first step (`VM::new`, `VM::with_context`) and the proto executor closure in `VM::new`. All three are now wrapped in `catch_unwind` and produce Un panic objects (`Outcome::Panic` / `Result<_, String>` / `ModuleError::Panic`). Combined with the existing step-level `catch_unwind` (`vm/mod.rs:219`) and JIT-side guards, no known path lets a raw Rust panic escape the process boundary; main stays alive and the error prints via `report()` with exit 1 |
| TD-18 | low | Types / ObjData.{type_name,declared,types} / Payload::TObj / three string vocabularies / JIT layout keying were five parallel type representations | rust/src/runtime/types.rs, rust/src/runtime/obj.rs:153-163, rust/src/runtime/world/classes.rs | **Resolved (2026-11-19)**: \declared\ deleted (dead), \	ypes\ lazily computed on first \x is T\ read, interned-identity comparisons via Rc::ptr_eq across Obj/Stru/Enu sites; remaining hierarchies (\Types\ vs \Payload::TObj\, \class_layouts\ name keying) documented in the since-removed docs/REFACTOR_TYPES.md (git history). |
| TD-20 | low | Four "frame" types with the same word (`context::Frame`, `function::CallFrame`, `vm::ArgFrame`, `vm::TryEntry`) and two scope abstractions | `rust/src/runtime/context.rs:8-13`, `rust/src/runtime/function.rs:181-186`, `rust/src/vm/mod.rs:85-99` |
| TD-21 | low | Four independent equality implementations (`obj::key_eq`, `collections::element_eq`, `collections::stru_eq`, `obj::base_binop`) and two clone implementations (`obj::clone_obj`, `obj::copy_obj`) | `rust/src/runtime/obj.rs:278-289`, `rust/src/runtime/collections.rs:469-494, 1271-1296` |
| TD-22 | low | Instance -> member -> self_ -> instance is a permanent Rc cycle (no Weak anywhere); find_bound writes self_ on every lookup | rust/src/runtime/obj.rs:445-456, 494-505 | **Closed by contract (D-093, 2026-11-24)**: bounded intentional cycle. `self_`/closure strong refs are required by bound-method lifetime (`let f = obj.m; obj = null; f()`) and lambda escape (D-080); Weak would change semantics and risk JIT raw-receiver UAF. Reclaimed at each `world::reset()` / process exit. GC and Weak conversion explicitly rejected. |
| TD-24 | low | Removed; scope.rs no longer exposes the dead pub get_slot_name. Dead code sweeps done 2026-11-19. | rust/src/runtime/scope.rs | **Resolved (2026-11-19)**: the only dead export flagged by the 2026 audit no longer exists. |
| TD-26 | low | C#-flavoured names in the Rust API | rust/src/runtime/collections.rs:1400-1433, rust/src/runtime/obj.rs:179-217 | **Resolved (2026-11-19)**: collections.rs Clr names renamed to obj_to_string / obj_type_name. Constructor family naming (plain / int_obj / bool_obj_new / str_new / world::bool_obj) kept ??rename blast radius too large for low-severity debt and docs already say construction is layered. |
| TD-27 | low | Error strings carried .NET exception names. **These were test-visible contract** (`test/negative/manifest.txt`) and were changed together with the manifest and docs | `rust/src/runtime/obj.rs`, `test/negative/manifest.txt` | **Resolved — superseded by D-092**: built-in panics now print `error: ...`; no `System.*` name remains in the contract |
| TD-31 | low | Vendored Apache-2.0-WITH-LLVM-exception crates (`cranelift-jit`, `wasmtime-internal-jit-icache-coherence`) ship without their `LICENSE` files, and `vendor/README.md:17-18` claims they are present; no upstream revision is recorded for them | `vendor/README.md:17-18`, `vendor/cranelift-jit/`, `vendor/icache-coherence/` | **Resolved (2026-11-19)**: upstream `LICENSE` (Apache-2.0 + LLVM exception, verbatim) shipped in both crate directories; `vendor/README.md` and `vendor/THIRD_PARTY.md` updated to say so. |
| TD-38 | low | http.*_async were byte-identical aliases of the sync versions | rust/src/native/http.rs:224-275 | **Resolved (2026-11-19)**: aliases removed (D-054 ??Un is cooperative, there is no async). test/unit/native/http.un updated. |
| TD-41 | low | Checked i64 arithmetic on all date/time paths ??add_days/hours/minutes/seconds/ms, add_years, add_months, time.from_epoch(_ms) return a catchable 'date arithmetic overflowed' error instead of silent wrap | rust/src/native/date_members.rs, rust/src/native/time.rs | **Resolved (2026-11-19)**: checked_ticks helper; add_years Feb-29 clamp already pinned; pinned by assert_err cases in test/unit/runtime/date.un and test/unit/native/time.un |
| TD-43 | low | `stopwatch.elapsed` returns a `date` holding the monotonic duration (not a timestamp); `start()` resets the timer instead of resuming | `rust/src/native/stopwatch.rs:174-180, 80-89` | **Resolved (2026-11-19)**: behaviour kept and documented in README.md / docs/README.ko.md ("Semantic notes" section) |
| TD-44 | low | `sys.version`/`os.version` return a fabricated `"Microsoft Windows NT 10.0.0"` regardless of the host | `rust/src/native/sys.rs:127-133`, `rust/src/native/os.rs:231-238` | **Resolved (2026-11-19)**: `sys.version()` now returns `"Un <crate version>"` (like `sys.version`), and `os.version()` returns `"<OS family> (Un <version>)"` — honest host self-report, no fabricated OS identity. |
| TD-45 | low | math.pow/abs/gcd at the i64 boundary: pow at the exact 2^63 boundary returned the saturated Int (9223372036854775807) instead of 9223372036854775808.0 ??the pinning test passed vacuously against the lossy Int->Float comparison. math.abs(i64::MIN) returned i64::MAX. math.gcd(MIN, ...) could go negative (lcm was already correct). hypot fixed to f64::hypot (2026-11-18); lcm overflows to a catchable error | rust/src/native/math.rs | **Resolved (2026-11-19)**: pow returns the exact float at the 2^63 boundary (regression-pinned via the type() check in math.un); abs/gcd handle i64::MIN via unsigned_abs / float fallback; pinned in test/unit/native/math.un |
| TD-47 | low | `os.rs` and `sys.rs` share overlapping helper bodies (name/arch/cpuCount/is64bit/version/hostname/username/tickCount) that could drift; `io.write`/`io.read` declare unusable `stream`/`input`/`output` stub params | `rust/src/native/os.rs`, `rust/src/native/sys.rs`, `rust/src/native/io.rs` | **Evidence-refresh (2026-11-19)**: the "near-verbatim copies" claim is overstated — they meaningfully differ (`sys` exposes getcalldepth/recursionlimit/args; `os` exposes setenv/unsetenv/tickCount/exec/etc.). Dedup is not worth it; only the overlapping helpers could be factored. Won't-fix this pass. |
| TD-48 | low | dict.add on a duplicate key panics with the .NET ArgumentException ??the message is byte-identical parity (D-024) and has no test | rust/src/native/members.rs:1362-1370 | **Resolved (2026-11-19)**: behaviour kept by design (D-024); pinned by test/negative/dict_add_duplicate.un (exit 1 + System.ArgumentException). **Renamed (2026-11-20, D-092)**: `System.*` naming removed; built-in panics print `error: ...`; manifest substring updated |
| TD-49 | low | exit() with a non-int code silently exits 0; ord accepts multi-char strings and returns the first codepoint; ascii on an astral char errors while returning UTF-16 units elsewhere; inspect.delattr only removes own instance members | rust/src/native/builtin.rs, rust/src/native/codes.rs, rust/src/native/inspect.rs | **Resolved (2026-11-19)**: exit() non-int returns a catchable error (pinned in errors.un); ord accepts exactly one char (pinned by test/negative/ord_multiple_chars.un + suite tests); inspect.delattr documents its own-instance scope; astral ascii keeps UTF-16 units (parity, untouched) |
| TD-50 | low | `hash` of a NaN key is unfindable (IEEE semantics, consistent with `.NET`); document, do not "fix" | `rust/src/runtime/obj.rs:299` | **Resolved (2026-11-19)**: documented in README.md / docs/README.ko.md ("Semantic notes" section); behaviour kept (.NET / IEEE parity) |
| TD-53 | low | The Int-JIT does not lower unary `Op::Neg` — any function whose body contains one (e.g. an unary minus on a constant) is rejected and falls back to the interpreter; `0.0 - x` (fsub) works fine and is what the Float strict canary uses | `rust/src/jit_bc.rs` (no `Op::Neg` arm) | **Resolved (2026-11-19)**: `Op::Neg` now lowers Int args to `ineg` (wrapping, matching the interpreter's `wrapping_neg`) and Float args to `fneg`; Bool/none/arena handles bail to the interpreter (which raises the exact error). The Float strict canary now uses `-0.5` / `-7` to keep the arm CI-verified. |
| TD-51 | medium | JIT native emission can fail at any stage (frontend debug asserts, Cranelift verifier, `catch_unwind`) and every failure was silently swallowed into an interpreter-fallback stub: the whole JIT could be dead with every gate green (it was, between `aad0f42` and Pass 18). | `rust/src/jit_bc.rs`, `.github/workflows/build.yml` | **Mitigated (2026-11-19):** `UV_JIT=2` strict canary makes any JIT candidate that fails to compile abort the process with exit code 2 and a named proto list, instead of a silent stub. Curated canaries `test/jit/strict.un` (Int), `strict_float.un` (Float math), `strict_collections.un` (IntVec/IntDict + nested calls + zero-arg), `strict_class.un` (Int-field methods), `strict_ops.un` (FusedIntMulAdd/bit ops/constant divisors) run under `UV_JIT=2` in three CI steps. Declare failures, setup panics and intrinsic rebinds can no longer be silently skipped — all are surfaced as exit 2. Verified locally (forced stub → exit 2; all five markers pass). Remaining subclasses still print via `UV_JIT_DEBUG=1` only. |
| TD-54 | low | **opt-in documented divergence**: 병렬 plain exe(`un build --parallel`)의 `pool.submit` future는 `.value()`만 지원하고, VM(`un run`)은 `wait`만 지원한다. 기본 deferred 경로는 변하지 않는다(P09 문서화 잔여) | `rust/nrt/src/scheduler.rs`, `rust/nrt/src/native.rs`, `docs/guides/CLI*.md` | **Documented divergence (2026-11-27, D-099 / P08 `488bf0b`)**: opt-in 경로에서 `wait`를 쓰는 프로그램은 기본 경로와 다르게 동작할 수 있음. 사용자 문서에 명시 예정; 게이트는 기본 경로 byte-identical 유지 |
| TD-55 | low | **opt-in documented divergence**: 병렬 task 내부 throw는 `wait` 지점의 error 메시지가 VM과 동일하지만, VM traceback 프레임(`-> <module>`, `-> go`)이 누락될 수 있다 | `rust/nrt/src/trace.rs`, `rust/nrt/src/scheduler.rs`, `scripts/gowait-parallel.ps1` | **Documented divergence (2026-11-27, P08 `488bf0b`)**: `scripts/gowait-parallel.ps1` 게이트는 **error 문구만 검증**하므로 프레임 누락은 회귀 실패로 잡히지 않는다. P09 문서화 잔여 |
| TD-56 | low | **잠재 이슈(확정 버그 아님)**: `rust/nrt/src/scheduler.rs`의 `cv.notify_all()`이 shared lock 없이 호출되고 `join_full`에 watchdog이 없다. 1회 30초 타임아웃이 관측됐으나 이후 100회 재현 시 재발하지 않음 | `rust/nrt/src/scheduler.rs` | **Potential issue — not a confirmed bug (2026-11-27 관측, P08 `488bf0b`)**: 재현 조건 미확정이므로 단정하지 않고 기록만 한다. 수정은 별도 Work/사용자 승인 대상; 이번 작업은 문서화만. `GOWAIT_IMPLEMENTATION_PLAN.md` §3.5 / `GOWAIT_THREAD_SAFETY_AUDIT.md` A-18 참조 |

## Resolved in 2026-11

evidence and `docs/project/DECISIONS.md` for the semantics decisions):
TD-01, TD-02, TD-04, TD-05, TD-06, TD-07, TD-12, TD-13, TD-14, TD-15, TD-16, TD-19, TD-23,
TD-25, TD-28, TD-29, TD-30, TD-31, TD-32, TD-33, TD-34, TD-35, TD-36, TD-37, TD-39, TD-40, TD-41, TD-42, TD-43, TD-44, TD-45, TD-46, TD-48, TD-49, TD-50, TD-51, TD-53.
TD-03, TD-08 (mitigated), TD-09, TD-11, TD-17, TD-18, TD-24, TD-26, TD-38 also closed in 2026-11.

- **TD-52** (JIT locals mis-resolution, fixed 2026-11-19): the Int-JIT resolved
  frame-local slots via the program's *global* name table indexed by
  proto-local indices, so a parameter rebound in body (`n = n - 1`) made every
  subsequent `LoadName n` read the stale FFI argument register — the emitted
  native loop never observed the decremented value and spun forever
  (`while n > 0` on a rebound parameter infinite-looped in native code; only
  `UV_JIT=0` was correct). Fixed by two-step resolution through
  `proto.names` (like `analyze_return_kinds`/`arena` construction) and by
  routing `LoadName` of a locally-declared name through the local slot
  (including the ABI-parameter shadow case via `local_name_to_slot`), with the
  entry block initialising parameter-shadowing locals from the FFI args.
  Verified: probes matching the hang class (countdown/`probe5`/`m1`/`p_const`)
  now terminate and agree byte-for-byte with UV_JIT=0; differential 88/88.

- **TD-51** (see table): strict canary mitigated 2026-11-19.

- **TD-53** (see table): `Op::Neg` now lowers to `ineg` (Int, wrapping) and
  `fneg` (Float); Bool/none/arena handles bail to the interpreter. The
  Float strict canary (`test/jit/strict_float.un`) keeps the arm CI-verified.

- **TD-16 / TD-40** (see table): the only unprotected plan acquisition
  before the first step was wrapped in `catch_unwind` (VM::new both sites,
  VM::with_context, plus the module loader's `validate()`-first guard kept).
  No known Rust panic escapes the process boundary; errors print through
  `report()` and exit 1.

Closed-out notes: TD-37 kept the deliberate API shape (4xx/5xx responses
return the body, matching the .NET reference — the Un http client does not
throw on unsuccessful status); the connect/read/write timeouts, the
64 MiB response cap with Content-Length validation, the `http` scheme-guard
message, and case-insensitive `set_header` names were the defects and are
fixed. TD-36's budget is per engine attempt (10M dispatch steps) and never
reports a negative match.

## 잔여 항목 메모 (2026-11-24, DOC-01 교차 확인)

이 메모는 상태 열을 바꾸지 않는다. 상태 판정은 위 표가 기준이다.

- 여전히 잔여: **TD-10**(`set_attr_from`의 refcount-clone 잔여, 재구조화 안 함),
  **TD-20**(frame 4종 + scope 2종 중복 명칭), **TD-21**(equality 4종 + clone 2종 중복),
  **TD-47**(os/sys helper 중복은 won't-fix). 모두 low/medium의 문서화·중복 성격이다.
- 컴파일 언어 잔여는 `docs/project/COMPILER_LANGUAGE_ROADMAP.md` §4 참조: D-096
  class/variant pattern + named field/spread, D-095 `func(..)` 반환 타입, D-097 상속/`__is__`,
  `go`/`wait` 진짜 병렬(안 A, 미승인; opt-in 안 C는 D-099 구현 완료 — 위 TD-54/TD-55/TD-56 참조),
  LANG5의 빌드 캐시 기초는 **구현 완료(2026-11-27)**.
- 확인 필요: `native/bytes.rs`의 임시 `List<Int>` 표현을 `Payload::Bytes`로 교체하는 어댑터 3함수 —
  **(2026-11-27 각주)** D-098 **구현 완료**: VM·NRT 양쪽에 `Payload::Bytes`를 추가하고 어댑터 3함수가 새 표현을 우선 사용한다. 본 메모는 당시 스냅샷으로 보존한다.
  원본 `docs/project/COMPILER_LANGUAGE_GAPS.md`는 저장소에 없어 초기 분석 근거를 직접 확인할 수 없다.