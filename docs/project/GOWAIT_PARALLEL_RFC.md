# GOWAIT_PARALLEL_RFC.md — `go`/`wait` 진짜 병렬 실행 RFC

> Work ID: W-2026-11-24-GOWAITRFC / 역할: ARC-00 (Architecture subagent)
> 작성일: 2026-11-24 / 상태: **제안(Proposal) — 구현 없음, 커밋 없음** (작성 시점 스냅샷, 보존)
>
> **(2026-11-27 각주)** 제안된 **안 C는 구현·커밋 완료**됐다: P00~P08 완료, P08 `488bf0b`,
> 새 ADR `docs/project/DECISIONS.md` D-099. 아래 "구현 없음/커밋 없음" 문구는 2026-11-24
> 작성 시점 한정이며 지금은 적용되지 않는다. 안 A는 여전히 미승인·미채택이다.
>
> 이 문서는 **설계 문서 하나만** 수정한 결과다. `rust/**`, `test/**`, `scripts/**`,
> `bench/**`, 기존 문서와 소스는 일체 변경하지 않았다. 인용된 줄은 작성 시점에
> 실제 파일을 정적 확인한 결과다(번호는 확인 시점 기준; 과거 문서의 번호와 다를 수 있다).
> 게이트 수치도 작성 시점 스냅샷이다 — 2026-11-27 현재값은 `cargo test` 430,
> `test.ps1` 51 files / 1638, `compile-check` 42/0, `exe-diff` 78/0
> (`docs/project/REFACTOR_SUMMARY.md` "Current baseline").
>
> 선행 문서: `docs/project/NATIVE_CONCURRENCY_DESIGN.md`(안 A/B 최초 검토, LANG2),
> `docs/project/COMPILER_LANGUAGE_ROADMAP.md:14`(LANG2 = 안 B 완료),
> `docs/project/DECISIONS.md` D-012 / D-054 / D-063 / D-080.

---

## 0. 결론 요약

1. **추천안: C — opt-in `go` 병렬 + 기본 deferred 유지.** `go`/`wait`의 기본 의미론
   (D-012/D-054/D-063)과 byte-identical 게이트를 보존하면서, 명시적으로 선택한
   프로그램에만 OS 스레드 병렬을 제공한다. 완전한 안 A(기본 병렬)는 가능하지만
   **breaking change + 게이트 전면 재정의**를 수반하므로 사용자 승인 없이는 착수하지 않는다.
2. 진짜 병렬은 VM의 `go` 자체가 deferred inline이므로(`rust/src/vm/mod.rs:1254-1306`)
   "NRT만 고치면 된다"가 아니라 **언어 semantics 전체를 바꾸는 작업**이다.
3. NRT는 `static mut`과 `&'static mut`가 단일 스레드를 **주석으로 전제**하고 있으며
   (`rust/nrt/src/class.rs:81-87`, `rust/nrt/src/trace.rs:23-29`), 하나라도 놓치면 UB다.
4. 공수 추정: 안 A **21–29 인일**, 안 C **16–22 인일**, 안 B **0.5–1 인일(이미 완료)**.
5. **구현 착수 전 사용자 승인 필요** (§8 체크리스트).

---

## 1. 현재 의미론 — 무엇이 "병렬이 아닌가"

### 1.1 VM 경로 (deferred cooperative inline)

