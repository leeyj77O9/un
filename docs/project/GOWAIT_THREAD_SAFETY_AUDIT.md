# GOWAIT_THREAD_SAFETY_AUDIT.md — opt-in 병렬(`un build --parallel`) 스레드 안전성 감사

> Work ID: **W-2026-11-25-P01** / 역할: ARC-00 주재 + NRT-06/RT-03 공동 검토 (무코드 감사)
> 작성일: 2026-11-25 / 상태: **review** — 정적 감사 산출물 완료. 코드·테스트·스크립트·기존 문서 변경 없음, 커밋 없음.
> 작업 디렉터리: `C:\Users\User\.local\share\opencode\worktree\20fb9f\gowait-p01`
>
> 선행 필수 읽기: `AGENTS.md` MANDATORY(규칙 1~8), `docs/project/GOWAIT_IMPLEMENTATION_PLAN.md` §4/§5,
> `docs/project/GOWAIT_PARALLEL_RFC.md` §5, `docs/project/DECISIONS.md` D-012/D-054/D-061/D-062/D-080(+ D-063),
> `docs/project/PROJECT_AUDIT.md` §2(severity 정의).
>
> 이 문서는 **RFC §2의 11행 + 감사 중 새로 발견한 항목**에 "기본 경로 / Send 경계 / opt-in 적용 여부"를
> 판정한 것이다. 인용된 `path:line`은 2026-11-25 작업 디렉터리의 실제 파일을 정적 확인한 값이며,
> RFC(2026-11-24)의 줄 번호와 다를 수 있다.

---

## 0. 방법·severity·범위 판정

### 0.1 severity 정의 (`PROJECT_AUDIT.md` §2 관례 준수)

| 등급 | 의미 | 이 감사에서의 판정 기준 |
|---|---|---|
| **S2** | 메모리 안전 / UB | `static mut`, `&'static mut`, raw-pointer 별칭이 두 스레드에서 동시에 도달 가능. **opt-in 착수 전 반드시 해소.** |
| **S1** | 정확성 / 결정성 | UB는 아니지만 결과·traceback·에러 문구·순서가 스케줄에 따라 달라져 명세 계약(D-062/063/080)이나 byte-identical 게이트가 깨짐. |
| **S4** | 정보 / 이미 보호됨 | `Mutex`/`OnceLock`/atomic으로 이미 보호되거나 병렬 경로에서 실행되지 않아 조치 불필요(확인만). |

### 0.2 감사 전제

- opt-in 진입점은 **`un build`의 plain native exe 전용**이다(계획 §1.1). `un run`(VM), `un native`(Vm 모드),
  `un build --embed`, `un check`는 기본 deferred를 유지한다(I1). 따라서 아래 **VM 항목(A-22~A-28)은
  "VM 자체를 병렬화할 경우"의 잠재 위험도이며, opt-in 경로에서는 활성화되지 않는다.**
- 기본 경로의 byte-identical 게이트 8종은 **수치도 비교 방식도 바꾸지 않는다**(I2). 아表의 "기본 경로 영향"은
  전부 **무변경**이며, 예외는 없다.
- 새 외부 crate는 도입하지 않는다(I8): `std::thread` + `Arc<Mutex>` + `Condvar`만 사용한다.

### 0.3 완결성 체크 (P01 완료 조건)

`rust/nrt/src` 전수 grep 결과 **`static mut` 7개, `&'static mut` 8개**를 확인했고, 아래 표의
A-01~A-16이 모두를 담당한다. **미배정 `static mut` / `&'static mut` = 0건.**
단, RFC §2에 없던 `net_native.rs:21,30`(A-09)과 `obj.rs:64`(A-07)가 새로 발견되었으며,
이 두 항목의 Work 배정은 §4에서 재확인을 요청한다.

---

## 1. 감사 표

> 형식: 항목(현재 동시성 위험) | severity | 근거 `path:line` | 기본 경로 영향 | 필요한 변경 | 담당 Work(P02~P05) | 상태

### 1.1 NRT — 클래스 레지스트리 / 필드 arena (P02)

