# COMPILE_TESTS.md — Compile Mode 테스트 현황과 검증

> 테스트는 실제 Un 코드로 작성한다 (AGENTS.md 규칙).
> 세 축으로 구성된다: (A) `un native`/`un check` 결과 테스트, (B) 결과면 안 되는 실패
> 테스트, (C) 인터프리터 vs 컴파일 모드 cross-validation.

## 현재 상태 (2026-11-20)

| 구분 | 상태 |
|---|---|
| `un check` / `un native` 기존 회귀 (`scripts/test.ps1`) | **green** — Positive 48 files / 1570 passed / 0 failed / 0 crashed, Negative 51 matched, Known 0 |
| `scripts/differential.ps1` (UV_JIT=0 vs JIT 바이트 비교) | **green** — 99 files, 0 differences |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | **green** — 48 identical, 0 mismatched (negative 42 identical, 9 compile-rejected skip) |
| `cargo test --release --workspace` | **green** — 227 passed (lib 219 + playground 8) |
| `scripts/compile-check.ps1` (compile-mode cross-validation) | **green** — 23 passed, 0 failed |
| `scripts/exe-diff.ps1` (exe ↔ `un run` 바이트 비교) | **green** — 14 passed, 0 failed |
| `un native` 바이너리 정확성 (native 실행 vs interpreter 실행) | **검증됨** — typed_pass.un, loop/recursion/collatz 벤치필, div-zero·재귀 bail 경로 포함 |

## 실행 방법

```powershell
# compile-mode cross-validation (test/compile/*.un)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\compile-check.ps1
# exe 검증 — --embed exe와 plain native exe가 un run과 바이트 동일한지
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\exe-diff.ps1
# compile-mode 벤치마크 측정
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\compile-bench.ps1
# 기존 회귀 (반드시 green 유지)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test.ps1
```

## 고정한 검증 파일

| 파일 | 용도 |
|---|---|
| `test/compile/typed_pass.un` | `un native`/`un run` 양쪽에서 동일하게 통과하는 컴파일-모드 코어 시맨틱 스모크 (alias/union/higher-order/closure capture bail/bool ops) |
| `test/compile/exceptions.un` | `try`/`catch`/`finally`/`throw` + dict missing-key + int/int · `//` div-zero(catchable) + INT_MIN // -1 wrap — interpreter와 `un native`가 바이트 동일한지를 검증하는 예외 경로 픽스처 |
| `test/compile/classes.un` | 클래스 생성/접근/메서드/`__init__`/상속 — `un native`/`un run` 양쪽에서 동일하게 통과하는 클래스 경로 픽스처 (plain `un build`에서도 exe로 빌드되는지 scripts/obj-demo.ps1가 게이트) |
| `test/compile/classes_list.un` | `list[Point]` 요소 타입의 클래스 리스트 — 리터럴/for-in/`xs[i]` read+write/`pop`/`pop(i)`/`add`/`insert`/`index_of`/`contains`/`len` · 함수 파라미터/반환(`-> list[point]`) — interpreter와 `un native`가 바이트 동일 |
| `test/compile/build/class_list.un` | plain native exe 게이트 — assert 모듈 없이 `use io`만 import하는 클래스 리스트 프로그램을 `scripts/exe-diff.ps1`이 plain `un build` exe로 링크해 `un run`과 바이트 비교 |
| `test/compile/build/bare_for.un` | plain native exe 게이트 — 로컬 어노테이션 초기화(`dp: list[int] = [0]`)와 반복 변수 없는 `for 0..100`을 쓰는 프로그램을 `scripts/exe-diff.ps1`이 plain `un build` exe로 링크해 `un run`과 바이트 비교 |
| `test/compile/iter_array.un` | `iter.array(v, n)` intrinsic — v가 int/float이면 `list[int]`/`list[float]`, `dp: list[int] = iter.array(...)`까지 포함해 interpreter와 `un native`/`un build --embed`가 바이트 동일 |
| `test/compile/build/iter_minmax.un` | plain native exe 게이트 — `iter.min`/`iter.max(list[int]/list[float])` — `scripts/exe-diff.ps1`이 plain `un build` exe로 링크해 `un run`과 바이트 비교 |
| `test/compile/build/native_modules.un` | plain native exe 게이트 — Int/Float/Bool/Str 결과 네이티브 모듈 멤버(math/os/sys/time/fs/codes/random/re)를 native exe로 링크해 `un run`과 stdout/stderr/exit code 바이트 비교 |
| `test/compile/native_modules.un` | 같은 내용의 interpreter ↔ `un native` 교차 검증 (assert 모듈 사용) |
| `test/compile/build/iter_array.un` | `iter_array.un`의 plain exe 게이트 — `iter.array` + 인덱스 write + `len`/`xs[i]` 읽기 — `scripts/exe-diff.ps1`이 plain `un build` exe로 링크해 `un run`과 바이트 비교 |
| `scripts/exe-diff.ps1` | `test/compile/*.un` → `--embed` exe 바이트 비교 + `test/compile/build/*.un` → plain exe 바이트 비교 (exit/stdout/stderr 전부) |
| `test/compile/matches.un` | match exhaustiveness 분석 — bool true/false arm·int arm·`_` wildcard·none arm을 포함한 exhaustive match 표기; interpreter와 `un native`가 바이트 동일 |
| `bench/native_fib.un` | 어노테이션된 fib 벤치마크 (native 경로 확인 및 `scripts/compile-bench.ps1` 입력) |
| `test/assert.un` | 어설트 모듈 (`assert_eq` / `assert_true` / `summary()`) |
| `test/compile/errors/*.un` | 거부가 예정된 프로그램 예제 — `un check`/`un native` 양쪽이 exit 1이며 `manifest.txt`의 기대 부분문자열이 에러 출력에 있어야 하는 부정 케이스 (`member call on list/str`, `undefined name`, `unassignable property`, `return type mismatch`, `untyped params`, syntax error 등) | 