| 지점 | 확인 내용 |
|---|---|
| `rust/src/compiler.rs:2156,2162` | `go` → `Op::MakeFuture`(별도 proto 캡처), `wait` → `Op::Await`. |
| `rust/src/vm/mod.rs:1254-1262` | `MakeFuture`는 proto를 **실행하지 않고** `future_pending(index, program, scope::snapshot(...))`에 담는다. |
| `rust/src/vm/mod.rs:1263-1306` | `Await`는 pending proto를 **현재 스레드에서** `execute_proto`로 실행하고 `state.borrow_mut().value = ...`로 memoize한다(`1304`). |
| `rust/src/native/flow.rs:41-68` | `FutureState { value, pending }`는 `Rc<RefCell<...>>`. `future_of()`는 즉시 완료된 future를 만든다. |
| `rust/src/native/flow.rs:69-98` | `PoolState { workers, closed }`, `LockState { held }`도 `Rc<RefCell>`. `flow.spawn(n)`은 숫자만 검사하고 풀 객체를 만들 뿐 스레드를 만들지 않는다. |
| `rust/src/native/flow.rs:163-206` | `pool_submit`은 `function::fn_call`을 **직접 호출**하고 `future_of(value)`로 감싼다. `pool_map`도 입력 순서대로 동기 실행한다. `workers` 필드는 실행에 사용되지 않는다. |
| `rust/src/runtime/scope.rs:283-304` | `snapshot()`이 상위 체인을 재귀 deep clone(각 level의 symbol/slot 복사, value cell 공유). D-063의 capture-by-value 기반. |
| `rust/src/runtime/function.rs:265`, `rust/src/runtime/world/executors.rs:26-32` | 호출은 `fn_call` / `execute_proto` 동기 경로. 스케줄러·JoinHandle·Channel은 존재하지 않는다. |

즉 `go`는 **"지연 계산 + wait 시점 인라인 실행 + 결과 memoization"**이고, `wait`는
동기화가 아니라 **계산 트리거**다. `time.sleep`이 `go` body 안에 있으면 `wait` 시점에
호출 스레드를 블록한다(D-012의 "semantic 차이 보고 항목"이 실제로 남아 있다).

### 1.2 고정된 관측 동작

`test/unit/runtime/async.un`이 다음을 고정한다:

- `pool.map(fn(x)->x*2, 10,20,30)` → `[20,40,60]` (**입력 순서**, 결과는 정렬해서 비교).
- `pool.submit(fn(x)->x+1, 5)` → future, `wait` → `6`.
- `go double(4)` → future, `wait` → `12`; `go slow(5)`(50 ms sleep) → `wait` → `10`.
- future는 `str`/`+`/호출 모두 `unsupported operand type(s)` 오류.
- 반복문 안의 `go`는 각 이터레이션 값을 캡처(`[1,4,9]`).
- `flow.lock()`의 `try_acquire`/`acquire`/`release`, 미보유 `release`는 `"lock not held"`.

이 테스트는 **값과 순서(결과 리스트)만 검사하고 스레드 수나 시간은 검사하지 않는다.**
병렬화되면 값은 그대로지만 `slow()`의 타이밍과 미해석 부수효과가 달라진다.

### 1.3 의사결정과 plain build 거부

| 조항 | 현재 내용 | 병렬화 시 충돌 |
|---|---|---|
| D-012 (`DECISIONS.md:90-99`) | single-threaded cooperative scheduler. 블로킹 네이티브가 전체를 멈춤을 "보고 항목"으로 명시 | 병렬이면 보고 항목 자체가 사라짐 |
| D-054 (`:557-567`) | `Rc<RefCell>`은 `Send` 아님 → 호출 스레드 실행, 인터리빙 다름 | 객체 모델 전제가 깨짐 |
| D-063 (`:646-663`) | fork 시 scope chain 복사, `wait` 시 child scope에서 proto 실행 후 memoize | snapshot이 `Send`여야 하거나 공유 모델로 교체 |
| D-080 (`:1057-1066`) | fork-time deep snapshot 유지(capture-by-value). 200k forks ≈ 1,058 ms, 병목 아님 | value cell 공유가 레이스 지점이 됨 |
| D-062 (`:633-644`) | `atomic`을 `Rc<RefCell<i64>>` 한 cell에 저장 | 진짜 원자 연산(`AtomicI64`)으로 재정의 필요 |
| LANG2 (`COMPILER_LANGUAGE_ROADMAP.md:14`) | 안 B 완료: AST 전체(미호출 함수 포함)에서 `Go`/`Wait` 검출 → 명시 거부 | 거부를 해제해야 병렬 빌드 가능 |

