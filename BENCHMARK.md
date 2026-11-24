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

- 측정 일시: 2026-11-24 10:48:23 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|26d457f
- 측정 커밋: 26d457f
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **4.318x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **14.599x 느리다**.
- Un VM은 Python보다 기하평균 **2.420x 느리다**.
- Un VM은 Node.js보다 기하평균 **152.539x 느리다**.
- Python은 Node.js보다 기하평균 **63.036x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 4.318x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.068x  | >1 = Un이 빠름 |
| Un VM / Python | 0.413x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.007x  | >1 = Un이 빠름 |
| Python / Node.js | 0.016x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 996.5057 | 2.8418 | 2.8571 | 2.8619 | 2333.9351 | 9.5657 | 821.288 | 3.366 |
| fib24 | 1 | 148.1655 | 1.9333 | 126.3657 | 0.2038 | 3.1378 | 0.2679 | 1.623 | 0.139 |
| native_fib24 | 1 | 139.6995 | 1.9292 | 126.0757 | 0.2075 | 3.2465 | 0.2665 | 1.683 | 0.138 |
| nested_sum_200 | 40000 | 6.6960 | 0.0182 | 0.0114 | 0.0115 | 16.4987 | 0.0505 | 906.522 | 2.775 |
| qsort500 | 500 | 12.5472 | 0.8570 | 15.0000 | 0.5250 | 0.8748 | 0.0952 | 1.021 | 0.111 |
| mergesort1000 | 1000 | 30.9671 | 1.5235 | 32.0000 | 0.5200 | 2.2144 | 0.0869 | 1.453 | 0.057 |
| rle1994 | 1994 | 5.2258 | 5.1566 | N/A | N/A | 2.4644 | 0.0486 | 0.478 | 0.009 |
| dict5000 | 10000 | 2.4776 | 0.2603 | 2.5714 | 0.1941 | 5.2694 | 0.1654 | 20.244 | 0.635 |
| iter10000 | 30000 | 14.7969 | 15.9092 | 27.1429 | 0.2311 | 4.6009 | 0.2033 | 0.289 | 0.013 |
| math10000 | 10000 | 14.6227 | 0.0264 | 0.0243 | 0.0264 | 4.9364 | 0.0628 | 186.985 | 2.379 |
| class1000 | 1000 | 2.8257 | 0.5735 | 1.9700 | 0.0608 | 1.2920 | 0.0075 | 2.253 | 0.013 |
| sieve30000 | 30000 | 38.9701 | 2.4691 | 44.7143 | 0.2633 | 14.0767 | 0.1992 | 5.701 | 0.081 |
| matrix48 | 110592 | 44.7368 | 11.3524 | N/A | N/A | 21.4933 | 0.2025 | 1.893 | 0.018 |
| sin20000 | 20000 | 17.1004 | 0.5388 | 0.5357 | 0.5407 | 5.6956 | 0.1542 | 10.571 | 0.286 |
| call200000 | 200000 | 209.5004 | 0.6864 | 156.1857 | 0.7484 | 137.7431 | 3.8191 | 200.675 | 5.564 |
| regex6000 | 6000 | 1.2116 | 1.2010 | N/A | N/A | 2.6004 | 0.0791 | 2.165 | 0.066 |
| str_unicode | 2000 | 3.8428 | 3.8245 | N/A | N/A | 2.2334 | 0.1932 | 0.584 | 0.051 |
| except10000 | 10000 | 3.4883 | 0.0265 | N/A | N/A | 3.3374 | 0.0215 | 125.940 | 0.811 |
| match20000 | 20000 | 29.4870 | 31.7822 | N/A | N/A | 9.7708 | 0.0492 | 0.307 | 0.002 |
| closure20000 | 20000 | 20.1633 | 20.9303 | N/A | N/A | 7.3704 | 0.0281 | 0.352 | 0.001 |
| inherit1000 | 1000 | 1.8580 | 1.9456 | N/A | N/A | 0.9632 | 0.0341 | 0.495 | 0.018 |
| set5000 | 5000 | 5.1560 | 4.9881 | N/A | N/A | 1.8052 | 0.0437 | 0.362 | 0.009 |
| enum10000 | 10000 | 6.8820 | 6.8580 | N/A | N/A | 4.8102 | 0.0124 | 0.701 | 0.002 |
| modimport2000 | 2000 | 0.3773 | 0.0047 | N/A | N/A | 0.3274 | 0.0027 | 69.660 | 0.574 |
| nanbox20000 | 20000 | 11.9025 | 11.8471 | N/A | N/A | 7.3393 | 0.0167 | 0.620 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 996.5057 | 2.8418 | 2.8571 | 2.8619 | 2333.9351 | 9.5657 | 821.288 | 3.366 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 6.6960 | 0.0182 | 0.0114 | 0.0115 | 16.4987 | 0.0505 | 906.522 | 2.775 |
| sieve30000 | 30000 | 38.9701 | 2.4691 | 44.7143 | 0.2633 | 14.0767 | 0.1992 | 5.701 | 0.081 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 209.5004 | 0.6864 | 156.1857 | 0.7484 | 137.7431 | 3.8191 | 200.675 | 5.564 |
| fib24 | 1 | 148.1655 | 1.9333 | 126.3657 | 0.2038 | 3.1378 | 0.2679 | 1.623 | 0.139 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 2.4776 | 0.2603 | 2.5714 | 0.1941 | 5.2694 | 0.1654 | 20.244 | 0.635 |
| class1000 | 1000 | 2.8257 | 0.5735 | 1.9700 | 0.0608 | 1.2920 | 0.0075 | 2.253 | 0.013 |
| rle1994 | 1994 | 5.2258 | 5.1566 | N/A | N/A | 2.4644 | 0.0486 | 0.478 | 0.009 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 12.5472 | 0.8570 | 15.0000 | 0.5250 | 0.8748 | 0.0952 | 1.021 | 0.111 |
| mergesort1000 | 1000 | 30.9671 | 1.5235 | 32.0000 | 0.5200 | 2.2144 | 0.0869 | 1.453 | 0.057 |
| matrix48 | 110592 | 44.7368 | 11.3524 | N/A | N/A | 21.4933 | 0.2025 | 1.893 | 0.018 |
| math10000 | 10000 | 14.6227 | 0.0264 | 0.0243 | 0.0264 | 4.9364 | 0.0628 | 186.985 | 2.379 |
| sin20000 | 20000 | 17.1004 | 0.5388 | 0.5357 | 0.5407 | 5.6956 | 0.1542 | 10.571 | 0.286 |
| regex6000 | 6000 | 1.2116 | 1.2010 | N/A | N/A | 2.6004 | 0.0791 | 2.165 | 0.066 |
| iter10000 | 30000 | 14.7969 | 15.9092 | 27.1429 | 0.2311 | 4.6009 | 0.2033 | 0.289 | 0.013 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.621 | 0.000 |
| fib24 | 0.876 | 0.000 |
| native_fib24 | 0.882 | 0.000 |
| nested_sum_200 | 0.895 | 0.000 |
| qsort500 | 0.201 | 0.007 |
| mergesort1000 | 0.232 | 0.015 |
| rle1994 | 1.053 | 0.000 |
| dict5000 | 1.342 | 0.000 |
| iter10000 | 0.441 | 0.001 |
| math10000 | 0.887 | 0.000 |
| class1000 | 0.293 | 0.000 |
| sieve30000 | 1.900 | 0.000 |
| matrix48 | 1.149 | 0.000 |
| sin20000 | 0.896 | 0.000 |
| call200000 | 0.771 | 0.000 |
| regex6000 | 1.449 | 0.000 |
| str_unicode | 1.355 | 0.000 |
| except10000 | 0.729 | 0.000 |
| match20000 | 1.103 | 0.000 |
| closure20000 | 0.744 | 0.000 |
| inherit1000 | -0.129 | 0.000 |
| set5000 | 1.005 | 0.000 |
| enum10000 | 0.941 | 0.000 |
| modimport2000 | -0.231 | 0.000 |
| nanbox20000 | 0.874 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **4.318x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **14.599배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 821.288x, call200000은 200.675x(Int 함수의 native 직접 호출), nested_sum_200은 906.522x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.366x, call200000 5.564x, nested_sum_200 2.775x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261124-084701\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | -5.0% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | -3.0% | 보통 |
| int_ops | python | -2.5% | 보통 |
| int_ops | node | -0.3% | 보통 |
| fib24 | unjit | -1.7% | 보통 |
| fib24 | compile | -13.3% | 개선 |
| fib24 | build | +13.4% | 회귀 |
| fib24 | python | +0.8% | 보통 |
| fib24 | node | -0.3% | 보통 |
| native_fib24 | unjit | -1.7% | 보통 |
| native_fib24 | compile | -12.7% | 개선 |
| native_fib24 | build | +13.8% | 회귀 |
| native_fib24 | python | +2.5% | 보통 |
| native_fib24 | node | -0.5% | 보통 |
| nested_sum_200 | unjit | -1.6% | 보통 |
| nested_sum_200 | compile | +0.0% | 보통 |
| nested_sum_200 | build | -15.8% | 개선 |
| nested_sum_200 | python | +0.8% | 보통 |
| nested_sum_200 | node | +6.1% | 보통 |
| qsort500 | unjit | +0.2% | 보통 |
| qsort500 | compile | -7.1% | 보통 |
| qsort500 | build | -5.6% | 보통 |
| qsort500 | python | +2.5% | 보통 |
| qsort500 | node | -6.1% | 보통 |
| mergesort1000 | unjit | -1.0% | 보통 |
| mergesort1000 | compile | -14.2% | 개선 |
| mergesort1000 | build | +3.3% | 보통 |
| mergesort1000 | python | +0.1% | 보통 |
| mergesort1000 | node | +0.6% | 보통 |
| rle1994 | unjit | -4.7% | 보통 |
| rle1994 | python | +7.9% | 보통 |
| rle1994 | node | +3.6% | 보통 |
| dict5000 | unjit | +0.0% | 보통 |
| dict5000 | compile | +5.9% | 보통 |
| dict5000 | build | +10.4% | 회귀 |
| dict5000 | python | -5.6% | 보통 |
| dict5000 | node | +12.8% | 회귀 |
| iter10000 | unjit | -5.5% | 보통 |
| iter10000 | compile | -3.6% | 보통 |
| iter10000 | build | +0.3% | 보통 |
| iter10000 | python | -4.6% | 보통 |
| iter10000 | node | +11.2% | 회귀 |
| math10000 | unjit | -0.4% | 보통 |
| math10000 | compile | -5.6% | 보통 |
| math10000 | build | +2.5% | 보통 |
| math10000 | python | +10.7% | 회귀 |
| math10000 | node | +0.3% | 보통 |
| class1000 | unjit | -0.4% | 보통 |
| class1000 | compile | -9.4% | 보통 |
| class1000 | build | -2.6% | 보통 |
| class1000 | python | +0.2% | 보통 |
| class1000 | node | +31.6% | 회귀 |
| sieve30000 | unjit | -1.3% | 보통 |
| sieve30000 | compile | -2.8% | 보통 |
| sieve30000 | build | +1.7% | 보통 |
| sieve30000 | python | -1.2% | 보통 |
| sieve30000 | node | -0.6% | 보통 |
| matrix48 | unjit | -2.0% | 보통 |
| matrix48 | python | -3.1% | 보통 |
| matrix48 | node | +0.2% | 보통 |
| sin20000 | unjit | +0.0% | 보통 |
| sin20000 | compile | -2.3% | 보통 |
| sin20000 | build | +0.3% | 보통 |
| sin20000 | python | +0.6% | 보통 |
| sin20000 | node | -10.6% | 개선 |
| call200000 | unjit | +0.3% | 보통 |
| call200000 | compile | -4.7% | 보통 |
| call200000 | build | +0.2% | 보통 |
| call200000 | python | +0.1% | 보통 |
| call200000 | node | -0.1% | 보통 |
| regex6000 | unjit | +0.1% | 보통 |
| regex6000 | python | -10.2% | 개선 |
| regex6000 | node | -12.2% | 개선 |
| str_unicode | unjit | -9.1% | 보통 |
| str_unicode | python | -7.1% | 보통 |
| str_unicode | node | +6.6% | 보통 |
| except10000 | unjit | -1.1% | 보통 |
| except10000 | python | +3.0% | 보통 |
| except10000 | node | +0.9% | 보통 |
| match20000 | unjit | -4.0% | 보통 |
| match20000 | python | -3.1% | 보통 |
| match20000 | node | -25.1% | 개선 |
| closure20000 | unjit | -35.7% | 개선 |
| closure20000 | python | +0.8% | 보통 |
| closure20000 | node | -2.1% | 보통 |
| inherit1000 | unjit | -4.1% | 보통 |
| inherit1000 | python | +1.0% | 보통 |
| inherit1000 | node | +189.0% | 회귀 |
| set5000 | unjit | -6.1% | 보통 |
| set5000 | python | -1.2% | 보통 |
| set5000 | node | +26.3% | 회귀 |
| enum10000 | unjit | -10.0% | 보통 |
| enum10000 | python | +2.1% | 보통 |
| enum10000 | node | +0.0% | 보통 |
| modimport2000 | unjit | -4.1% | 보통 |
| modimport2000 | python | -4.6% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | +0.2% | 보통 |
| nanbox20000 | python | -0.1% | 보통 |
| nanbox20000 | node | +0.6% | 보통 |

## 15. Historical Comparison

이전 결과(20261124-084701) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

