# COMPILE_PROGRESS.md — Un Compile Mode 진행 상황

> 세션이 끊겨도 이 파일과 `COMPILE_DESIGN.md`, `COMPILE_FINDINGS.md`만 읽으면
> 이어서 작업할 수 있어야 한다. 각 단계 완료 시 갱신한다.

## 마지막 갱신: 2026-11-19 (native 실행 검증 완료)

## 단계별 상태

| Phase | 작업 | 상태 | 검증 |
|---|---|---|---|
| 1 | 아키텍처 분석 + 설계 | **완료** | `scripts/test.ps1` 1538 passed / 48 negative (baseline 유지) |
| 2 | `typeck` (정적 분석·타입 검사·추론) | **완료** | `un check` 구현·검증; `test/compile/typed_pass.un` 통과 양쪽 모드 |
| 3 | `tir` (Typed IR + 검증기) | **완료** | core 판정 통과 함수 5개(add/square/mixed/id_bool/fib) native 실행 |
| 4 | `natcode` (Cranelift 코드젠) | **완료** | `un native` 바이너리 fib(24) native 실행, bail 경로 interpreter와 출력 동일 |
| 5 | 런타임 통합 (jit_cache 등록, VM 경로) | **완료** | TIR entries는 `UV_JIT` 우회 등록(`get_tir_entry`), `try_call_jit`에서 최우선 조회 |
| 6 | 고급 기능 (class/union 네이티브) | 보류 (설계만) | core 부분집합 밖 — interpreter fallback으로 정상 동작 |
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
- `scripts/compile-check.ps1`: 2 passed.
- `scripts/compile-bench.ps1`: un native fib(24) ≈ 169-174ms (un run ≈ 194-240ms).

## 진행 로그

- 2026-11-19 09:2x — Phase 1 완료.
- 2026-11-19 10:xx — typeck Pass-1 globals leak 버그 발견·수정, DotDot/Any 추론 수정.
- 2026-11-19 11:xx — Cranelift 블록 seal/머지 수정 (fib 네이티브 컴파일 성공 → 무한루프 수정).
- 2026-11-19 11:5x — 전체 검증 green, 문서 갱신.

## 다음 작업 (v2 이후)

1. class/collection/union/`any` native 지원 (core 부분집합 확장).
2. TIR 검증기에 class intrinsic / Float math intrinsic 포함 (jit_bc 재사용 검토).
3. 실패 테스트 프레임워크 (`test/compile/errors/` + manifest) 구축.
4. 벤치마크의 compile-mode 열을 `scripts/bench.ps1`에 정식 편입.
5. exhaustiveness 분석 (match 도달 가능성) — `COMPILE_DESIGN.md` §4.6.

## 완료 (v1.5)

- class 네이티브 지원 (plain `un build`, `<classes_init>` 실행 등록·브릿지).
- `int / int`·`//` 0 나누기·`int % 0` — 인터프리터와 바이트 동일한 catch 동작.
- Int/Float math intrinsic + `int(x)`/`float(x)` cast를 TIR에 인라인.
- match exhaustiveness 분석 — bool/none 은 해당 arm만, 그 외 타입은 `_` wildcard arm 필수.
