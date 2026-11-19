# COMPILE_PROGRESS.md — Un Compile Mode 진행 상황

> 세션이 끊겨도 이 파일과 `COMPILE_DESIGN.md`, `COMPILE_FINDINGS.md`만 읽으면
> 이어서 작업할 수 있어야 한다. 각 단계 완료 시 갱신한다.

## 마지막 갱신: 2026-11-19 (v2 마무리: list[class] + exe-diff 하네스)

## 단계별 상태

| Phase | 작업 | 상태 | 검증 |
|---|---|---|---|
| 1 | 아키텍처 분석 + 설계 | **완료** | `scripts/test.ps1` 1538 passed / 48 negative (baseline 유지) |
| 2 | `typeck` (정적 분석·타입 검사·추론) | **완료** | `un check` 구현·검증; `test/compile/typed_pass.un` 통과 양쪽 모드 |
| 3 | `tir` (Typed IR + 검증기) | **완료** | core 판정 통과 함수 5개(add/square/mixed/id_bool/fib) native 실행 |
| 4 | `natcode` (Cranelift 코드젠) | **완료** | `un native` 바이너리 fib(24) native 실행, bail 경로 interpreter와 출력 동일 |
| 5 | 런타임 통합 (jit_cache 등록, VM 경로) | **완료** | TIR entries는 `UV_JIT` 우회 등록(`get_tir_entry`), `try_call_jit`에서 최우선 조회 |
| 6 | 고급 기능 (class/collection/union/any) | **완료** — union/any는 설계대로 interpreter fallback (Vm 모드) | core 부분집합: Int/Float/Bool/Str/None/Class + `list[int]`/`list[float]`/`list[class]` + `dict[str, int]`/`dict[str, float]`; class는 plain `un build`에서 native (`<classes_init>` 등록 + VM 브릿지); union/any는 bail로 안전 동작 |
| 7 | 테스트/벤치마크/문서 | **완료** | `scripts/compile-check.ps1`, `compile-bench.ps1`, 문서 |

## 완료 항목 (이번 세션)

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

### 검증
- `scripts/test.ps1`: Positive 47/1538, Negative 48 — **baseline 유지**.
- `scripts/differential.ps1`: 95 files 0 differences — **green**.
- `scripts/unbc-roundtrip.ps1 -IncludeNegative`: 47 files identical, negative 39 — **green**.
- `cargo test --release`: 213 passed (1회 `a_cleared_jit_program_runs_identically`가
  flake로 1회 실패했으나 재실행 green; 기존 테스트 순서 경쟁, 본 변경과 무관 — F-022 참조).
- `scripts/compile-check.ps1`: 18 passed (fixture 6 + untyped-source-rejected + errors 11).
- `scripts/exe-diff.ps1`: **신규** — `test/compile/*.un`을 `un build --embed`로 링크한 exe가
  `un run`과 stdout/stderr/exit code 바이트 동일인지, `test/compile/build/*.un`은 plain
  `un build` exe가 동일한지 검사. plain build 불가 fixture(assert 모듈 사용 등)는
  embed로만 검증한다.
- `scripts/compile-bench.ps1`: un native fib(24) ≈ 169-174ms (un run ≈ 194-240ms).

## 진행 로그

- 2026-11-19 09:2x — Phase 1 완료.
- 2026-11-19 10:xx — typeck Pass-1 globals leak 버그 발견·수정, DotDot/Any 추론 수정.
- 2026-11-19 11:xx — Cranelift 블록 seal/머지 수정 (fib 네이티브 컴파일 성공 → 무한루프 수정).
- 2026-11-19 11:5x — 전체 검증 green, 문서 갱신.
- 2026-11-19 (오후) — v2 마무리: TIR에 ListClass(`list[class]`) 도입(리터럴/for-in/xs[i]/pop/insert/add/reverse/sort/index_of/contains/len), plain `un build` exe가 `un run`과 바이트 동일. class exe 게이트(`obj-demo (un build class list)`) 추가, `scripts/exe-diff.ps1` 신설(embed/plain exe 바이트 비교).

## 완료 (v2)

1. class/collection native 지원 — Int/Float 코어 + 클래스 생성·메서드·필드·상속 (plain `un build`, `<classes_init>` 임베드 등록) + `dict[str, int]`/`dict[str, float]` + `list[int]`/`list[float]` + `list[class]`. union/`any`의 narrowing 결과는 Vm 모드에서 interpreter fallback이 의도된 설계.
2. TIR 인트린직 — class intrinsic (`unrt_*` VM 브릿지), Float math intrinsics, `int(x)`/`float(x)` 언어 cast 인라인.
3. 실패 테스트 프레임워크 — `test/compile/errors/` + manifest (11 fixtures), `compile-check.ps1`가 `un check`/`un native` 양쪽 exit 1 + 부분문자열 검사.
4. 벤치마크 compile-mode 열 — `scripts/bench.ps1 -Group native` (fib(24) 정수; un native ≈ 167ms vs un run ≈ 200ms vs unjit ≈ 230ms).
5. exhaustiveness 검사 — bool/none 은 해당 arm(또는 wildcard) 필수, 그 외 타입은 `_` arm 필수 (F-014 해결). 픽스처 `test/compile/matches.un`.

## 다음 작업 (v3 후보)

1. `scripts/exe-diff.ps1`를 CI(`.github/workflows/build.yml`)에 편입 검토 — 현재는 수동 게이트.
2. `dict[str, Class]` 값 지원, `list[list[T]]` 중첩 컬렉션 — 현재 `--embed` 전용.
3. native exe의 소스 컨텍스트 traceback.
4. union/tuple/class tag 기반 exhaustiveness 정밀화 (F-014 잔여; 거짓 양성 없어야 함).

## 완료 (v1.5)

- class 네이티브 지원 (plain `un build`, `<classes_init>` 실행 등록·브릿지).
- `int / int`·`//` 0 나누기·`int % 0` — 인터프리터와 바이트 동일한 catch 동작.
- Int/Float math intrinsic + `int(x)`/`float(x)` cast를 TIR에 인라인.
- match exhaustiveness 분석 — bool/none 은 해당 arm만, 그 외 타입은 `_` wildcard arm 필수.
