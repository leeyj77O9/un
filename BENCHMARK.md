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

- 측정 일시: 2026-11-19 08:39:15 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|4b4440d
- 측정 커밋: 4b4440d
- Un 실행 파일: `rust\target\release\unvm.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **7.527x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **6.757x 느리다**.
- Un VM은 Python보다 기하평균 **3.182x 느리다**.
- Un VM은 Node.js보다 기하평균 **161.818x 느리다**.
- Python은 Node.js보다 기하평균 **50.859x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 7.527x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.148x  | >1 = Un이 빠름 |
| Un VM / Python | 0.314x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.006x  | >1 = Un이 빠름 |
| Python / Node.js | 0.020x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1649.9752 | 2.8434 | 2369.0718 | 9.5690 | 833.183 | 3.365 |
| fib24 | 1 | 194.0758 | 1.9310 | 3.3082 | 0.2673 | 1.713 | 0.138 |
| nested_sum_200 | 40000 | 8.7374 | 0.0175 | 16.6056 | 0.0492 | 948.891 | 2.811 |
| qsort500 | 500 | 16.2520 | 0.8536 | 0.9179 | 0.0809 | 1.075 | 0.095 |
| mergesort1000 | 1000 | 42.9857 | 1.5230 | 2.1923 | 0.0819 | 1.439 | 0.054 |
| rle1994 | 1994 | 7.1144 | 7.3640 | 2.2936 | 0.0390 | 0.311 | 0.005 |
| dict5000 | 10000 | 3.6564 | 3.6575 | 5.3599 | 0.1667 | 1.465 | 0.046 |
| iter10000 | 30000 | 17.3630 | 4.0880 | 4.9739 | 0.2647 | 1.217 | 0.065 |
| math10000 | 10000 | 19.0264 | 0.0517 | 4.8040 | 0.0628 | 92.921 | 1.215 |
| class1000 | 1000 | 4.5846 | 0.5617 | 1.2844 | 0.0072 | 2.287 | 0.013 |
| sieve30000 | 30000 | 57.5810 | 2.4718 | 14.3947 | 0.2548 | 5.824 | 0.103 |
| matrix48 | 110592 | 71.8301 | 11.3514 | 22.2303 | 0.2012 | 1.958 | 0.018 |
| sin20000 | 20000 | 22.7764 | 0.5400 | 5.7386 | 0.1573 | 10.627 | 0.291 |
| call200000 | 200000 | 267.1547 | 0.6838 | 141.6131 | 3.8222 | 207.097 | 5.590 |
| regex6000 | 6000 | 1.2613 | 1.2083 | 2.7444 | 0.0895 | 2.271 | 0.074 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1649.9752 | 2.8434 | 2369.0718 | 9.5690 | 833.183 | 3.365 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 8.7374 | 0.0175 | 16.6056 | 0.0492 | 948.891 | 2.811 |
| sieve30000 | 30000 | 57.5810 | 2.4718 | 14.3947 | 0.2548 | 5.824 | 0.103 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 267.1547 | 0.6838 | 141.6131 | 3.8222 | 207.097 | 5.590 |
| fib24 | 1 | 194.0758 | 1.9310 | 3.3082 | 0.2673 | 1.713 | 0.138 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 3.6564 | 3.6575 | 5.3599 | 0.1667 | 1.465 | 0.046 |
| class1000 | 1000 | 4.5846 | 0.5617 | 1.2844 | 0.0072 | 2.287 | 0.013 |
| rle1994 | 1994 | 7.1144 | 7.3640 | 2.2936 | 0.0390 | 0.311 | 0.005 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 16.2520 | 0.8536 | 0.9179 | 0.0809 | 1.075 | 0.095 |
| mergesort1000 | 1000 | 42.9857 | 1.5230 | 2.1923 | 0.0819 | 1.439 | 0.054 |
| matrix48 | 110592 | 71.8301 | 11.3514 | 22.2303 | 0.2012 | 1.958 | 0.018 |
| math10000 | 10000 | 19.0264 | 0.0517 | 4.8040 | 0.0628 | 92.921 | 1.215 |
| sin20000 | 20000 | 22.7764 | 0.5400 | 5.7386 | 0.1573 | 10.627 | 0.291 |
| regex6000 | 6000 | 1.2613 | 1.2083 | 2.7444 | 0.0895 | 2.271 | 0.074 |
| iter10000 | 30000 | 17.3630 | 4.0880 | 4.9739 | 0.2647 | 1.217 | 0.065 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.157 | 0.000 |
| fib24 | 0.157 | 0.000 |
| nested_sum_200 | 0.224 | 0.000 |
| qsort500 | -0.408 | 0.000 |
| mergesort1000 | -0.324 | 0.000 |
| rle1994 | 0.502 | 0.000 |
| dict5000 | 0.743 | 0.000 |
| iter10000 | 0.280 | 0.001 |
| math10000 | 0.162 | 0.000 |
| class1000 | -0.298 | 0.000 |
| sieve30000 | 1.200 | 0.000 |
| matrix48 | 0.619 | 0.000 |
| sin20000 | 0.161 | 0.000 |
| call200000 | 0.159 | 0.000 |
| regex6000 | 0.084 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1649.9752 | 2.8434 | 2369.0718 | 9.5690 | 833.183 | 3.365 |
| fib24 | 1 | 194.0758 | 1.9310 | 3.3082 | 0.2673 | 1.713 | 0.138 |
| nested_sum_200 | 40000 | 8.7374 | 0.0175 | 16.6056 | 0.0492 | 948.891 | 2.811 |
| qsort500 | 500 | 16.2520 | 0.8536 | 0.9179 | 0.0809 | 1.075 | 0.095 |
| mergesort1000 | 1000 | 42.9857 | 1.5230 | 2.1923 | 0.0819 | 1.439 | 0.054 |
| rle1994 | 1994 | 7.1144 | 7.3640 | 2.2936 | 0.0390 | 0.311 | 0.005 |
| dict5000 | 10000 | 3.6564 | 3.6575 | 5.3599 | 0.1667 | 1.465 | 0.046 |
| iter10000 | 30000 | 17.3630 | 4.0880 | 4.9739 | 0.2647 | 1.217 | 0.065 |
| math10000 | 10000 | 19.0264 | 0.0517 | 4.8040 | 0.0628 | 92.921 | 1.215 |
| class1000 | 1000 | 4.5846 | 0.5617 | 1.2844 | 0.0072 | 2.287 | 0.013 |
| sieve30000 | 30000 | 57.5810 | 2.4718 | 14.3947 | 0.2548 | 5.824 | 0.103 |
| matrix48 | 110592 | 71.8301 | 11.3514 | 22.2303 | 0.2012 | 1.958 | 0.018 |
| sin20000 | 20000 | 22.7764 | 0.5400 | 5.7386 | 0.1573 | 10.627 | 0.291 |
| call200000 | 200000 | 267.1547 | 0.6838 | 141.6131 | 3.8222 | 207.097 | 5.590 |
| regex6000 | 6000 | 1.2613 | 1.2083 | 2.7444 | 0.0895 | 2.271 | 0.074 |

## 13. Performance Analysis

- **전반적으로 Un JIT은 Python보다 기하평균 **7.527x 빠르다**.**
- **전반적으로 Un JIT은 Node.js보다 기하평균 **6.757배 느리다**.**
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 833.183x, call200000은 207.097x(Int 함수의 native 직접 호출), nested_sum_200은 948.891x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.365x, call200000 5.590x, nested_sum_200 2.811x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261119-083134\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | -0.2% | 보통 |
| int_ops | python | +0.2% | 보통 |
| int_ops | node | +0.1% | 보통 |
| fib24 | unjit | -0.2% | 보통 |
| fib24 | python | -0.5% | 보통 |
| fib24 | node | +0.3% | 보통 |
| nested_sum_200 | unjit | +0.0% | 보통 |
| nested_sum_200 | python | -0.5% | 보통 |
| nested_sum_200 | node | +4.0% | 보통 |
| qsort500 | unjit | +0.0% | 보통 |
| qsort500 | python | +5.7% | 보통 |
| qsort500 | node | -22.9% | 개선 |
| mergesort1000 | unjit | +1.0% | 보통 |
| mergesort1000 | python | +1.4% | 보통 |
| mergesort1000 | node | +19.0% | 회귀 |
| rle1994 | unjit | +2.2% | 보통 |
| rle1994 | python | -7.7% | 보통 |
| rle1994 | node | -2.3% | 보통 |
| dict5000 | unjit | -64.9% | 개선 |
| dict5000 | python | -2.0% | 보통 |
| dict5000 | node | +19.3% | 회귀 |
| iter10000 | unjit | +9.1% | 보통 |
| iter10000 | python | +6.5% | 보통 |
| iter10000 | node | +25.5% | 회귀 |
| math10000 | unjit | -8.0% | 보통 |
| math10000 | python | +10.4% | 회귀 |
| math10000 | node | -0.3% | 보통 |
| class1000 | unjit | -82.3% | 개선 |
| class1000 | python | -3.8% | 보통 |
| class1000 | node | +9.1% | 보통 |
| sieve30000 | unjit | -0.8% | 보통 |
| sieve30000 | python | -1.5% | 보통 |
| sieve30000 | node | +15.4% | 회귀 |
| matrix48 | unjit | -3.5% | 보통 |
| matrix48 | python | +1.2% | 보통 |
| matrix48 | node | -6.5% | 보통 |
| sin20000 | unjit | -0.1% | 보통 |
| sin20000 | python | +2.0% | 보통 |
| sin20000 | node | +1.2% | 보통 |
| call200000 | unjit | -0.2% | 보통 |
| call200000 | python | -1.4% | 보통 |
| call200000 | node | +0.1% | 보통 |
| regex6000 | unjit | -22.5% | 개선 |
| regex6000 | python | +0.9% | 보통 |
| regex6000 | node | -5.0% | 보통 |

## 15. Historical Comparison

이전 결과(20261119-083134) 대비 **10% 초과 변동 항목만** 요약한다. 전체 추적은 ench/results/<timestamp>/results.csv와 14절을 참고.

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| qsort500 | node | -22.9% | 개선 |
| mergesort1000 | node | +19.0% | 회귀 |
| dict5000 | unjit | -64.9% | 개선 |
| dict5000 | node | +19.3% | 회귀 |
| iter10000 | node | +25.5% | 회귀 |
| math10000 | python | +10.4% | 회귀 |
| class1000 | unjit | -82.3% | 개선 |
| sieve30000 | node | +15.4% | 회귀 |
| regex6000 | unjit | -22.5% | 개선 |

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