| 항목 | severity | 근거 `path:line` | 기본 경로 영향 | 필요한 변경 | 담당 Work | 상태 |
|---|---|---|---|---|---|---|
| **A-01** 3개 `static mut` 이름 캐시(`CLASS_ID_CACHE`/`FIELD_IDX_CACHE`/`METHOD_IDX_CACHE`). `addr_of_mut!` raw ptr로 읽고 `*cache.add(slot)=...`로 쓰므로 worker 2개가 같은 slot에 동시 쓰기 → 데이터 레이스 + stale hit 판정 꼬임 | S2 | `rust/nrt/src/class.rs:84-90,95-100,103-113,129-155` | 무변경. "plain exe는 single-threaded" 주석이 유일한 안전 근거이며 single 스레드에서는 hit/write 순서가 현재과 동일 | `thread_local!{RefCell<[NameCacheEntry;16]>}` 또는 `OnceLock<Mutex<...>>`. `cached_index()`의 `in_bounds` hit 검증 로직(`:142-149`) 재사용. 전자는 객체 이동 시 캐시 무효 가능성 있음 | **P02** | open |
| **A-02** `FIELD_ARENA`/`FIELD_BUMP` bump arena. `reserve`→`set_len`→`as_mut_ptr().add(off)`→`*bump=needed`가 원자적이지 않아 두 스레드가 같은 offset에 필드 블록을 덮어씀 | S2 | `rust/nrt/src/class.rs:277-302` | 무변경. bump 순서는 single 스레드에서 결정적 | `Mutex<Vec<u64>>` + `AtomicUsize`(fetch_add)로 교체. `NObj.b`는 opaque i64 handle 유지 → C ABI 시그니처 변경 0 | **P02** | open |
| **A-03** `instance_fields(p) -> &'static mut [u64]` — arena 포인터를 직접 역참조하여 공유 가변 슬라이스 반환 | S2 | `rust/nrt/src/class.rs:304-310` | 무변경 | handle table + `Mutex` 가드 기반 접근으로 전환, `&'static mut` 반환 제거. C ABI는 `u64` 유지 | **P02** | open |
| **A-04** `alloc`/`obj`/`obj_mut`가 `Box::into_raw` 포인터를 `&'static mut NObj`로 별칭. 두 worker가 같은 handle의 `a`/`b`를 동시 수정 가능 | S2 | `rust/nrt/src/obj.rs:45-57` | 무변경. 객체 수명은 process-long로 동일 | `obj_mut` 경로를 handle + lock 테이블로 전환. immutable `SMALL_INT_OBJS`(`:35-43`)에 `obj_mut`이 호출되지 않는 불변식도 함께 명시 | **P02** | open |
| **A-08** immutable 작은 정수 static 배열 + `PENDING: AtomicU64`. 자체로는 안전하지만 `obj_mut` 불변식과 에러 cell(A-13)의 선제 조건 | S4 | `rust/nrt/src/obj.rs:35-43,47` | 무변경 | small int에 쓰기 경로 없음 확인만. `PENDING`은 A-13에서 처리 | **P02**(확인) | open |
| **A-09** `client_state`/`socket_state`가 `&'static mut net::HttpState` / `&'static mut net::WsState` 반환 — **RFC §2 표에 누락된 `&'static mut` 2개**. 공유 http/ws 객체에 동시 요청 시 상태 레이스 | S2 | `rust/nrt/src/net_native.rs:21-37` | 무변경 | per-object `Mutex` 또는 handle table로 전환. plain exe에서는 http/ws가 도달 가능(D-066)하므로 opt-in 병렬에서 실제 위험 | **P02/P03 배정 미정** → §4 U-01 | open |

### 1.2 NRT — 리스트 / dict / tuple-set (P02)

| 항목 | severity | 근거 `path:line` | 기본 경로 영향 | 필요한 변경 | 담당 Work | 상태 |
|---|---|---|---|---|---|---|
| **A-05** `list::items(p) -> &'static mut Vec<u64>` — 리스트 payload를 raw ptr로 공유 가변 별칭 | S2 | `rust/nrt/src/list.rs:8-15` | 무변경 | handle + `Mutex<Vec<u64>>` 인덱스 테이블 | **P02** | open |
| **A-06** `dict::data(p) -> &'static mut DictData` — `pairs`에 대한 공유 가변 별칭 | S2 | `rust/nrt/src/dict.rs:109-113` | 무변경 | handle + `Mutex<DictData>` 인덱스 테이블 | **P02** | open |
| **A-07** `seq_items(p) -> &'static mut Vec<u64>` (Tuple/Set tag 14/15) — **RFC §2 표에 누락된 별칭**. element vector 공유 가변 | S2 | `rust/nrt/src/obj.rs:17-22,64-74` | 무변경 | handle + `Mutex<Vec<u64>>`. A-04와 동일 테이블 재사용 가능 | **P02** | open |