`rust/src/compiler.rs:261-290`의 `find_unsupported_async_node()`는 `NodeKind::Unary`의
`TokenType::Go | Wait`를 **재귀 전체 트리**에서 찾아
`"go/wait are not supported in plain build; rebuild with \`un build --embed\`"`
(`compiler.rs:271-273`)를 낸다. `docs/guides/COMPILED_LANGUAGE.md:232-233`도
`go`/`wait`와 `flow`/`atomic`을 명시 거부로 문서화했다.

---

## 2. NRT 스레드 안전성 제약 (병렬 진입 시 UB 목록)

| 파일:줄 | 현재 상태 | 병렬화에 필요한 변경 |
|---|---|---|
| `nrt/src/class.rs:81-97` | 3개 `static mut [NameCacheEntry;16]` — "plain exe는 single-threaded" 주석 | `thread_local!{RefCell<...>}` 또는 `OnceLock<Mutex<...>>`; hit 검증 로직(`:118-152`) 재사용. 전자는 worker 간 객체 이동 시 캐시 무효 |
| `nrt/src/class.rs:227-248` | `FIELD_ARENA`/`FIELD_BUMP` bump arena, `reserve`/`set_len`/포인터 산술 | **반드시** `Mutex<Vec<u64>>` + `AtomicUsize` bump. thread-local arena는 "go로 만든 객체를 wait 스레드에서 읽기"가 깨짐 |
| `nrt/src/class.rs:250-255` | `instance_fields() -> &'static mut [u64]` | handle table 또는 `Arc<Mutex<...>>`; C ABI는 `u64` 유지 |
| `nrt/src/trace.rs:23-37` | `static mut FRAME_DEPTH` + `addr_of_mut!` (RMW 회피 최적화) | `thread_local!{Cell<usize>}`; worker trace를 결과 버퍼로 전달 |
| `nrt/src/trace.rs:38-46` | 전역 `FRAME_SLOTS[512]` (`AtomicU64`) | task-local 슬롯 + wait 시점 병합. 전역 유지 시 프레임이 섞임 |
| `nrt/src/trace.rs:47,158-173` | `ERROR_SITE: Mutex`, `traceback()`이 전역 상태를 읽음 | task-local error site + trace snapshot을 `unrt_wait` 결과에 포함 |
| `nrt/src/obj.rs:35-64`, `nrt/src/list.rs:5-8`, `nrt/src/dict.rs:109` | `&'static mut NObj` / `&'static mut Vec<u64>` / `&'static mut DictData` | `u64` 핸들 + `Mutex<Vec<...>>` 인덱스 테이블(AABI 호환 최선) |
| `nrt/src/globals.rs:11`, `nrt/src/closure.rs:12-13` | `Mutex`/`LazyLock`로 등록 테이블은 보호 | 담기는 capture 값의 `Send` 보장 필요 |
| `nrt/src/abi.rs:80-93` | `usize → extern "C" fn` transmute | 함수 포인터는 `Send+Sync`지만 callee의 정적 가변 상태 접근이 선행 수정돼야 안전 |
| `nrt/src/native.rs:604` | `flow`/`atomic`/`time.sleep` 미구현 fallback | 실제 구현 + D-062 재정의 |
| VM 측 `rust/src/runtime/obj.rs:11,30-49`, `context.rs:15-49`, `world/mod.rs:19-64` | `Obj = Rc<RefCell<ObjData>>`, `World = thread_local!`(stdout/log/global_scope가 `Rc<RefCell>`) | VM도 병렬화한다면 `Arc<Mutex>` 전환 또는 최소 stdout/scope 공유 분리 |