## 알려진 차이 (문서화된, 의도적)

1. `test/test_suite.un` 등 **의도적으로 동적 타입인 테스트는 compile mode에서 거부된다.**
   이유: compile mode는 모든 매개변수/반환 어노테이션을 요구한다. 거부되는 것이
   정상이며, 컴파일-모드 게이트로 사용하지 않는다. `un check`는 `exit 1`과 함께
   최대 50개의 진단을 출력한다.
2. `un run`(인터프리터)과 `un native`는 `test/compile/*.un`에서 **바이트 동일**한
   stdout/stderr/exit code를 내야 한다. `scripts/compile-check.ps1`이 이를 검사한다.
3. `test/compile/errors/`의 `.un` 픽스처는 `un check`/`un native` 양쪽 모두 exit 1과
   `manifest.txt`에 고정한 부분문자열 진단(예: `'list' object has no attribute 'a'`)
   이 나오는지 검증한다. 에러 케이스를 추가하려면 픽스처를 추가하고 `manifest.txt`에
   `path|substring` 한 행을 더하면 된다.
3. 네이티브 실행 중 bail이 일어나면(예: `//` 제수 0, `INT_MIN // -1`, 네이티브 재귀
   깊이 초과) VM이 해당 호출을 interpreter로 재실행한다. 이 경로는 interpreter와
   출력/종료 코드가 바이트 동일하며, 그것이 측정·검증의 근거다.
4. 재미있는 함정: `for i in a..b`의 `..`는 AST에서 `Binary(DotDot)`으로 나타나며,
   typeck는 이것을 int 범위로 인정한다. 네이티브에서는 `iter.range(a, b)`와 동일하게
   `TirStmt::ForRange`로 하강한다.

## 실패 테스트 프레임워크

- 현재는 `scripts/compile-check.ps1`이 어노테이션 누락 소스가 `un check`/`un native`
  모두 `exit 1`과 `requires a type annotation` 진단을 내는지를 인라인으로 검사한다.
- 정식 negative manifest(`test/compile/errors/*.un` + `manifest.txt`)는 다음 단계
  항목으로 명시한다. 현재 없어도 green이며, 추가 시 이 문서를 갱신한다.

## 검증되지 않은 영역 (v1에서 제외)

- TIR core에서 **class/closure/collection/union/`any`/native 호출/부동소수 함수(sqrt 등)**는
  네이티브화되지 않는다. 인터프리터가 실행한다 (의도된 동작, 버그 아님).
- **수렴 검사(exhaustiveness)**는 구현하지 않는다. union 매개변수의 전수 검사가 필요한
  프로그램은 v2에서 다룬다 (`COMPILE_DESIGN.md` §4.6 참조).
