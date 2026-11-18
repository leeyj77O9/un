# AUDIT_PROGRESS.md — Phase 4 진행 기록

작성: 2026-11-18. Phase 3 계획(`AUDIT_PLAN.md`)에 따라 우선순위별 수정을 적용하고,
각 단계마다 빌드/테스트를 반복했다. 기준: `main` @ `2f67935` + 사용자 미커밋 변경 유지.

## 완료 (수정 + 검증)

### P0/P1 정확성

- **A1 lexer: 주석 전용 라인이 Indent/Dedent를 발생시키던 문제 수정** — 주석-only 라인은
  indent 추적을 건너뛰도록 했다. (`rust/src/lexer.rs`)
- **A2 compound assignment가 receiver/subscript를 2회 평가하던 문제 수정** — `make().v += x`,
  `a[g()] += x`가 receiver/key/bounds를 1회만 평가하도록 desugar를
  `tmp = receiver; tmp.name = tmp.name op rhs` 형태로 변경 (Slice bounds도 동일).
  (`rust/src/optimize.rs`)
- **A3 int 상수 폴딩이 overflow 시 컴파일 에러** — `wrapping_add/sub/mul`로 runtime semantics
  (wrapping)와 일치시킴. `x = MAX * 2` → `-2` (이전: 프로그램 전체 컴파일 실패).
- **A4 float `a // b` 상수 폴드가 saturating cast** — `prims::cast_long`으로 runtime semantics와
  일치시킴 (이전: `1e30 // 1.0` 상수 경로 `MAX` vs 런타임 `MIN`).
- **A5 (int `**` 폴드 boundary)** — 조사 결과 fold와 runtime이 동일한 saturating 방식으로
  **의도적으로 일치**하고 있어 새 오류가 아니다. 언어 semantics 보존을 위해 변경 보류 (보고서 기록).
- **A7/A8 JIT nested call이 logical 태그를 `== 2`만 검사** — call site가 callee의
  `pure_kind`(RET_INT/RET_FLOAT/RET_BOOL/RET_INTLIST 정확히 하나)에 대해 기대 태그를
  검증하고, 계약 밖 태그면 frame 통째로 bail→interpreter. 이종(INT|FLOAT/INT|INTLIST)
  kind는 컴파일 타임에 거부(인터프리터로 fallback). `-> xs` probe는 유지.
  (`rust/src/jit_bc.rs`)
- **A9 JIT `Op::Return`이 instance/method/ctor/self 레지스터를 반환하던 문제** — 해당 마커를
  Return 시 bail 하도록 수정 (raw `Rc` 포인터가 Int로 누설되던 문제).
- **A10 KeyMap hash/Eq 불일치 (Int vs Float)** — 정수형 float는 `v as i64`로 hash (0.0 포함 -0.0).
  `d = {1: 'a'}; d[1.0] = 'b'; len(d) == 1` 성립. (`rust/src/runtime/obj.rs`)
- **A11 key_eq 비대칭 (Int vs Enu)** — `int_binop Eq/NEq`에 `Payload::Enu` arm 추가
  (`enu_binop`과 대칭). `1 == Color.GREEN == true` both ways.
- **A12 KeyMap/Set RefCell double-borrow → 프로세스 abort** — 스냅샷 기반으로 재작성
  (`dict_to_str`, `set_to_str`, `dict_get_item`/`set_get_item`, `dict_in`, `dict_equals`,
  `set_equals`, `set_binop` add/sub/xor, `dict_equals`, `set` union/except/intersect,
  `obj::obj_in`의 `contains_key`, VM의 `ArgKwSpread`). 추가로 **VM의 `Op::Call`에
  `is_panic` 검사 추가** (native 함수가 panic 객체를 반환하면 Exception으로 전파) 및
  **`run()` 전체를 `catch_unwind`로 감싸 Rust panic을 Un `Outcome::Panic`으로 변환**
  (TD-40 완화). `str({"k": Boom()})` (print 중 dict mutation) 이제 `exit 0`.
  (`rust/src/runtime/collections.rs`, `rust/src/runtime/obj.rs`, `rust/src/vm/mod.rs`, `rust/src/native/iter.rs`)
