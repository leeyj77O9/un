# BENCHMARK.md

Un 벤치마크 결과입니다. 실행: `scripts\bench.ps1 -Group all -Report` (Windows)

## 1. Benchmark Overview

Un(Rust 바이트코드 컴파일러 + 레지스터 VM + Cranelift JIT)을 CPython 3.13과 Node.js 26과
비교하기 위한 벤치마크다. 각 테스트는 bench/<name>.un, bench/node/<name>.mjs,
bench/python/<name>.py의 세 파일이 **같은 알고리즘, 같은 입력, 같은 검증**을 공유한다.

| 노드 | 내용 |
|---|---|
| 측정 언어 | Un VM (UV_JIT=0), Un JIT(기본), Compile Mode(un native), standalone exe(un build), CPython, Node.js |
| 측정 방식 | 언어 3레인: 워밍업 2회 → reps=7회 / Compile: 자체 타이밍 1회 / Exe: 워밍업 1회 + 7회, Median 비교 |
| 저장 위치 | bench/results/<timestamp>/results.csv, results.json, results.md |
| 보고서 | 이 문서(BENCHMARK.md)은 -Report로 자동 생성된다 |
| 비교 지표 | 중앙값, 종합은 기하평균(∏ rᵢ^(1/n)) |

## 2. Environment

