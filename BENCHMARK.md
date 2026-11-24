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

- 측정 일시: 2026-11-24 18:56:02 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|9f330a0
- 측정 커밋: 9f330a0
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **4.306x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **15.189x 느리다**.
- Un VM은 Python보다 기하평균 **2.381x 느리다**.
- Un VM은 Node.js보다 기하평균 **155.735x 느리다**.
- Python은 Node.js보다 기하평균 **65.398x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 4.306x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.066x  | >1 = Un이 빠름 |
| Un VM / Python | 0.420x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.006x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1066.9117 | 2.8882 | 2.8571 | 2.9356 | 2427.0530 | 9.8640 | 840.334 | 3.415 |
| fib24 | 1 | 138.9254 | 2.0373 | 128.1214 | 0.2108 | 3.1655 | 0.2686 | 1.554 | 0.132 |
| native_fib24 | 1 | 152.4261 | 1.9531 | 124.3786 | 0.2097 | 3.1958 | 0.2658 | 1.636 | 0.136 |
| nested_sum_200 | 40000 | 6.8793 | 0.0181 | 0.0114 | 0.0118 | 16.1640 | 0.0472 | 893.039 | 2.608 |
| qsort500 | 500 | 12.2125 | 0.8573 | 14.2857 | 0.5386 | 0.8838 | 0.1196 | 1.031 | 0.140 |
| mergesort1000 | 1000 | 30.3417 | 1.5120 | 31.7143 | 0.5289 | 2.1324 | 0.0813 | 1.410 | 0.054 |
| rle1994 | 1994 | 5.3150 | 5.1622 | N/A | N/A | 2.3246 | 0.0420 | 0.450 | 0.008 |
| dict5000 | 10000 | 2.4847 | 0.2588 | 2.4286 | 0.1883 | 5.4946 | 0.1342 | 21.231 | 0.519 |
| iter10000 | 30000 | 13.6837 | 14.5418 | 25.2857 | 0.2337 | 4.7786 | 0.2356 | 0.329 | 0.016 |
| math10000 | 10000 | 14.3788 | 0.0279 | 0.0257 | 0.0277 | 4.5465 | 0.0641 | 162.957 | 2.297 |
| class1000 | 1000 | 2.6895 | 0.5764 | 1.9543 | 0.0629 | 1.2739 | 0.0085 | 2.210 | 0.015 |
| sieve30000 | 30000 | 38.6110 | 2.4797 | 45.2857 | 0.2861 | 14.2336 | 0.2136 | 5.740 | 0.086 |
| matrix48 | 110592 | 45.5130 | 11.6099 | N/A | N/A | 22.9239 | 0.2103 | 1.975 | 0.018 |
| sin20000 | 20000 | 16.0704 | 0.5436 | 0.5486 | 0.5534 | 5.7393 | 0.1527 | 10.558 | 0.281 |
| call200000 | 200000 | 203.6503 | 0.6999 | 156.7757 | 0.7797 | 147.5637 | 3.8369 | 210.835 | 5.482 |
| regex6000 | 6000 | 1.2168 | 1.5343 | N/A | N/A | 2.6046 | 0.0957 | 1.698 | 0.062 |
| str_unicode | 2000 | 3.5891 | 3.6377 | N/A | N/A | 2.3998 | 0.1941 | 0.660 | 0.053 |
| except10000 | 10000 | 3.5588 | 0.0262 | N/A | N/A | 3.3186 | 0.0214 | 126.664 | 0.817 |
| match20000 | 20000 | 29.0050 | 30.8760 | N/A | N/A | 10.1528 | 0.0586 | 0.329 | 0.002 |
| closure20000 | 20000 | 19.4885 | 20.1670 | N/A | N/A | 7.6143 | 0.0283 | 0.378 | 0.001 |
| inherit1000 | 1000 | 2.1380 | 1.8579 | N/A | N/A | 0.9728 | 0.0126 | 0.524 | 0.007 |
| set5000 | 5000 | 4.6220 | 5.6415 | N/A | N/A | 1.7890 | 0.0359 | 0.317 | 0.006 |
| enum10000 | 10000 | 6.9200 | 6.8381 | N/A | N/A | 4.7926 | 0.0124 | 0.701 | 0.002 |
| modimport2000 | 2000 | 0.3757 | 0.0046 | N/A | N/A | 0.3322 | 0.0027 | 72.217 | 0.587 |
| nanbox20000 | 20000 | 12.1407 | 12.2531 | N/A | N/A | 7.4191 | 0.0160 | 0.605 | 0.001 |
| dict_str5000 | 10000 | 5.4974 | 5.3676 | N/A | N/A | N/A | N/A | N/A | N/A |
| bytes_ops | 10000 | 14.5734 | 14.5995 | N/A | N/A | N/A | N/A | N/A | N/A |
| slice_ops | 4000 | 4.2992 | 4.3018 | N/A | N/A | N/A | N/A | N/A | N/A |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1066.9117 | 2.8882 | 2.8571 | 2.9356 | 2427.0530 | 9.8640 | 840.334 | 3.415 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 6.8793 | 0.0181 | 0.0114 | 0.0118 | 16.1640 | 0.0472 | 893.039 | 2.608 |
| sieve30000 | 30000 | 38.6110 | 2.4797 | 45.2857 | 0.2861 | 14.2336 | 0.2136 | 5.740 | 0.086 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 203.6503 | 0.6999 | 156.7757 | 0.7797 | 147.5637 | 3.8369 | 210.835 | 5.482 |
| fib24 | 1 | 138.9254 | 2.0373 | 128.1214 | 0.2108 | 3.1655 | 0.2686 | 1.554 | 0.132 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 2.4847 | 0.2588 | 2.4286 | 0.1883 | 5.4946 | 0.1342 | 21.231 | 0.519 |
| class1000 | 1000 | 2.6895 | 0.5764 | 1.9543 | 0.0629 | 1.2739 | 0.0085 | 2.210 | 0.015 |
| rle1994 | 1994 | 5.3150 | 5.1622 | N/A | N/A | 2.3246 | 0.0420 | 0.450 | 0.008 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 12.2125 | 0.8573 | 14.2857 | 0.5386 | 0.8838 | 0.1196 | 1.031 | 0.140 |
| mergesort1000 | 1000 | 30.3417 | 1.5120 | 31.7143 | 0.5289 | 2.1324 | 0.0813 | 1.410 | 0.054 |
| matrix48 | 110592 | 45.5130 | 11.6099 | N/A | N/A | 22.9239 | 0.2103 | 1.975 | 0.018 |
| math10000 | 10000 | 14.3788 | 0.0279 | 0.0257 | 0.0277 | 4.5465 | 0.0641 | 162.957 | 2.297 |
| sin20000 | 20000 | 16.0704 | 0.5436 | 0.5486 | 0.5534 | 5.7393 | 0.1527 | 10.558 | 0.281 |
| regex6000 | 6000 | 1.2168 | 1.5343 | N/A | N/A | 2.6046 | 0.0957 | 1.698 | 0.062 |
| iter10000 | 30000 | 13.6837 | 14.5418 | 25.2857 | 0.2337 | 4.7786 | 0.2356 | 0.329 | 0.016 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.066 | 0.000 |
| fib24 | 0.731 | 0.000 |
| native_fib24 | 0.731 | 0.000 |
| nested_sum_200 | 0.881 | 0.000 |
| qsort500 | 0.167 | 0.007 |
| mergesort1000 | 0.286 | 0.015 |
| rle1994 | 1.057 | 0.000 |
| dict5000 | 1.222 | 0.000 |
| iter10000 | 0.753 | 0.001 |
| math10000 | 0.756 | 0.000 |
| class1000 | 0.437 | 0.000 |
| sieve30000 | 1.914 | 0.000 |
| matrix48 | 1.192 | 0.000 |
| sin20000 | 0.899 | 0.000 |
| call200000 | 0.916 | 0.000 |
| regex6000 | 1.151 | 0.000 |
| str_unicode | 1.369 | 0.000 |
| except10000 | 0.734 | 0.000 |
| match20000 | 1.122 | 0.000 |
| closure20000 | 0.178 | 0.000 |
| inherit1000 | 0.589 | 0.000 |
| set5000 | 1.159 | 0.000 |
| enum10000 | 0.731 | 0.000 |
| modimport2000 | 0.258 | 0.000 |
| nanbox20000 | 0.895 | 0.000 |
| dict_str5000 | N/A | N/A |
| bytes_ops | N/A | N/A |
| slice_ops | N/A | N/A |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **4.306x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **15.189배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 840.334x, call200000은 210.835x(Int 함수의 native 직접 호출), nested_sum_200은 893.039x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.415x, call200000 5.482x, nested_sum_200 2.608x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261124-184402\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | +1.3% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | +2.1% | 보통 |
| int_ops | python | -1.5% | 보통 |
| int_ops | node | +2.2% | 보통 |
| fib24 | unjit | +4.5% | 보통 |
| fib24 | compile | -0.3% | 보통 |
| fib24 | build | -0.5% | 보통 |
| fib24 | python | -1.3% | 보통 |
| fib24 | node | +0.1% | 보통 |
| native_fib24 | unjit | +0.6% | 보통 |
| native_fib24 | compile | -3.4% | 보통 |
| native_fib24 | build | -2.0% | 보통 |
| native_fib24 | python | -38.7% | 개선 |
| native_fib24 | node | -43.6% | 개선 |
| nested_sum_200 | unjit | +0.0% | 보통 |
| nested_sum_200 | compile | -11.1% | 개선 |
| nested_sum_200 | build | -4.0% | 보통 |
| nested_sum_200 | python | -30.6% | 개선 |
| nested_sum_200 | node | -2.1% | 보통 |
| qsort500 | unjit | -5.2% | 보통 |
| qsort500 | compile | -9.1% | 보통 |
| qsort500 | build | -7.2% | 보통 |
| qsort500 | python | -1.6% | 보통 |
| qsort500 | node | +25.6% | 회귀 |
| mergesort1000 | unjit | -5.8% | 보통 |
| mergesort1000 | compile | -3.9% | 보통 |
| mergesort1000 | build | -3.0% | 보통 |
| mergesort1000 | python | -0.3% | 보통 |
| mergesort1000 | node | -8.2% | 보통 |
| rle1994 | unjit | -4.1% | 보통 |
| rle1994 | python | -3.4% | 보통 |
| rle1994 | node | -12.3% | 개선 |
| dict5000 | unjit | -1.0% | 보통 |
| dict5000 | compile | -10.5% | 개선 |
| dict5000 | build | -3.8% | 보통 |
| dict5000 | python | -3.9% | 보통 |
| dict5000 | node | +0.1% | 보통 |
| iter10000 | unjit | -4.2% | 보통 |
| iter10000 | compile | +0.0% | 보통 |
| iter10000 | build | +0.4% | 보통 |
| iter10000 | python | -3.0% | 보통 |
| iter10000 | node | +21.6% | 회귀 |
| math10000 | unjit | +6.9% | 보통 |
| math10000 | compile | +0.0% | 보통 |
| math10000 | build | +7.5% | 보통 |
| math10000 | python | -4.3% | 보통 |
| math10000 | node | +2.9% | 보통 |
| class1000 | unjit | -44.5% | 개선 |
| class1000 | compile | -1.2% | 보통 |
| class1000 | build | -4.5% | 보통 |
| class1000 | python | -2.2% | 보통 |
| class1000 | node | -21.3% | 개선 |
| sieve30000 | unjit | -1.5% | 보통 |
| sieve30000 | compile | +2.3% | 보통 |
| sieve30000 | build | +4.2% | 보통 |
| sieve30000 | python | +0.4% | 보통 |
| sieve30000 | node | -14.3% | 개선 |
| matrix48 | unjit | -0.1% | 보통 |
| matrix48 | python | -1.4% | 보통 |
| matrix48 | node | -7.4% | 보통 |
| sin20000 | unjit | +0.9% | 보통 |
| sin20000 | compile | -1.0% | 보통 |
| sin20000 | build | -2.1% | 보통 |
| sin20000 | python | -0.1% | 보통 |
| sin20000 | node | -0.4% | 보통 |
| call200000 | unjit | +2.0% | 보통 |
| call200000 | compile | -1.6% | 보통 |
| call200000 | build | -1.9% | 보통 |
| call200000 | python | -0.5% | 보통 |
| call200000 | node | +0.4% | 보통 |
| regex6000 | unjit | +24.1% | 회귀 |
| regex6000 | python | -0.2% | 보통 |
| regex6000 | node | -4.5% | 보통 |
| str_unicode | unjit | -9.5% | 보통 |
| str_unicode | python | +5.5% | 보통 |
| str_unicode | node | +3.1% | 보통 |
| except10000 | unjit | -0.8% | 보통 |
| except10000 | python | -1.9% | 보통 |
| except10000 | node | -0.5% | 보통 |
| match20000 | unjit | -2.0% | 보통 |
| match20000 | python | -0.9% | 보통 |
| match20000 | node | +14.2% | 회귀 |
| closure20000 | unjit | -7.2% | 보통 |
| closure20000 | python | +0.6% | 보통 |
| closure20000 | node | +0.4% | 보통 |
| inherit1000 | unjit | -1.0% | 보통 |
| inherit1000 | python | +0.7% | 보통 |
| inherit1000 | node | +6.8% | 보통 |
| set5000 | unjit | +17.8% | 회귀 |
| set5000 | python | -0.5% | 보통 |
| set5000 | node | +3.5% | 보통 |
| enum10000 | unjit | -2.3% | 보통 |
| enum10000 | python | +2.0% | 보통 |
| enum10000 | node | -0.8% | 보통 |
| modimport2000 | unjit | +7.0% | 보통 |
| modimport2000 | python | -2.9% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | -0.1% | 보통 |
| nanbox20000 | python | -0.3% | 보통 |
| nanbox20000 | node | -1.8% | 보통 |
| dict_str5000 | unjit | -2.7% | 보통 |
| bytes_ops | unjit | +1.0% | 보통 |
| slice_ops | unjit | +1.2% | 보통 |

## 15. Historical Comparison

이전 결과(20261124-184402) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

