# COMPILE_PROGRESS.md — Un Compile Mode 진행 상황

> 세션이 끊겨도 이 파일과 `COMPILE_DESIGN.md`, `COMPILE_FINDINGS.md`만 읽으면
> 이어서 작업할 수 있어야 한다. 각 단계 완료 시 갱신한다.

## 마지막 갱신: 2026-11-23 (v2 마무리: list[class] + exe-diff 하네스; 제네릭/데이터 enum/CI 편입 반영)

## 단계별 상태

| Phase | 작업 | 상태 | 검증 |
|---|---|---|---|
| 1 | 아키텍처 분석 + 설계 | **완료** | `scripts/test.ps1` 1638 passed / 51 negative (baseline 유지) |
| 2 | `typeck` (정적 분석·타입 검사·추론) | **완료** | `un check` 구현·검증; `test/compile/typed_pass.un` 통과 양쪽 모드 |
| 3 | `tir` (Typed IR + 검증기) | **완료** | core 판정 통과 함수 5개(add/square/mixed/id_bool/fib) native 실행 |
| 4 | `natcode` (Cranelift 코드젠) | **완료** | `un native` 바이너리 fib(24) native 실행, bail 경로 interpreter와 출력 동일 |
| 5 | 런타임 통합 (jit_cache 등록, VM 경로) | **완료** | TIR entries는 `UV_JIT` 우회 등록(`get_tir_entry`), `try_call_jit`에서 최우선 조회 |
| 6 | 고급 기능 (class/collection/union/any) | **완료** — union/any는 설계대로 interpreter fallback (Vm 모드) | core 부분집합: Int/Float/Bool/Str/None/Class + `list[int]`/`list[float]`/`list[class]` + `dict[str, int]`/`dict[str, float]`/`dict[int, int]`; class는 plain `un build`에서 native (TIR 메타데이터 등록 + `libnrt.a`, `<classes_init>`/VM 브릿지 아님); union/any는 bail로 안전 동작 |
| 7 | 테스트/벤치마크/문서 | **완료** | `scripts/compile-check.ps1`, `compile-bench.ps1`, 문서 |

## 완료 항목 (이번 세션)

### no-VM exe 성능 개선 (2026-11-23)
- trace frame을 소유 `String` 대신 정적 이름 포인터로 저장해 호출당 할당을 제거했다
  (`nrt/trace.rs`). `fib24` 5.89ms → 2.66ms, `call200000` 8.17ms → 3.58ms(Node 추월).
- 클래스 필드/메서드 조회를 raw 바이트 비교로 바꾸고,`__init__`/메서드 디스크립터를
  뮤텍스 밖으로 복사해 재진입 교착을 피했다(`nrt/class.rs`). `class1000` 0.612ms → 0.403ms.
- 소정수(0..16384) boxed 객체를 지연 캐시하고 리스트 반복값은 한 객체를 공유한다
  (`nrt/obj.rs`, `nrt/list.rs`). `iter10000` 0.743ms → 0.314ms, `dict5000` 1.02ms → 0.62ms.
- 정수 dict 해시를 SipHash에서 splitmix64로 경량화했다(`nrt/dict.rs`).
- `sieve30000` 2.41ms → 0.43ms, `mergesort1000` 1.57ms → 0.67ms, `qsort500` 0.80ms → 0.56ms.
- `BENCHMARK.md`는 `scripts/bench.ps1 -Group all -Report`로 갱신했다.

### 검수 수정 (2026-11-23)
- 다중 인스턴스 제네릭이 proto_span을 공유해 JIT 캐시에서 덮어써지던 문제를 수정했다.
  공유 proto는 JIT 등록을 건너뛰고 VM이 해석한다 (`natcode.rs`).
- 파서가 `d[k](1)`을 명시적 타입 인자로 오인하던 회귀를 수정했다. generic 함수 이름만
  타입 인자 문법으로 해석한다 (`parser.rs`).