- 측정 일시: 2026-11-24 13:50:28 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|9d2943c
- 측정 커밋: 9d2943c
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **4.301x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **14.723x 느리다**.
- Un VM은 Python보다 기하평균 **2.397x 느리다**.
- Un VM은 Node.js보다 기하평균 **151.769x 느리다**.
- Python은 Node.js보다 기하평균 **63.329x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 4.301x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.068x  | >1 = Un이 빠름 |
| Un VM / Python | 0.417x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.007x  | >1 = Un이 빠름 |
| Python / Node.js | 0.016x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1025.6099 | 2.8473 | 3.0000 | 2.8680 | 2380.3276 | 9.9230 | 835.995 | 3.485 |
| fib24 | 1 | 140.2132 | 1.9363 | 125.7857 | 0.2087 | 3.1939 | 0.2851 | 1.649 | 0.147 |
| native_fib24 | 1 | 134.4571 | 1.9627 | 124.8886 | 0.2119 | 3.1419 | 0.2869 | 1.601 | 0.146 |
| nested_sum_200 | 40000 | 6.8651 | 0.0183 | 0.0114 | 0.0127 | 16.6734 | 0.0468 | 911.115 | 2.557 |
| qsort500 | 500 | 12.0137 | 0.8766 | 15.1429 | 0.5246 | 0.8956 | 0.0856 | 1.022 | 0.098 |
| mergesort1000 | 1000 | 30.1883 | 1.5007 | 32.2857 | 0.5051 | 2.3295 | 0.1117 | 1.552 | 0.074 |
| rle1994 | 1994 | 5.2114 | 5.1203 | N/A | N/A | 2.4374 | 0.0544 | 0.476 | 0.011 |
| dict5000 | 10000 | 2.6930 | 0.2594 | 2.4286 | 0.1890 | 5.3600 | 0.1502 | 20.663 | 0.579 |
| iter10000 | 30000 | 13.7871 | 15.4068 | 25.5714 | 0.2437 | 4.4738 | 0.1217 | 0.290 | 0.008 |
| math10000 | 10000 | 14.3488 | 0.0261 | 0.0243 | 0.0253 | 4.7335 | 0.0628 | 181.360 | 2.406 |
| class1000 | 1000 | 2.6755 | 0.5967 | 1.9014 | 0.0680 | 1.2806 | 0.0116 | 2.146 | 0.019 |
| sieve30000 | 30000 | 39.5435 | 2.4822 | 43.2857 | 0.2586 | 14.4351 | 0.2053 | 5.815 | 0.083 |
| matrix48 | 110592 | 46.3486 | 11.3597 | N/A | N/A | 21.9746 | 0.2115 | 1.934 | 0.019 |
| sin20000 | 20000 | 16.1496 | 0.5389 | 0.5457 | 0.5488 | 5.6299 | 0.2122 | 10.447 | 0.394 |
| call200000 | 200000 | 199.0122 | 0.6852 | 151.1214 | 0.7614 | 140.9533 | 3.8225 | 205.711 | 5.579 |
| regex6000 | 6000 | 1.4230 | 1.5255 | N/A | N/A | 2.7739 | 0.0963 | 1.818 | 0.063 |
| str_unicode | 2000 | 3.9145 | 3.6509 | N/A | N/A | 2.2695 | 0.1904 | 0.622 | 0.052 |
| except10000 | 10000 | 3.5002 | 0.0263 | N/A | N/A | 3.5033 | 0.0216 | 133.205 | 0.821 |
| match20000 | 20000 | 28.9465 | 31.1208 | N/A | N/A | 9.9051 | 0.0664 | 0.318 | 0.002 |
| closure20000 | 20000 | 19.5744 | 24.4643 | N/A | N/A | 7.4616 | 0.0286 | 0.305 | 0.001 |
| inherit1000 | 1000 | 2.2011 | 1.9836 | N/A | N/A | 0.9606 | 0.0172 | 0.484 | 0.009 |
| set5000 | 5000 | 4.5557 | 4.5175 | N/A | N/A | 1.7831 | 0.0333 | 0.395 | 0.007 |
| enum10000 | 10000 | 6.9918 | 7.2244 | N/A | N/A | 4.7336 | 0.0125 | 0.655 | 0.002 |
| modimport2000 | 2000 | 0.3755 | 0.0045 | N/A | N/A | 0.3319 | 0.0027 | 73.756 | 0.600 |
| nanbox20000 | 20000 | 12.2079 | 12.4638 | N/A | N/A | 7.4554 | 0.0164 | 0.598 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1025.6099 | 2.8473 | 3.0000 | 2.8680 | 2380.3276 | 9.9230 | 835.995 | 3.485 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 6.8651 | 0.0183 | 0.0114 | 0.0127 | 16.6734 | 0.0468 | 911.115 | 2.557 |
| sieve30000 | 30000 | 39.5435 | 2.4822 | 43.2857 | 0.2586 | 14.4351 | 0.2053 | 5.815 | 0.083 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 199.0122 | 0.6852 | 151.1214 | 0.7614 | 140.9533 | 3.8225 | 205.711 | 5.579 |
| fib24 | 1 | 140.2132 | 1.9363 | 125.7857 | 0.2087 | 3.1939 | 0.2851 | 1.649 | 0.147 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 2.6930 | 0.2594 | 2.4286 | 0.1890 | 5.3600 | 0.1502 | 20.663 | 0.579 |
| class1000 | 1000 | 2.6755 | 0.5967 | 1.9014 | 0.0680 | 1.2806 | 0.0116 | 2.146 | 0.019 |
| rle1994 | 1994 | 5.2114 | 5.1203 | N/A | N/A | 2.4374 | 0.0544 | 0.476 | 0.011 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 12.0137 | 0.8766 | 15.1429 | 0.5246 | 0.8956 | 0.0856 | 1.022 | 0.098 |
| mergesort1000 | 1000 | 30.1883 | 1.5007 | 32.2857 | 0.5051 | 2.3295 | 0.1117 | 1.552 | 0.074 |
| matrix48 | 110592 | 46.3486 | 11.3597 | N/A | N/A | 21.9746 | 0.2115 | 1.934 | 0.019 |
| math10000 | 10000 | 14.3488 | 0.0261 | 0.0243 | 0.0253 | 4.7335 | 0.0628 | 181.360 | 2.406 |
| sin20000 | 20000 | 16.1496 | 0.5389 | 0.5457 | 0.5488 | 5.6299 | 0.2122 | 10.447 | 0.394 |
| regex6000 | 6000 | 1.4230 | 1.5255 | N/A | N/A | 2.7739 | 0.0963 | 1.818 | 0.063 |
| iter10000 | 30000 | 13.7871 | 15.4068 | 25.5714 | 0.2437 | 4.4738 | 0.1217 | 0.290 | 0.008 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.871 | 0.000 |
| fib24 | 0.876 | 0.000 |
| native_fib24 | 0.876 | 0.000 |
| nested_sum_200 | 0.899 | 0.000 |
| qsort500 | 0.125 | 0.007 |
| mergesort1000 | 0.277 | 0.015 |
| rle1994 | 1.108 | 0.000 |
| dict5000 | 1.528 | 0.000 |
| iter10000 | 0.253 | 0.001 |
| math10000 | 0.756 | 0.000 |
| class1000 | 0.437 | 0.000 |
| sieve30000 | 1.750 | 0.000 |
| matrix48 | 1.136 | 0.000 |
| sin20000 | 0.651 | 0.000 |
| call200000 | 0.751 | 0.000 |
| regex6000 | 1.349 | 0.000 |
| str_unicode | 1.259 | 0.000 |
| except10000 | 0.731 | 0.000 |
| match20000 | 0.570 | 0.000 |
| closure20000 | 0.754 | 0.000 |
| inherit1000 | 0.580 | 0.000 |
| set5000 | 0.753 | 0.000 |
| enum10000 | 0.753 | 0.000 |
| modimport2000 | 0.284 | 0.000 |
| nanbox20000 | 0.736 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **4.301x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **14.723배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 835.995x, call200000은 205.711x(Int 함수의 native 직접 호출), nested_sum_200은 911.115x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.485x, call200000 5.579x, nested_sum_200 2.557x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261124-131352\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | -0.8% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | -5.6% | 보통 |
| int_ops | python | -0.6% | 보통 |
| int_ops | node | +3.6% | 보통 |
| fib24 | unjit | -0.1% | 보통 |
| fib24 | compile | -1.2% | 보통 |
| fib24 | build | +0.1% | 보통 |
| fib24 | python | +0.9% | 보통 |
| fib24 | node | +5.9% | 보통 |
| native_fib24 | unjit | +1.6% | 보통 |
| native_fib24 | compile | -2.9% | 보통 |
| native_fib24 | build | +1.3% | 보통 |
| native_fib24 | python | -1.7% | 보통 |
| native_fib24 | node | +1.7% | 보통 |
| nested_sum_200 | unjit | +0.5% | 보통 |
| nested_sum_200 | compile | +0.0% | 보통 |
| nested_sum_200 | build | +8.1% | 보통 |
| nested_sum_200 | python | +1.4% | 보통 |
| nested_sum_200 | node | -4.9% | 보통 |
| qsort500 | unjit | -5.7% | 보통 |
| qsort500 | compile | +6.0% | 보통 |
| qsort500 | build | +6.3% | 보통 |
| qsort500 | python | +2.4% | 보통 |
| qsort500 | node | -5.7% | 보통 |
| mergesort1000 | unjit | -0.9% | 보통 |
| mergesort1000 | compile | +0.9% | 보통 |
| mergesort1000 | build | -1.7% | 보통 |
| mergesort1000 | python | +6.6% | 보통 |
| mergesort1000 | node | +54.5% | 회귀 |
| rle1994 | unjit | -1.7% | 보통 |
| rle1994 | python | -3.6% | 보통 |
| rle1994 | node | +56.3% | 회귀 |
| dict5000 | unjit | -0.5% | 보통 |
| dict5000 | compile | -10.5% | 개선 |
| dict5000 | build | -0.3% | 보통 |
| dict5000 | python | -4.9% | 보통 |
| dict5000 | node | -19.8% | 개선 |
| iter10000 | unjit | +3.1% | 보통 |
| iter10000 | compile | +4.1% | 보통 |
| iter10000 | build | +5.1% | 보통 |
| iter10000 | python | -6.8% | 보통 |
| iter10000 | node | -41.2% | 개선 |
| math10000 | unjit | -0.4% | 보통 |
| math10000 | compile | -5.6% | 보통 |
| math10000 | build | +1.4% | 보통 |
| math10000 | python | +6.0% | 보통 |
| math10000 | node | +0.3% | 보통 |
| class1000 | unjit | +4.6% | 보통 |
| class1000 | compile | -1.5% | 보통 |
| class1000 | build | +13.4% | 회귀 |
| class1000 | python | +1.9% | 보통 |
| class1000 | node | -10.1% | 개선 |
| sieve30000 | unjit | -0.9% | 보통 |
| sieve30000 | compile | -1.9% | 보통 |
| sieve30000 | build | +0.4% | 보통 |
| sieve30000 | python | +1.9% | 보통 |
| sieve30000 | node | -8.3% | 보통 |
| matrix48 | unjit | -3.2% | 보통 |
| matrix48 | python | -1.3% | 보통 |
| matrix48 | node | +3.9% | 보통 |
| sin20000 | unjit | -0.1% | 보통 |
| sin20000 | compile | +0.0% | 보통 |
| sin20000 | build | +1.0% | 보통 |
| sin20000 | python | -2.4% | 보통 |
| sin20000 | node | +40.7% | 회귀 |
| call200000 | unjit | +0.1% | 보통 |
| call200000 | compile | +0.2% | 보통 |
| call200000 | build | -0.3% | 보통 |
| call200000 | python | -1.3% | 보통 |
| call200000 | node | -0.2% | 보통 |
| regex6000 | unjit | +25.4% | 회귀 |
| regex6000 | python | -0.6% | 보통 |
| regex6000 | node | +5.1% | 보통 |
| str_unicode | unjit | -0.8% | 보통 |
| str_unicode | python | -8.8% | 보통 |
| str_unicode | node | +5.7% | 보통 |
| except10000 | unjit | -5.1% | 보통 |
| except10000 | python | +1.6% | 보통 |
| except10000 | node | +1.4% | 보통 |
| match20000 | unjit | +1.0% | 보통 |
| match20000 | python | +0.0% | 보통 |
| match20000 | node | +15.9% | 회귀 |
| closure20000 | unjit | +1.1% | 보통 |
| closure20000 | python | +2.6% | 보통 |
| closure20000 | node | +1.4% | 보통 |
| inherit1000 | unjit | +4.7% | 보통 |
| inherit1000 | python | +0.5% | 보통 |
| inherit1000 | node | +87.0% | 회귀 |
| set5000 | unjit | -2.7% | 보통 |
| set5000 | python | -0.3% | 보통 |
| set5000 | node | -0.9% | 보통 |
| enum10000 | unjit | +3.3% | 보통 |
| enum10000 | python | +2.3% | 보통 |
| enum10000 | node | +0.0% | 보통 |
| modimport2000 | unjit | -2.2% | 보통 |
| modimport2000 | python | +2.8% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | +4.6% | 보통 |
| nanbox20000 | python | +0.6% | 보통 |
| nanbox20000 | node | -0.6% | 보통 |

