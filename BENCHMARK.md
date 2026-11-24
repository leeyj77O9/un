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

- 측정 일시: 2026-11-24 11:25:00 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|8ae1aac
- 측정 커밋: 8ae1aac
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **4.334x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **15.694x 느리다**.
- Un VM은 Python보다 기하평균 **2.400x 느리다**.
- Un VM은 Node.js보다 기하평균 **163.233x 느리다**.
- Python은 Node.js보다 기하평균 **68.015x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 4.334x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.064x  | >1 = Un이 빠름 |
| Un VM / Python | 0.417x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.006x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 998.4435 | 2.8520 | 2.8571 | 2.8687 | 2335.7361 | 9.5822 | 818.982 | 3.360 |
| fib24 | 1 | 133.3920 | 2.0162 | 126.3371 | 0.2026 | 3.1358 | 0.2682 | 1.555 | 0.133 |
| native_fib24 | 1 | 139.8901 | 2.0063 | 126.4700 | 0.2099 | 3.1252 | 0.2694 | 1.558 | 0.134 |
| nested_sum_200 | 40000 | 6.7730 | 0.0181 | 0.0114 | 0.0114 | 15.8799 | 0.0480 | 877.343 | 2.652 |
| qsort500 | 500 | 11.9289 | 0.8521 | 14.1429 | 0.5410 | 0.8657 | 0.0957 | 1.016 | 0.112 |
| mergesort1000 | 1000 | 29.7489 | 1.5370 | 31.7143 | 0.5010 | 2.2077 | 0.0851 | 1.436 | 0.055 |
| rle1994 | 1994 | 5.1388 | 5.0995 | N/A | N/A | 2.2833 | 0.0488 | 0.448 | 0.010 |
| dict5000 | 10000 | 2.5513 | 0.2623 | 2.5714 | 0.1911 | 5.3325 | 0.1413 | 20.330 | 0.539 |
| iter10000 | 30000 | 13.7859 | 14.6122 | 24.7143 | 0.2343 | 4.6546 | 0.1635 | 0.319 | 0.011 |
| math10000 | 10000 | 13.6256 | 0.0280 | 0.0257 | 0.0253 | 4.4237 | 0.0629 | 157.989 | 2.246 |
| class1000 | 1000 | 2.6167 | 0.5876 | 1.8714 | 0.0599 | 1.3434 | 0.0058 | 2.286 | 0.010 |
| sieve30000 | 30000 | 37.7282 | 2.4852 | 42.5714 | 0.2726 | 14.1422 | 0.2238 | 5.691 | 0.090 |
| matrix48 | 110592 | 44.1983 | 11.3520 | N/A | N/A | 21.7510 | 0.2424 | 1.916 | 0.021 |
| sin20000 | 20000 | 15.6876 | 0.5382 | 0.5443 | 0.5388 | 5.6167 | 0.1531 | 10.436 | 0.284 |
| call200000 | 200000 | 197.2872 | 0.6850 | 147.0286 | 0.7477 | 137.9718 | 3.8122 | 201.419 | 5.565 |
| regex6000 | 6000 | 1.4947 | 1.2117 | N/A | N/A | 2.6779 | 0.0719 | 2.210 | 0.059 |
| str_unicode | 2000 | 3.8892 | 3.5441 | N/A | N/A | 2.2994 | 0.1884 | 0.649 | 0.053 |
| except10000 | 10000 | 3.4310 | 0.0263 | N/A | N/A | 3.2781 | 0.0214 | 124.643 | 0.814 |
| match20000 | 20000 | 27.7628 | 29.9257 | N/A | N/A | 9.8129 | 0.0457 | 0.328 | 0.002 |
| closure20000 | 20000 | 19.3623 | 19.6549 | N/A | N/A | 7.3993 | 0.0284 | 0.376 | 0.001 |
| inherit1000 | 1000 | 2.2084 | 1.8729 | N/A | N/A | 0.9702 | 0.0096 | 0.518 | 0.005 |
| set5000 | 5000 | 4.5494 | 4.5695 | N/A | N/A | 1.7606 | 0.0344 | 0.385 | 0.008 |
| enum10000 | 10000 | 7.0241 | 6.9803 | N/A | N/A | 4.7447 | 0.0124 | 0.680 | 0.002 |
| modimport2000 | 2000 | 0.3768 | 0.0044 | N/A | N/A | 0.3250 | 0.0027 | 73.864 | 0.614 |
| nanbox20000 | 20000 | 11.8377 | 11.7714 | N/A | N/A | 7.2962 | 0.0164 | 0.620 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 998.4435 | 2.8520 | 2.8571 | 2.8687 | 2335.7361 | 9.5822 | 818.982 | 3.360 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 6.7730 | 0.0181 | 0.0114 | 0.0114 | 15.8799 | 0.0480 | 877.343 | 2.652 |
| sieve30000 | 30000 | 37.7282 | 2.4852 | 42.5714 | 0.2726 | 14.1422 | 0.2238 | 5.691 | 0.090 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 197.2872 | 0.6850 | 147.0286 | 0.7477 | 137.9718 | 3.8122 | 201.419 | 5.565 |
| fib24 | 1 | 133.3920 | 2.0162 | 126.3371 | 0.2026 | 3.1358 | 0.2682 | 1.555 | 0.133 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 2.5513 | 0.2623 | 2.5714 | 0.1911 | 5.3325 | 0.1413 | 20.330 | 0.539 |
| class1000 | 1000 | 2.6167 | 0.5876 | 1.8714 | 0.0599 | 1.3434 | 0.0058 | 2.286 | 0.010 |
| rle1994 | 1994 | 5.1388 | 5.0995 | N/A | N/A | 2.2833 | 0.0488 | 0.448 | 0.010 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 11.9289 | 0.8521 | 14.1429 | 0.5410 | 0.8657 | 0.0957 | 1.016 | 0.112 |
| mergesort1000 | 1000 | 29.7489 | 1.5370 | 31.7143 | 0.5010 | 2.2077 | 0.0851 | 1.436 | 0.055 |
| matrix48 | 110592 | 44.1983 | 11.3520 | N/A | N/A | 21.7510 | 0.2424 | 1.916 | 0.021 |
| math10000 | 10000 | 13.6256 | 0.0280 | 0.0257 | 0.0253 | 4.4237 | 0.0629 | 157.989 | 2.246 |
| sin20000 | 20000 | 15.6876 | 0.5382 | 0.5443 | 0.5388 | 5.6167 | 0.1531 | 10.436 | 0.284 |
| regex6000 | 6000 | 1.4947 | 1.2117 | N/A | N/A | 2.6779 | 0.0719 | 2.210 | 0.059 |
| iter10000 | 30000 | 13.7859 | 14.6122 | 24.7143 | 0.2343 | 4.6546 | 0.1635 | 0.319 | 0.011 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.621 | 0.000 |
| fib24 | 0.626 | 0.000 |
| native_fib24 | 0.876 | 0.000 |
| nested_sum_200 | 0.893 | 0.000 |
| qsort500 | 0.135 | 0.007 |
| mergesort1000 | 0.267 | 0.015 |
| rle1994 | 1.082 | 0.000 |
| dict5000 | 1.295 | 0.000 |
| iter10000 | 0.507 | 0.001 |
| math10000 | 0.246 | 0.000 |
| class1000 | -0.245 | 0.000 |
| sieve30000 | 1.198 | 0.000 |
| matrix48 | 1.223 | 0.000 |
| sin20000 | 0.894 | 0.000 |
| call200000 | 0.903 | 0.000 |
| regex6000 | 1.577 | 0.000 |
| str_unicode | 1.427 | 0.000 |
| except10000 | 0.733 | 0.000 |
| match20000 | 1.145 | 0.000 |
| closure20000 | 0.756 | 0.000 |
| inherit1000 | -0.254 | 0.000 |
| set5000 | 0.907 | 0.000 |
| enum10000 | 0.733 | 0.000 |
| modimport2000 | 0.416 | 0.000 |
| nanbox20000 | 0.731 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **4.334x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **15.694배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 818.982x, call200000은 201.419x(Int 함수의 native 직접 호출), nested_sum_200은 877.343x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.360x, call200000 5.565x, nested_sum_200 2.652x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261124-104236\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | +0.4% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | +0.2% | 보통 |
| int_ops | python | +0.1% | 보통 |
| int_ops | node | +0.2% | 보통 |
| fib24 | unjit | +4.3% | 보통 |
| fib24 | compile | +0.0% | 보통 |
| fib24 | build | -0.6% | 보통 |
| fib24 | python | -0.1% | 보통 |
| fib24 | node | +0.1% | 보통 |
| native_fib24 | unjit | +4.0% | 보통 |
| native_fib24 | compile | +0.3% | 보통 |
| native_fib24 | build | +1.1% | 보통 |
| native_fib24 | python | -3.7% | 보통 |
| native_fib24 | node | +1.1% | 보통 |
| nested_sum_200 | unjit | -0.5% | 보통 |
| nested_sum_200 | compile | +0.0% | 보통 |
| nested_sum_200 | build | -0.3% | 보통 |
| nested_sum_200 | python | -3.8% | 보통 |
| nested_sum_200 | node | -5.0% | 보통 |
| qsort500 | unjit | -0.6% | 보통 |
| qsort500 | compile | -5.7% | 보통 |
| qsort500 | build | +3.0% | 보통 |
| qsort500 | python | -1.0% | 보통 |
| qsort500 | node | +0.5% | 보통 |
| mergesort1000 | unjit | +0.9% | 보통 |
| mergesort1000 | compile | -0.9% | 보통 |
| mergesort1000 | build | -3.7% | 보통 |
| mergesort1000 | python | -0.3% | 보통 |
| mergesort1000 | node | -2.1% | 보통 |
| rle1994 | unjit | -1.1% | 보통 |
| rle1994 | python | -7.3% | 보통 |
| rle1994 | node | +0.4% | 보통 |
| dict5000 | unjit | +0.8% | 보통 |
| dict5000 | compile | +0.0% | 보통 |
| dict5000 | build | -1.5% | 보통 |
| dict5000 | python | +1.2% | 보통 |
| dict5000 | node | -14.6% | 개선 |
| iter10000 | unjit | -8.2% | 보통 |
| iter10000 | compile | -8.9% | 보통 |
| iter10000 | build | +1.4% | 보통 |
| iter10000 | python | +1.2% | 보통 |
| iter10000 | node | -19.6% | 개선 |
| math10000 | unjit | +6.1% | 보통 |
| math10000 | compile | +5.9% | 보통 |
| math10000 | build | -4.1% | 보통 |
| math10000 | python | -10.4% | 개선 |
| math10000 | node | +0.2% | 보통 |
| class1000 | unjit | +2.5% | 보통 |
| class1000 | compile | -5.0% | 보통 |
| class1000 | build | -1.5% | 보통 |
| class1000 | python | +4.0% | 보통 |
| class1000 | node | -22.7% | 개선 |
| sieve30000 | unjit | +0.7% | 보통 |
| sieve30000 | compile | -4.8% | 보통 |
| sieve30000 | build | +3.5% | 보통 |
| sieve30000 | python | +0.5% | 보통 |
| sieve30000 | node | +12.3% | 회귀 |
| matrix48 | unjit | +0.0% | 보통 |
| matrix48 | python | +1.2% | 보통 |
| matrix48 | node | +19.7% | 회귀 |
| sin20000 | unjit | -0.1% | 보통 |
| sin20000 | compile | +1.6% | 보통 |
| sin20000 | build | -0.3% | 보통 |
| sin20000 | python | -1.4% | 보통 |
| sin20000 | node | -0.7% | 보통 |
| call200000 | unjit | -0.2% | 보통 |
| call200000 | compile | -5.9% | 보통 |
| call200000 | build | -0.1% | 보통 |
| call200000 | python | +0.2% | 보통 |
| call200000 | node | -0.2% | 보통 |
| regex6000 | unjit | +0.9% | 보통 |
| regex6000 | python | +3.0% | 보통 |
| regex6000 | node | -9.1% | 보통 |
| str_unicode | unjit | -7.3% | 보통 |
| str_unicode | python | +3.0% | 보통 |
| str_unicode | node | -2.5% | 보통 |
| except10000 | unjit | -0.8% | 보통 |
| except10000 | python | -1.8% | 보통 |
| except10000 | node | -0.5% | 보통 |
| match20000 | unjit | -5.8% | 보통 |
| match20000 | python | +0.4% | 보통 |
| match20000 | node | -7.1% | 보통 |
| closure20000 | unjit | -6.1% | 보통 |
| closure20000 | python | +0.4% | 보통 |
| closure20000 | node | +1.1% | 보통 |
| inherit1000 | unjit | -3.7% | 보통 |
| inherit1000 | python | +0.7% | 보통 |
| inherit1000 | node | -71.8% | 개선 |
| set5000 | unjit | -8.4% | 보통 |
| set5000 | python | -2.5% | 보통 |
| set5000 | node | -21.3% | 개선 |
| enum10000 | unjit | +1.8% | 보통 |
| enum10000 | python | -1.4% | 보통 |
| enum10000 | node | +0.0% | 보통 |
| modimport2000 | unjit | -6.4% | 보통 |
| modimport2000 | python | -0.7% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | -0.6% | 보통 |
| nanbox20000 | python | -0.6% | 보통 |
| nanbox20000 | node | -1.8% | 보통 |

## 15. Historical Comparison

이전 결과(20261124-104236) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