### 1.3 NRT — 전역 테이블 / capture / ABI (P03)

| 항목 | severity | 근거 `path:line` | 기본 경로 영향 | 필요한 변경 | 담당 Work | 상태 |
|---|---|---|---|---|---|---|
| **A-10** `GLOBALS: Mutex<Vec<u64>>` — 테이블 자체는 안전. 단 slot swap의 선후가 스케줄 의존 → opt-in에서 `unrt_global_set` 동시 호출은 last-writer 비결정 | S4 | `rust/nrt/src/globals.rs:7,11,21-45` | 무변경(기존 plain build와 slot 번호·값 동일) | 구조 변경 불필요. opt-in의 동시 global 쓰기는 문서화된 차이로 취급(P09). 기존 `unrt_*` 시그니처 변경 0 | **P03** | won't-fix(구조) / opt-in 문서화 |
| **A-11** `LAMBDAS: LazyLock<Mutex<HashMap>>` 등록 테이블은 보호되나, `ClosureData.captures: Vec<u64>`를 `&'static ClosureData`로 공유 → capture handle의 `Send` 보장이 명시되지 않음 | S2 | `rust/nrt/src/closure.rs:12-13,16-28,31-49` | 무변경 | capture 값의 `Send` 요건 명시 + A-04/A-05 해소 후 "어떤 handle이 스레드 간 이동 가능한지" 판정표 작성 | **P03** | open |
| **A-12** `usize → extern "C" fn` transmute. 함수 포인터 자체는 `Send+Sync`지만, callee가 A-01~A-07의 정적 가변 상태를 읽으면 여전히 UB | S2 | `rust/nrt/src/abi.rs:32-93` | 무변경 | 코드 변경보다 **검증 게이트**: P02 완료 후 callee 경로에 `static mut`/`&'static mut`이 0임을 확인해야 안전 선언 가능 | **P03(P02 선행)** → §4 U-03 | open |
| **A-13** `PENDING: AtomicU64` 단일 전역 error cell + `unrt_take_error`/`unrt_write_error`/`unrt_dump_uncaught`. atomic이라 메모리 안전하지만 worker 간 에러가 서로를 덮어씀 → 잘못된 traceback/에러 문구 | S1 | `rust/nrt/src/obj.rs:47,438-451`; `rust/nrt/src/trace.rs:240-242` | 무변경 | task-local error cell로 전환하고 `unrt_wait` 결과에 `(value, tag, trace, err)` 포함. `obj.rs`는 P02 소유, trace 흐름은 P04 소유 → 공동 조정 필요 | **P02+P04 공동** → §4 U-02 | open |

### 1.4 NRT — traceback (P04)

