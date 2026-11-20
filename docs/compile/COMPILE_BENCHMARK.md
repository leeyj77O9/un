# COMPILE_BENCHMARK.md — Compile Mode 벤치마크

측정 결과는 머신마다 다르므로, 아래 수치는 **측정 환경과 재현 방법**과 함께 기록한다.

## 측정 환경

- 2026-11-19 (exe 행 추가 2026-11-20), Windows x64 (GNU toolchain), `rustc stable-x86_64-pc-windows-gnu` `--release`
- 벤치마크 스크립트: `bench/native_fib.un` (annotated fib(24), 7 reps, warmup 2, median)
- plain exe 벤치: `bench/native_fib_exe.un` — Un 모듈 import 없이 `use io`/`use time`만 쓰는 core-compile 가능한 변형. 700회 fib(24)를 프로세스 안에서 돌려 회당 ms를 출력하고, `compile-bench.ps1`이 7회 실행 median을 낸다(다른 모드의 in-process median과 같은 단위).
- 실행 파일: `rust\target\release\un.exe`

재현:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\compile-bench.ps1
# 측정 루프가 bench.harness를 쓰므로 수동 비교:
.\rust\target\release\un.exe native bench\native_fib.un
# plain exe 직접 비교:
.\rust\target\release\un.exe build bench\native_fib_exe.un -o %TEMP%\native_fib_exe.exe
%TEMP%\native_fib_exe.exe
```

## 측정 결과 (median of 7, ms)

| 모드 | median | 비고 |
|---|---|---|
| `un run` (VM + 기존 bytecode JIT) | 194.3–240.0 | JIT 동작, compile mode 미사용 |
| `un run` (VM, `UV_JIT=0`) | 195.5–198.1 | 순수 인터프리터 |
| `un native` (Compile Mode) | **169.3–174.3** | core 함수 native + 하네스 interp |
| `un build` (standalone exe) | **≈0.17–0.18** | 2026-11-20 재측정. 위 세 모드와 달리 `bench.harness`가 없는 자체 타이밍(700회 fib)이라 순수 fib 계산 시간만 비교 가능 |

→ compile mode가 `un run` 대비 약 **10–13%** 빠르다. 이 수치가 작게 나오는 이유는
측정이 `bench.harness.measure`(interpreter) + 함수 호출 오버헤드에 지배되기 때문이며,
TIR core 경로가 실제로 native로 실행됨은 `UV_TIR_DEBUG=1`로 확인할 수 있다
(예: `TIRDBG OK proto=1 fib`).
plain exe는 Cranelift가 fib를 직접 네이티브 코드로 만든 결과로 **JIT 대비 약 10배,
인터프리터 대비 약 1000배** 빠르다. 단, exe 행은 `bench.harness`(assert/결과 검증)가
없고 700회 fib 자체의 회당 시간만 재므로 위 세 행과 동일한 조건은 아니다.

## 주의사항

- **Compile Mode의 이득은 core 부분집합(int/float/bool 산술·비교·직접 호출·제어흐름)에
  한정된다.** 프로그램의 상당 부분이 `any`/동적 경계로 이루어져 있으면 기존 실행과
  같거나 미미하게 빠르다. 이 벤치마크는 core에 들어가는 순수 int 재귀를 측정한다.
- `fib(24)`은 약 100k회 재귀 호출이다. 순수 정수 루프만 있는 워크로드라면 격차가
  더 벌어지지만, 아직 루프 벤치마크를 compile-mode에서 측정하지 않았다.
- `bench.ps1`(Node/Python 비교)은 compile mode 열을 아직 포함하지 않는다. 추가 시
  `scripts/compile-bench.ps1`을 참조하라.
- 검증(`=== Result: 2 passed, 0 failed ===`)은 세 모드 모두 통과해야 한다. native가
  틀린 답을 내면 측정 전에 스크립트가 실패로 끝난다.