## 15. Historical Comparison

이전 결과(20261124-131352) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
중복 표를 생략한다. 전체 추적은 bench/results/<timestamp>/results.csv, 판정 기준은 §14를 참고.

## 16. Benchmark Methodology

방법론(하네스 프로토콜 BENCH/RESULT/SAMPLE/MEM, warmup/reps, 통계 정의,
회귀 게이트 기준, CSV/JSON 스키마)은 `docs/guides/BENCHMARK_GUIDE.md`가 단일 기준이다.
Compile Mode 재현 노트와 단위 차이는 `docs/compile/COMPILE_BENCHMARK.md`를 참고.

- 재생성: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group all -Report`
- 이 문서는 위 -Report 단일 생성기로만 갱신한다(수동 수정 금지).

## 17. Known Limitations

- Un에는 힙 메모리 측정 API가 없다(Node/Python은 tracemalloc/heapUsed 기준 참고치).
- Un의 sys.version()은 `Un <version>`, os.version()은 `<OS family> (Un <version>)`를 반환한다(TD-44 해결, 2026-11-19).
- JIT은 Int 부분집합(IntVec/IntDict/class-method intrinsic)만 native 컴파일하고 나머지는 인터프리터로 정확히 실행한다.
- Python의 시스템 PATH가 broken WindowsApps stub일 수 있다. 이 러너는 경로를 자동 탐지한다.
- Python의 tracemalloc은 Python heap만 잰다. Node의 heapUsed는 V8 heap만 잰다. 둘 다 RSS 비교가 아니다.
- 동시성 모델이 다르므로 동시성 벤치는 측정하지 않는다(TD-39).

