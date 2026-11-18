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

- 측정 일시: 2026-11-18 21:53:35 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|fbf39bf
- 측정 커밋: fbf39bf
- Un 실행 파일: `rust\target\release\unvm.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **6.729x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **7.455x 느리다**.
- Un VM은 Python보다 기하평균 **3.253x 느리다**.
- Un VM은 Node.js보다 기하평균 **163.185x 느리다**.
- Python은 Node.js보다 기하평균 **50.166x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 6.729x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.134x  | >1 = Un이 빠름 |
| Un VM / Python | 0.307x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.006x  | >1 = Un이 빠름 |
| Python / Node.js | 0.020x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1714.0621 | 2.8712 | 2393.3698 | 9.5838 | 833.578 | 3.338 |
| fib24 | 1 | 186.9422 | 2.5318 | 3.1505 | 0.2679 | 1.244 | 0.106 |
| nested_sum_200 | 40000 | 8.7654 | 0.0127 | 16.2501 | 0.0630 | 1279.535 | 4.961 |
| qsort500 | 500 | 15.8950 | 0.8798 | 0.8784 | 0.0861 | 0.998 | 0.098 |
| mergesort1000 | 1000 | 43.4192 | 1.6054 | 2.1389 | 0.0679 | 1.332 | 0.042 |
| rle1994 | 1994 | 7.3071 | 7.0178 | 2.3089 | 0.0525 | 0.329 | 0.007 |
| dict5000 | 10000 | 3.7052 | 3.7003 | 5.2753 | 0.1490 | 1.426 | 0.040 |
| iter10000 | 30000 | 16.7778 | 3.8467 | 5.0307 | 0.2192 | 1.308 | 0.057 |
| math10000 | 10000 | 19.5082 | 0.0542 | 4.6479 | 0.0626 | 85.755 | 1.155 |
| class1000 | 1000 | 4.6305 | 0.5675 | 1.3185 | 0.0054 | 2.323 | 0.010 |
| sieve30000 | 30000 | 58.2848 | 2.4608 | 14.1277 | 0.2346 | 5.741 | 0.095 |
| matrix48 | 110592 | 71.1113 | 11.3438 | 21.5124 | 0.2031 | 1.896 | 0.018 |
| sin20000 | 20000 | 25.5035 | 0.5370 | 5.7537 | 0.1572 | 10.715 | 0.293 |
| call200000 | 200000 | 266.6432 | 1.7655 | 138.8592 | 3.8680 | 78.651 | 2.191 |
| regex6000 | 6000 | 1.2194 | 2.0310 | 2.6961 | 0.1111 | 1.327 | 0.055 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1714.0621 | 2.8712 | 2393.3698 | 9.5838 | 833.578 | 3.338 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 8.7654 | 0.0127 | 16.2501 | 0.0630 | 1279.535 | 4.961 |
| sieve30000 | 30000 | 58.2848 | 2.4608 | 14.1277 | 0.2346 | 5.741 | 0.095 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 266.6432 | 1.7655 | 138.8592 | 3.8680 | 78.651 | 2.191 |
| fib24 | 1 | 186.9422 | 2.5318 | 3.1505 | 0.2679 | 1.244 | 0.106 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 3.7052 | 3.7003 | 5.2753 | 0.1490 | 1.426 | 0.040 |
| class1000 | 1000 | 4.6305 | 0.5675 | 1.3185 | 0.0054 | 2.323 | 0.010 |
| rle1994 | 1994 | 7.3071 | 7.0178 | 2.3089 | 0.0525 | 0.329 | 0.007 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 15.8950 | 0.8798 | 0.8784 | 0.0861 | 0.998 | 0.098 |
| mergesort1000 | 1000 | 43.4192 | 1.6054 | 2.1389 | 0.0679 | 1.332 | 0.042 |
| matrix48 | 110592 | 71.1113 | 11.3438 | 21.5124 | 0.2031 | 1.896 | 0.018 |
| math10000 | 10000 | 19.5082 | 0.0542 | 4.6479 | 0.0626 | 85.755 | 1.155 |
| sin20000 | 20000 | 25.5035 | 0.5370 | 5.7537 | 0.1572 | 10.715 | 0.293 |
| regex6000 | 6000 | 1.2194 | 2.0310 | 2.6961 | 0.1111 | 1.327 | 0.055 |
| iter10000 | 30000 | 16.7778 | 3.8467 | 5.0307 | 0.2192 | 1.308 | 0.057 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.157 | 0.000 |
| fib24 | 0.157 | 0.000 |
| nested_sum_200 | 0.161 | 0.000 |
| qsort500 | -0.438 | 0.000 |
| mergesort1000 | -0.327 | 0.000 |
| rle1994 | 0.457 | 0.000 |
| dict5000 | 0.575 | 0.000 |
| iter10000 | 0.280 | 0.001 |
| math10000 | 0.162 | 0.000 |
| class1000 | -1.042 | 0.000 |
| sieve30000 | 1.212 | 0.000 |
| matrix48 | 0.620 | 0.000 |
| sin20000 | 0.159 | 0.000 |
| call200000 | 0.159 | 0.000 |
| regex6000 | 1.717 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1714.0621 | 2.8712 | 2393.3698 | 9.5838 | 833.578 | 3.338 |
| fib24 | 1 | 186.9422 | 2.5318 | 3.1505 | 0.2679 | 1.244 | 0.106 |
| nested_sum_200 | 40000 | 8.7654 | 0.0127 | 16.2501 | 0.0630 | 1279.535 | 4.961 |
| qsort500 | 500 | 15.8950 | 0.8798 | 0.8784 | 0.0861 | 0.998 | 0.098 |
| mergesort1000 | 1000 | 43.4192 | 1.6054 | 2.1389 | 0.0679 | 1.332 | 0.042 |
| rle1994 | 1994 | 7.3071 | 7.0178 | 2.3089 | 0.0525 | 0.329 | 0.007 |
| dict5000 | 10000 | 3.7052 | 3.7003 | 5.2753 | 0.1490 | 1.426 | 0.040 |
| iter10000 | 30000 | 16.7778 | 3.8467 | 5.0307 | 0.2192 | 1.308 | 0.057 |
| math10000 | 10000 | 19.5082 | 0.0542 | 4.6479 | 0.0626 | 85.755 | 1.155 |
| class1000 | 1000 | 4.6305 | 0.5675 | 1.3185 | 0.0054 | 2.323 | 0.010 |
| sieve30000 | 30000 | 58.2848 | 2.4608 | 14.1277 | 0.2346 | 5.741 | 0.095 |
| matrix48 | 110592 | 71.1113 | 11.3438 | 21.5124 | 0.2031 | 1.896 | 0.018 |
| sin20000 | 20000 | 25.5035 | 0.5370 | 5.7537 | 0.1572 | 10.715 | 0.293 |
| call200000 | 200000 | 266.6432 | 1.7655 | 138.8592 | 3.8680 | 78.651 | 2.191 |
| regex6000 | 6000 | 1.2194 | 2.0310 | 2.6961 | 0.1111 | 1.327 | 0.055 |

## 13. Performance Analysis

- **전반적으로 Un JIT은 Python보다 기하평균 **6.729x 빠르다**.**
- **전반적으로 Un JIT은 Node.js보다 기하평균 **7.455배 느리다**.**
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 833.578x, call200000은 78.651x(Int 함수의 native 직접 호출), nested_sum_200은 1279.535x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.338x, call200000 2.191x, nested_sum_200 4.961x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261118-205755\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | -8.9% | 보통 |
| int_ops | python | -20.9% | 개선 |
| int_ops | node | -9.5% | 보통 |
| fib24 | unjit | -6.1% | 보통 |
| fib24 | python | -21.3% | 개선 |
| fib24 | node | -18.3% | 개선 |
| nested_sum_200 | unjit | +0.0% | 보통 |
| nested_sum_200 | python | -17.8% | 개선 |
| nested_sum_200 | node | +197.2% | 회귀 |
| qsort500 | unjit | -13.1% | 개선 |
| qsort500 | python | -13.0% | 개선 |
| qsort500 | node | -18.9% | 개선 |
| mergesort1000 | unjit | -3.2% | 보통 |
| mergesort1000 | python | -17.9% | 개선 |
| mergesort1000 | node | -42.1% | 개선 |
| rle1994 | unjit | -27.7% | 개선 |
| rle1994 | python | -24.4% | 개선 |
| rle1994 | node | +9.8% | 보통 |
| dict5000 | unjit | -14.9% | 개선 |
| dict5000 | python | -20.8% | 개선 |
| dict5000 | node | -38.0% | 개선 |
| iter10000 | unjit | -18.0% | 개선 |
| iter10000 | python | -21.6% | 개선 |
| iter10000 | node | -17.3% | 개선 |
| math10000 | unjit | -6.2% | 보통 |
| math10000 | python | -18.1% | 개선 |
| math10000 | node | -3.1% | 보통 |
| class1000 | unjit | -17.8% | 개선 |
| class1000 | python | -14.2% | 개선 |
| class1000 | node | -46.0% | 개선 |
| sieve30000 | unjit | -5.8% | 보통 |
| sieve30000 | python | -20.8% | 개선 |
| sieve30000 | node | -10.6% | 개선 |
| matrix48 | unjit | -9.5% | 보통 |
| matrix48 | python | -20.2% | 개선 |
| matrix48 | node | -12.0% | 개선 |
| sin20000 | unjit | -6.6% | 보통 |
| sin20000 | python | -20.6% | 개선 |
| sin20000 | node | -19.4% | 개선 |
| call200000 | unjit | -6.2% | 보통 |
| call200000 | python | -22.2% | 개선 |
| call200000 | node | -6.7% | 보통 |
| regex6000 | unjit | +25.5% | 회귀 |
| regex6000 | python | -21.4% | 개선 |
| regex6000 | node | +18.8% | 회귀 |

## 15. Historical Comparison

이전 결과(20261118-205755) 대비 **10% 초과 변동 항목만** 요약한다. 전체 추적은 ench/results/<timestamp>/results.csv와 14절을 참고.

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | python | -20.9% | 개선 |
| fib24 | python | -21.3% | 개선 |
| fib24 | node | -18.3% | 개선 |
| nested_sum_200 | python | -17.8% | 개선 |
| nested_sum_200 | node | +197.2% | 회귀 |
| qsort500 | unjit | -13.1% | 개선 |
| qsort500 | python | -13.0% | 개선 |
| qsort500 | node | -18.9% | 개선 |
| mergesort1000 | python | -17.9% | 개선 |
| mergesort1000 | node | -42.1% | 개선 |
| rle1994 | unjit | -27.7% | 개선 |
| rle1994 | python | -24.4% | 개선 |
| dict5000 | unjit | -14.9% | 개선 |
| dict5000 | python | -20.8% | 개선 |
| dict5000 | node | -38.0% | 개선 |
| iter10000 | unjit | -18.0% | 개선 |
| iter10000 | python | -21.6% | 개선 |
| iter10000 | node | -17.3% | 개선 |
| math10000 | python | -18.1% | 개선 |
| class1000 | unjit | -17.8% | 개선 |
| class1000 | python | -14.2% | 개선 |
| class1000 | node | -46.0% | 개선 |
| sieve30000 | python | -20.8% | 개선 |
| sieve30000 | node | -10.6% | 개선 |
| matrix48 | python | -20.2% | 개선 |
| matrix48 | node | -12.0% | 개선 |
| sin20000 | python | -20.6% | 개선 |
| sin20000 | node | -19.4% | 개선 |
| call200000 | python | -22.2% | 개선 |
| regex6000 | unjit | +25.5% | 회귀 |
| regex6000 | python | -21.4% | 개선 |
| regex6000 | node | +18.8% | 회귀 |

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
- JIT은 Int 부분집합(IntVec/IntDict/class-method intrinsic)만 native 컴파일하고 나머지는 인터프리터로 정확히 실행한다.
- Python의 시스템 PATH가 broken WindowsApps stub일 수 있다. 이 러너는 경로를 자동 탐지한다.
- Python의 tracemalloc은 Python heap만 잰다. Node의 heapUsed는 V8 heap만 잰다. 둘 다 RSS 비교가 아니다.
- 동시성 모델이 다르므로 동시성 벤치는 측정하지 않는다(TD-39).

