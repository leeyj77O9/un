# AUDIT_PLAN.md — Un 프로젝트 감사 Phase 3: 개선 계획

작성: 2026-11-18. 근거: `AUDIT_FINDINGS.md` (P0~P4) / `AUDIT.md` (기준선).
원칙: 정확성 최우선, 기존 semantics 보존, 측정 기반 성능 최적화만, 사용자 미커밋 파일 수술은 최소화하며 보고.

## 0. 워킹 트리 규칙

- 사용자 미커밋 파일(`rust/src/jit_bc.rs` 등)은 A7/A8/A9/A32/A33/A35/D1 수정을 위해 **최소 단위**로만 편집하고, 모든 변경은 `AUDIT_PROGRESS.md`에 기록한다.
- `main.un`, `bench/string.un`, `docs/CODE_NORMALIZATION_REPORT.md` 미수정.
- opcode 값(`Op` repr) 불변. `.unbc` 계약 불변. `.NET` 예외 문자열 불변(TD-27).

## 1. 수정 순서 (의존 관계 고려)

### Step 1 — P0/P1 정확성 (semantics)

| 순서 | ID | 작업 | 파일 | 검증 |
| --- | --- | --- | --- | --- |
| 1 | A1 | lexer: 주석-only 라인을 indent 추적에서 제외 (NewLine 방출은 유지) | `rust/src/lexer.rs` | 재현 t1.un 해소 + test.ps1 + differential |
| 2 | A2 | optimize: compound assignment desugar — LHS가 property/index/call이면 target을 한 번만 평가하도록 임시 바인딩 도입 | `rust/src/optimize.rs`, `rust/src/compiler.rs`(검토), `rust/src/ast.rs`(검토) | 재현 t13.un → mk[0]==1; test.ps1; differential |
| 3 | A3 | optimize: const fold int add/sub/mul → `wrapping_*` | `rust/src/optimize.rs` | t3.un: `9223372036854775807 * 2` → `-2` 런타임 semantics와 일치 |
| 4 | A4 | optimize: float `//` fold → `prims::cast_long` | `rust/src/optimize.rs` | t4b.un: `9223372036854775807` → `-9223372036854775808` |
| 5 | A5 | optimize: int `**` fold `>` → `>=` (A5, semantics 일관성) | `rust/src/optimize.rs` | fold 미수행(Caught)으로 런타임 경로 |
| 6 | A10 | runtime: `hash_code(Float integral in i64 range) => v as i64 as u64` | `rust/src/runtime/obj.rs` | d[1]=x; d[1.0]=y → len(d)==1 확인 스크립트 |
| 7 | A11 | runtime: key_eq Enu/Int 대칭화 — `int_binop Eq/NEq Enu`를 `enu_binop`과 동일 semantics로 허용 | `rust/src/runtime/obj.rs` | enum 접근(A43 fix 후)으로 d[1] vs d[Color.GREEN] 확인 |
| 8 | A12 | runtime: KeyMap/RefCell borrow 하 user code 제거 — dict_to_str/set_to_str 스냅샷, dict_equals/set_equals 양쪽 스냅샷+로컬 map, dict_get_item/set_get_item let 바인딩, dict_in overlap 스냅샷, set_binop union/except/intersect 스냅샷, vm ArgKwSpread 스냅샷, obj.rs:957 let 바인딩 | `rust/src/runtime/collections.rs`, `rust/src/vm/mod.rs`, `rust/src/runtime/obj.rs` | t12.un 재현 해소 + test.ps1 + differential + cargo test |
| 9 | A13 | runtime: `key_eq`/Eq dispatch에 `Rc::ptr_eq` fast path (identity) — 순환 자기비교는 true/false로 결정, hash_code의 List/Tup/Json 경로는 깊이 제한(순환 감지) 또는 ptr_eq 가드 | `rust/src/runtime/obj.rs`, `rust/src/runtime/collections.rs` | `l=[]; l.append(l); l == l` → true; `d={}; d[l]=1` → 명확한 오류/값 (설계 결정: identity true) |
| 10 | A14 | runtime/iters: `Enumerator::next` → `Result<Option<Obj>, Obj>`; zip이 Panic을 전파 | `rust/src/runtime/iters.rs`, `rust/src/native/iter.rs` | predicate가 throw하는 .un 스크립트로 오류 전파 확인 |
| 11 | A15/A16 | runtime/int_list: `un_list_int_set` refs underflow 수정 (0 하한), `free_force` → refcounted `free`로 교체, `free`가 0-ref 슬롯을 실제 drop | `rust/src/runtime/int_list.rs` | JIT 관련 int_list 단위 테스트 + differential + 중첩 리스트 stress (사용자 파일이므로 최소 수술) |
| 12 | A17 | runtime/date: parse hour/min/sec 범위 검사 + ticks_from_civil checked 곱셈 | `rust/src/runtime/date.rs` | `date::parse("2024-01-01 25:00:00")` → None/오류; cargo test |
| 13 | A19 | native/fs: move_or_copy — source가 dir일 때 destination이 기존 파일이면 destination을 rename 전에 삭제하지 않도록 순서 수정 (rename 우선, 실패 시만 copy fallback; source가 file이 아니면 remove 생략) | `rust/src/native/fs.rs` | t7.un 시나리오 → dst.txt 유지 or 명확한 오류 |
| 14 | A20 | native/net+http: `parse_uri` 전체 텍스트에서 CR/LF 거부(헤더/요청라인 주입 차단), `set_header` key/value에서 CR/LF(및 name의 `:`) 거부 | `rust/src/native/net.rs`, `rust/src/native/http.rs`, (ws 경로는 parse_uri 공유) | HTTP 캡처 재현 → 거부 확인; ws.connect CRLF URL 거부 |
| 15 | A21 | native/random: `random.range` len을 i128로 계산, `random.int` span==2^64 특수 처리 | `rust/src/native/random.rs` | `random.int(MIN, MAX)` 균일 동작 smoke; `random.range(MIN, MAX)` → 1M cap 오류 |
| 16 | A22 | native/ws: 재조립 총량 cap (예: 64 MiB) → 초과 시 오류 | `rust/src/native/ws.rs` | 코드 리뷰 수준 (네트워크 없음) |
| 17 | A23 | native/io: sandbox(allow-list 활성) 시 `io.open`은 base 디렉터리 밖 경로 거부 | `rust/src/native/io.rs` | playground 시나리오 유사 테스트(샌드박스 상태로 unvm 실행? WASM만 해당 — native에서는 is_allowed와 무관하므로 동작 불변, 코드 리뷰) |
| 18 | A25 | native/random: bytes/string/choices에 cap (예: 10 MiB / 1M) + 오류 | `rust/src/native/random.rs` | 오류 메시지 확인 |
| 19 | A43 | runtime: `get_attr_from`에서 TObj(Single name) → 등록된 클래스 객체로 위임 (receiver=클래스 객체) — `Color.RED` 동작 | `rust/src/runtime/obj.rs` | docs 예제 `enum Color … io.write(Color.RED …)` → `0 1 2`; test.ps1 |
| 20 | A26 | native/flow+vm: `Op::Await`이 Future가 error/Propagate payload면 그대로 반환하지 말고 error로 전파 (실제 동작은 flow.rs/VM 분석 후 최소 수정) | `rust/src/native/flow.rs`, `rust/src/vm/mod.rs` | pool 예제에서 에러가 wait에서 raise |