| 항목 | severity | 근거 `path:line` | 기본 경로 영향 | 필요한 변경 | 담당 Work | 상태 |
|---|---|---|---|---|---|---|
| **A-14** `static mut FRAME_DEPTH: usize` + `addr_of_mut!().read()/write()`. push/pop이 Raw ptr RMW라 worker 간 깊이가 섞임 | S2 | `rust/nrt/src/trace.rs:17-37,119-142` | 무변경(Recursion benchmark의 RMW 제거 최적화는 single 스레드에서 동일) | `thread_local!{Cell<usize}>`. worker별 깊이 → wait 시점 병합 | **P04** | open |
| **A-15** `FRAME_SLOTS: OnceLock<Vec<[AtomicU64;4]>>` 전역 512슬롯. `nrt_push_frame`이 depth 인덱스에 기록하므로 두 스레드 프레임이 서로 덮어써 traceback 프레임이 뒤섞임 | S1 | `rust/nrt/src/trace.rs:22,38-46,119-137,158-173` | 무변경(프레임 순서·collapse·compact 출력 바이트 동일) | task-local 슬롯 + `unrt_wait` 결과의 trace snapshot. wait 스레드에서 직렬 출력(I5 에러 문구 유지) | **P04** | open |
| **A-16** `ERROR_SITE: Mutex<Option<(usize,usize)>>` 전역 단일 cell. `nrt_set_error_site`/`take_error_site`/`traceback()`이 공유 → 에러 위치가 뒤섞여 stderr 바이트 불일치 | S1 | `rust/nrt/src/trace.rs:47,144-156,158-173,240-242` | 무변경 | task-local error site + snapshot을 결과 버퍼에 포함. `try/using exit` 문구는 그대로(I5) | **P04** | open |
| **A-17** `SOURCE: Mutex<Option<SourceInfo>>` — 등록 후 read-only, 이미 Mutex 보호 | S4 | `rust/nrt/src/trace.rs:16,96-117` | 무변경 | 변경 불필요 | 없음 | won't-fix |

### 1.5 NRT — 스케줄러 / opt-in 계약 (P05, P06 밖)

| 항목 | severity | 근거 `path:line` | 기본 경로 영향 | 필요한 변경 | 담당 Work | 상태 |
|---|---|---|---|---|---|---|
| **A-18** 공유 태스크 큐·worker·드레인이 **아직 존재하지 않음**. `rust/nrt/src/scheduler.rs` 없음; 유일한 스레드 API는 CPU 수 조회뿐 | S1 | `rust/nrt/src/native.rs:353`(`available_parallelism`), `rust/nrt/src/` 파일 목록 | 무변경(새 모듈만 추가) | 신규 `scheduler.rs`: `std::thread::Builder` + `Arc<Mutex<VecDeque<Task>>>` + `Condvar`, graceful drain+join, `close()` 후 `"pool is closed"` 유지, 자기 참조 wait inline fallback | **P05** | open |
| **A-19** 순서 계약 미정의: 태스크 시작 순서 미보장, `go` body 직접 stdout 미보장, 다중 예외의 수집 순서 = wait 호출 순서, 동일 future 재`wait` memoization | S1 | `docs/project/GOWAIT_PARALLEL_RFC.md:135-147`(§3.2 계약 초안), `docs/project/GOWAIT_IMPLEMENTATION_PLAN.md:136-145` | 무변경(기본 deferred에서는 시작 순서가 결정적) | P05가 계약을 API로 노출, P07이 `pool.map` 입력 순서 보장. opt-in 게이트는 `RESULT|` 집합 비교로 검증 | **P05(+P07)** | open |
| **A-20** plain build의 `go`/`wait` 명시 거부가 opt-in 해제 지점. 현재 거부 메시지는 test-visible 계약 | S4 | `rust/src/compiler.rs:272,277` | 무변경(거부 메시지 유지, I5) | opt-in에서만 해제. opcode `MakeFuture=0x63`/`Await=0x64` 유지(I3) | 없음(**P06**) | won't-fix(기본) |
| **A-21** RFC §2의 `native.rs:604` 인용은 현재 파일에서 확정되지 않음(줄 내용이 정수 범위 검사). flow/atomic 자체가 NRT에 미구현인 사실은 유지 | S4 | `rust/nrt/src/native.rs:590-614`; `rust/src/compiler.rs:272` | 무변경 | 구현은 P06/P07 범위. 본 감사에서는 조치 없음 | 없음(**P06/P07**) | won't-fix |

### 1.6 VM 측 — opt-in에서는 병렬화되지 않음 (전부 won't-fix for opt-in)

> 아래는 **만약 VM을 병렬화한다면** 생기는 위험도이다. opt-in은 plain exe 전용이므로 활성화되지 않으며,
> `AGENTS.md`의 "plain exe는 VM을 전혀 포함하지 않는다"와 `class.rs:1-2`("no bytecode or VM class registry needed")가
> 이를 뒷받침한다. I1~I3에 따라 **기본 경로 변경은 0건**이다.

