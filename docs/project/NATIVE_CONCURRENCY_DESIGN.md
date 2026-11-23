# NATIVE_CONCURRENCY_DESIGN.md — no-VM exe의 `go`/`wait` 설계 검토

> Work ID: W-2026-11-24-LANG2 (`docs/project/COMPILER_LANGUAGE_ROADMAP.md:14`)
> 담당: VM-02 + NRT-06 설계 / 작성일: 2026-11-24 / 커밋 없음(설계 문서만)
>
> 이 문서는 **설계만** 다룬다. 코드 수정은 포함하지 않는다. 인용된 줄은 작성 시점
> worktree의 실제 파일을 정적 확인한 결과이며, 빌드 산출물이 없어 런타임 실행으로
> 확인하지 못한 항목은 `확인 필요`로 표시했다.

---

## 0. 결론 요약

1. NRT는 **명시적으로 단일 스레드**를 가정한다. `static mut` 캐시·arena·프레임
   카운터의 SAFETY 주석이 그 근거를 직접 적고 있다(`rust/nrt/src/class.rs:81-87`,
   `rust/nrt/src/trace.rs:23-29`).
2. 더 근본적으로 **VM의 `go`/`wait` 자체가 병렬이 아니다**. `go`는 work를 future에
   담고 `wait` 시점에 호출 스레드에서 proto를 실행한다
   (`rust/src/compiler.rs:1890-1900`, `rust/src/vm/mod.rs:1219-1269`, D-063).
   따라서 "no-VM exe에서 진짜 병렬"은 NRT만의 문제가 아니라 언어 semantics 변경이다.
3. 현재 no-VM 경로는 `go`/`wait`를 core에서 제외하며
   (`docs/compile/COMPILE_DESIGN.md:293-294`, `rust/src/tir.rs:1392-1394`),
   상당수 경우 명시 오류가 나지만 **미호출 함수는 조용히 누락**될 수 있다(§1.4).
4. **권장안은 B(명시적 거부 + `--embed` 안내)** 다. 안 A(진짜 병렬)는 객체 모델
   전면 개편 + semantics 변경 + 비결정적 traceback을 수반하며, 기존 byte-identical
   게이트(`exe-diff 42/0`)와 양립하기 어렵다.

---

## 1. 현재 상태: 단일 스레드 근거

### 1.1 NRT의 `static mut` 캐시와 arena

**`rust/nrt/src/class.rs`**

| 줄 | 내용 | 의미 |
|---|---|---|
| `class.rs:81-87` | `SAFETY: plain \`un build\` executables are single-threaded (the NRT subset has no \`go\`/\`wait\`), so these \`static mut\` slots have one reader/writer.` | 단일 스레드 가정이 **주석으로 명시된 설계 전제** |
| `class.rs:88-97` | `NAME_CACHE_SLOTS = 16`, `CLASS_ID_CACHE`, `FIELD_IDX_CACHE`, `METHOD_IDX_CACHE` (`static mut [NameCacheEntry; 16]`) | 이름→인덱스 direct-mapped 캐시 3종 |
| `class.rs:99-110` | `addr_of_mut!`로 raw pointer를 얻어 참조를 만들지 않음 | `static_mut_refs` 경고 회피 + `Sync` 질문 우회 |
| `class.rs:118-152` | `cached_index()` — miss 시 선형 탐색 후 `(ptr,len,owner,idx)` 기록 | hit는 frozen registry bounds check로 검증; 경합 시 **잘못된 인덱스 대신 fallback**이지만 데이터 레이스 자체는 UB |
| `class.rs:217-222` | `Instance fields never need deallocation (plain executables are short-lived and single-threaded) ... static mut FIELD_ARENA: Vec<u64>, static mut FIELD_BUMP: usize` | 필드 bump arena도 단일 스레드 전제 |
| `class.rs:224-242` | `alloc_field_block()` — `reserve` / `set_len` / 포인터 산술 | 두 스레드 동시 호출 시 `Vec` 메타데이터 레이스 → UB |
| `class.rs:244-250` | `instance_fields()` → `&'static mut [u64]` | 인스턴스 필드가 공유 가변 별칭 가능 |
| `class.rs:279-280` | `NONE_OBJ: OnceLock<u64>` | 불변 공유는 이미 안전 |
| `class.rs:288` | `__init__` 호출 전 descriptor 복사 (Mutex 비재진입 회피) | registry `CLASSES: Mutex`(`class.rs:16`)는 등록용, hot path는 `FROZEN: OnceLock`(`class.rs:19`) |