> `WORLD`가 `thread_local!`인 점(`world/mod.rs:58-60`)이 결정적이다. worker 스레드가
> 자기 `WORLD`를 갖게 되면 stdout·natives·module_cache가 공유되지 않으므로,
> **VM 병렬화는 NRT 병렬화보다 범위가 크다.** 안 C는 이 문제를 우회하기 위해
> opt-in 경로를 네이티브(/plain build) 전용으로 두고 VM은 deferred로 유지하는 변형도 허용한다(§6).

---

## 3. 제안: 진짜 병렬 실행 모델

### 3.1 스케줄러 (외부 crate 없이)

`crossbeam`은 추가하지 않는다. `rust-toolchain.toml`/`Cargo.toml` 의존성 정책(AGENTS.md)상
새 crate는 ARC-00 승인 사항이고, 필요한 기능은 표준 라이브러리로 충분하다.

```
Task = struct {
    entry: extern "C" fn(*mut u8) -> u64,   // proto wrapper
    ctx:   Arc<...>,                        // Send 전용 snapshot
    out:   Arc<Mutex<Option<TaskResult>>>,  // (value, tag, trace, exception)
}
Scheduler = {
    queue: Arc<Mutex<VecDeque<Task>>>,
    cv:    Arc<Condvar>,
    workers: Vec<JoinHandle<()>>,
    closed: AtomicBool,
}
```

- `flow.spawn(n)`이 worker 수를 결정(기본 4, `hardware_concurrency()` 상한).
  worker는 `std::thread::Builder::new().stack_size(64 MiB...)`로 생성하고
  프로그램 종료 전 `join()`한다(`main.rs:21-31`의 기존 worker 패턴 재사용).
- 큐는 `Mutex<VecDeque>` + `Condvar`. work-stealing은 구현 난도 대비 이득이 적으므로
  단일 공유 큐로 시작하고, 벤치에서 큐 경합(contention)이 병목으로 확인되면 개선한다.
- 풀 종료는 명시적 `close()`(드레인 후 join) 또는 스코프 종료 시 자동 join.
  `close()` 이후 `submit`은 기존 `"pool is closed"` 오류를 유지한다.

### 3.2 스케줄링 순서 보장 범위 (명세에 추가할 계약)

| 대상 | 보장 |
|---|---|
| `pool.map` 결과 리스트 | **입력 순서** (인덱스 안정) — 실행 완료 순서와 무관 |
| `wait` 반환값 | `wait` 호출 순서대로 수령; 완료 시점은 관측하지 않음 |
| 태스크 시작 순서 | **미보장** (best-effort FIFO, 스레드 경쟁 허용) |
| stdout/stderr 직접 출력 | **미보장** → `go` body의 직접 print는 진단(경고 또는 거부)하거나 태스크 로컬 버퍼에 모아 `wait` 시점에 병합 |
| 동일 future 재`wait` | memoized 동일 값 + 동일 trace(D-063 유지 시) |
| 예외 | `wait` 지점에서 재투입. 동시에 여러 태스크가 실패하면 **수집 순서 = wait 호출 순서**로 고정 |
| 자기 참조 `wait` | 금지(데드락). 풀 포화 시 `wait` 중인 태스크가 다른 태스크를 `wait`하면 호출 스레드 inline fallback |
| 공유 객체 | 값 복사(capture-by-value)만 안전. `atomic`/`lock`으로 명시 동기화한 경우에만 공유 허용 |

### 3.3 Future / ABI

- `Payload::Future`를 `value/pending`에서
  `JoinHandle + Arc<Mutex<Option<Result>>>`로 교체. 결과에는 `(value, tag, trace_snapshot, err)`
  를 담는다. VM deferred 경로와 동일한 `future_value()` 인터페이스를 유지해
  `test/unit/runtime/async.un`의 타입/연산 오류 문구를 보존한다.
