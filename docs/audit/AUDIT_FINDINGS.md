# AUDIT_FINDINGS.md — Un 프로젝트 감사 발견 사항 (Phase 2)

> **2026-11-18 갱신**: 각 항목의 상태는 `AUDIT_PROGRESS.md`(수정 내역)와
> `AUDIT_FINAL.md`(최종 결과)를 참고하라. 이 문서는 발견 시점의 스냅샷이다.
>
> 작성: 2026-11-18. 기준: `main` @ `2f67935` + 사용자 미커밋 변경(JIT nested call 등) 포함 워킹 트리.
> 모든 P1 항목은 코드 리딩으로 원인을 확인하고, 가능한 경우 실제 실행으로 재현했다
> (재현 표시가 `[run]`인 것은 `unvm.exe`로 확인, `[read]`는 코드 리딩으로 확인).

심각도: P0 치명(UB/데이터 손상/프로세스 중단) · P1 핵심 의미론 오류/보안 · P2 성능·유지보수 · P3 정리.

## A. 정확성 / 의미론 (P1 중심)

| ID | 영역 | 문제 | 재현 | 심각도 |
| --- | --- | --- | --- | --- |
| A1 | frontend/lexer | 주석 전용 라인이 `Indent`/`Dedent`를 발생시켜, 블록보다 깊게 들여쓰인 주석/최상위 주석/괄호 안 주석에서 `unexpected primary token Indent` (또는 빈 함수 본문이 조용히 허용) | `[run]` `x = 1` 뒤 `    # c` → 오류 | P1 |
| A2 | frontend/optimize | compound assignment가 LHS를 두 번 평가 — `make().v += 10`에서 `make()`가 2회 호출되고 store는 버려진 임시 객체로 감 (side effect 중복 + lost update). Property/Index target 공통 | `[run]` (호출 횟수 2) | P1 |
| A3 | frontend/optimize | int 상수 폴딩이 `checked_add/sub/mul` → 컴파일 시간 `System.OverflowException`; 런타임은 wrapping (`int(MAX)+1 == int(MIN)` 테스트도 있음). `x = 9223372036854775807 * 2` → 프로그램 전체 컴파일 실패 | `[run]` | P1 |
| A4 | frontend/optimize | float `a // b` 폴드가 `(a / b) as i64` (saturating, NaN→0); 런타임은 `cast_long` (NaN/∞→i64::MIN sentinel). `1e30 // 1.0` 상수 경로 `9223372036854775807` vs 런타임 `-9223372036854775808` | `[run]` | P1 |
| A5 | compiler/optimize | int `**` 폴드 `d > i64::MAX as f64` 경계에서 `2**63`이 `2^63.0`로 통과 후 saturate (Runtime도 동일 버그, 동작은 일치). `>=`로 통일 권장 | `[read]` | P2 |
| A6 | compiler/VM | (compile/VM subagent 보고 대기 중) | | |
| A7 | JIT | **nested JIT→JIT call이 callee의 runtime tag를 `== 2`만 검사.** void callee(TAG_NONE=5)가 RET_INT로 오인되어 Int(0)으로 바인딩 → interpreter와 다른 결과 | `[run]` UV_JIT=0 `'none' + int` 오류 vs UV_JIT=1 → `1` 출력 | P1 |
| A8 | JIT | `RET_INTLIST` 포함 callee kind는 runtime tag 검사 없이 IntVec 핸들로 바인딩 (`un_list_int_len(5)` = -1이 "길이"로). A7과 동일 시정 | `[read]` | P1 |
| A9 | JIT | `Op::Return`이 `instance_regs`/`self_regs`/`method_regs`/`class_ctor_regs`를 검사하지 않아 `return self`(class method)가 raw `Rc` 포인터를 Int로 반환 → 포인터 누설 + 값 소실. 코드 주석("instance는 bails")과 불일치 | `[read]` | P1 |
| A10 | runtime | **KeyMap hash/Eq 불일치 (Int vs Float)**: `hash_code(Int(1))=1`, `hash_code(Float(1.0))=to_bits(1.0)`인데 `key_eq`는 f64 비교로 true → `d[1]='a'; d[1.0]='b'` → 같은 key 두 항목, `d[1]` 비일관. S1-11 fix는 -0.0만 커버 | `[read]` | P1 |
| A11 | runtime | **key_eq 비대칭 (Int vs Enu)**: `enu_eq_int`true / `int_eq_enu`unsupported(false). 같은 hash(1)에 다른 판정 → 삽입 순서에 따라 dict/set에 중복/별칭 | `[read]` | P1 |
| A12 | runtime | **KeyMap/Set borrow 하에서 user code 실행 → RefCell double borrow → 프로세스 abort.** 확인 사이트: `dict_to_str` (623 패닉 `[run]`), `set_to_str`, `dict_equals`/`set_equals`, `dict_in`(overlap), `set_binop`(union/except/intersect), `dict_get_item`/`set_get_item` (None 분기 display), `members.rs` 경유 포함, `vm::ArgKwSpread` (비-Str key repr), `obj.rs:957` contains_key | `[run]` `str({"k": Boom()})` — `__str__`에서 `d["new"]=1` → `RefCell already borrowed`, exit 1 | P1 |
| A13 | runtime | **순환값 → 무한 재귀 → stack overflow (hash/eq/display)**: `l = []; l.append(l)` 후 dict/set 삽입(hash_code 재귀) 또는 `l == l`/`str(l)` → abort. `Rc::ptr_eq` fast path 없음 | `[read]` | P1 |
| A14 | runtime/iters | `iters::Enumerator::next`가 `Step::Panic`을 `Some(p)`로 매핑 → `zip`이 predicate 패닉 객체를 튜플 원소로 흘려보냄 (조용한 데이터 오염) | `[read]` | P1 |
| A15 | runtime/int_list | `un_list_int_set` outer-row swap: `slots[old_idx].refs -= 1` — refs가 0일 수 있음 (`IntListArena::free`가 0-ref 슬롯을 drop하지 않음) → release에서 wrap→4294967295, debug에서 panic-across-FFI | `[read]` | P1 |
| A16 | runtime/int_list | `free_force`가 `refs` 무시하고 슬롯을 `take()` → `P1`,`P2`가 공유하던 row가 `P1` 해제 시 삭제 → `P2`는 dead handle → snapshot 실패 → JIT call 전체 discard (정답은 맞지만 JIT 성능 상실). `free`는 0-ref drop을 하지 않아 zero-ref rows 누적 | `[read]` | P1/P2 |
| A17 | runtime/date | `date::parse("2024-01-01 25:99:99")` — hour/minute/second 범위 검사 없음 → 다음 날로 롤오버; `hour=10^14` → `hh * 36_000_000_000` i64 overflow → release wrap/debug panic | `[read]` | P1/P2 | → **수정됨** (`runtime/date.rs` hour/min/sec `0..=23/59/59` 검증) |
| A18 | runtime/iters | `len(repeat(xs, -1))`, `len(range(0, i64::MAX, 2))` — `UnOp::Len`이 bounded 검사 없이 `count()` 호출 → wrapping으로 거짓 값; `Range` 진행이 `wrapping_add(step)` → `range(MAX-1, MAX, 2)` 무한 루프; `count(Filter)`는 predicate를 2번 실행 | `[read]` | P2 |
| A19 | native/fs | **`fs.move`가 rename 전에 destination을 삭제 (source가 정규파일이 아닐 때)** — `fs.move(dir, "dst.txt")` → `dst.txt`가 디렉토리가 되고 기존 파일 내용 영구 소실, 성공 반환 | `[run]` 확인 — 파일 내용 날아감, exit 0 | P1 |
| A20 | native/net/http | **HTTP request/header CRLF injection**: `set_header`가 key/value에 CR/LF 검증 없음, `build_request`가 raw로 기록, `parse_uri`가 path의 CR/LF도 막지 않음. 라이브 TCP 캡처로 확인됨 (서브에이전트) | `[read]` + 외부 검증 | P1 |
| A21 | native/random | `random.range(i64::MIN, i64::MAX)` — `max - min + 1` overflow → len=-1 → 1,000,000 cap 통과 → ~2^64 iteration → hang/OOM. `random.int(i64::MIN, i64::MAX)` — `span as u64` = 0 → `below(0)` = 0 → 항상 `i64::MIN` | `[read]` | P1 |
| A22 | native/ws | WS receive: continuation frame 재조립 총량 cap 없음 + ping flood 무한 루프 → 단일 `receive()`가 메모리/CPU 고갈 | `[read]` | P1 |
| A23 | native/io | **`io.open`이 sandbox allow-list 우회**: absolute path가 `PathBuf::push`에서 base를 대체 → playground sandboxed 코드가 호스트의 임의 파일을 r/w. `use` 기반 module gate와 무관하게 동작 (WASM에서는 fs 백엔드 없어 실질 영향 없음; native embedding에 치명적) | `[read]` | P1/P2 |
| A24 | native/net | `parse_uri` — bracketed IPv6 `[::1]`가 `valid_host`의 `[` 분기에서 `[`/`]` 문자 자체가 허용 집합에 없어 **모든 IPv6 리터럴 URL 파싱 실패** (dead code branch) | `[read]` | P2 |
| A25 | native/random | `random.bytes(n)`/`random.string(n)`/`random.choices(x, n)` — count cap 없음 → OOM-by-argument | `[read]` | P1/P2 |
| A26 | runtime/vm | `pool.submit` 미래값이 error 객체를 그대로 담고 `Op::Await`이 `is_err` 미검사 → pooled 함수가 에러를 반환하면 값이 되어 흘러감 (`wait`이 재-raise하지 않음) | `[read]` | P2 |
| A27 | frontend/parser | 같은 줄에 문장 2개 (`x = 1 y = 2`) 허용 — 개행 누락을 오류로 보고하지 않음 | `[read]` | P2 |
| A28 | frontend/parser | `0x8000000000000000` (i64::MIN 패턴) 리터럴 거부 — bitwise 언어에서 거부 ("integer literal is too large") | `[read]` | P3 |
| A29 | frontend/optimize | folder의 `a / b`가 debug build에서 i64::MIN / -1 → panic (latent; release에서는 wrap) | `[read]` | P2 |
| A30 | runtime/flow | atomic overflow wraps silently (TD-39 기존 문서화) — `pool` worker_count 미사용 등 — 기존 문서화 유지 | 문서화됨 | P2(기존) |
| A31 | runtime/scope | `scope::snapshot` deep clone — snapshot의 슬롯 테이블은 `slot_scope`가 그대로 복사되어 `is_owner == false` → 절대 읽히지 않음. clone의 절반은 순수 낭비 | `[read]` | P2 |
| A32 | JIT | hand-written bail exit들이 `un_jit_depth_exit()`를 호출하지 않아 JIT_DEPTH 누수 → 1000회 누수 시 depth guard가 영구 발동 → 프로세스 전체 JIT 무한 fallback (의미론은 유지) | `[read]` | P2 |
| A33 | JIT | length fast path가 `un_list_int_len`의 -1(dead handle) 센티넬을 검사하지 않음 (a8의 파생) | `[read]` | P2 |
| A34 | JIT | end-of-stream `builder.ins().return_(&[])` (emit_stream 종단, 모든 proto가 Return으로 끝나므로 현재 도달 불가) — hidden pair 미기록 → 호출자는 (0, TAG_INT) 읽음 | `[read]` | P3 |
| A35 | JIT | `xs.add(v)` push 실패(0 반환)를 무시하고 값 바인딩 | `[read]` | P2 |
| A36 | JIT | `un_instance_int_ok`/`un_instance_int_set`(positional, name check 없음)이 FFI로 여전히 exported — 향후 positional 호출자 재등장 시 layout check 우회 | `[read]` | P3 |
| A37 | JIT | `Op::Return`이 intlist 반환 시 `keep_all_intlist=true`로 frame 내 모든 dead IntVec handles 정리 생략 → arena 누적 | `[read]` | P2 |
| A38 | runtime/int_dict | `alloc`이 tombstone 재사용 안 함; `clear()`가 `slots.clear()`로 id 재발급 → teardown 후 stale id가 다른 dict를 가리킴 (documented teardown-only, 취약) | `[read]` | P3 |
| A39 | runtime/iters | `count(Filter)`가 filter를 materialize하면서 predicate를 실행하고 이후 실제 iteration에서 또 실행 — side-effect predicate 2회 실행 | `[read]` | P2 |
| A40 | native/http | 3xx/4xx 처리 불일치: `get`은 non-2xx에서 오류, 나머지는 body 그대로 반환; redirect 없음 (.NET reference와 불일치, 문서화 안됨) | `[read]` | P2 |
| A41 | native/date | TD-41 미커버: `add_months/add_minutes/add_seconds/add_ms/add_years` i64 wrap (same class); `ticks_from_civil` wrap | `[read]` | P3 |
| A42 | native/regex | `find_all` start-scan: 위치마다 10M step 예산 리셋 → `(a+)+b` on "aaa…"에서 len×10M (TD-36 파생인데 문서 미기재) | `[read]` | P2 |
| A43 | runtime/enum | **`enum` 멤버에 `Color.RED`로 접근하면 `'type' object has no attribute 'RED'`** — 문서화된 언어 기능이 현재 빌드에서 완전히 동작하지 않음. 원인: LoadName(클래스명)→class_tobj, `get_attr_from`이 TObj의 등록 클래스 객체로 위임하지 않음. 테스트도 없음 | `[run]` | P1 |

