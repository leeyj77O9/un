# BENCHMARK.md

Un 벤치마크 결과입니다. 실행: `scripts\bench.ps1 -Group all -Report` (Windows)

## 1. Benchmark Overview

Un(Rust 바이트코드 컴파일러 + 레지스터 VM + Cranelift JIT)을 CPython 3.13과 Node.js 26과
비교하기 위한 벤치마크다. 각 테스트는 bench/<name>.un, bench/node/<name>.mjs,
bench/python/<name>.py의 세 파일이 **같은 알고리즘, 같은 입력, 같은 검증**을 공유한다.

| 노드 | 내용 |
|---|---|
| 측정 언어 | Un VM (UV_JIT=0), Un JIT(기본), CPython, Node.js |
| 측정 방식 | 워밍업 2회 → reps=7회 개별 측정, Median 비교 |
| 저장 위치 | bench/results/<timestamp>/results.csv, results.json, results.md |
| 보고서 | 이 문서(BENCHMARK.md)은 -Report로 자동 생성된다 |
| 비교 지표 | 중앙값, 종합은 기하평균(∏ rᵢ^(1/n)) |

## 2. Environment

- ENV|un|os=Microsoft Windows NT 10.0.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|982d5f3
- 측정 커밋: 982d5f3
- Un 실행 파일: `rust\target\release\unvm.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **6.313x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **9.456x 느리다**.
- Un VM은 Python보다 기하평균 **4.582x 느리다**.
- Un VM은 Node.js보다 기하평균 **273.548x 느리다**.
- Python은 Node.js보다 기하평균 **59.698x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(Int 부분집합, floor/sqrt·sin intrinsic, IntDict, 클래스 Int 필드 메서드)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 6.313x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.106x  | >1 = Un이 빠름 |
| Un VM / Python | 0.218x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.004x  | >1 = Un이 빠름 |
| Python / Node.js | 0.017x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 4452.2669 | 3.1818 | 3189.8712 | 10.7777 | 1002.537 | 3.387 |
| fib24 | 1 | 252.3533 | 1.4474 | 5.2006 | 0.2958 | 3.593 | 0.204 |
| nested_sum_200 | 40000 | 20.1904 | 0.0204 | 23.4778 | 0.0675 | 1150.873 | 3.309 |
| qsort500 | 500 | 26.3982 | 0.8969 | 1.2312 | 0.1018 | 1.373 | 0.114 |
| mergesort1000 | 1000 | 67.9051 | 1.8980 | 3.1957 | 0.1058 | 1.684 | 0.056 |
| rle1994 | 1994 | 15.4245 | 15.5104 | 2.9673 | 0.0651 | 0.191 | 0.004 |
| dict5000 | 10000 | 5.5161 | 5.1260 | 7.0483 | 0.1303 | 1.375 | 0.025 |
| iter10000 | 30000 | 25.0247 | 6.4006 | 6.5359 | 0.2373 | 1.021 | 0.037 |
| math10000 | 10000 | 31.5453 | 0.0171 | 6.8633 | 0.0631 | 401.363 | 3.690 |
| class1000 | 1000 | 6.9280 | 0.8898 | 1.5598 | 0.0066 | 1.753 | 0.007 |
| sieve30000 | 30000 | 81.2192 | 5.3779 | 18.9113 | 0.4607 | 3.516 | 0.086 |
| matrix48 | 110592 | 97.0726 | 97.3843 | 29.2939 | 0.1967 | 0.301 | 0.002 |
| sin20000 | 20000 | 37.2067 | 0.6257 | 7.3659 | 0.1776 | 11.772 | 0.284 |
| call200000 | 200000 | 475.1573 | 0.7171 | 181.2338 | 3.8598 | 252.732 | 5.383 |
| regex6000 | 6000 | 18.6147 | 20.2963 | 3.5912 | 0.1132 | 0.177 | 0.006 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 4452.2669 | 3.1818 | 3189.8712 | 10.7777 | 1002.537 | 3.387 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 20.1904 | 0.0204 | 23.4778 | 0.0675 | 1150.873 | 3.309 |
| sieve30000 | 30000 | 81.2192 | 5.3779 | 18.9113 | 0.4607 | 3.516 | 0.086 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 475.1573 | 0.7171 | 181.2338 | 3.8598 | 252.732 | 5.383 |
| fib24 | 1 | 252.3533 | 1.4474 | 5.2006 | 0.2958 | 3.593 | 0.204 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 5.5161 | 5.1260 | 7.0483 | 0.1303 | 1.375 | 0.025 |
| class1000 | 1000 | 6.9280 | 0.8898 | 1.5598 | 0.0066 | 1.753 | 0.007 |
| rle1994 | 1994 | 15.4245 | 15.5104 | 2.9673 | 0.0651 | 0.191 | 0.004 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 26.3982 | 0.8969 | 1.2312 | 0.1018 | 1.373 | 0.114 |
| mergesort1000 | 1000 | 67.9051 | 1.8980 | 3.1957 | 0.1058 | 1.684 | 0.056 |
| matrix48 | 110592 | 97.0726 | 97.3843 | 29.2939 | 0.1967 | 0.301 | 0.002 |
| math10000 | 10000 | 31.5453 | 0.0171 | 6.8633 | 0.0631 | 401.363 | 3.690 |
| sin20000 | 20000 | 37.2067 | 0.6257 | 7.3659 | 0.1776 | 11.772 | 0.284 |
| regex6000 | 6000 | 18.6147 | 20.2963 | 3.5912 | 0.1132 | 0.177 | 0.006 |
| iter10000 | 30000 | 25.0247 | 6.4006 | 6.5359 | 0.2373 | 1.021 | 0.037 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.157 | 0.000 |
| fib24 | 0.157 | 0.000 |
| nested_sum_200 | 0.161 | 0.000 |
| qsort500 | -0.454 | 0.000 |
| mergesort1000 | -0.339 | 0.000 |
| rle1994 | 0.479 | 0.000 |
| dict5000 | 0.542 | 0.000 |
| iter10000 | -0.111 | 0.001 |
| math10000 | 0.162 | 0.000 |
| class1000 | -0.306 | 0.000 |
| sieve30000 | 1.290 | 0.000 |
| matrix48 | 0.640 | 0.000 |
| sin20000 | 0.161 | 0.000 |
| call200000 | 0.222 | 0.000 |
| regex6000 | 1.717 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 4452.2669 | 3.1818 | 3189.8712 | 10.7777 | 1002.537 | 3.387 |
| fib24 | 1 | 252.3533 | 1.4474 | 5.2006 | 0.2958 | 3.593 | 0.204 |
| nested_sum_200 | 40000 | 20.1904 | 0.0204 | 23.4778 | 0.0675 | 1150.873 | 3.309 |
| qsort500 | 500 | 26.3982 | 0.8969 | 1.2312 | 0.1018 | 1.373 | 0.114 |
| mergesort1000 | 1000 | 67.9051 | 1.8980 | 3.1957 | 0.1058 | 1.684 | 0.056 |
| rle1994 | 1994 | 15.4245 | 15.5104 | 2.9673 | 0.0651 | 0.191 | 0.004 |
| dict5000 | 10000 | 5.5161 | 5.1260 | 7.0483 | 0.1303 | 1.375 | 0.025 |
| iter10000 | 30000 | 25.0247 | 6.4006 | 6.5359 | 0.2373 | 1.021 | 0.037 |
| math10000 | 10000 | 31.5453 | 0.0171 | 6.8633 | 0.0631 | 401.363 | 3.690 |
| class1000 | 1000 | 6.9280 | 0.8898 | 1.5598 | 0.0066 | 1.753 | 0.007 |
| sieve30000 | 30000 | 81.2192 | 5.3779 | 18.9113 | 0.4607 | 3.516 | 0.086 |
| matrix48 | 110592 | 97.0726 | 97.3843 | 29.2939 | 0.1967 | 0.301 | 0.002 |
| sin20000 | 20000 | 37.2067 | 0.6257 | 7.3659 | 0.1776 | 11.772 | 0.284 |
| call200000 | 200000 | 475.1573 | 0.7171 | 181.2338 | 3.8598 | 252.732 | 5.383 |
| regex6000 | 6000 | 18.6147 | 20.2963 | 3.5912 | 0.1132 | 0.177 | 0.006 |

## 13. Performance Analysis

- **전반적으로 Un JIT은 Python보다 기하평균 **6.313x 빠르다**.**
- **전반적으로 Un JIT은 Node.js보다 기하평균 **9.456배 느리다**.**
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 1002.537x, math10000은 401.363x(floor/sqrt intrinsic), call200000은 252.732x(Int 함수의 native 직접 호출), nested_sum_200은 1150.873x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.387x, call200000 5.383x, nested_sum_200 3.309x, math10000 3.690x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, Float-start 누산기 루프(sin20000, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261117-204720\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| regex6000 | unjit | +8.9% | 보통 |
| regex6000 | python | +3.9% | 보통 |
| regex6000 | node | +49.5% | 회귀 |

## 15. Historical Comparison

이전 결과(20261117-204720) 대비 **10% 초과 변동 항목만** 요약한다. 전체 추적은 ench/results/<timestamp>/results.csv와 14절을 참고.

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| regex6000 | node | +49.5% | 회귀 |

## 16. Benchmark Methodology

측정 명령:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1                 # All lanes
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group unvm -NoSave
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group unjit -NoSave
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group python -NoSave
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group node -NoSave
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Only qsort500,mergesort1000
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Compare bench\results\<이전results.csv>
```