### Step 2 — JIT (사용자 미커밋 파일, 최소 수술)

| 순서 | ID | 작업 | 파일 | 검증 |
| --- | --- | --- | --- | --- |
| 21 | A7/A8 | `emit_user_call` + `emit_class_method_call`: tag을 pure_kind의 합법 집합(RET_INT→{0}, RET_FLOAT→{1}, RET_BOOL→{3}, RET_INTLIST→{6})과 비교해 벗어나면 bail stub((0,2) return)로 전환 | `rust/src/jit_bc.rs` | t11.un: UV_JIT=1도 `'none' + int` 오류; 정상 nested call(t-nested-fib) 그대로 동작; differential |
| 22 | A9 | `Op::Return`: `instance_regs`/`self_regs`/`method_regs`/`class_ctor_regs`에 든 register는 bail | `rust/src/jit_bc.rs` | `fn f() { -> self }` (method) JIT → interpreter fallback, raw pointer 미노출 |
| 23 | A32 | hand-written bail exit 전부 `un_jit_depth_exit()` 호출하는 헬퍼(`emit_bail_exit`)로 교체 — 10여 곳 | `rust/src/jit_bc.rs` | depth counter 정상 (UV_JIT_DEBUG) + test.ps1 + differential |
| 24 | A33 | `len(xs)` fast path에 `un_list_int_len` 결과 <0 → bail 추가 | `rust/src/jit_bc.rs` | 코드 리뷰 |
| 25 | A35 | `xs.add(v)` push 반환값 확인 → 실패 시 해당 경로는 계속 진행하되 플래그/bail? (설계: push 실패 = poison; 이후 사용 시 bail하므로 값만 0 바인딩 유지, 대신 len fast path의 -1 검사(A33)로 차단) | `rust/src/jit_bc.rs` | 코드 리뷰 |
| 26 | A34 | emit_stream 종단에 hidden pair 기록 추가 (latent) | `rust/src/jit_bc.rs` | 코드 리뷰 |
| 27 | A37 | `keep_all_intlist`를 intlist-값 register만 keep하도록 축소 (가능하면), 불가하면 주석+문서화 | `rust/src/jit_bc.rs` | 코드 리뷰 |
| 28 | D1 | clippy self_assign arm 삭제 (`_ => {}`가 커버) | `rust/src/jit_bc.rs` | `cargo clippy --release` error 해소 |

### Step 3 — 테스트/도구 (P2)