## B. 성능 (P2)

| ID | 위치 | 문제 | 비고 |
| --- | --- | --- | --- |
| B1 | obj.rs:626, obj.rs:881 | TD-08/TD-10 확인 — invoke/call args.to_vec, set_attr_from 3× clone (위치 이동: 515→626, 743→881) | 기존 부채 |
| B2 | scope.rs | TD-09 확인 (snapshot deep clone + slots clone은 읽히지도 않음, A31) | 기존 부채 |
| B3 | jit_bc.rs | TD-51 확인 — compile 실패 silently swallowed (stub) | 기존 부채 |
| B4 | jit_cache.rs:92 | `unsafe impl Send for JitEntry` — 단일 스레드라 현재 sound, clear_program 후 PLAN_CACHE stale Entry (P3) | 기존 TD-05 |
| B5 | int_dict.rs:46 | arena `slots` 무한 성장 (tombstone alloc 미재사용) | 기존 TD 일부 |
| B6 | source.rs | TD-11 확인 (Source::get_line O(n)) | 기존 부채 |

## C. 테스트/도구/문서 (P2/P3)

| ID | 위치 | 문제 | 심각도 |
| --- | --- | --- | --- |
| C1 | scripts/test.ps1 | `[FAIL]`인데 assertion 0 && exit≠0 → exit code 0으로 통과 (badFiles 미사용) | **P2** |
| C2 | scripts/test.ps1, differential.ps1, unbc-roundtrip.ps1 | per-test timeout 없음 — hang 시 suite 전체 정지 | **P2** |
| C3 | differential.ps1 | "JIT on" lane에 UV_JIT=1 명시 (기본값과 어긋날 수 있음) | P3 |
| C4 | unbc-roundtrip.ps1 | source tree에 .unbc 쓰고 finally 삭제 | P3 |
| C5 | docs | 카운트 drift: 1415→1419 (README/docs README/TEST_GUIDE), 204→205 (ARCHITECTURE), 81파일/33,633줄/86 .un→83/36,518/88 (AGENTS/REFACTOR_LOG), D-072→D-077 (3개 문서), PROJECT_AUDIT §6 "현재 상태" 프레임 stale | P3 |
| C6 | docs/DECISIONS.md | D-012 "Proposed" + 삭제된 파일 참조 (D-054는 Accepted/implemented) — 모순; D-065 "199/199" stale | P3 |
| C7 | negative manifest | empty substring 2행 + 단일문자 substring 행 여럿 — anchoring 권장 | P3 |
| C8 | 테스트 누락 | JIT corpus 전무 (.un에 JIT 경로 강제 테스트 없음), `.unbc` corruption negative, go-future body throw → wait 재-raise 미테스트 | P2 |
| C9 | vendor | vendor/README.md region upstream 오기재; TD-31의 인용이 현재 README와 불일치 | P3 |
| C10 | BENCHMARK.md | math10000 unjit "+139312.8%" 등 bogus baseline CSV 기반 "회귀" 표시 — baseline이 물리적으로 불가능 | P3 |
| C11 | bench.ps1 | 결과 파일 미검증, geomean 누락 lane 조용히 제외, exit code가 측정 실패만 반영 | P3 |