**`rust/nrt/src/trace.rs`**

| 줄 | 내용 | 의미 |
|---|---|---|
| `trace.rs:16` | `static SOURCE: Mutex<Option<SourceInfo>>` | 소스 정보는 뮤텍스로 이미 보호 |
| `trace.rs:17-21` | `Relaxed atomic stores ... depth counter is a plain cell because plain no-VM executables are single-threaded` | 프레임 슬롯은 atomic이지만 깊이 카운터는 비원자 |
| `trace.rs:23-29` | `Plain executables are single-threaded (no \`go\`/\`wait\` in the NRT subset) ... static mut FRAME_DEPTH: usize` + `addr_of_mut!` 접근 | 두 번째 명시적 단일 스레드 전제 |
| `trace.rs:38-46` | `FRAME_SLOTS: OnceLock<Vec<[AtomicU64; 4]>>`, `MAX_FRAMES = 512` | 전역 프레임 슬롯 배열 |
| `trace.rs:47` | `static ERROR_SITE: Mutex<Option<(usize, usize)>>` | 에러 위치도 **프로세스 전역** (태스크별 아님) |
| `trace.rs:120-137` | `nrt_push_frame` / `nrt_pop_frame` — 전역 depth 증가/감소 | 스레드 간 interleaving 시 프레임 스택이 섞임 |
| `trace.rs:158-173` | `traceback()` — 전역 depth/slots/site를 읽어 출력 | 스레드 간 경합 시 잘못된 traceback |

**기타 NRT 전역 상태**

- `rust/nrt/src/obj.rs:35-47`: 객체는 `Box::into_raw`로 **의도적 누수**, `obj_mut()`가
  `&'static mut NObj`를 반환. `Send`/`Sync`가 아니라 스레드 간 전달 불가.
- `rust/nrt/src/list.rs:5` `items() -> &'static mut Vec<u64>`,
  `rust/nrt/src/dict.rs:109` `data() -> &'static mut DictData` — 동일 패턴.
- `rust/nrt/src/globals.rs:11` `GLOBALS: Mutex<Vec<u64>>` — 핸들 자체는 `u64`라
  뮤텍스는 안전하지만 가리키는 객체가 여전히 비동시성.
- `rust/nrt/src/closure.rs:12-13` `LazyLock<Mutex<HashMap<...>>>` — 등록 테이블은
  안전하나 capture된 값의 `Send` 여부는 확인 필요(정적 미검증).
- `rust/nrt/src/native.rs:604`: 미구현 모듈은
  `native module '<m>.<name>' is not supported in a no-VM executable` 오류.
  `flow`, `time.sleep`에 해당 케이스가 없음(`native.rs:559-571`은 시계 함수만).

### 1.2 설계 문서의 core 제외

`docs/compile/COMPILE_DESIGN.md:282-297`:

- `284-285`: 하나라도 어긋나면 **컴파일 타임에 core가 아니게 되어 인터프리터로 실행**
  (bail-at-compile-time).
- `287-294`: 허용 타입/연산/문/호출 목록. **`293-294`: "클래스 메서드, match, try,
  go/wait, defer, list/dict/str 연산, 네이티브 호출, FString 등은 core 제외"**.
- `296-297`: core가 아닌 함수는 JIT 대상이면 네이티브, 아니면 bytecode 인터프리터.
- `299-301`: 이 제약은 Vm 모드(`un native`) 대상. plain build는 클래스·예외·Str·
  list/dict·closure 등은 `libnrt.a`로 실행하지만 **go/wait은 여전히 범위 밖**.

### 1.3 의사결정 기록과 사용자 문서