| 항목 | severity(잠재) | 근거 `path:line` | 기본 경로 영향 | 필요한 변경(참고용; opt-in에서는 미실시) | 담당 Work | 상태 |
|---|---|---|---|---|---|---|
| **A-22** `Obj = Rc<RefCell<ObjData>>`, payload 전부 `Rc<RefCell>` → `Send` 아님 | S2 | `rust/src/runtime/obj.rs:1-2,11,22-60` | 무변경 | VM 병렬화 시에만 `Arc<Mutex>` 전환. opt-in은 NRT 경로만 사용 | 없음 | won't-fix for opt-in |
| **A-23** `World = thread_local!` — stdout/log/global_scope/natives/module_cache가 스레드별로 분리됨. worker가 자기 WORLD를 갖게 되면 공유 상태 깨짐 → **VM 병렬화가 NRT보다 범위가 큼** | S2 | `rust/src/runtime/world/mod.rs:19-56,58-60` | 무변경 | VM 병렬화 시 전역 공유 재설계 필요. 안 C는 이 문제를 우회하기 위해 opt-in을 native 전용으로 둠(RFC §2 단서) | 없음 | won't-fix for opt-in |
| **A-24** `Frames`/`Traces`/`Context`가 `Rc<RefCell>` — 공유 frame/trace에 동시 push 가능 | S2 | `rust/src/runtime/context.rs:9-29,37-49` | 무변경 | VM 병렬화 시 task-local trace 필요(P04와 동일 문제의 VM 버전) | 없음 | won't-fix for opt-in |
| **A-25** `FnCell = Rc<RefCell<FnData>>`, `args: Rc<RefCell<Vec<ArgData>>>`, `local_names_cache: RefCell` | S2 | `rust/src/runtime/function.rs:71,97,102,265` | 무변경 | VM 병렬화 시 `Arc<Mutex>` 전환 | 없음 | won't-fix for opt-in |
| **A-26** `snapshot()`이 `d.slots.clone()` — `Obj`(Rc) 얕은 복사로 value cell 공유. D-080의 capture-by-value 계약상 기본 경로에서는 의도된 동작이지만, worker 간 공유되면 레이스 지점 | S1 | `rust/src/runtime/scope.rs:7,42-56,283-304`; `rust/src/vm/mod.rs:1257-1261` | 무변경(capture-by-value 관측 동작 동일) | opt-in은 `Send` 스냅샷 별도 전달 + value cell 복사. 기본은 D-080 유지 | 없음 | won't-fix for opt-in |
| **A-27** `FutureState`/`PoolState`/`LockState`가 `Rc<RefCell>`, `pool_submit`/`pool_map`이 `fn_call` 직접 호출(동기) | S1 | `rust/src/native/flow.rs:9-23,41-68,163-205` | 무변경(`async.un` 기대값 수정 금지, I1/C-8) | opt-in에서는 NRT 스케줄러(P05)가 대체하므로 VM flow 수정 없음 | 없음 | won't-fix for opt-in |
| **A-28** `MakeFuture`가 scope snapshot을 담기만 하고 `Await`가 **호출 스레드에서** `execute_proto` 후 `state.borrow_mut().value` memoize — deferred cooperative 계약 그 자체 | S4 | `rust/src/vm/mod.rs:1254-1306`(특히 `1284`, `1304`) | 무변경(opcode·memoization·에러 전파 경로 전부 유지) | 변경 금지(I1/I3). D-012/D-054/D-063의 계약 | 없음 | won't-fix for opt-in |

---

## 2. VM 경로 변경 필요 여부 — 결론

**결론: VM 측 코드 변경은 필요 없다(0건).** opt-in `un build --parallel`은 plain native exe만 대상이고
(계획 §1.1), plain exe는 VM을 포함하지 않는다. 따라서 A-22~A-28은 "VM 자체를 병렬화할 때"의
잠재 위험도로 기록만 하고 **모두 `won't-fix for opt-in`**으로 분류한다.

근거:

1. `rust/src/compiler.rs:272,277` — plain build는 `go`/`wait`를 명시 거부하므로, 현재 plain exe에는
   VM의 deferred future 경로가 아예 도달하지 않는다.