- 새 심볼(초안): `unrt_go(task_fn, ctx_ptr, ctx_len) -> u64`,
  `unrt_wait(handle, out_ptr) -> u64`, `unrt_trace(handle)`.
  **기존 `unrt_*` 시그니처는 변경 금지**, 새 심볼은 ARC-00 승인 사항
  (`COMPILER_LANGUAGE_ROADMAP.md:21`).
- TIR에 `Go`/`Wait` variant 추가(`tir.rs:452,514`) 및 `expr_ok`/`stmt_ok` 갱신,
  `natcode.rs`가 task wrapper를 emit. VM은 기본 deferred 경로를 그대로 둔다(안 C).

---

## 4. 호환성 영향

| 축 | 영향 | 완화 |
|---|---|---|
| `go`/`wait` 결과 순서 | 값 자체는 동일. `pool.map`은 입력 순서 보장 가능. `loop` 안 `go`의 캡처 값도 동일 | capture-by-value 스냅샷 유지(D-080). 단, value cell 공유분은 복사로 전환 |
| 예외 전파 순서 | reference는 `Task.Result`가 `AggregateException`을 flatten rethrow. 현재는 `Await`가 `Outcome::*`를 그대로 전파(`vm/mod.rs:1286-1292`) | `wait` 지점에서 deterministic 재투입. 다중 실패는 wait 호출 순서로 직렬화 |
| 공유 상태 레이스 | `Rc<RefCell>` 객체·`WORLD` thread_local·value cell 공유가 전부 레이스 지점 | `Arc<Mutex>`/`Atomic` 전환 + `atomic`/`lock` 외 공유 쓰기 거부 또는 진단 |
| traceback 결정성 | 전역 `FRAME_DEPTH`/`FRAME_SLOTS`/`ERROR_SITE`가 스레드 간 섞임 | task-local trace → 결과 버퍼 → `wait` 스레드에서 출력(직렬화). 이게 byte-identical 유지의 핵심 |
| 타이밍 | `test/unit/runtime/async.un:41-46`의 `slow()`(50 ms)가 실제 병렬이 되면 전체 실행 시간이 줄지만 값은 동일 | 스톱워치 출력을 기대값에 포함하지 않도록 테스트 갱신(테스트 변경은 구현 단계에서 별도 승인) |

> **(2026-11-27 관측 각주 — opt-in documented divergence / 잠재 이슈)** 위 표는 설계 시점 예측이다. 실제 P08(`488bf0b`) 완료 후 관측된 차이는
> `docs/project/GOWAIT_IMPLEMENTATION_PLAN.md` §3.5 및 `docs/project/TECHNICAL_DEBT.md` TD-54/TD-55/TD-56에 기록했다:
> ① 병렬 plain exe의 `pool.submit` future는 `.value()`만 지원하고 VM(`un run`)은 `wait`만 지원(documented divergence),
> ② 병렬 task 내부 throw는 `wait` 지점 error 메시지가 동일하나 VM traceback 프레임(`-> <module>`, `-> go`)이 누락될 수 있고 게이트는 error 문구만 검증(documented divergence),
> ③ `rust/nrt/src/scheduler.rs`의 `cv.notify_all()` 무-lock 호출 및 `join_full` watchdog 부재 — 1회 30초 타임아웃 관측, 이후 100회 재현 실패 → **잠재 이슈로만 기록, 확정 버그 아님**.
> 세 항목 모두 P09 문서화 잔여이며 기본 deferred 경로의 byte-identical 게이트에는 영향이 없다.

---

## 5. 대안 3개 비교