- 워밍업 2회, 이후 reps=7회 개별 측정해 Min/Median/Average/Max/StdDev(모집단)를 낸다.
- 비교 지표는 **중앙값**(중위수). 종합은 **기하평균** ∏ rᵢ^(1/n).
- 비율은 `baseline` ÷ `candidate` 로 정의한다(1.00x=동일, 2.00x=candidate가 2배 빠름).
- 각 스크립트는 측정 전에 자기검증을 수행한다. 실패하면 측정값이 나오지 않고 실행 실패로 집계한다.
- 시간 측정: Un `time.stopwatch().tick()`(100ns 틱, ms환산), Node `performance.now()`, Python `time.perf_counter()`.
- Un은 UV_JIT=0으로 VM만, 기본값으로 JIT(런타임)을 측정한다.

## 17. Known Limitations

- Un에는 힙 메모리 측정 API가 없다(Node/Python은 tracemalloc/heapUsed 기준 참고치).
- Un의 sys.version()/os.version은 실제 호스트와 무관한 고정 문자열을 반환한다(TD-44).
- JIT은 Int 부분집합(floor/sqrt, sin intrinsic, IntDict, 클래스 Int 필드 메서드)만 native 컴파일하고 나머지는 인터프리터로 정확히 실행한다.
- Python의 시스템 PATH가 broken WindowsApps stub일 수 있다. 이 러너는 경로를 자동 탐지한다.
- Python의 tracemalloc은 Python heap만 잰다. Node의 heapUsed는 V8 heap만 잰다. 둘 다 RSS 비교가 아니다.
- 동시성 모델이 다르므로 동시성 벤치는 측정하지 않는다(TD-39).

