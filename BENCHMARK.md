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

- 측정 일시: 2026-11-23 18:21:35 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|957873d
- 측정 커밋: 957873d
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **3.789x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **17.172x 느리다**.
- Un VM은 Python보다 기하평균 **3.395x 느리다**.
- Un VM은 Node.js보다 기하평균 **220.895x 느리다**.
- Python은 Node.js보다 기하평균 **65.065x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 3.789x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.058x  | >1 = Un이 빠름 |
| Un VM / Python | 0.295x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.005x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1677.0732 | 2.8486 | 2.8571 | 2.8717 | 2367.1368 | 9.6774 | 830.983 | 3.397 |
| fib24 | 1 | 197.1246 | 1.9288 | 168.0800 | 0.1789 | 3.1713 | 0.2675 | 1.644 | 0.139 |
| native_fib24 | 1 | 189.5884 | 1.9319 | 166.0900 | 0.1805 | 3.2762 | 0.2710 | 1.696 | 0.140 |
| nested_sum_200 | 40000 | 9.2092 | 0.0187 | 0.0129 | 0.0133 | 15.9979 | 0.0473 | 855.503 | 2.529 |
| qsort500 | 500 | 15.8760 | 0.8513 | 20.5714 | 0.5350 | 0.8836 | 0.0988 | 1.038 | 0.116 |
| mergesort1000 | 1000 | 44.6058 | 1.4971 | 46.4286 | 0.5391 | 2.1907 | 0.0883 | 1.463 | 0.059 |
| rle1994 | 1994 | 7.2040 | 7.2095 | N/A | N/A | 2.4480 | 0.0581 | 0.340 | 0.008 |
| dict5000 | 10000 | 3.7241 | 0.2606 | 3.7143 | 0.1880 | 5.2428 | 0.1481 | 20.118 | 0.568 |
| iter10000 | 30000 | 17.0961 | 21.1122 | 36.0000 | 0.2254 | 4.7258 | 0.1540 | 0.224 | 0.007 |
| math10000 | 10000 | 18.8252 | 0.0273 | 0.0243 | 0.0257 | 4.5248 | 0.0628 | 165.744 | 2.300 |
| class1000 | 1000 | 4.6089 | 0.5983 | 3.1486 | 0.0619 | 1.2952 | 0.0112 | 2.165 | 0.019 |
| sieve30000 | 30000 | 58.2041 | 2.4553 | 62.8571 | 0.2674 | 14.3336 | 0.2570 | 5.838 | 0.105 |
| matrix48 | 110592 | 72.7242 | 11.3456 | N/A | N/A | 21.6083 | 0.1806 | 1.905 | 0.016 |
| sin20000 | 20000 | 22.5234 | 0.5394 | 0.5457 | 0.5402 | 5.6448 | 0.1554 | 10.465 | 0.288 |
| call200000 | 200000 | 275.7703 | 0.6858 | 200.9157 | 0.7604 | 142.5200 | 4.0115 | 207.816 | 5.849 |
| regex6000 | 6000 | 1.2074 | 1.2532 | N/A | N/A | 2.5665 | 0.0796 | 2.048 | 0.064 |
| str_unicode | 2000 | 4.6128 | 4.5467 | N/A | N/A | 2.5217 | 0.1868 | 0.555 | 0.041 |
| except10000 | 10000 | 5.3800 | 0.0270 | N/A | N/A | 3.2560 | 0.0214 | 120.593 | 0.793 |
| match20000 | 20000 | 49.1684 | 48.7489 | N/A | N/A | 9.8884 | 0.0470 | 0.203 | 0.001 |
| closure20000 | 20000 | 26.5753 | 27.0897 | N/A | N/A | 7.6227 | 0.0285 | 0.281 | 0.001 |
| inherit1000 | 1000 | 2.8092 | 2.9057 | N/A | N/A | 0.9773 | 0.0171 | 0.336 | 0.006 |
| set5000 | 5000 | 7.4802 | 7.5744 | N/A | N/A | 1.7765 | 0.0332 | 0.235 | 0.004 |
| enum10000 | 10000 | 11.0627 | 11.2944 | N/A | N/A | 4.7630 | 0.0123 | 0.422 | 0.001 |
| modimport2000 | 2000 | 0.5858 | 0.0052 | N/A | N/A | 0.3393 | 0.0027 | 65.250 | 0.519 |
| nanbox20000 | 20000 | 18.2667 | 17.3335 | N/A | N/A | 7.4248 | 0.0165 | 0.428 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1677.0732 | 2.8486 | 2.8571 | 2.8717 | 2367.1368 | 9.6774 | 830.983 | 3.397 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 9.2092 | 0.0187 | 0.0129 | 0.0133 | 15.9979 | 0.0473 | 855.503 | 2.529 |
| sieve30000 | 30000 | 58.2041 | 2.4553 | 62.8571 | 0.2674 | 14.3336 | 0.2570 | 5.838 | 0.105 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 275.7703 | 0.6858 | 200.9157 | 0.7604 | 142.5200 | 4.0115 | 207.816 | 5.849 |
| fib24 | 1 | 197.1246 | 1.9288 | 168.0800 | 0.1789 | 3.1713 | 0.2675 | 1.644 | 0.139 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 3.7241 | 0.2606 | 3.7143 | 0.1880 | 5.2428 | 0.1481 | 20.118 | 0.568 |
| class1000 | 1000 | 4.6089 | 0.5983 | 3.1486 | 0.0619 | 1.2952 | 0.0112 | 2.165 | 0.019 |
| rle1994 | 1994 | 7.2040 | 7.2095 | N/A | N/A | 2.4480 | 0.0581 | 0.340 | 0.008 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 15.8760 | 0.8513 | 20.5714 | 0.5350 | 0.8836 | 0.0988 | 1.038 | 0.116 |
| mergesort1000 | 1000 | 44.6058 | 1.4971 | 46.4286 | 0.5391 | 2.1907 | 0.0883 | 1.463 | 0.059 |
| matrix48 | 110592 | 72.7242 | 11.3456 | N/A | N/A | 21.6083 | 0.1806 | 1.905 | 0.016 |
| math10000 | 10000 | 18.8252 | 0.0273 | 0.0243 | 0.0257 | 4.5248 | 0.0628 | 165.744 | 2.300 |
| sin20000 | 20000 | 22.5234 | 0.5394 | 0.5457 | 0.5402 | 5.6448 | 0.1554 | 10.465 | 0.288 |
| regex6000 | 6000 | 1.2074 | 1.2532 | N/A | N/A | 2.5665 | 0.0796 | 2.048 | 0.064 |
| iter10000 | 30000 | 17.0961 | 21.1122 | 36.0000 | 0.2254 | 4.7258 | 0.1540 | 0.224 | 0.007 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.727 | 0.000 |
| fib24 | 0.876 | 0.000 |
| native_fib24 | 0.876 | 0.000 |
| nested_sum_200 | 0.643 | 0.000 |
| qsort500 | 0.134 | 0.007 |
| mergesort1000 | 0.304 | 0.015 |
| rle1994 | 1.079 | 0.000 |
| dict5000 | 1.148 | 0.000 |
| iter10000 | 0.355 | 0.001 |
| math10000 | 0.743 | 0.000 |
| class1000 | 0.292 | 0.000 |
| sieve30000 | 1.747 | 0.000 |
| matrix48 | 1.194 | 0.000 |
| sin20000 | 0.896 | 0.000 |
| call200000 | 0.748 | 0.000 |
| regex6000 | 1.386 | 0.000 |
| str_unicode | 1.351 | 0.000 |
| except10000 | 0.748 | 0.000 |
| match20000 | 0.631 | 0.000 |
| closure20000 | 0.749 | 0.000 |
| inherit1000 | -0.272 | 0.000 |
| set5000 | 0.520 | 0.000 |
| enum10000 | 0.869 | 0.000 |
| modimport2000 | 0.356 | 0.000 |
| nanbox20000 | 0.731 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **3.789x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **17.172배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 830.983x, call200000은 207.816x(Int 함수의 native 직접 호출), nested_sum_200은 855.503x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.397x, call200000 5.849x, nested_sum_200 2.529x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261123-172647\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | -0.1% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | -3.5% | 보통 |
| int_ops | python | -1.3% | 보통 |
| int_ops | node | +0.9% | 보통 |
| fib24 | unjit | -0.5% | 보통 |
| fib24 | compile | -2.9% | 보통 |
| fib24 | build | -1.7% | 보통 |
| fib24 | python | -4.0% | 보통 |
| fib24 | node | -0.3% | 보통 |
| native_fib24 | unjit | -4.1% | 보통 |
| native_fib24 | compile | -3.7% | 보통 |
| native_fib24 | build | +0.3% | 보통 |
| native_fib24 | python | -1.2% | 보통 |
| native_fib24 | node | +1.9% | 보통 |
| nested_sum_200 | unjit | -28.4% | 개선 |
| nested_sum_200 | compile | +12.5% | 회귀 |
| nested_sum_200 | build | +8.1% | 보통 |
| nested_sum_200 | python | -3.3% | 보통 |
| nested_sum_200 | node | -5.8% | 보통 |
| qsort500 | unjit | -0.6% | 보통 |
| qsort500 | compile | -2.7% | 보통 |
| qsort500 | build | +1.2% | 보통 |
| qsort500 | python | +3.2% | 보통 |
| qsort500 | node | -7.4% | 보통 |
| mergesort1000 | unjit | -2.5% | 보통 |
| mergesort1000 | compile | -3.3% | 보통 |
| mergesort1000 | build | +0.1% | 보통 |
| mergesort1000 | python | +1.7% | 보통 |
| mergesort1000 | node | +5.0% | 보통 |
| rle1994 | unjit | -2.6% | 보통 |
| rle1994 | python | +2.1% | 보통 |
| rle1994 | node | -5.1% | 보통 |
| dict5000 | unjit | -0.9% | 보통 |
| dict5000 | compile | +0.0% | 보통 |
| dict5000 | build | -9.6% | 보통 |
| dict5000 | python | -2.1% | 보통 |
| dict5000 | node | -6.3% | 보통 |
| iter10000 | unjit | +14.4% | 회귀 |
| iter10000 | compile | -4.2% | 보통 |
| iter10000 | build | -1.8% | 보통 |
| iter10000 | python | -3.3% | 보통 |
| iter10000 | node | -26.5% | 개선 |
| math10000 | unjit | +2.2% | 보통 |
| math10000 | compile | -15.0% | 개선 |
| math10000 | build | +0.1% | 보통 |
| math10000 | python | +1.6% | 보통 |
| math10000 | node | +0.5% | 보통 |
| class1000 | unjit | -0.3% | 보통 |
| class1000 | compile | -4.9% | 보통 |
| class1000 | build | +9.5% | 보통 |
| class1000 | python | +0.4% | 보통 |
| class1000 | node | +55.6% | 회귀 |
| sieve30000 | unjit | -2.6% | 보통 |
| sieve30000 | compile | -3.3% | 보통 |
| sieve30000 | build | -13.5% | 개선 |
| sieve30000 | python | -1.5% | 보통 |
| sieve30000 | node | +10.3% | 회귀 |
| matrix48 | unjit | -1.3% | 보통 |
| matrix48 | python | -2.9% | 보통 |
| matrix48 | node | -10.2% | 개선 |
| sin20000 | unjit | -0.3% | 보통 |
| sin20000 | compile | -1.0% | 보통 |
| sin20000 | build | -1.7% | 보통 |
| sin20000 | python | -1.9% | 보통 |
| sin20000 | node | +0.1% | 보통 |
| call200000 | unjit | -0.2% | 보통 |
| call200000 | compile | -6.9% | 보통 |
| call200000 | build | -0.9% | 보통 |
| call200000 | python | -1.0% | 보통 |
| call200000 | node | +4.8% | 보통 |
| regex6000 | unjit | -15.3% | 개선 |
| regex6000 | python | -3.1% | 보통 |
| regex6000 | node | -19.0% | 개선 |
| str_unicode | unjit | -3.4% | 보통 |
| str_unicode | python | +5.4% | 보통 |
| str_unicode | node | +4.2% | 보통 |
| except10000 | unjit | +0.0% | 보통 |
| except10000 | python | -1.0% | 보통 |
| except10000 | node | +0.0% | 보통 |
| match20000 | unjit | -7.3% | 보통 |
| match20000 | python | -5.9% | 보통 |
| match20000 | node | -26.3% | 개선 |
| closure20000 | unjit | -10.0% | 보통 |
| closure20000 | python | -22.8% | 개선 |
| closure20000 | node | +0.0% | 보통 |
| inherit1000 | unjit | -9.1% | 보통 |
| inherit1000 | python | +3.6% | 보통 |
| inherit1000 | node | -12.8% | 개선 |
| set5000 | unjit | -0.6% | 보통 |
| set5000 | python | -1.6% | 보통 |
| set5000 | node | -11.5% | 개선 |
| enum10000 | unjit | +1.9% | 보통 |
| enum10000 | python | -0.8% | 보통 |
| enum10000 | node | -23.6% | 개선 |
| modimport2000 | unjit | +2.0% | 보통 |
| modimport2000 | python | -3.6% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | -1.1% | 보통 |
| nanbox20000 | python | +0.3% | 보통 |
| nanbox20000 | node | +0.6% | 보통 |

## 15. Historical Comparison

이전 결과(20261123-172647) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