- **A13 순환값 스택 오버플로** — `binop(Eq/NEq)` 재귀 깊이 32 제한 (`EQ_DEPTH`),
  `to_str` (표시) 재귀 깊이 32 제한 (`[...]`로 대체), `hash_code` 재귀 깊이 32 제한,
  `key_eq`에 `Rc::ptr_eq` identity fast-path 추가. `l = []; l.add(l); l == l; str(l)` 모두 정상.
- **A14 `iters::Enumerator::next`가 predicate panic을 값으로 흘리던 문제** — `Result<Option<Obj>, Obj>`로
  변경, `zip`이 패닉을 전파. `list(xs.map(fn(x) -> x // 0))` → `int division by zero` 오류 전파 확인.
- **A15/A16 int_list arena refcount** — `un_list_int_set` outer-row swap의 `refs -= 1`을
  `saturating_sub`로 (underflow wrap→panic-across-FFI 방지). 부모 해제 시 자식을 refcounted
  `free`로 해제 (공유 row가 부모 해제 시 통째로 사라지지 않음).
- **A17 `date::parse`가 hour/minute/second를 검사하지 않던 문제** — 범위 검사 추가
  (`2024-01-01 25:00:00` 거부) — 보류: String-parts 검증이 복잡해 별도 패스로 분리. **미적용 (아래 참고)**
- **A19 `fs.move`가 rename 전에 destination을 삭제하던 문제** — source가 정규 파일이 아니면
  destination을 건드리지 않고 `"destination exists"` 오류 반환. 또한 rename 실패 시
  file이면 copy+remove fallback (EXDEV 등). (`rust/src/native/fs.rs`)
- **A20 HTTP request/header CRLF injection** — `net::parse_uri`가 URI 전체에서 CR/LF 거부
  (경로/호스트/요청라인), `http::set_header`가 name에 `:`/CR/LF, value에 CR/LF 거부.
  `ws.connect`도 `parse_uri`를 쓰므로 동일하게 차단됨. (`rust/src/native/net.rs`, `rust/src/native/http.rs`)
- **A21 `random.int(MIN, MAX)`가 항상 MIN 반환 / `random.range`가 overflow 시 무제한** —
  span을 i128로 계산; `span > u64::MAX`면 `next_u64` 사용; range의 len 검사 i128로.
  (`rust/src/native/random.rs`)
- **A22 WebSocket continuation frame 재조립 무제한** — 누적 64 MiB cap 추가 (msg 초과 시 오류).
- **A23 `io.open` sandbox escape** — allow-list 활성(sandbox) 상태에서 base 디렉터리 밖의
  절대 경로/`..` 체인을 `"io.open: path escapes the sandbox root"` 오류로 거부.
  상대 경로는 그대로 허용. `world::sandbox_active()` 추가.
  playground Rust 단위 테스트 2개 추가 (playground suite 5/5 통과).
  (`rust/src/native/io.rs`, `rust/src/runtime/world/io.rs`, `rust/playground/src/lib.rs`)
- **A25 `random.bytes`/`string`/`choices` 무제한 할당** — 10 MiB / 1M cap + 오류.
- **A26 `pool.submit` future가 error 객체를 값으로 흘리던 문제** — `Op::Await`가
  Future 값이 error/panic payload면 그대로 반환하지 않고 오류/패닉으로 전파.
  `x // 0` pooled fn → `wait`에서 `int division by zero` 확인.
- **A43 `enum` 멤버 접근 불가 + enum 자동 값 1부터 시작** — `get_attr_from`이 TObj(type object)에서
  등록된 클래스 객체로 위임하도록 수정 (`Color.RED` → 0, `Color.BLUE` → 2, `d[Color.RED] = 1` 동작,
  `1 == Color.GREEN` true). compiler의 enum counter `1i64` → `0i64` (reference/.NET과 문서의 0 기반).

### JIT (사용자 미커밋 파일 `rust/src/jit_bc.rs` — 최소 단위 수정, 보고서에 명시)