| 항목 | A. 진짜 병렬(기본) | B. 현재 deferred + 명시 거부 | C. opt-in 병렬 + 기본 deferred **(추천)** |
|---|---|---|---|
| 사용자 영향 | **breaking**: blocking sleep/interleaving/순서 변화, 공유 상태 레이스 | 변화 없음(`--embed` 안내만) | 기본은 무변경, opt-in한 프로그램만 새 의미론 |
| D-012/D-054/D-063 | **폐기** | 유지 | "기본은 유지, opt-in만 예외"로 개정 |
| NRT 객체 모델 | 전면 `Send+Sync`화 | 무변경 | opt-in 경로에만 적용(결국 핵심 공수는 A와 유사) |
| 새 ABI 심볼 | 3+ (승인 필요) | 없음 | 3+ (승인 필요, opt-in 빌드에서만 emit) |
| traceback | 재설계 + 직렬화 | 그대로 | opt-in 경로만 재설계, 기본은 그대로 |
| 게이트 | `exe-diff 71/0`, `differential 102/0` 전면 재정의 | 현 게이트 유지 | **현 게이트 유지**(기본 경로) + opt-in 전용 게이트 추가 |
| 의존성 | std만 가능(권장) / crossbeam은 승인 | 없음 | std만 가능(권장) |
| 공수 | 21–29 인일 | 0.5–1 인일(이미 완료) | 16–22 인일 |
| 되돌리기 | 어려움(semantics 공개) | 쉬움 | 쉬움(opt-in 플래그 제거) |

### 추천: 안 C

**한 줄**: 기본 `go`/`wait`는 D-012/D-054/D-063의 deferred cooperative로 두고,
명시적 opt-in(`un build --parallel` / `flow.parallel { ... }` 또는 `go!`)일 때만
NRT 스레드 풀로 병렬 실행한다.

**왜 C인가**

1. **게이트 보존이 최우선**: 현재 `scripts/exe-diff.ps1`은 71/0,
   `scripts/differential.ps1`은 102 파일/0 차이, `scripts/test.ps1`은 Positive
   51 files / 1638 passed를 요구한다(AGENTS.md 기준; 과거 문서의 42/0·51/0은 이전 수치).
   안 A는 이 전부를 "순서 무관 비교"로 바꿔야 해서 회귀 탐지력이 급격히 떨어진다.
   안 C는 기본 레인을 그대로 두고 병렬 레인만 추가하므로 기존 게이트가 살아 있다.
2. **사용자 요청("전부 진행")과 양립**: 병렬 기능은 실제로 제공되지만,
   "기본 동작을 바꿔도 된다"는 별도 승인 없이는 기본을 건드리지 않는다.
   `--embed`가 이미 escape hatch로 자리 잡은 프로젝트 철학과도 일관된다.
3. **breaking change를 되돌릴 수 있다**: opt-in 플래그/구문만 제거하면 이전 상태로 복귀.
4. **정직한 공수**: opt-in이라도 병렬 경로 자체는 A와 거의 같은 `Send+Sync` 작업이
   필요하다(절약되는 것은 기본 경로 마이그레이션·문서·게이트 재정의 약 5 인일).

> **단서**: 사용자가 "기본을 진짜 병렬로 바꿔도 된다"고 명시적으로 승인하면 안 A로
> 전환한다. 그 경우 §8의 게이트 재정의 항목이 전부 필수가 된다.

---

## 6. 구현 공수와 마일스톤 (1인 기준, 리뷰/CI 시간 제외)

| 마일스톤 | 내용 | A | C |
|---|---|---|---|
| M0 | 결정 개정 + 사용자 승인 + 게이트 정책 확정 | 1 | 1 |
| M1 | 스레드 안전성 감사(§2 전수), `Send` 경계 정의 | 2 | 1.5 |
| M2 | NRT 객체 모델: `static mut` 제거, handle table, arena mutex | 6 | 5 |
| M3 | traceback task-local화 + 결과 버퍼 + 직렬화 출력 | 3 | 2.5 |
| M4 | 스케줄러 코어(`std::thread`+`Arc<Mutex>`+`Condvar`) + graceful join | 3 | 2.5 |
| M5 | TIR/natcode/compiler/VM 통합, 새 `unrt_*` 심볼 | 4 | 3 |
| M6 | `flow`/`pool`/`lock`/`atomic` 실제 병렬 semantics + D-062 재정의 | 3 | 2 |
| M7 | 게이트 재정의 + 테스트/negative/strict canary 갱신 | 4 | 1.5 |
| M8 | 문서 동기화(README 한/영, ARCHITECTURE, COMPILED_LANGUAGE, BENCHMARK_GUIDE) | 1.5 | 1 |
| **합계** | | **21–29 인일** | **16–22 인일** |

