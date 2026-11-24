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

- 측정 일시: 2026-11-24 20:53:39 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|ca47f3f
- 측정 커밋: ca47f3f
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **4.428x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **15.157x 느리다**.
- Un VM은 Python보다 기하평균 **2.364x 느리다**.
- Un VM은 Node.js보다 기하평균 **158.661x 느리다**.
- Python은 Node.js보다 기하평균 **67.117x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 4.428x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.066x  | >1 = Un이 빠름 |
| Un VM / Python | 0.423x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.006x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1027.3914 | 2.8539 | 2.8571 | 2.9290 | 2415.5697 | 9.6144 | 846.410 | 3.369 |
| fib24 | 1 | 135.5194 | 2.0643 | 126.2686 | 0.2070 | 3.1144 | 0.2684 | 1.509 | 0.130 |
| native_fib24 | 1 | 146.8768 | 1.9395 | 126.4771 | 0.2089 | 3.3322 | 0.2680 | 1.718 | 0.138 |
| nested_sum_200 | 40000 | 6.9493 | 0.0182 | 0.0114 | 0.0117 | 16.0951 | 0.0530 | 884.346 | 2.912 |
| qsort500 | 500 | 12.1221 | 0.8610 | 14.7143 | 0.4991 | 0.8501 | 0.0915 | 0.987 | 0.106 |
| mergesort1000 | 1000 | 29.9492 | 1.5179 | 31.8571 | 0.5229 | 2.1319 | 0.0684 | 1.405 | 0.045 |
| rle1994 | 1994 | 5.2265 | 5.1972 | N/A | N/A | 2.3590 | 0.0402 | 0.454 | 0.008 |
| dict5000 | 10000 | 2.4738 | 0.2623 | 2.5714 | 0.1876 | 5.6808 | 0.1390 | 21.658 | 0.530 |
| iter10000 | 30000 | 14.2182 | 14.5966 | 24.7143 | 0.2306 | 4.7459 | 0.2154 | 0.325 | 0.015 |
| math10000 | 10000 | 13.8062 | 0.0261 | 0.0243 | 0.0259 | 4.6982 | 0.0625 | 180.008 | 2.395 |
| class1000 | 1000 | 2.6409 | 0.5677 | 1.9343 | 0.0600 | 1.3257 | 0.0055 | 2.335 | 0.010 |
| sieve30000 | 30000 | 38.6065 | 2.4515 | 44.2857 | 0.2676 | 14.9105 | 0.2230 | 6.082 | 0.091 |
| matrix48 | 110592 | 45.7675 | 11.4046 | N/A | N/A | 22.1380 | 0.2103 | 1.941 | 0.018 |
| sin20000 | 20000 | 16.0713 | 0.5400 | 0.5429 | 0.5464 | 5.6582 | 0.1550 | 10.478 | 0.287 |
| call200000 | 200000 | 200.3235 | 0.6866 | 174.3400 | 0.7631 | 144.4071 | 3.8268 | 210.322 | 5.574 |
| regex6000 | 6000 | 1.2946 | 1.2688 | N/A | N/A | 2.6263 | 0.0725 | 2.070 | 0.057 |
| str_unicode | 2000 | 3.8044 | 3.5929 | N/A | N/A | 2.3127 | 0.1912 | 0.644 | 0.053 |
| except10000 | 10000 | 3.4923 | 0.0264 | N/A | N/A | 3.2697 | 0.0215 | 123.852 | 0.814 |
| match20000 | 20000 | 29.0083 | 30.5830 | N/A | N/A | 10.1258 | 0.0544 | 0.331 | 0.002 |
| closure20000 | 20000 | 19.1934 | 20.4416 | N/A | N/A | 7.6815 | 0.0282 | 0.376 | 0.001 |
| inherit1000 | 1000 | 2.2063 | 1.8824 | N/A | N/A | 0.9975 | 0.0180 | 0.530 | 0.010 |
| set5000 | 5000 | 4.5653 | 4.5730 | N/A | N/A | 1.7795 | 0.0509 | 0.389 | 0.011 |
| enum10000 | 10000 | 6.9069 | 6.8081 | N/A | N/A | 4.9859 | 0.0125 | 0.732 | 0.002 |
| modimport2000 | 2000 | 0.3760 | 0.0044 | N/A | N/A | 0.3424 | 0.0026 | 77.818 | 0.591 |
| nanbox20000 | 20000 | 12.3654 | 12.0569 | N/A | N/A | 7.6005 | 0.0165 | 0.630 | 0.001 |
| dict_str5000 | 10000 | 5.3586 | 5.3225 | N/A | N/A | N/A | N/A | N/A | N/A |
| bytes_ops | 10000 | 14.5907 | 14.3709 | N/A | N/A | N/A | N/A | N/A | N/A |
| slice_ops | 4000 | 4.2119 | 4.2186 | N/A | N/A | N/A | N/A | N/A | N/A |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1027.3914 | 2.8539 | 2.8571 | 2.9290 | 2415.5697 | 9.6144 | 846.410 | 3.369 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 6.9493 | 0.0182 | 0.0114 | 0.0117 | 16.0951 | 0.0530 | 884.346 | 2.912 |
| sieve30000 | 30000 | 38.6065 | 2.4515 | 44.2857 | 0.2676 | 14.9105 | 0.2230 | 6.082 | 0.091 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 200.3235 | 0.6866 | 174.3400 | 0.7631 | 144.4071 | 3.8268 | 210.322 | 5.574 |
| fib24 | 1 | 135.5194 | 2.0643 | 126.2686 | 0.2070 | 3.1144 | 0.2684 | 1.509 | 0.130 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 2.4738 | 0.2623 | 2.5714 | 0.1876 | 5.6808 | 0.1390 | 21.658 | 0.530 |
| class1000 | 1000 | 2.6409 | 0.5677 | 1.9343 | 0.0600 | 1.3257 | 0.0055 | 2.335 | 0.010 |
| rle1994 | 1994 | 5.2265 | 5.1972 | N/A | N/A | 2.3590 | 0.0402 | 0.454 | 0.008 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 12.1221 | 0.8610 | 14.7143 | 0.4991 | 0.8501 | 0.0915 | 0.987 | 0.106 |
| mergesort1000 | 1000 | 29.9492 | 1.5179 | 31.8571 | 0.5229 | 2.1319 | 0.0684 | 1.405 | 0.045 |
| matrix48 | 110592 | 45.7675 | 11.4046 | N/A | N/A | 22.1380 | 0.2103 | 1.941 | 0.018 |
| math10000 | 10000 | 13.8062 | 0.0261 | 0.0243 | 0.0259 | 4.6982 | 0.0625 | 180.008 | 2.395 |
| sin20000 | 20000 | 16.0713 | 0.5400 | 0.5429 | 0.5464 | 5.6582 | 0.1550 | 10.478 | 0.287 |
| regex6000 | 6000 | 1.2946 | 1.2688 | N/A | N/A | 2.6263 | 0.0725 | 2.070 | 0.057 |
| iter10000 | 30000 | 14.2182 | 14.5966 | 24.7143 | 0.2306 | 4.7459 | 0.2154 | 0.325 | 0.015 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.135 | 0.000 |
| fib24 | 0.875 | 0.000 |
| native_fib24 | 0.884 | 0.000 |
| nested_sum_200 | 0.749 | 0.000 |
| qsort500 | 0.224 | 0.007 |
| mergesort1000 | 1.226 | 0.015 |
| rle1994 | 1.129 | 0.000 |
| dict5000 | 1.486 | 0.000 |
| iter10000 | 0.867 | 0.001 |
| math10000 | 0.768 | 0.000 |
| class1000 | 0.293 | 0.000 |
| sieve30000 | 1.934 | 0.000 |
| matrix48 | 1.173 | 0.000 |
| sin20000 | 0.880 | 0.000 |
| call200000 | 0.897 | 0.000 |
| regex6000 | 1.361 | 0.000 |
| str_unicode | 1.336 | 0.000 |
| except10000 | 0.737 | 0.000 |
| match20000 | 0.610 | 0.000 |
| closure20000 | 0.754 | 0.000 |
| inherit1000 | -0.126 | 0.000 |
| set5000 | 0.520 | 0.000 |
| enum10000 | 0.731 | 0.000 |
| modimport2000 | 0.153 | 0.000 |
| nanbox20000 | 0.477 | 0.000 |
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

