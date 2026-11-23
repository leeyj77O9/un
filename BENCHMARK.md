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

- 측정 일시: 2026-11-24 01:04:53 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|93ed136
- 측정 커밋: 93ed136
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **3.970x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **16.945x 느리다**.
- Un VM은 Python보다 기하평균 **2.950x 느리다**.
- Un VM은 Node.js보다 기하평균 **198.424x 느리다**.
- Python은 Node.js보다 기하평균 **67.265x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 3.970x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.059x  | >1 = Un이 빠름 |
| Un VM / Python | 0.339x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.005x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1249.8877 | 2.8495 | 2.8571 | 2.9894 | 2413.6170 | 9.6407 | 847.032 | 3.383 |
| fib24 | 1 | 172.2573 | 1.9409 | 152.5300 | 0.1837 | 3.1984 | 0.2658 | 1.648 | 0.137 |
| native_fib24 | 1 | 169.2604 | 1.9326 | 151.9714 | 0.1812 | 3.1130 | 0.2668 | 1.611 | 0.138 |
| nested_sum_200 | 40000 | 7.6457 | 0.0184 | 0.0129 | 0.0131 | 16.3702 | 0.0471 | 889.685 | 2.560 |
| qsort500 | 500 | 14.9440 | 0.8746 | 18.0000 | 0.5423 | 0.8601 | 0.0882 | 0.983 | 0.101 |
| mergesort1000 | 1000 | 40.1932 | 1.5190 | 41.8571 | 0.5156 | 2.2925 | 0.0875 | 1.509 | 0.058 |
| rle1994 | 1994 | 6.3497 | 6.3185 | N/A | N/A | 2.3048 | 0.0353 | 0.365 | 0.006 |
| dict5000 | 10000 | 2.9006 | 0.2612 | 3.1429 | 0.1939 | 5.6446 | 0.1425 | 21.610 | 0.546 |
| iter10000 | 30000 | 16.3053 | 18.5471 | 30.7143 | 0.2387 | 5.1755 | 0.2015 | 0.279 | 0.011 |
| math10000 | 10000 | 18.5837 | 0.0286 | 0.0286 | 0.0250 | 4.8059 | 0.0628 | 168.038 | 2.196 |
| class1000 | 1000 | 4.7492 | 0.5768 | 2.9957 | 0.0624 | 1.2995 | 0.0056 | 2.253 | 0.010 |
| sieve30000 | 30000 | 49.0251 | 2.7173 | 54.1429 | 0.2687 | 14.9455 | 0.2037 | 5.500 | 0.075 |
| matrix48 | 110592 | 59.2776 | 11.3313 | N/A | N/A | 21.6790 | 0.2096 | 1.913 | 0.018 |
| sin20000 | 20000 | 20.8955 | 0.5393 | 0.5600 | 0.5531 | 5.6757 | 0.1544 | 10.524 | 0.286 |
| call200000 | 200000 | 254.3439 | 0.6848 | 191.8257 | 0.7771 | 143.4419 | 3.8225 | 209.465 | 5.582 |
| regex6000 | 6000 | 1.2012 | 1.2257 | N/A | N/A | 2.6994 | 0.0873 | 2.202 | 0.071 |
| str_unicode | 2000 | 4.4109 | 4.3997 | N/A | N/A | 2.2906 | 0.3830 | 0.521 | 0.087 |
| except10000 | 10000 | 3.8961 | 0.0269 | N/A | N/A | 3.3188 | 0.0213 | 123.375 | 0.792 |
| match20000 | 20000 | 43.4304 | 45.6002 | N/A | N/A | 9.9839 | 0.0516 | 0.219 | 0.001 |
| closure20000 | 20000 | 24.9129 | 25.2940 | N/A | N/A | 7.3247 | 0.0281 | 0.290 | 0.001 |
| inherit1000 | 1000 | 2.7982 | 2.7918 | N/A | N/A | 1.0280 | 0.0125 | 0.368 | 0.004 |
| set5000 | 5000 | 7.1210 | 7.4025 | N/A | N/A | 1.8476 | 0.0382 | 0.250 | 0.005 |
| enum10000 | 10000 | 8.6896 | 8.8604 | N/A | N/A | 5.0403 | 0.0130 | 0.569 | 0.001 |
| modimport2000 | 2000 | 0.4289 | 0.0050 | N/A | N/A | 0.3326 | 0.0027 | 66.520 | 0.540 |
| nanbox20000 | 20000 | 15.3513 | 14.6747 | N/A | N/A | 7.6319 | 0.0164 | 0.520 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1249.8877 | 2.8495 | 2.8571 | 2.9894 | 2413.6170 | 9.6407 | 847.032 | 3.383 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 7.6457 | 0.0184 | 0.0129 | 0.0131 | 16.3702 | 0.0471 | 889.685 | 2.560 |
| sieve30000 | 30000 | 49.0251 | 2.7173 | 54.1429 | 0.2687 | 14.9455 | 0.2037 | 5.500 | 0.075 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 254.3439 | 0.6848 | 191.8257 | 0.7771 | 143.4419 | 3.8225 | 209.465 | 5.582 |
| fib24 | 1 | 172.2573 | 1.9409 | 152.5300 | 0.1837 | 3.1984 | 0.2658 | 1.648 | 0.137 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 2.9006 | 0.2612 | 3.1429 | 0.1939 | 5.6446 | 0.1425 | 21.610 | 0.546 |
| class1000 | 1000 | 4.7492 | 0.5768 | 2.9957 | 0.0624 | 1.2995 | 0.0056 | 2.253 | 0.010 |
| rle1994 | 1994 | 6.3497 | 6.3185 | N/A | N/A | 2.3048 | 0.0353 | 0.365 | 0.006 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 14.9440 | 0.8746 | 18.0000 | 0.5423 | 0.8601 | 0.0882 | 0.983 | 0.101 |
| mergesort1000 | 1000 | 40.1932 | 1.5190 | 41.8571 | 0.5156 | 2.2925 | 0.0875 | 1.509 | 0.058 |
| matrix48 | 110592 | 59.2776 | 11.3313 | N/A | N/A | 21.6790 | 0.2096 | 1.913 | 0.018 |
| math10000 | 10000 | 18.5837 | 0.0286 | 0.0286 | 0.0250 | 4.8059 | 0.0628 | 168.038 | 2.196 |
| sin20000 | 20000 | 20.8955 | 0.5393 | 0.5600 | 0.5531 | 5.6757 | 0.1544 | 10.524 | 0.286 |
| regex6000 | 6000 | 1.2012 | 1.2257 | N/A | N/A | 2.6994 | 0.0873 | 2.202 | 0.071 |
| iter10000 | 30000 | 16.3053 | 18.5471 | 30.7143 | 0.2387 | 5.1755 | 0.2015 | 0.279 | 0.011 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.728 | 0.000 |
| fib24 | 0.875 | 0.000 |
| native_fib24 | 0.626 | 0.000 |
| nested_sum_200 | 0.752 | 0.000 |
| qsort500 | 0.170 | 0.007 |
| mergesort1000 | 1.259 | 0.015 |
| rle1994 | 0.507 | 0.000 |
| dict5000 | 0.823 | 0.000 |
| iter10000 | 0.678 | 0.001 |
| math10000 | 0.887 | 0.000 |
| class1000 | 0.004 | 0.000 |
| sieve30000 | 1.919 | 0.000 |
| matrix48 | 1.150 | 0.000 |
| sin20000 | 0.896 | 0.000 |
| call200000 | 0.903 | 0.000 |
| regex6000 | 2.419 | 0.000 |
| str_unicode | 1.395 | 0.000 |
| except10000 | 0.471 | 0.000 |
| match20000 | 1.137 | 0.000 |
| closure20000 | 0.740 | 0.000 |
| inherit1000 | -0.134 | 0.000 |
| set5000 | 0.580 | 0.000 |
| enum10000 | 0.272 | 0.000 |
| modimport2000 | 0.342 | 0.000 |
| nanbox20000 | 0.738 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **3.970x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **16.945배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 847.032x, call200000은 209.465x(Int 함수의 native 직접 호출), nested_sum_200은 889.685x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.383x, call200000 5.582x, nested_sum_200 2.560x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261124-004015\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | +0.4% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | +4.3% | 보통 |
| int_ops | python | +1.5% | 보통 |
| int_ops | node | +0.7% | 보통 |
| fib24 | unjit | +0.5% | 보통 |
| fib24 | compile | -0.5% | 보통 |
| fib24 | build | +2.9% | 보통 |
| fib24 | python | +1.4% | 보통 |
| fib24 | node | -0.3% | 보통 |
| native_fib24 | unjit | -0.1% | 보통 |
| native_fib24 | compile | -0.4% | 보통 |
| native_fib24 | build | +0.9% | 보통 |
| native_fib24 | python | -2.2% | 보통 |
| native_fib24 | node | -0.2% | 보통 |
| nested_sum_200 | unjit | -1.6% | 보통 |
| nested_sum_200 | compile | +12.5% | 회귀 |
| nested_sum_200 | build | -0.1% | 보통 |
| nested_sum_200 | python | -0.1% | 보통 |
| nested_sum_200 | node | +103.9% | 회귀 |
| qsort500 | unjit | -1.1% | 보통 |
| qsort500 | compile | -3.8% | 보통 |
| qsort500 | build | +3.2% | 보통 |
| qsort500 | python | -0.2% | 보통 |
| qsort500 | node | -3.9% | 보통 |
| mergesort1000 | unjit | +1.0% | 보통 |
| mergesort1000 | compile | -6.4% | 보통 |
| mergesort1000 | build | -1.6% | 보통 |
| mergesort1000 | python | +2.2% | 보통 |
| mergesort1000 | node | -20.4% | 개선 |
| rle1994 | unjit | -4.1% | 보통 |
| rle1994 | python | -0.1% | 보통 |
| rle1994 | node | -28.0% | 개선 |
| dict5000 | unjit | +0.2% | 보통 |
| dict5000 | compile | -4.3% | 보통 |
| dict5000 | build | -0.9% | 보통 |
| dict5000 | python | +2.5% | 보통 |
| dict5000 | node | -4.5% | 보통 |
| iter10000 | unjit | +10.3% | 회귀 |
| iter10000 | compile | -0.9% | 보통 |
| iter10000 | build | +3.0% | 보통 |
| iter10000 | python | +4.2% | 보통 |
| iter10000 | node | -11.2% | 개선 |
| math10000 | unjit | +5.1% | 보통 |
| math10000 | compile | +17.6% | 회귀 |
| math10000 | build | +0.5% | 보통 |
| math10000 | python | +8.7% | 보통 |
| math10000 | node | +0.2% | 보통 |
| class1000 | unjit | +1.8% | 보통 |
| class1000 | compile | +3.5% | 보통 |
| class1000 | build | -1.1% | 보통 |
| class1000 | python | +0.9% | 보통 |
| class1000 | node | -17.6% | 개선 |
| sieve30000 | unjit | +9.7% | 보통 |
| sieve30000 | compile | -3.6% | 보통 |
| sieve30000 | build | +1.0% | 보통 |
| sieve30000 | python | +1.4% | 보통 |
| sieve30000 | node | -6.2% | 보통 |
| matrix48 | unjit | -0.1% | 보통 |
| matrix48 | python | +0.3% | 보통 |
| matrix48 | node | +6.7% | 보통 |
| sin20000 | unjit | -0.1% | 보통 |
| sin20000 | compile | +2.6% | 보통 |
| sin20000 | build | +1.7% | 보통 |
| sin20000 | python | -0.2% | 보통 |
| sin20000 | node | -3.4% | 보통 |
| call200000 | unjit | -0.1% | 보통 |
| call200000 | compile | -1.1% | 보통 |
| call200000 | build | +1.5% | 보통 |
| call200000 | python | +0.7% | 보통 |
| call200000 | node | +0.0% | 보통 |
| regex6000 | unjit | -10.7% | 개선 |
| regex6000 | python | +0.5% | 보통 |
| regex6000 | node | -1.8% | 보통 |
| str_unicode | unjit | -18.6% | 개선 |
| str_unicode | python | -7.8% | 보통 |
| str_unicode | node | +104.5% | 회귀 |
| except10000 | unjit | +0.0% | 보통 |
| except10000 | python | +0.9% | 보통 |
| except10000 | node | +0.0% | 보통 |
| match20000 | unjit | +0.1% | 보통 |
| match20000 | python | -4.9% | 보통 |
| match20000 | node | +6.4% | 보통 |
| closure20000 | unjit | -11.6% | 개선 |
| closure20000 | python | -6.6% | 보통 |
| closure20000 | node | -1.4% | 보통 |
| inherit1000 | unjit | -1.4% | 보통 |
| inherit1000 | python | +8.6% | 보통 |
| inherit1000 | node | -0.8% | 보통 |
| set5000 | unjit | +2.4% | 보통 |
| set5000 | python | +1.0% | 보통 |
| set5000 | node | +7.9% | 보통 |
| enum10000 | unjit | -11.5% | 개선 |
| enum10000 | python | +8.8% | 보통 |
| enum10000 | node | +4.8% | 보통 |
| modimport2000 | unjit | +0.0% | 보통 |
| modimport2000 | python | +1.8% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | -4.6% | 보통 |
| nanbox20000 | python | -4.7% | 보통 |
| nanbox20000 | node | +1.2% | 보통 |

## 15. Historical Comparison

이전 결과(20261124-004015) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