- B(현재 상태)는 이미 완료: 추가 공수 0.5–1 인일(문서 한 줄 보강 수준).
- 각 단계는 `cargo build --release` 0 errors/0 warnings, `cargo test --release --workspace`
  314 passed(작성 시점 스냅샷; **2026-11-27 실측은 430 passed**)를 통과해야 다음 단계로 넘어간다(새 경고 금지).
- M2의 `unsafe` 제거 누락 한 곳이 전체 crate 안전성을 무효화하므로 M1 감사를 게이트화한다.

---

## 7. 필요한 결정 (사용자 승인 항목)

### 7.1 의사결정 기록 개정 여부

| 결정 | 안 A | 안 C (추천) |
|---|---|---|
| D-012 | **폐기** — cooperative → 실제 OS 스레드 | "기본은 cooperative, opt-in만 예외"로 개정 |
| D-054 | **폐기** — `Rc<RefCell>` → `Arc<Mutex>` | 기본 유지, opt-in 경로만 `Send+Sync` |
| D-063 | **폐기** — scope 복사 공유로 전환 | 기본 유지(capture-by-value), opt-in은 `Send` 스냅샷 |
| D-080 | fork snapshot 비용 근거가 달라짐 → 재검토 | 유지(기본 경로 변경 없음) |
| D-062 | `atomic`을 `AtomicI64`로 재정의 | opt-in 경로에서만 재정의 |
| LANG2 완료 조건 | "명시 거부" → "opt-in 병렬 허용"으로 갱신 | 동일( opt-in 추가 ) |

### 7.2 byte-identical 게이트 재정의 범위

| 스크립트 | 안 A | 안 C |
|---|---|---|
| `scripts/exe-diff.ps1` (71/0) | **전면 재정의**: stdout 순서 정규화, 태스크 결과를 wait 순서로 재배치하는 비교 모드 필요. negative fixture에 비결정 케이스 이동 | 기본 71/0 유지 + `--parallel` 빌드용 별도 manifest(값/집합 비교) 추가 |
| `scripts/differential.ps1` (102/0) | JIT on/off 간 스레드 interleaving이 달라 "0 차이" 불가 → 병렬 케이스 제외 또는 결정적 시드/순서 강제 | 기본 102/0 유지(기본 deferred), 병렬은 opt-in이므로 무영향 |
| `scripts/edge-fuzz.ps1` | 5레인(vm/jit/native/node/python)에 병렬 레인 추가 시 cross-language 결정성 비교 불가 → `RESULT|` 값 비교만 유지 | 기본 5레인 유지, 병렬 레인은 별도 케이스로 분리 |
| `scripts/test.ps1` | `async.un` 타이밍/순서 기대 재정의, `KnownDivergent` 등록 검토 | 기본 그대로, opt-in 테스트는 별도 manifest |
| `scripts/unbc-roundtrip.ps1`, `compile-check.ps1` | `go`/`wait`가 바이트코드 왕복에 포함되면 opcode(`MakeFuture=0x63`, `Await=0x64`) 호환성 유지 확인 | 변경 없음 |
| strict canary (`UV_JIT=2`) | `go`/`wait`는 JIT 대상 밖이므로 원칙적 무영향, 신규 task wrapper가 fallback stub이 되는지 확인 | 동일(무영향 확인만) |
| `scripts/bench.ps1` (6레인) | 병렬 레인 추가 시 측정 분산 증가 → 통계 comparing 방식 재정의 | 현재 6레인 유지, opt-in 벤치는 별도 그룹 |