- `docs/project/DECISIONS.md:90-99` D-012: `go`/`wait`는 single-threaded cooperative
  scheduler. `98-99` "블로킹 네이티브(`time.sleep`)를 `wait` 없이 호출하면
  single-threaded VM은 전체가 stop한다(C#는 병렬)."
- `docs/project/DECISIONS.md:557-567` D-054: `Rc<RefCell<..>>`는 `Send`가 아니라
  스레드로 넘길 수 없어 cooperative로 구현. `566-567` "D-063으로 대체: deferred
  future, wait 시점에 go 시점 scope chain 복사본에서 실행".
- `docs/project/DECISIONS.md:646-659` D-063: `MAKE_FUTURE`가 chain 복사본을 담고
  `AWAIT`가 child scope에서 proto 실행 후 memoize.
- `docs/spec/ARCHITECTURE.md:24-25`: "The runtime is single-threaded and cooperative:
  `go`/`wait`, `pool` and `lock` all execute on the calling thread".
- `docs/README.ko.md:603-605`: "`flow.spawn(n)` ... OS 스레드를 만들지 않는다 ...
  실제 실행은 호출자 스레드에서 인라인으로 이루어진다."
- `docs/guides/BENCHMARK_GUIDE.md:198`: pool/go remain single-threaded cooperative shims.

즉 **"진짜 병렬"은 한 번도 구현된 적이 없고, 오히려 반복적으로 설계 원칙으로
채택돼 온 동작**이다.

### 1.4 VM/no-VM의 실제 `go`/`wait` 경로와 누락 지점

- `rust/src/compiler.rs:1890-1900`: `go`는 `begin_proto("go")` → `Op::MakeFuture`
  (코드는 별도 proto로 캡처). `1903-1906`: `wait`는 `Op::Await`.
- `rust/src/vm/mod.rs:1219-1227`: `MakeFuture`는 `future_pending(module_index, program,
  scope::snapshot(...))` — work를 **실행하지 않고** 저장.
- `rust/src/vm/mod.rs:1228-1269`: `Await`는 pending proto를 **현재 스레드에서**
  `execute_proto`로 실행하고 결과를 memoize(`1269` `state.borrow_mut().value = ...`).
  병렬 스케줄러/JoinHandle은 존재하지 않음.
- `rust/src/native/flow.rs:9-23`: `FutureState{value, pending}`는
  `Rc<RefCell<...>>`; `PoolState`, `LockState`도 동일.
- `rust/src/native/flow.rs:163-180`: `pool_submit`은 `function::fn_call`을 **직접**
  호출해 `future_of(value)`로 감쌈(비동기 아님).
- `rust/src/runtime/obj.rs:11`: `pub type Obj = Rc<RefCell<ObjData>>`;
  `obj.rs:30-43`에 `Future`, `Pool`, `Lock`, `AtomicInt`, `Stream` 모두 `Rc<RefCell>`.
- `rust/src/runtime/context.rs:15,26,37-49`: `Frames`/`Traces`가 `Rc<RefCell>`,
  `Context = Rc<RefCell<ContextData>>`.
- `rust/src/runtime/world/mod.rs:19-60,62-64`: `WORLD`는 `thread_local!`이지만 내부에
  `Rc<RefCell<String>>` stdout/log와 `Scope`를 보유 → 스레드 간 공유 불가.

**no-VM 빌드에서 왜 "조용히"될 수 있는가:**

1. `rust/src/tir.rs:1385-1395`: `body_is_core`가 `TokenType::Wait | TokenType::Go`를
   만나면 `tirdbg_reject` 후 `*out = false`. TIR에는 `Go`/`Wait` 표현이 없음
   (`TirExpr`/`TirStmt` 정의 `tir.rs:452,514`에 해당 variant 없음).
2. `rust/src/tir.rs:3423`: `lower_unary`의 `_ => Err("unsupported unary operator in the
   core")` — Go/Wait는 여기 걸림.
3. `rust/src/tir.rs:1895-1898,1913-1915`: `core_ok && body_ok`인 함수만 `eligible`이고
   나머지는 TIR에서 건너뜀.
4. **명시 오류가 나는 경우**:
   - eligible 함수가 비-core 함수를 호출 → `tir.rs:4217-4218`
     `call to non-core function '<name>'` → `main.rs:1041-1043` `tir: ...` 출력.
   - 모든 함수가 비-core → `tir.functions`가 비어 `nrt_supported`(`tir.rs:712`) false →
     `main.rs:1050-1053` "requires the VM runtime; rebuild with `un build --embed`".
   - 최상위 `go` 문 → `compiler.rs:77-116`의 `find_ignored_top_level_statement`가
     잡아 `main.rs:1013-1017`에서 명시 오류.
5. **조용히 누락되는 경우(확인 필요 — 산출물 없어 런타임 미검증)**: 위 어느 경로도
   밟지 않는 `go`/`wait` 함수, 즉 **eligible 함수 어디에서도 참조되지 않는 함수**는
   TIR에서 제외되고 `natcode.rs:447`이 `tir.functions`만 순회하므로 exe에 심볼이
   없다. `nrt_supported`는 남은 함수만 검사하므로 빌드는 성공한다.
   `natcode.rs:487-498`의 entry 검사도 `entry_name`이 누락됐을 때만 실패한다.
   `flow.*`/`atomic.*` 호출은 `tir.rs:702-707`의 `native_call_ok`가 거부 목록에
   포함하지 않아 통과하지만 NRT `unrt_call_native`에는 구현이 없어
   `native.rs:604`에서 **런타임** 오류가 난다(빌드 시 거부 아님).
   → 로드맵 완료 조건인 "명시적 거부"에 아직 미달하는 지점이다.

---

## 2. 안 A: no-VM exe에서 진짜 병렬 `go`/`wait` 지원

> 전제 변경: D-012/D-054/D-063을 폐기하고, 언어 semantics를 "deferred cooperative"에서
> "실제 OS 스레드 병렬"로 바꾼다. 사용자 승인이 필요한 **breaking change**다.

### 2.1 스레드/스케줄러

| 파일 | 변경 |
|---|---|
| `rust/nrt/src/scheduler.rs` (신규) | work-stealing pool (예: `std::thread::scope` + `VecDeque<Task>` 또는 `crossbeam` 대신 표준 라이브러리만으로 구현 — `Cargo.toml` 의존성 추가는 AGENTS.md 규칙상 ARC-00 승인 필요). `Task = extern "C" fn(*mut u8) -> u64 + Send` + hidden result buffer. |
| `rust/nrt/src/lib.rs:8-19` | `mod scheduler;` 추가. `nrt_init_threads(n)` 심볼 등록(새 `unrt_*` 심볼은 ARC-00 승인 필요 — roadmap §2.2). |
| `rust/src/natcode.rs` | `go`를 만나면 task wrapper 심볼을 emit하고 `unrt_go(fn_ptr, ctx_ptr, ...)` 호출을 삽입. `wait`는 `unrt_wait(handle)` 호출. 현재 `TirExpr`에 `Go`/`Wait`가 없으므로 `tir.rs:452,514`에 variant 추가 + `tir.rs:637-696`의 `expr_ok`/`stmt_ok` 갱신. |
| `rust/src/vm/mod.rs:1219-1269` | VM 경로도 동일 실행기로 통일하거나, VM은 기존 cooperative로 두고 no-VM만 병렬로 두면 **byte-identical 게이트가 깨진다** → `exe-diff.ps1` 재정의 필요. |
| `rust/src/native/flow.rs` | `FutureState`를 `value/pending`에서 `JoinHandle`/`Arc<Mutex<Option<Obj>>>`로 교체. `pool.submit/map`도 실제 큐로 전환. |

### 2.2 NRT `static mut` 제거

| 파일:줄 | 현재 | 변경안 |
|---|---|---|
| `nrt/class.rs:92-97` | 3개 `static mut` 캐시 | `thread_local! { RefCell<[...]> }` (스레드별 캐시, hit 검증 로직 `118-152` 재사용) 또는 `OnceLock<Mutex<[...]>>`. 전자는 hot path 무락, 후자는 단순하지만 RMW 비용. |
| `nrt/class.rs:221-222,224-242` | `FIELD_ARENA`/`FIELD_BUMP` bump arena | `thread_local!` arena(인스턴스가 스레드 간 이동 불가해짐 → 객체 공유 불가) 또는 `Mutex<Vec<u64>>` + `AtomicUsize` bump. 전자는 "go로 만든 객체를 wait한 스레드에서 읽기"가 안 되므로 **후자 필수**. |
| `nrt/trace.rs:29-37` | `static mut FRAME_DEPTH` | `thread_local! { Cell<usize> }` (스레드별 호출 스택). |
| `nrt/trace.rs:38-46` | 전역 `FRAME_SLOTS` 512 | `thread_local!` 슬롯 배열로 변경. worker 종료 시 trace를 결과와 함께 전달해야 함. |
| `nrt/trace.rs:47,145-156` | 전역 `ERROR_SITE: Mutex` | Task-local error site + 결과 버퍼에 함께 저장. |
| `nrt/trace.rs:158-173` | `traceback()` 전역 읽기 | trace snapshot을 `unrt_wait`가 반환하는 result에 포함시켜 **호출 스레드에서 출력**. byte-identical 유지의 핵심. |

### 2.3 `Rc<RefCell>` → `Send + Sync` 또는 스레드 로컬 분리

| 계층 | 파일:줄 | 변경 |
|---|---|---|
| VM 객체 | `rust/src/runtime/obj.rs:11,30-43` | `Obj = Arc<Mutex<ObjData>>`(또는 `RwLock`). `Payload::Future/Pool/Lock/AtomicInt/Stream`의 `Rc<RefCell>` → `Arc<Mutex>`/`Arc<AtomicI64>`. 수백 곳 `borrow()`/`borrow_mut()` 호출부 전면 수정. |
| Context | `rust/src/runtime/context.rs:15,26,37-49` | `Frames`/`Traces`/`Context` → `Arc<Mutex<...>>`, `Rc<Source>` → `Arc<Source>`. |
| World | `rust/src/runtime/world/mod.rs:19-64` | 현재 `thread_local!` WORLD를 유지하면 worker가 자기 World를 가지게 되어 `stdout`/`natives`/`module_cache`가 공유되지 않음. `Arc<Mutex<World>>`로 전환하거나 최소한 `stdout`/`log`/`global_scope`만 공유 객체로 분리. |
| Scope | `rust/src/runtime/scope.rs` (확인 필요 — 정독 안 함) | slot table이 `Rc<RefCell>` 기반이면 동일 개편 필요. D-063의 "scope chain 복사본"도 `Send`여야 worker로 이동 가능. |
| NRT 객체 | `nrt/obj.rs:35-47`, `nrt/list.rs:5`, `nrt/dict.rs:109`, `nrt/class.rs:244-250` | raw `u64` 핸들 + `&'static mut` 제거. `Arc<NObj>`/handle table(`Mutex<Vec<...>>` + 인덱스) 중 택1. **ABI는 `u64` 유지**해야 하므로 handle-table 방식이 C ABI 호환에 유리. |
| NRT 전역 | `nrt/globals.rs:11`, `nrt/closure.rs:12` | 이미 `Mutex`/`LazyLock`로 보호되나 담기는 값이 `Send`가 되어야 함. |

### 2.4 C ABI

- `nrt/abi.rs:80-93` `call_native_typed`는 `usize → extern "C" fn` transmute.
  함수 포인터 자체는 `Send + Sync`지만 **정적 가변 상태를 읽는 callee는 아님**
  (§2.2.high); 위 수정이 선행돼야 안전.
- 새 심볼 예시: `unrt_go(task_fn: usize, ctx: *const u8, ctx_len: usize) -> u64`,
  `unrt_wait(handle: u64, out: *mut u64) -> u64`, `unrt_task_trace(handle) -> ...`.
  **기존 심볼 시그니처 변경 금지**(roadmap §2.2), 새 심볼은 ARC-00 승인.
- 멀티스레드 진입점은 `std::thread` 생성 결과를 프로세스 종료 전 join해야 하며,
  `main.rs:21-31`의 worker-thread 패턴(64 MiB 스택)을 NRT 쪽에도 적용할지 결정 필요.

### 2.5 traceback

- worker에서 예외가 나면 Rust `String` 메시지 + 프레임 snapshot + error site를
  결과 버퍼로 보내고, `wait`를 호출한 스레드가 `traceback()`(`trace.rs:158-238`)을
  실행해야 stdout 바이트 순서가 결정적이다.
- 두 task가 동시에 에러를 내면 출력 순서가 non-deterministic → `exe-diff.ps1`의
  byte-identical 게이트와 충돌. ** 출력 직렬화(전역 `Mutex<Vec<String>>` + 종료 시
  순서화)가 별도 설계 항목.**
- caret/줄 번호는 `SOURCE`(`trace.rs:16`)를 공유하므로 읽기 전용화(`OnceLock`) 가능.

### 2.6 그 밖에 필요한 것

- `nrt/native.rs`: `flow.spawn/lock`, `atomic.*`, `time.sleep` 구현 추가(현재
  `native.rs:604` fallback). `atomic`은 `Mutex`/`AtomicI64`로 의미 재정의 필요
  (D-062는 RefCell 전제).
- `docs/README.ko.md:603-605`, `docs/spec/ARCHITECTURE.md:24-25`,
  `docs/guides/BENCHMARK_GUIDE.md:198`, D-012/D-054/D-063 모두 개정.
- `test/unit/runtime/async.un:41-46`의 `slow()` 테스트는 병렬이 되면 타이밍이 달라짐
  (값은 같아도 stopwatch 출력이 달라질 수 있음) → 기대값 재정의.

### 2.7 안 A의 위험

| 위험 | 설명 |
|---|---|
| semantics break | D-012/D-054/D-063 폐기. `go` 안 블로킹 호출이 전체를 멈추지 않게 됨 → 기존 프로그램 동작 변화. |
| byte-identical 게이트 | `exe-diff.ps1 42/0`, `differential.ps1 0 차이`가 concurrently 동작을 커버하지 못함(비결정성). |
| UB 위험 | `static mut`/`&'static mut`을 놓친 곳 하나가 데이터 레이스 → crate 안전성 무효. |
| 비용 | `runtime/**` 전면 리팩터 + NRT 6개 파일 + TIR/natcode + 문서/테스트. 수 주 단위. |
| 의존성 | 스케줄러를 외부 crate로 가져오려면 vendor 패치 or ARC-00 승인 필요(AGENTS.md). |

---

## 3. 안 B: 병렬 미지원을 **명시적 거부**로 바꾸는 최소 패치 경로 (권장)

목표: "조용히 fallback/누락" 경로를 0으로 만들고, `go`/`wait`(및 협업 shim 모듈)를
쓰면 **빌드 시점**에 "requires `--embed`"를 명시한다. semantics, ABI, 객체 모델,
traceback은 그대로 둔다.

### 3.1 패치 1 — AST 수준 감지기 (파일: `rust/src/compiler.rs`)

- `src/compiler.rs:77-116`의 `find_ignored_top_level_statement`와 같은 패턴으로
  `find_unsupported_async_node(root)` 추가:
  - `NodeKind::Unary`이고 `node.op == Some(TokenType::Go | TokenType::Wait)` 인
    노드를 재귀 탐색(함수 본문 포함, **최상위만이 아니라 전체 트리**).
  - `NodeKind::Call`에서 `native_call_shape`가 `(flow, ...)` 또는 `(atomic, ...)`인
    것도 같은 진단으로 수집(선택, §3.3).
  - 반환형은 `IgnoredTopLevel`과 동일한 `{start, length, message}` 구조 재사용 가능.
- **핵심**: TIR 가용성(`body_is_core`)이 아니라 **AST 존재 여부**로 판정해야
  §1.4의 "미호출 함수 조용 누락"을 막을 수 있다.
- `tir.rs:1392-1394`의 거부는 유지(VM mode core 제약은 그대로).

### 3.2 패치 2 — 빌드 드라이버에서 명시 오류 (파일: `rust/src/main.rs`)

- `main.rs:1013-1017` 바로 뒤(코드 파싱 후, typeck 전)에 감지기 호출 삽입:
  ```text
  un build: 'go'/'wait' are not supported in a no-VM executable
  (single-threaded cooperative runtime); rebuild with `un build --embed`
  ```
  `LangError::new(start, length, message).display(&source)`로 caret까지 출력해
  기존 진단 스타일(`1014-1016`)과 동일하게 유지.
- 기존 `main.rs:1050-1054`의 일반 "requires the VM runtime" 메시지는 그대로 두되,
  AST 감지가 먼저 실행되므로 async는 구체 메시지가 나온다.
- `un build --embed` 분기(`main.rs:836`)는 변경하지 않는다 — bytecode 경로는
  `compiler.rs`의 `MakeFuture`/`Await`와 VM이 그대로 지원한다(정적 확인, 런타임
  미검증: worktree에 빌드 산출물 없음).

### 3.3 패치 3 — 네이티브 모듈 거부 목록 (파일: `rust/src/tir.rs`)

- `tir.rs:702-707` `native_call_ok`에 `module == "flow" || module == "atomic"` 추가.
  이렇게 하면 `nrt_supported`가 false가 되어 `main.rs:1052`의 `--embed` 안내가
  나온다. 패치 2의 구체 진단을 쓰려면 AST 감지 쪽에 같은 모듈을 포함시키면 된다.
- 이 변경은 **오직 plain build에만 영향**(`nrt_supported` 호출부는 `main.rs:1050`).
  `un run`/`un native`는 `native_call_ok`를 쓰지 않으므로 무영향.
- `time.sleep`은 NRT 미구현(`native.rs:604`)이지만 typeck/TIR가 `time`을 허용하므로
  같은 방식으로 거부할지 여부는 **확인 필요**(별도 감사 항목으로 분리 권장).

### 3.4 패치 4 — 문서 (파일 4개)

| 파일 | 변경 |
|---|---|
| `docs/compile/COMPILE_DESIGN.md:293-294` | "core 제외" 뒤에 "plain build는 AST 단계에서 명시적 거부, `--embed`만 지원" 추가. |
| `docs/project/COMPILER_LANGUAGE_ROADMAP.md:14` | 완료 조건을 "NRT 단일 스레드 가정 제거 **또는** 명시적 거부" 중 **후자 달성**으로 갱신. |
| `docs/README.ko.md:603-605` / `docs/README.md` 대응 구간 | no-VM exe에서 `flow`/`go`/`wait` 미지원을 명시. |
| `docs/project/DECISIONS.md` D-012/D-054/D-063 | "이 결정은 유지되며 no-VM exe에서는 빌드 시 거부한다" 한 줄 추가. |

### 3.5 패치 5 — 테스트 (파일: `test/negative/` + `scripts/`)

- `test/negative/build_go_wait.un`: `fn f() -> go 1` + `main()` 미호출 형태로 추가 →
  `test/negative/manifest.txt`에 expected exit 1 + substring
  `not supported in a no-VM executable` 등록. **미호출 함수 케이스가 핵심**.
- `test/negative/build_flow.un`: `use flow` + `flow.spawn(2)` → 동일 거부 확인.
- `scripts/exe-diff.ps1`는 기존 42/0 유지(거부 대상은 manifest의 negative로 이동).
- `un build --embed`로 같은 스크립트가 정상 실행되는지 확인(빌드 산출물 필요 —
  이 단계에서는 **확인 필요**).

### 3.6 안 B의 장점

- 코드 변경 4~5개 파일, 신규 ABI 심볼 없음, 객체 모델/traceback 무변경.
- byte-identical 게이트 유지. D-012/D-054/D-063과 정합.
- 사용자에게 "왜 안 되는지"와 "어떻게 하면 되는지"를 동시에 알려줌.
- LANG1/LANG4 등 후속 Work와 파일 충돌 없음(`main.rs`는 LANG5가 소유 → 순서 조정
  필요: roadmap §1의 Wave 2 LANG5 이후에 패치 2 적용).

---

## 4. 두 안 비교와 권장안

| 항목 | 안 A (진짜 병렬) | 안 B (명시적 거부) |
|---|---|---|
| 사용자 영향 | semantics 변경(interleaving, blocking sleep), 비결정 출력 | 기존 동작 유지, `--embed` 안내만 추가 |
| 코드 범위 | `nrt/**` 6+, `runtime/**` 10+, `tir.rs`, `natcode.rs`, `compiler.rs`, `vm/**`, `native/flow.rs` | `compiler.rs`, `main.rs`, `tir.rs` + 문서/테스트 |
| C ABI | 새 심볼 3+ (ARC-00 승인 필요), 기존 시그니처 유지 | 변경 없음 |
| unsafe/UB 위험 | 높음(`static mut`·`&'static mut` 잔존 시 데이터 레이스) | 없음 |
| traceback byte-identical | 재설계 필요(직렬화) | 그대로 |
| 게이트 | `exe-diff 42/0` 재정의 필요 | 42/0 유지 |
| 의존성 | 스케줄러 crate 검토 필요 | 없음 |
| 기간 | 수 주 | 수 시간~1일 |
| 되돌리기 | 어려움(semantics 이미 공개) | 쉬움 |

**권장: 안 B.** 병렬 실행은 언어 수준 결정(D-012/D-054/D-063)과 맞지 않고,
현재 게이트 체계(byte-identical)가 전제하는 결정성과 충돌한다. 먼저 B로
"조용한 실패"를 없애고, 필요하면 사용자 합의 하에 별도 RFC로 A를 재검토한다.

### 4.1 단계별 Work 분해 (안 B 기준)

| 단계 | Work | 담당 | 파일 | 완료 조건 |
|---|---|---|---|---|
| B-1 | AST async 감지기 | VM-02 | `rust/src/compiler.rs` | 함수 본문/중첩 포함 `Go`/`Wait` 노드 검출, 단위 테스트 1건 |
| B-2 | 빌드 드라이버 진단 | ARC-00 승인 후 VM-02 | `rust/src/main.rs` (LANG5와 순서 조정) | caret 포함 명시 오류 + exit 1, `--embed` 분기 무변경 |
| B-3 | shim 모듈 거부 | CMP-05/NRT-06 협의 | `rust/src/tir.rs:702-707` | `nrt_supported`가 `flow`/`atomic` 사용 program 거부 |
| B-4 | 문서 동기화 | VM-02 + NRT-06 | §3.4 4개 파일 | roadmap 완료 조건 갱신, D-xxx 유지 명시 |
| B-5 | negative 테스트 | QA-09 협의 | `test/negative/**`, `manifest.txt` | 미호출 `go` 함수까지 거부 확인, exe-diff 42/0 유지 |
| B-6 | (선택) A의 RFC | ARC-00 | 별도 문서 | 사용자 승인 후에만 착수 |

---

## 5. 확인 필요 항목

1. `un build --embed`가 `go`/`wait`를 실제로 실행하는지 — 정적 경로
   (`compiler.rs:1890-1906` → `vm/mod.rs:1219-1269`)는 확인했으나 worktree에
   `rust/target/release/un.exe`가 없어 **런타임 미검증**.
2. `rust/src/runtime/scope.rs`의 slot table 표현(`Rc<RefCell>` 여부) — 안 A의
   `Send` 전환 범위 산정에 필요, 정독하지 않음.
3. `nrt/closure.rs` capture 값의 `Send` 여부 — 정적 확인 불가.
4. `time.sleep`이 typeck/TIR에서 어떤 경로로 lowing 되는지(안 B-3에 포함할지).
5. 미호출 `go` 함수가 실제로 exe에서 누락되는지 — `natcode.rs:447`의 순회 근거는
   확인했으나 빌드 실행으로 재현하지는 못함.

---

## 6. 근거 파일 색인

- `rust/nrt/src/class.rs:16,19,75-110,118-152,217-250,279-288`
- `rust/nrt/src/trace.rs:16-47,120-156,158-173`
- `rust/nrt/src/obj.rs:35-47`, `rust/nrt/src/list.rs:5`, `rust/nrt/src/dict.rs:109`
- `rust/nrt/src/globals.rs:11`, `rust/nrt/src/closure.rs:12-13`
- `rust/nrt/src/abi.rs:75-93`, `rust/nrt/src/lib.rs:1-19`, `rust/nrt/src/native.rs:559-604`
- `rust/src/compiler.rs:50-132,1890-1906`
- `rust/src/vm/mod.rs:1219-1269`
- `rust/src/native/flow.rs:9-23,41-83,163-180`
- `rust/src/runtime/obj.rs:11,30-43`, `rust/src/runtime/context.rs:15-49`
- `rust/src/runtime/world/mod.rs:19-64`
- `rust/src/tir.rs:452,514,613-721,1225-1264,1303-1310,1392-1394,1844-1898,1913-1915,3397-3424,3423,4217-4218`
- `rust/src/natcode.rs:447-498`
- `rust/src/main.rs:21-31,1013-1017,1041-1054`
- `docs/compile/COMPILE_DESIGN.md:282-301`
- `docs/project/DECISIONS.md:90-99,557-567,646-659`
- `docs/spec/ARCHITECTURE.md:24-25`, `docs/README.ko.md:603-605`
- `docs/guides/BENCHMARK_GUIDE.md:198`, `docs/project/COMPILER_LANGUAGE_ROADMAP.md:8-16`