- **A32 bail exit들이 `un_jit_depth_exit()`를 호출하지 않던 문제** — `emit_bail_exit` 헬퍼로
  통일 (모든 hand-written bail exit — class_make miss, method call bail, GetAttr name miss ×2,
  SetAttr miss, dict set/index/get bail ×3, IntVec store/iter bail ×2); depth 누수 제거.
- **A33 `len(xs)` fast path가 -1(dead handle) 센티넬을 검사하지 않던 문제** — `-1`이면
  bail exit로.
- **A34 `emit_stream` 종단 bare `return_` (hidden pair 미기록)** — `(0, TAG_BAIL)` 기록.
- **A35 `xs.add(v)` push 실패(0)를 무시하던 문제** — A33(len sentinel)·GetItem bail로
  대부분 후속 탐지됨을 재확인. push 자체는 회복 경로가 있어 코드 유지 + 문서화.
- **D1 clippy self-assignment** — `Op::Unwrap` arm의 no-op 할당 제거.
- **emit debugging hook**: `emit_user_call`의 각 거부 경로에 `jit_debug()`-gated eprintln 추가.
- **param_regs Return arm 규칙 유지** (param→INTLIST speculation 그대로) — 대안으로
  "param→INT 선호"를 적용했다가 `ident_list([7,8,9])` → `len()==1`로 회귀 실패를
  differential이 잡아내 ident 동작 파손을 확인하고 즉시 되돌림.
- **0x9(INT|INTLIST) heterogeneous 호출부**: 진입 시 컴파일 타임 거부 (opt-in 방식이
  아닌 단일 태그 검증으로). 이종 반환 sub-JIT-call 사이트는 인터프리터로 fallback —
  **fib24/qsort500/mergesort1000이 인터프리터로 실행됨(의도된 손실, 보고서 참고)**.

### 컴파일러 / 검증기 / .unbc (P0 클래스)

- **F-1/F-8 `.unbc` 검증이 verifier와 translator 불일치** — `Program::validate`가 depth 분석을
  `transfer::analyze`(single source)에 위임; verifier와 translator의 차이(핸들러 depth seeding,
  `Return` 이후 dead code 처리) 제거. `with_context` 모듈 로드 경로에서도
  `program.validate()` 수행 — plain-source 모듈의 >256-register 함수에서 패닉하던 문제 해소.
- **F-1 (Rot/Dup2/PopN/Tag 등 transfer 미지원 opcode가 verifier를 통과)** — validate가
  해당 opcode를 거부 (plan_for의 `.expect`는 이제 사실상 도달 불가).
- **F-2 `exc.var_slot` 미검증** — `proto.names.len()` 범위 검증 추가.
- **F-3 `proto.locals` 미검증** — `proto.names.len()` 범위 검증 추가.
- **F-4 `patterns` 미검증** — `read_pattern`에 깊이 제한(64) + validate에서 pattern tree
  재귀 검증 (Bind→names, Literal→consts) + 깊이 제한.
- **F-5 `Slice`/`SetSlice` dst가 수학적으로 256을 넘어갈 수 있던 문제** — dst 음수/초과 시
  명확한 오류.
- **F-6 `Arg*`가 args stack 빈 채로 panic** — `args.last_mut().expect` → `else` 브랜치에서
  `Outcome::Error("malformed bytecode …")`.
- **F-7 handler depth 미시딩** — F-1(analyzer 위임)으로 함께 해소.
- **`unbc::read` 무제한 할당** — 모든 `Vec::with_capacity(count)` 앞에 남은 바이트 수 기반
  `guard` 추가 (`count > remaining bytes` → Err). 깊은 pattern nesting도 읽기 시점에서 차단.
- **module.rs 통합**: `.unbc` 경로 포함 두 실행 경로 모두 validate를 거친다.

### 테스트/도구 (P2)

- **C1 `scripts/test.ps1`의 exit-code hole 수정** — `[FAIL]`인데 `0 failed && 0 crashed && nonzero exit`
  인 스크립트를 overall exit 1로 집계 (badFiles 반영).
- **C2 per-test timeout 부재** — 세 스크립트(`test.ps1`, `differential.ps1`,
  `unbc-roundtrip.ps1`)에 `-TimeoutSec` 인수(기본 120초) 추가, .NET Process + async stream
  패턴으로 kill 후 synthetic code 124 반환. hang 테스트로 검증 (hang.un: 2초 타임아웃 동작).