- NRT dict 해시를 `eq_obj` 동치에 맞게 정규화하고, 교차 타입 키는 전체 탐색으로 폴백한다
  (`nrt/src/dict.rs`).
- 제네릭 본문에서만 호출되는 제네릭 함수도 인스턴스를 생성하도록 확장했다 (`tir.rs`).
  mangled 이름에 `__mono_` 접두어를 붙여 사용자 함수와 충돌하지 않게 했다.
- `[T: int | float]` 제약이 union 타입을 거짓 거부하지 않도록 수정했다 (`typeck.rs`).
- JIT `BuildDict`가 preserve 목록에 누락돼 dict 표시가 지워지던 회귀를 수정했다 (`jit_bc.rs`).
- CI에 `compile-check`/`exe-diff`/`edge-fuzz` 스텝을 추가하고 README 한/영에 제네릭을 반영했다.
- 데이터를 담는 enum variant(`enum Test[T,U]`의 `a: int`, `Test.a(42)`, `.value`, match
  분해)를 구현했다 — VM/Compile Mode 경로만 동작하고 plain no-VM build는 여전히 거부한다.

### 제네릭 Phase 2–4 (2026-11-22)
- **Phase 2 — 모노모피제이션**: `expand_generics`가 호출부에서 추론된 타입 인자별로
  `FnCheck`를 복제·치환해 `identity__int` 같은 구체 인스턴스를 만든다. `@dep:` 마커로
  재귀/교차 제네릭 호출도 인스턴스에 연결한다. `un native`와 plain `un build` 모두
  제네릭 함수를 네이티브 실행한다 (`test/compile/build/generics.un`).
- **Phase 3 — 제네릭 클래스**: `class Box[T]`의 필드·생성자·메서드에서 타입 변수를 사용할 수
  있다. 클래스 제네릭을 포함한 프로그램은 TIR 네이티브 경로에서 제외되고 VM/`--embed`로
  실행된다 (`test/compile/generic_class.un`).
- **Phase 4 — 제약**: `[T: int | float]` / `[T: int]` 등의 bound를 파싱·검증한다. 위반 시
  `type argument ... does not satisfy bound` 오류 (`test/compile/errors/generic_bound.un`).
- Vm 모드의 정상 폴백(Str/heap/native-call)에서 panic 메시지가 새어나가지 않도록
  `native_compile`이 해당 함수를 미리 제외한다.

### 제네릭 Phase 1 (2026-11-22)
- `Ty::Generic { id, name }` 추가, `fn name[T, U](...)` 문법 파싱. 타입 파라미터 노드는
  함수 노드의 마지막 자식으로 저장해 기존 위치 기반 reader를 유지했다.
- 호출부 추론: `apply_sig`가 인자 타입과 시그니처를 unify하고 반환 타입에 치환한다.
  충돌/미해결은 `any`로 widen해 동적 폴백. `name[int](x)` 명시적 타입 인자도 지원.
- VM은 typeck을 쓰지 않으므로 소거 패스 불필요. `un native`는 TIR이 제네릭을
  non-core로 판정해 인터프리터로 폴백(Phase 2에서 모노모피제이션 예정).
- 추가 테스트: `test/compile/generics.un`, `test/compile/errors/generic_arg_type.un`.

### dict 성능/지원 확장 (2026-11-22)
- JIT 바이트코드 번역기가 레지스터 재사용 시 객체 타입 표시를 남겨 IntDict 루프를
  거절하던 버그를 수정했다 (`clear_specialized`). `dict5000` JIT 3.74ms → 0.26ms.
- Compile Mode core 타입에 `dict[int, int]`를 추가했다 (TIR 하향·검증·natcode 박싱,
  `unrt_*` C ABI는 타입 비의존적이라 무변경). `test/compile/dict_int.un`,
  `test/compile/build/dict_int.un` 추가.
- NRT dict를 선형 탐색에서 값 기반 해시 인덱스로 교체하고 `time.monotonic()` 해상도를
  마이크로초로 높였다. no-VM exe `dict5000` 74.9ms → 1.10ms, `un native` 19.7ms → 3.71ms.