## D. 코드 품질 / clippy (P3)

| ID | 위치 | 문제 |
| --- | --- | --- |
| D1 | rust/src/jit_bc.rs:3499 | clippy error: self-assignment (`Op::Unwrap` arm) — 사용자 미커밋 파일 (보고 후 수정) |
| D2 | 전체 | clippy 155 warnings (borrowed expression implements required traits 33, same-type cast 13, manual 11, … + type_complexity, redundant guard) |

## E. 확인했으나 문제 아님 / 기존 부채 유지

- `Op` enum `#[repr(u8)]` 고정 — 변경 금지 (유지).
- .NET exception 이름 문자열 — negative manifest 계약 (유지, TD-27).
- `LoadUpv`/`StoreUpv` 등 ABI placeholder — 제거 금지.
- `native::init` 명시적 등록 — 어트리뷰트 기반 도입 금지.
- `atomic`/`pool`/`lock` cooperative (TD-39) — 설계상 의도; `pool.worker_count` 미사용도 의도된 shim.
- `PLAN_CACHE` 무한 성장 (TD-03), JIT native code 미해제 (TD-05), JIT silent stub (TD-51) — 기존 문서화, 이번에 재확인만.
- KeyMap hash Int/Float (A10) 외에 `-0.0` 정규화는 기존 fix(S1-11)로 정확함을 재확인.
- 런타임 wrapping int op(`wrapping_add` 등) — 런타임 semantics는 wrapping이 의도(테스트 있음). A3는 folder만의 문제.
- `id_of = idx + ID_BASE` outer-iterator 2^61 충돌 — 실질 도달 불가 (P3, 가드 권장).
- IDEMPOTENT: `cargo build` 0 warnings, `cargo test --release` 205/205, test.ps1 green, negative 41/41 — 현재 main은 녹색이나 A1/A13/A19/A43 등은 green suite가 감지하지 못하는 결함.