- **C3 differential runner의 "JIT on" lane이 `UV_JIT=1` 고정** — 이제 기본 설정(unsetting)을
  lane으로 사용 (기본값이 바뀌어도 "default == JIT on" 유지).
- **새 회귀 테스트 6개 추가** (약화 없음): `test/unit/parser/comments.un`,
  `const_fold.un`, `test/unit/runtime/dict_hash.un`, `enum.un`, `reentrant.un`,
  `compound_assign.un` (모두 신규 파일, `test/MANIFEST.txt`에 등록).

### 문서 (P3)

- **C5**: `docs/TEST_GUIDE.md`의 1391/1415 수치를 1454→1482 이후 현재값으로,
  `docs/REFACTOR_SUMMARY.md`·`docs/PROJECT_AUDIT.md`의 D-001…D-072 → D-001…D-077.
- **C6**: `docs/DECISIONS.md`의 D-012 상태를 Accepted로(살아있는 동작은 D-054/D-063 참조),
  D-065의 `cargo test 199 passed` → `205 passed`.
- **C9**: `vendor/README.md`의 `region` upstream 표기를 `Amanieu/region`으로 수정.
- 미커밋 사용자 문서(AGENTS.md/README.md/docs/README.md/docs/ARCHITECTURE.md/
  BENCHMARK.md)는 건드리지 않았다 — 드리프트 목록은 AUDIT_FINAL.md §6에 기록.

## 검증 상태 (각 단계 공통)

- `cargo build --release` 오류 0, rustc 경고 0 (grep 기준).
- `cargo test --release --workspace` — **210/210 통과** (unvm 205 + playground 5).
- `cargo test --release --workspace` ? **210/210 통과** (unvm 205 + playground 5).
  Negative 41 matched / 0 mismatched; Known 0; exit 0.
- `scripts/differential.ps1` — **87 files, 0 differences**.
- `scripts/unbc-roundtrip.ps1 -IncludeNegative` — Positive **46/46 identical**, Negative 34 identical.
- `wasm32-unknown-unknown` playground 빌드 성공.
- `wasm32-unknown-unknown` playground build 성공.
- clippy: 오류 0개; 경고는 대부분 style/pedantic (baseline과 유사하게 ~155개 남음 — 삭제하지 않음).

## 의도적으로 보류한 항목과 그 이유

- **A5 int `**` 폴드 boundary (`2 ** 63`)** — fold와 runtime이 모두 동일한 saturating 방식으로
  **일관되게** 동작하며, 고치는 순간 언어 semantics가 바뀐다 (기존 프로그램의 값 변경). 문서에만 남김.
- **A17 `date::parse` hour/min/sec 범위 검사** — `cargo` differential 검증이 필요한 부분이라
  별도 패스로. `ticks_from_civil` wrap도 동일 범주 (TD-41 클래스).
- **A37 `keep_all_intlist=true`가 frame의 모든 IntVec handle을 해제하지 않는 것** — arena
  잔여 관찰은 필요하나 의미론 저하 위험(공유 row 해제)이 커서 코드 유지, 문서화만.
- **F-10~F-12**: `.unbc` `call_names` u16 절삭, module-scope `defer`/`using` 미실행,
  `validate`의 도달 불가 arm — 모두 저우선, 미변경 (`AUDIT_FINDINGS`에 기록).
- **이종 반환 JIT 호출부(fib24/qsort500/mergesort1000이 인터프리터로 실행)** — 타입-태그
  문제를 근본적으로 푸는 full dual-tail specialization은 연구 과제로, 이번 범위에서는
  정확성(인터프린터 fallback)을 택했다. 관련 수치는 AUDIT_FINAL.md의 성능 섹션.
- **C7~C11** (negative manifest weak spots, BENCHMARK.md bogus regression 행,
  bench.ps1 결과 미검증 등) — 각각 기록만. `BENCHMARK.md`/`bench.ps1`은 사용자
  미커밋 파일이라 미접촉.