- 전반적으로 Un JIT은 Python보다 기하평균 **4.428x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **15.157배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 846.410x, call200000은 210.322x(Int 함수의 native 직접 호출), nested_sum_200은 884.346x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.369x, call200000 5.574x, nested_sum_200 2.912x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261124-185012\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | -1.2% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | -0.2% | 보통 |
| int_ops | python | -0.5% | 보통 |
| int_ops | node | -2.5% | 보통 |
| fib24 | unjit | +1.3% | 보통 |
| fib24 | compile | -1.4% | 보통 |
| fib24 | build | -1.8% | 보통 |
| fib24 | python | -1.6% | 보통 |
| fib24 | node | -0.1% | 보통 |
| native_fib24 | unjit | -0.7% | 보통 |
| native_fib24 | compile | +1.7% | 보통 |
| native_fib24 | build | -0.4% | 보통 |
| native_fib24 | python | +4.3% | 보통 |
| native_fib24 | node | +0.8% | 보통 |
| nested_sum_200 | unjit | +0.6% | 보통 |
| nested_sum_200 | compile | +0.0% | 보통 |
| nested_sum_200 | build | -0.6% | 보통 |
| nested_sum_200 | python | -0.4% | 보통 |
| nested_sum_200 | node | +12.3% | 회귀 |
| qsort500 | unjit | +0.4% | 보통 |
| qsort500 | compile | +3.0% | 보통 |
| qsort500 | build | -7.3% | 보통 |
| qsort500 | python | -3.8% | 보통 |
| qsort500 | node | -23.5% | 개선 |
| mergesort1000 | unjit | +0.4% | 보통 |
| mergesort1000 | compile | +0.5% | 보통 |
| mergesort1000 | build | -1.1% | 보통 |
| mergesort1000 | python | +0.0% | 보통 |
| mergesort1000 | node | -15.9% | 개선 |
| rle1994 | unjit | +0.7% | 보통 |
| rle1994 | python | +1.5% | 보통 |
| rle1994 | node | -4.3% | 보통 |
| dict5000 | unjit | +1.4% | 보통 |
| dict5000 | compile | +5.9% | 보통 |
| dict5000 | build | -0.4% | 보통 |
| dict5000 | python | +3.4% | 보통 |
| dict5000 | node | +3.6% | 보통 |
| iter10000 | unjit | +0.4% | 보통 |
| iter10000 | compile | -2.3% | 보통 |
| iter10000 | build | -1.3% | 보통 |
| iter10000 | python | -0.7% | 보통 |
| iter10000 | node | -8.6% | 보통 |
| math10000 | unjit | -6.5% | 보통 |
| math10000 | compile | -5.6% | 보통 |
| math10000 | build | -6.7% | 보통 |
| math10000 | python | +3.3% | 보통 |
| math10000 | node | -2.5% | 보통 |
| class1000 | unjit | -1.5% | 보통 |
| class1000 | compile | -1.0% | 보통 |
| class1000 | build | -4.6% | 보통 |
| class1000 | python | +4.1% | 보통 |
| class1000 | node | -35.3% | 개선 |
| sieve30000 | unjit | -1.1% | 보통 |
| sieve30000 | compile | -2.2% | 보통 |
| sieve30000 | build | -6.5% | 보통 |
| sieve30000 | python | +4.8% | 보통 |
| sieve30000 | node | +4.4% | 보통 |
| matrix48 | unjit | -1.8% | 보통 |
| matrix48 | python | -3.4% | 보통 |
| matrix48 | node | +0.0% | 보통 |
| sin20000 | unjit | -0.7% | 보통 |
| sin20000 | compile | -1.0% | 보통 |
| sin20000 | build | -1.3% | 보통 |
| sin20000 | python | -1.4% | 보통 |
| sin20000 | node | +1.5% | 보통 |
| call200000 | unjit | -1.9% | 보통 |
| call200000 | compile | +11.2% | 회귀 |
| call200000 | build | -2.1% | 보통 |
| call200000 | python | -2.1% | 보통 |
| call200000 | node | -0.3% | 보통 |
| regex6000 | unjit | -17.3% | 개선 |
| regex6000 | python | +0.8% | 보통 |
| regex6000 | node | -24.2% | 개선 |
| str_unicode | unjit | -1.2% | 보통 |
| str_unicode | python | -3.6% | 보통 |
| str_unicode | node | -1.5% | 보통 |
| except10000 | unjit | +0.8% | 보통 |
| except10000 | python | -1.5% | 보통 |
| except10000 | node | +0.5% | 보통 |
| match20000 | unjit | -0.9% | 보통 |
| match20000 | python | -0.3% | 보통 |
| match20000 | node | -7.2% | 보통 |
| closure20000 | unjit | +1.4% | 보통 |
| closure20000 | python | +0.9% | 보통 |
| closure20000 | node | -0.4% | 보통 |
| inherit1000 | unjit | +1.3% | 보통 |
| inherit1000 | python | +2.5% | 보통 |
| inherit1000 | node | +42.9% | 회귀 |
| set5000 | unjit | -18.9% | 개선 |
| set5000 | python | -0.5% | 보통 |
| set5000 | node | +41.8% | 회귀 |
| enum10000 | unjit | -0.4% | 보통 |
| enum10000 | python | +4.0% | 보통 |
| enum10000 | node | +0.8% | 보통 |
| modimport2000 | unjit | -4.3% | 보통 |
| modimport2000 | python | +3.1% | 보통 |
| modimport2000 | node | -3.7% | 보통 |
| nanbox20000 | unjit | -1.6% | 보통 |
| nanbox20000 | python | +2.4% | 보통 |
| nanbox20000 | node | +3.1% | 보통 |
| dict_str5000 | unjit | -0.8% | 보통 |
| bytes_ops | unjit | -1.6% | 보통 |
| slice_ops | unjit | -1.9% | 보통 |

## 15. Historical Comparison

이전 결과(20261124-185012) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

