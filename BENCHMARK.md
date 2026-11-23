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

- 측정 일시: 2026-11-24 03:19:25 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|2421d1a
- 측정 커밋: 2421d1a
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **4.023x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **17.625x 느리다**.
- Un VM은 Python보다 기하평균 **2.951x 느리다**.
- Un VM은 Node.js보다 기하평균 **209.258x 느리다**.
- Python은 Node.js보다 기하평균 **70.905x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 4.023x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.057x  | >1 = Un이 빠름 |
| Un VM / Python | 0.339x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.005x  | >1 = Un이 빠름 |
| Python / Node.js | 0.014x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1251.2373 | 2.8741 | 2.8571 | 3.0083 | 2411.8869 | 9.8997 | 839.180 | 3.444 |
| fib24 | 1 | 173.2647 | 1.9340 | 152.1900 | 0.1838 | 3.1744 | 0.3004 | 1.641 | 0.155 |
| native_fib24 | 1 | 172.5317 | 1.9287 | 150.6071 | 0.1780 | 3.6574 | 0.2692 | 1.896 | 0.140 |
| nested_sum_200 | 40000 | 7.4434 | 0.0189 | 0.0114 | 0.0150 | 15.7218 | 0.0281 | 831.841 | 1.487 |
| qsort500 | 500 | 14.4069 | 0.8485 | 17.4286 | 0.5100 | 0.8707 | 0.0840 | 1.026 | 0.099 |
| mergesort1000 | 1000 | 40.5375 | 1.5267 | 43.0000 | 0.5421 | 2.1291 | 0.0667 | 1.395 | 0.044 |
| rle1994 | 1994 | 6.1875 | 5.9819 | N/A | N/A | 2.7481 | 0.0450 | 0.459 | 0.008 |
| dict5000 | 10000 | 3.1343 | 0.2618 | 3.1429 | 0.1787 | 5.5033 | 0.1410 | 21.021 | 0.539 |
| iter10000 | 30000 | 16.3208 | 16.5541 | 31.0000 | 0.2367 | 4.8252 | 0.2042 | 0.291 | 0.012 |
| math10000 | 10000 | 18.0577 | 0.0285 | 0.0257 | 0.0261 | 4.8239 | 0.0627 | 169.260 | 2.200 |
| class1000 | 1000 | 4.5789 | 0.5787 | 2.8957 | 0.0615 | 1.2779 | 0.0061 | 2.208 | 0.011 |
| sieve30000 | 30000 | 48.7906 | 2.5699 | 53.5714 | 0.3019 | 15.0854 | 0.2361 | 5.870 | 0.092 |
| matrix48 | 110592 | 60.6824 | 11.8834 | N/A | N/A | 22.5743 | 0.2061 | 1.900 | 0.017 |
| sin20000 | 20000 | 21.1375 | 0.5431 | 0.5571 | 0.5601 | 5.9224 | 0.1561 | 10.905 | 0.287 |
| call200000 | 200000 | 252.0975 | 0.6850 | 185.8014 | 0.7695 | 140.7264 | 3.8795 | 205.440 | 5.664 |
| regex6000 | 6000 | 1.4425 | 1.2031 | N/A | N/A | 2.7905 | 0.0842 | 2.319 | 0.070 |
| str_unicode | 2000 | 4.2260 | 4.2716 | N/A | N/A | 2.3457 | 0.1894 | 0.549 | 0.044 |
| except10000 | 10000 | 4.0500 | 0.0267 | N/A | N/A | 3.4574 | 0.0214 | 129.491 | 0.801 |
| match20000 | 20000 | 42.1440 | 45.7097 | N/A | N/A | 10.3839 | 0.0529 | 0.227 | 0.001 |
| closure20000 | 20000 | 24.9666 | 26.0000 | N/A | N/A | 7.4985 | 0.0285 | 0.288 | 0.001 |
| inherit1000 | 1000 | 2.6298 | 2.7593 | N/A | N/A | 0.9645 | 0.0122 | 0.350 | 0.004 |
| set5000 | 5000 | 7.1174 | 7.2776 | N/A | N/A | 1.7616 | 0.0334 | 0.242 | 0.005 |
| enum10000 | 10000 | 9.6514 | 9.3893 | N/A | N/A | 4.8348 | 0.0123 | 0.515 | 0.001 |
| modimport2000 | 2000 | 0.4424 | 0.0050 | N/A | N/A | 0.3314 | 0.0027 | 66.280 | 0.540 |
| nanbox20000 | 20000 | 15.3954 | 14.9834 | N/A | N/A | 7.6316 | 0.0161 | 0.509 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1251.2373 | 2.8741 | 2.8571 | 3.0083 | 2411.8869 | 9.8997 | 839.180 | 3.444 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 7.4434 | 0.0189 | 0.0114 | 0.0150 | 15.7218 | 0.0281 | 831.841 | 1.487 |
| sieve30000 | 30000 | 48.7906 | 2.5699 | 53.5714 | 0.3019 | 15.0854 | 0.2361 | 5.870 | 0.092 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 252.0975 | 0.6850 | 185.8014 | 0.7695 | 140.7264 | 3.8795 | 205.440 | 5.664 |
| fib24 | 1 | 173.2647 | 1.9340 | 152.1900 | 0.1838 | 3.1744 | 0.3004 | 1.641 | 0.155 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 3.1343 | 0.2618 | 3.1429 | 0.1787 | 5.5033 | 0.1410 | 21.021 | 0.539 |
| class1000 | 1000 | 4.5789 | 0.5787 | 2.8957 | 0.0615 | 1.2779 | 0.0061 | 2.208 | 0.011 |
| rle1994 | 1994 | 6.1875 | 5.9819 | N/A | N/A | 2.7481 | 0.0450 | 0.459 | 0.008 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 14.4069 | 0.8485 | 17.4286 | 0.5100 | 0.8707 | 0.0840 | 1.026 | 0.099 |
| mergesort1000 | 1000 | 40.5375 | 1.5267 | 43.0000 | 0.5421 | 2.1291 | 0.0667 | 1.395 | 0.044 |
| matrix48 | 110592 | 60.6824 | 11.8834 | N/A | N/A | 22.5743 | 0.2061 | 1.900 | 0.017 |
| math10000 | 10000 | 18.0577 | 0.0285 | 0.0257 | 0.0261 | 4.8239 | 0.0627 | 169.260 | 2.200 |
| sin20000 | 20000 | 21.1375 | 0.5431 | 0.5571 | 0.5601 | 5.9224 | 0.1561 | 10.905 | 0.287 |
| regex6000 | 6000 | 1.4425 | 1.2031 | N/A | N/A | 2.7905 | 0.0842 | 2.319 | 0.070 |
| iter10000 | 30000 | 16.3208 | 16.5541 | 31.0000 | 0.2367 | 4.8252 | 0.2042 | 0.291 | 0.012 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.621 | 0.000 |
| fib24 | 0.731 | 0.000 |
| native_fib24 | 0.637 | 0.000 |
| nested_sum_200 | 0.749 | 0.000 |
| qsort500 | 0.199 | 0.007 |
| mergesort1000 | 1.190 | 0.015 |
| rle1994 | 1.094 | 0.000 |
| dict5000 | 1.251 | 0.000 |
| iter10000 | 0.896 | 0.001 |
| math10000 | 0.457 | 0.000 |
| class1000 | 0.437 | 0.000 |
| sieve30000 | 1.779 | 0.000 |
| matrix48 | 1.159 | 0.000 |
| sin20000 | 0.896 | 0.000 |
| call200000 | 0.749 | 0.000 |
| regex6000 | 1.334 | 0.000 |
| str_unicode | 1.375 | 0.000 |
| except10000 | 0.729 | 0.000 |
| match20000 | 1.123 | 0.000 |
| closure20000 | 0.912 | 0.000 |
| inherit1000 | -0.107 | 0.000 |
| set5000 | 0.967 | 0.000 |
| enum10000 | 0.505 | 0.000 |
| modimport2000 | 0.155 | 0.000 |
| nanbox20000 | 0.874 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **4.023x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **17.625배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 839.180x, call200000은 205.440x(Int 함수의 native 직접 호출), nested_sum_200은 831.841x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.444x, call200000 5.664x, nested_sum_200 1.487x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261124-005758\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | +0.9% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | +0.6% | 보통 |
| int_ops | python | -0.1% | 보통 |
| int_ops | node | +2.7% | 보통 |
| fib24 | unjit | -0.4% | 보통 |
| fib24 | compile | -0.2% | 보통 |
| fib24 | build | +0.0% | 보통 |
| fib24 | python | -0.8% | 보통 |
| fib24 | node | +13.0% | 회귀 |
| native_fib24 | unjit | -0.2% | 보통 |
| native_fib24 | compile | -0.9% | 보통 |
| native_fib24 | build | -1.8% | 보통 |
| native_fib24 | python | +17.5% | 회귀 |
| native_fib24 | node | +0.9% | 보통 |
| nested_sum_200 | unjit | +2.7% | 보통 |
| nested_sum_200 | compile | -11.1% | 개선 |
| nested_sum_200 | build | +14.4% | 회귀 |
| nested_sum_200 | python | -4.0% | 보통 |
| nested_sum_200 | node | -40.3% | 개선 |
| qsort500 | unjit | -3.0% | 보통 |
| qsort500 | compile | -3.2% | 보통 |
| qsort500 | build | -6.0% | 보통 |
| qsort500 | python | +1.2% | 보통 |
| qsort500 | node | -4.8% | 보통 |
| mergesort1000 | unjit | +0.5% | 보통 |
| mergesort1000 | compile | +2.7% | 보통 |
| mergesort1000 | build | +5.2% | 보통 |
| mergesort1000 | python | -7.1% | 보통 |
| mergesort1000 | node | -23.8% | 개선 |
| rle1994 | unjit | -5.3% | 보통 |
| rle1994 | python | +19.2% | 회귀 |
| rle1994 | node | +27.5% | 회귀 |
| dict5000 | unjit | +0.2% | 보통 |
| dict5000 | compile | +0.0% | 보통 |
| dict5000 | build | -7.8% | 보통 |
| dict5000 | python | -2.5% | 보통 |
| dict5000 | node | -1.1% | 보통 |
| iter10000 | unjit | -10.7% | 개선 |
| iter10000 | compile | +0.9% | 보통 |
| iter10000 | build | -0.8% | 보통 |
| iter10000 | python | -6.8% | 보통 |
| iter10000 | node | +1.3% | 보통 |
| math10000 | unjit | -0.3% | 보통 |
| math10000 | compile | -10.0% | 보통 |
| math10000 | build | +4.0% | 보통 |
| math10000 | python | +0.4% | 보통 |
| math10000 | node | -0.2% | 보통 |
| class1000 | unjit | +0.3% | 보통 |
| class1000 | compile | -3.3% | 보통 |
| class1000 | build | -1.3% | 보통 |
| class1000 | python | -1.7% | 보통 |
| class1000 | node | +8.9% | 보통 |
| sieve30000 | unjit | -5.4% | 보통 |
| sieve30000 | compile | -1.1% | 보통 |
| sieve30000 | build | +12.3% | 회귀 |
| sieve30000 | python | +0.9% | 보통 |
| sieve30000 | node | +15.9% | 회귀 |
| matrix48 | unjit | +4.9% | 보통 |
| matrix48 | python | +4.1% | 보통 |
| matrix48 | node | -1.7% | 보통 |
| sin20000 | unjit | +0.7% | 보통 |
| sin20000 | compile | -0.5% | 보통 |
| sin20000 | build | +1.3% | 보통 |
| sin20000 | python | +4.3% | 보통 |
| sin20000 | node | +1.1% | 보통 |
| call200000 | unjit | +0.0% | 보통 |
| call200000 | compile | -3.1% | 보통 |
| call200000 | build | -1.0% | 보통 |
| call200000 | python | -1.9% | 보통 |
| call200000 | node | +1.5% | 보통 |
| regex6000 | unjit | -1.8% | 보통 |
| regex6000 | python | +3.4% | 보통 |
| regex6000 | node | -3.6% | 보통 |
| str_unicode | unjit | -2.9% | 보통 |
| str_unicode | python | +2.4% | 보통 |
| str_unicode | node | -50.5% | 개선 |
| except10000 | unjit | -0.7% | 보통 |
| except10000 | python | +4.2% | 보통 |
| except10000 | node | +0.5% | 보통 |
| match20000 | unjit | +0.2% | 보통 |
| match20000 | python | +4.0% | 보통 |
| match20000 | node | +2.5% | 보통 |
| closure20000 | unjit | +2.8% | 보통 |
| closure20000 | python | +2.4% | 보통 |
| closure20000 | node | +1.4% | 보통 |
| inherit1000 | unjit | -1.2% | 보통 |
| inherit1000 | python | -6.2% | 보통 |
| inherit1000 | node | -2.4% | 보통 |
| set5000 | unjit | -1.7% | 보통 |
| set5000 | python | -4.7% | 보통 |
| set5000 | node | -12.6% | 개선 |
| enum10000 | unjit | +6.0% | 보통 |
| enum10000 | python | -4.1% | 보통 |
| enum10000 | node | -5.4% | 보통 |
| modimport2000 | unjit | +0.0% | 보통 |
| modimport2000 | python | -0.4% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | +2.1% | 보통 |
| nanbox20000 | python | +0.0% | 보통 |
| nanbox20000 | node | -1.8% | 보통 |

## 15. Historical Comparison

이전 결과(20261124-005758) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