2. `rust/nrt/src/class.rs:1-2` — NRT는 "no bytecode or VM class registry"를 전제로 독립 실행된다.
3. `rust/src/runtime/world/mod.rs:58-60`의 `WORLD: thread_local!` 때문에 VM 병렬화는 stdout·natives·module_cache
   공유 재설계까지 요구되어 **NRT 병렬화보다 엄격히 큰 범위**다(RFC §2 단서). 안 C는 이 범위를 우회한다.
4. I1/I2/I3에 따라 `un run`/`--embed`/`unbc-roundtrip`/`differential`의 기본 경로는 바이트 동치여야 하므로,
   VM 타입을 `Arc<Mutex>`로 바꾸는 순간 게이트가 깨진다.

따라서 **P02~P05의 모든 예산은 `rust/nrt/**`에만 투입**되고, VM 소유 파일
(`rust/src/runtime/**`, `rust/src/vm/**`, `rust/src/native/flow.rs`)은 P01 이후에도 **변경 대상이 아니다.**
P06에서 `Go`/`Wait` TIR variant를 추가할 때도 VM의 deferred 분기는 그대로 drop된다(계획 §2 P06 완료 조건).

---

## 3. P02~P05 예산 재검증

| Work | 계획 인일 | 감사 항목 | 재검증 판정 |
|---|---|---|---|
| **P02** | 3.0 | A-01, A-02, A-03, A-04, A-05, A-06, A-07, A-08(+A-09 배정 시) | 7개 S2를 3.0 인일로 처리하는 계획의 추정은 **유지 가능**. 단 A-09(`net_native.rs:21,30`)가 소유 파일 표(`class/obj/list/dict`)에 없음 → 배정 시 **+0.3~0.5 인일** 필요(U-01). A-13의 `obj.rs` 측도 P02 소유와 겹침(U-02). |
| **P03** | 2.0 | A-10, A-11, A-12(+A-09 배정 시) | 구조 변경은 2건뿐이라 **유지 가능**. A-12는 코드 수정이 아니라 P02 완료 후 검증 게이트이므로 별도 공수 불필요. 단 P02 ∥ P03 착수 규칙(계획 §2.1)과 A-12의 P02 선행 요건이 충돌 → 순서 조정만 필요(U-03), 공수 증가 없음. |
| **P04** | 2.5 | A-13(공동), A-14, A-15, A-16 | 전역 정적 3종 + error cell 이관으로 **유지 가능**. A-13의 `obj.rs` 절반을 P02가 맡으면 2.5 인일 내. P02 미완료 시 A-15/A-16의 trace 병합도 지연되므로 의존 그래프(P02→P04)는 그대로 유효. |
| **P05** | 2.5(그중 0.5 스파이크) | A-18, A-19 | 신규 모듈 1개 + 계약 노출이므로 **유지 가능**. 기존 `unrt_*` 시그니처 변경 0, 기본 게이트 무영향. |
| **합계** | **10.0** | 28개 항목 중 opt-in 해소 대상 15개 + 확인 4개 | **예산 10.0 인일 그대로 유지**(전체 20.0 인일 중 절반). 감사 결과 새로운 Work(P10 등)는 필요 없으며, 누락 항목은 기존 P02/P03 소유 범위 재획정으로 흡수한다. |

> 재검증 결론: **감사가 예산을 올리지 않는다.** 다만 아래 U-01~U-04의 소유권/순서 판정은
> ARC-00 승인 없이는 P02 착수가 `blocked`가 된다(SESSION_SYSTEM §8.2).

---

## 4. 사용자 재확인이 필요한 항목

계획 §6의 C-1~C-12은 **그대로 미승인 상태**로 유지한다(특히 C-4 새 ABI 3심볼, C-5 `--parallel` CLI 옵션,
C-10 하위 Work ID 발급). 이 감사에서 **새로 확인이 필요한 항목**은 다음 4건이다.

- **U-01 (신규, 소유권)**: `rust/nrt/src/net_native.rs:21,30`의 `&'static mut` 2개는 RFC §2 표에도,
  P02(`class/obj/list/dict`)·P03(`globals/closure/abi`) 소유 파일 표에도 없다.
  P02에 추가할지 P03에 추가할지, 추가 공수(0.3~0.5 인일)를 승인하는가?