| 순서 | ID | 작업 | 파일 |
| --- | --- | --- | --- |
| 29 | C1 | test.ps1: 종료 조건에 `$pos.badFiles -gt 0` 포함 | `scripts/test.ps1` |
| 30 | C2 | test.ps1/differential/unbc-roundtrip: per-test timeout (기본 120s, runspace job 또는 Start-Process -PassThru + Wait-Process -Timeout, kill 후 crashed 집계) | `scripts/*.ps1` |
| 31 | C3 | differential.ps1: JIT-on lane에서 UV_JIT 미설정 사용 | `scripts/differential.ps1` |

### Step 4 — 문서/부채 (P3, 저위험)

| 순서 | ID | 작업 | 파일 |
| --- | --- | --- | --- |
| 32 | C5 | 1415→1419, 204→205, 81/33,633/86→83/36,518/88, D-072→D-077 (3건), ARCHITECTURE 200→205, REFACTOR_SUMMARY | README.md, docs/README.md, docs/TEST_GUIDE.md, docs/ARCHITECTURE.md, AGENTS.md?(사용자 미커밋 — 제외/보고), docs/REFACTOR_LOG.md, docs/PROJECT_AUDIT.md(§6 "as of" 명시) |
| 33 | C6 | DECISIONS D-012 status/참조 정리, D-065 카운트 수정 | docs/DECISIONS.md |
| 34 | C9 | vendor/README.md region upstream 오기재 수정 | vendor/README.md |
| 35 | C10 | BENCHMARK.md bogus regression 행 표시 | BENCHMARK.md (사용자 미커밋 — 별도 보고 후? 수정 대신 지적만; 사용자가 -Report로 재생성을 권장) |
| 36 | 신규 | AUDIT 결과를 docs/TECHNICAL_DEBT.md에는 기록하지 않음 (AUDIT_*.md가 기준) — 단, A 시리즈 중 "미수정" 항목만 TD에 신규 등록 | docs/TECHNICAL_DEBT.md |

### Step 5 — 회귀 테스트 추가 (사용자 테스트 약화 금지, 추가만)

| 순서 | ID | 작업 |
| --- | --- | --- |
| 37 | C8 | `test/unit/runtime/enum.un` 신규? — 기존에 없다면 **추가만** (수정 아님): enum member access smoke — 단, enum이 문서대로 동작해야 통과하므로 A43 수정 후 작성 |
| 38 | A1 | `test/unit/parser/comments.un` 신규: 주석 들여쓰기 케이스 |
| 39 | A3/A4 | `test/unit/parser/const_fold.un` 신규 또는 기존 optimizer 테스트 파일에 추가? — 새 파일 `test/unit/parser/const_fold.un` |
| 40 | A10/A11 | `test/unit/runtime/dict_hash.un` (Int/Float 키, Enu 키) — A10/A11/A43 |
| 41 | A12 | `test/unit/runtime/reentrant.un` — __str__/__eq__가 dict/set을 변경하는 케이스가 abort하지 않고 동작 |
| 42 | MANIFEST/등록 | 신규 파일을 `test/MANIFEST.txt`에 등록 (삭제/수정 아님, 추가) |

### 측정 (성능 개선 확정 금지, 측정만)

- `scripts/bench.ps1 -Group un -NoSave` 전후 비교 (Step 완료 후) — A2 fix 전후 fib24/class1000 등.
- 함수 호출/dict 경로 마이크로 벤치는 변경 없으면 미측정으로 표기.

## 2. 예상 부작용 / 롤백 전략

- A2: desugar 도입 시 AST 형태 변경 → 컴파일러 backend 변경 최소화 위해 `Assign`에 "evaluate-target-once" 정규형(`SetProperty(receiver=tmp, …)`)을 optimize가 아니라 **compiler**에서 처리할 수도 있음. 구현 중 택1, differential로 검증.
- A9: class method `return self` — bail로 전환 → 인터프리터가 처리 → 기존 동작(의미상 정답) 복귀. JIT 속도만 소폭 저하.
- A13: ptr_eq identity — 사용자가 cycle 객체를 dict key로 쓰는 코드가 오류 대신 "정상 동작"하게 될 수 있음(개선). hash_code 깊이 제한은 거부(StackOverflow→명확한 오류 아님)보다 cycle이 key로 쓰이지 않도록 `combine_hash` recursion에 depth cap(예: 64) + `SystemError` 반환이 안전.
- A23: sandbox에서 io.open이 거부되던 동작이 환경에 따라 바뀔 수 있음 — None(비샌드박스)에서는 불변임을 명시.
- A7: nested JIT call 정상 케이스의 오버헤드 극소 (icmp 1~2개 추가).

## 3. 검증 체계

매 Step 이후: `cargo build --release` → 관련 `test/**.un` → `scripts/test.ps1` →
가능하면 `scripts/differential.ps1` → `cargo test --release` (최종) →
`scripts/unbc-roundtrip.ps1` (compiler/bytecode 접점 변경 시).