### typeck 수정 (인터프리터 무영향, `un native`/`un check` 경로만 변경)
- Pass-1 선언 수집이 **함수 본문의 지역 변수까지 모듈 전역으로 등록**하던 버그를
  수정했다 (`collect_nested_block`/`collect_nested_stmt` 도입). 이 버그 때문에
  `x = 0` 같은 단순 대입의 타입이 `Unknown`에 묶여 TIR eligibility가 전부 거부됐다.
  (자세한 내용은 `COMPILE_FINDINGS.md` F-017.)
- `for i in a..b`의 `..` (Binary(DotDot)) 범위에서 루프 변수를 `int`로 추론하는
  분기를 추가했다 (F-019).
- `check_binop`에서 `Any`가 한쪽 피연산일 때 결과 타입을 `float`로 추측하던 것을
  `Any`로 전파하도록 수정했다 (F-019).

### TIR 검증기 수정
- `validate`의 `locals` 시드 오류 수정: params만 시드하고 `Let` 선언 순서를
  매칭한다 (F-018).
- `ForRange`의 암묵적 int 루프 변수를 검증기에서도 암묵적 `Let`으로 인식하도록
  수정했다 (F-018).

### Cranelift 코드젠 수정 (정상 동작 검증됨)
- **`If` arm이 `Return`으로 종료될 때 오작동하던 치명적 버그 2개 수정**:
  1) then-arm 종료 시 else/merge 블록을 비운 채 seal하여 Cranelift가 빈 블록을
     삭제하고 `brif` 참조가 끊기는 문제 (빈 블록은 반드시 terminator를 채워야 한다).
  2) then-arm의 `terminated`가 남아 `if` 다음 문장(return)이 통째로 방출되지
     않던 문제 (merge 블록 진입 시 flag 리셋).
  (F-020, F-021.)
- **`While`/`ForRange` 헤더를 back-edge 방출 전에 seal하던 SSA 버그 수정**:
  cranelift-frontend는 블록 seal 시점에 알려진 모든 선행 블록으로 SSA를 구성하므로,
  헤더는 back-edge(= 본문 말단 jump / skip_blk jump) 방출 **후**에 seal해야 한다.
  이 버그가 루프 카운터 증가를 SSA에서 누락시켜 네이티브 무한 루프를 일으켰다.

### CLI
- `un check <file.un>`: 정적 분석만. 성공 exit 0, 실패 exit 1 (최대 50개 진단).
- `un native <file.un>`: 분석 + TIR 변환 + Cranelift 네이티브 등록 + VM 실행.
- `un run`/`un compile`: **변경 없음**.

### 검증 (2026-11-23 재측정)
- `scripts/test.ps1`: Positive 51 files / 1638 passed / 0 failed / 0 crashed,
  Negative 51 matched / 0 documented divergences, Known 0 — **baseline 유지**.
- `scripts/differential.ps1`: 102 files 0 differences — **green**.
- `scripts/unbc-roundtrip.ps1 -IncludeNegative`: positive 51/51 identical,
  negative 42 identical + 9 skipped(compile-rejected) — **green**.
- `cargo test --release --workspace`: 273 passed (nrt 4 + lib 246 + un bin/LSP 15 +
  playground 8), 0 failed — **green**.
- `scripts/compile-check.ps1`: 36 passed, 0 failed.
- `scripts/exe-diff.ps1`: 42 passed, 0 failed — `test/compile/*.un`을 `un build --embed`로
  링크한 exe가 `un run`과 stdout/stderr/exit code 바이트 동일인지, `test/compile/build/*.un`은
  plain `un build` exe가 동일한지 검사. plain build 불가 fixture(assert 모듈 사용 등)는
  embed로만 검증한다. CI 스텝으로 편입됨(아래 "다음 작업" 참조).