## F. 기존 감사(docs/PROJECT_AUDIT.md) 재확인

- S1-1..S1-21 fix 상태: 재검증 없이 신뢰하지 않음 — frontend/runtime subagent가
  확인한 결과 A10은 S1-11의 미커버 잔여 항목이며, 새 결함은 별도 목록으로 올렸다.
- S2-3의 "IntDict arena needs catch_unwind"은 그대로 유효 (TD-40).
- TD-04/TD-06/TD-07 등 "resolved" 항목은 이 감사에서 재개방하지 않음.

## G. Compiler/VM subagent 보고 (A6)

(상세는 `AUDIT_PLAN.md` Step 1 항목에 반영됨)

- **F-1 (P0)** `plan_for` `.expect` — module 로드 경로(`with_context`)가 `validate()`를
  건너뛰어 256-register 초과 함수에서 프로세스 abort → `module.rs`에서 실행 전 validate 추가.
- **F-2 (P0)** `ExcEntry.var_slot` 미검증 — `proto.names.len()` 범위 검증 추가.
- **F-3 (P0)** `proto.locals` 미검증 — `proto.names.len()` 범위 검증 추가.
- **F-4 (P0)** `Pattern::Tuple` 무한 중첩 `read_pattern` 재귀 스택 오버플로 + Literal/Bind
  인덱스 미검증 → validate에 깊이 제한(64) + program-wide 인덱스 검증 추가.
- **F-5 (P0)** `Slice`/`SetSlice` dst가 u8 wrap → 레지스터 OOB 접근 → transfer에서 dst 음수/256+ 시 명시적 오류.
- **F-6 (P0)** `Op::ArgPos` 등에서 `args.last_mut().expect` panic → `malformed bytecode` Error로 변환.
- **F-7 (P1)** verifier와 translator의 depth 분석이 서로 다름(핸들러 seeding, dead code) →
  verifier가 `transfer::analyze`에 위임하도록 통합.
- **부가** `unbc::read`의 `Vec::with_capacity(count)` 앞에 남은 바이트 수 기반 일괄 guard 추가.