### 7.3 사용자 승인 체크리스트

- [ ] **목표 선택**: A(기본 병렬, breaking) / **C(opt-in 병렬, 추천)** / B(현 상태 유지).
- [ ] 기본 semantics 변경 승인 — A를 고를 경우에만 필요.
- [ ] 새 `unrt_*` 심볼 3개 승인 (기존 시그니처 변경 금지 원칙 유지).
- [ ] 외부 crate 의존성: **crossbeam 미도입, `std::thread`만 사용**하는 방안 승인.
- [ ] D-012/D-054/D-063/D-062/D-080 개정 문구 승인(§7.1).
- [ ] 게이트 재정의 범위 승인(§7.2) — 특히 `exe-diff`/`differential`의 비교 강도 완화 여부.
- [ ] `test/unit/runtime/async.un` 등 기존 테스트 기대값 변경 승인.
- [ ] 공수 16–22 인일(C) / 21–29 인일(A) 및 단계별 착수 순서 승인.
- [ ] opt-in 문법 선택: `go!` / `flow.parallel { }` / `un build --parallel` 중 확정.
- [ ] 구현 착수 시점(이 RFC 승인 후 별도 Work ID 발급) 확정.

> **(2026-11-27 각주)** 위 체크리스트의 승인은 `docs/project/GOWAIT_IMPLEMENTATION_PLAN.md` §6의 **C-1~C-12 12행**으로 갈음했다(안 C 확정, `go!`/`flow.parallel { }` 보류, 안 A 미승인). 그 결과 P00~P08이 구현·커밋 완료됐다. 위 `[ ]`는 작성 시점 상태로 보존한다.

---

## 8. 결론

**구현 착수 전 사용자 승인 필요.**

이 문서는 조사와 설계만을 담는다. 코드 수정, 테스트 변경, 게이트 스크립트 변경,
커밋은 모두 하지 않았다. 사용자가 §7.3 체크리스트를 승인하고 별도 Work를 발급하기
전까지는 `rust/**`, `test/**`, `scripts/**`, `bench/**`를 건드리지 않는다.
승인 없이 진행 가능한 유일한 후속 작업은 이 RFC의 문구 보완(설계 문서만)이다.

> **(2026-11-27 각주)** 위 결론은 2026-11-24 작성 시점 한정이다. 사용자가 승인을 갈음한 뒤 안 C가 구현·커밋 완료됐으므로(P08 `488bf0b`, D-099), 현재 잔여는 **P09 문서 정리**(§4 각주의 TD-54/TD-55/TD-56 포함)뿐이다. 본 RFC는 historical 제안 문서로 보존한다.

---

### 근거 파일 색인 (확인 시점)

- `rust/src/compiler.rs:261-290,2156,2162`
- `rust/src/vm/mod.rs:1254-1306` / `rust/src/vm/transfer.rs:176-177`
- `rust/src/native/flow.rs:9-98,145-249`
- `rust/src/runtime/{obj.rs:11,30-49, scope.rs:1-7,42-67,283-304, context.rs, function.rs:265,604, world/mod.rs:19-64, world/executors.rs:9-32}`
- `rust/nrt/src/{class.rs:16,81-110,217-255, trace.rs:16-47, obj.rs:35-64, list.rs:5-8, dict.rs:109, globals.rs:11, closure.rs:12-13, abi.rs:80-93, native.rs:604}`
- `docs/project/{DECISIONS.md:90-99,557-567,633-644,646-663,1057-1066, NATIVE_CONCURRENCY_DESIGN.md, COMPILER_LANGUAGE_ROADMAP.md:8-29}`
- `docs/guides/COMPILED_LANGUAGE.md:222-241`, `docs/spec/ARCHITECTURE.md:24-25`
- `scripts/{exe-diff.ps1, differential.ps1, edge-fuzz.ps1}`, `test/unit/runtime/async.un`, `AGENTS.md`