- **U-02 (신규, 공동 소유)**: `rust/nrt/src/obj.rs:47`의 `PENDING` 에러 cell은 파일 소유가 P02,
  동작 흐름이 P04(traceback 직렬화)에 걸쳐 있다. A-13을 P02+P04 공동으로 둘지, P02가 `obj.rs` 측만
  이관하고 P04가 trace 측만 맡을지 확정하는가?
- **U-03 (신규, 순서)**: 계획 §2.1은 P02 ∥ P03 병렬 착수를 허용하지만, A-12(abi transmute 안전 선언)는
  P02의 정적 가변 제거 완료를 전제한다. "P03의 abi 항목은 P02 완료 후 검증 게이트로만 수행"으로
  순서를 고정하는가? (공수 변화 없음)
- **U-04 (신규, 문서 인용)**: RFC §2의 `nrt/src/native.rs:604` 인용은 현재 줄 내용과 맞지 않는다.
  plain build 거부 지점이 `compiler.rs:272` 하나인지, NRT 쪽에도 별도 fallback 심볼이 필요한지
  확인하는가? (필요 시 P06/P07 범위)
- **U-05 (범위 재확인)**: VM 측 A-22~A-28을 전부 `won't-fix for opt-in`으로 분류한 것 — 즉
  **"un run --parallel" 같은 VM 병렬 옵션은 이번 범위에 없다**는 점을 재확인하는가?

---

## 5. 보고 요약 (SESSION_SYSTEM §2.3)

| 항목 | 내용 |
|---|---|
| **Work ID** | **W-2026-11-25-P01** (선행 W-2026-11-25-GOWAITPARALLEL / P01) |
| **결과** | opt-in plain exe 병렬을 위한 스레드 안전성 무코드 감사 완료. RFC §2의 11행 전수 + 신규 발견 3건을 포함해 판정. `static mut` 7개 / `&'static mut` 8개 전부 담당 Work 배정 → **미해결 0건**. VM 경로 변경 불필요 결론 도출. |
| **작성 파일** | `docs/project/GOWAIT_THREAD_SAFETY_AUDIT.md` **1개만 신규 작성**. 기존 문서·코드·테스트·스크립트 수정 없음. |
| **감사 항목 수 / 심각도 분포** | **총 28항목** — **S2 15건**, **S1 7건**, **S4 6건**. 이 중 opt-in 해소/계약 대상은 NRT의 **S2 11건(A-01~A-07, A-09, A-11, A-12, A-14) + S1 5건(A-13, A-15, A-16, A-18, A-19) = 16건**이며, VM 7건(A-22~A-28)은 전부 `won't-fix for opt-in`. |
| **VM 경로 변경 필요 여부** | **불필요(0건)**. opt-in은 plain native exe 전용이고 plain exe는 VM 미포함(`compiler.rs:272`, `nrt/src/class.rs:1-2`). `WORLD: thread_local!`로 VM 병렬화 범위가 더 크므로(RFC §2) 안 C가 회피. A-22~A-28은 잠재 위험 기록만. |
| **P02~P05 예산 재검증** | **10.0 인일 그대로 유지**(P02 3.0 / P03 2.0 / P04 2.5 / P05 2.5). 누락 항목은 기존 소유 범위 재획정으로 흡수, 신규 Work 불필요. 단 U-01(0.3~0.5 인일)·U-03(순서) 판정 필요. |
| **사용자 재확인 필요** | U-01(`net_native.rs` 소유권+공수), U-02(`PENDING` 공동 소유), U-03(P02∥P03 순서와 abi 검증 게이트), U-04(RFC `native.rs:604` 인용 정정), U-05(VM 병렬 옵션 미포함 확인) + 기존 C-1~C-12 미승인 유지. |
| **Git 상태** | **커밋 없음.** 이 세션은 신규 파일 1개만 추가함(`?? docs/project/GOWAIT_THREAD_SAFETY_AUDIT.md`). 작업 디렉터리의 선행 변경(`M AGENTS.md`, `?? docs/project/GOWAIT_IMPLEMENTATION_PLAN.md`)은 **일체 건드리지 않음**. |
| **완료 판정** | **`review`** (정적 감사 산출물 완성). 구현 착수는 U-01~U-05 및 C-1~C-12 승인과 하위 Work ID 발급 후 — `merged` 아님. |