- `test/test_suite.un`: 109 passed, 0 failed.
- `scripts/bench.ps1 -Group all`: 6 lanes (Un VM / Un JIT / Compile Mode / standalone
  exe / Python / Node); `scripts/bench-report.ps1`이 `bench/report.html` 생성.
  전체 결과는 루트 `BENCHMARK.md`(2026-11-24 01:04 +09:00, 64e4b7e 재생성;
  Un JIT/Python geomean 3.970x) 참조. `scripts/compile-bench.ps1`은 상세 비교용.

## 진행 로그

- 2026-11-19 09:2x — Phase 1 완료.
- 2026-11-19 10:xx — typeck Pass-1 globals leak 버그 발견·수정, DotDot/Any 추론 수정.
- 2026-11-19 11:xx — Cranelift 블록 seal/머지 수정 (fib 네이티브 컴파일 성공 → 무한루프 수정).
- 2026-11-19 11:5x — 전체 검증 green, 문서 갱신.
- 2026-11-19 (오후) — v2 마무리: TIR에 ListClass(`list[class]`) 도입(리터럴/for-in/xs[i]/pop/insert/add/reverse/sort/index_of/contains/len), plain `un build` exe가 `un run`과 바이트 동일. class exe 게이트(`obj-demo (un build class list)`) 추가, `scripts/exe-diff.ps1` 신설(embed/plain exe 바이트 비교).

## 완료 (v2)

1. class/collection native 지원 — Int/Float 코어 + 클래스 생성·메서드·필드·상속 (plain `un build`, TIR 메타데이터로 네이티브 등록 + `libnrt.a`) + `dict[str, int]`/`dict[str, float]` + `list[int]`/`list[float]` + `list[class]`. union/`any`의 narrowing 결과는 Vm 모드에서 interpreter fallback이 의도된 설계.
2. TIR 인트린직 — class intrinsic (`unrt_*` NRT 런타임 함수; 과거 VM 브릿지는 plain 경로에서 제거), Float math intrinsics, `int(x)`/`float(x)` 언어 cast 인라인.
3. 실패 테스트 프레임워크 — `test/compile/errors/` + manifest (19 fixtures), `compile-check.ps1`가 `un check`/`un native` 양쪽 exit 1 + 부분문자열 검사.
4. 벤치마크 compile-mode 열 — `scripts/bench.ps1 -Group native` (fib(24) 정수;
   현재 기준 un native ≈ 171 ms, un run(VM) ≈ 191 ms, un jit ≈ 1.9 ms,
   standalone exe ≈ 6 ms). 이전 "unjit ≈ 230 ms" 수치는 JIT 퇴화기
   (AUDIT-001)의 것으로 현재는 유효하지 않다.
5. exhaustiveness 검사 — bool/none 은 해당 arm(또는 wildcard) 필수, 그 외 타입은 `_` arm 필수 (F-014 해결). 픽스처 `test/compile/matches.un`.

## 다음 작업 (v3 후보)

1. `list[list[T]]` 중첩 컬렉션의 element 재인덱싱 — element가 불투명 핸들
   (`ObjElem::Obj`)로 취급되어 아직 lowering하지 않는다 (`OBJ_MILESTONES.md`).
   `dict[str, ClassName]` 값은 네이티브 지원됨.
2. union/tuple/class tag 기반 exhaustiveness 정밀화 (F-014 잔여; 거짓 양성 없어야 함).

> 완료된 항목은 위 "완료 (v2)"에 정리했다. CI 12스텝 편입(`225d08d`)과
> native exe traceback(`OBJ_MILESTONES.md`)는 더 이상 다음 작업이 아니다.

## 완료 (v1.5)

- class 네이티브 지원 (plain `un build`, TIR 메타데이터 등록 + `libnrt.a` 독립 런타임).
- `int / int`·`//` 0 나누기·`int % 0` — 인터프리터와 바이트 동일한 catch 동작.
- Int/Float math intrinsic + `int(x)`/`float(x)` cast를 TIR에 인라인.
- match exhaustiveness 분석 — bool/none 은 해당 arm만, 그 외 타입은 `_` wildcard arm 필수.
