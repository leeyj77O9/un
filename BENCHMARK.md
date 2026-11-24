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

- 측정 일시: 2026-11-24 14:42:22 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|180d3db
- 측정 커밋: 180d3db
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **4.424x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **15.429x 느리다**.
- Un VM은 Python보다 기하평균 **2.331x 느리다**.
- Un VM은 Node.js보다 기하평균 **159.116x 느리다**.
- Python은 Node.js보다 기하평균 **68.256x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 4.424x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.065x  | >1 = Un이 빠름 |
| Un VM / Python | 0.429x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.006x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1014.7230 | 2.8475 | 2.8571 | 2.8707 | 2361.8238 | 9.5954 | 829.438 | 3.370 |
| fib24 | 1 | 136.7688 | 1.9554 | 125.2057 | 0.2122 | 3.2366 | 0.2676 | 1.655 | 0.137 |
| native_fib24 | 1 | 145.8255 | 1.9341 | 126.4714 | 0.2118 | 3.1320 | 0.2677 | 1.619 | 0.138 |
| nested_sum_200 | 40000 | 6.8950 | 0.0182 | 0.0114 | 0.0120 | 16.6489 | 0.0465 | 914.775 | 2.555 |
| qsort500 | 500 | 12.3442 | 0.8460 | 14.4286 | 0.5231 | 0.8866 | 0.0918 | 1.048 | 0.109 |
| mergesort1000 | 1000 | 29.8341 | 1.5149 | 31.4286 | 0.5097 | 2.2615 | 0.0922 | 1.493 | 0.061 |
| rle1994 | 1994 | 5.0989 | 5.1902 | N/A | N/A | 2.3485 | 0.0393 | 0.452 | 0.008 |
| dict5000 | 10000 | 2.5142 | 0.2606 | 2.4286 | 0.1947 | 5.4632 | 0.1470 | 20.964 | 0.564 |
| iter10000 | 30000 | 13.5765 | 14.3959 | 24.4286 | 0.2291 | 4.6916 | 0.1762 | 0.326 | 0.012 |
| math10000 | 10000 | 13.6270 | 0.0262 | 0.0243 | 0.0257 | 4.7264 | 0.0625 | 180.397 | 2.385 |
| class1000 | 1000 | 2.6288 | 0.5647 | 1.9014 | 0.0608 | 1.3020 | 0.0075 | 2.306 | 0.013 |
| sieve30000 | 30000 | 38.6056 | 2.4780 | 43.0000 | 0.2721 | 14.4209 | 0.2128 | 5.820 | 0.086 |
| matrix48 | 110592 | 47.0038 | 11.3645 | N/A | N/A | 21.9631 | 0.2150 | 1.933 | 0.019 |
| sin20000 | 20000 | 15.8243 | 0.5381 | 0.5457 | 0.5503 | 5.8666 | 0.1505 | 10.902 | 0.280 |
| call200000 | 200000 | 200.0136 | 0.6844 | 148.8800 | 0.7626 | 140.8520 | 3.8210 | 205.804 | 5.583 |
| regex6000 | 6000 | 1.2293 | 1.2338 | N/A | N/A | 2.6478 | 0.0728 | 2.146 | 0.059 |
| str_unicode | 2000 | 3.6531 | 3.5807 | N/A | N/A | 2.2779 | 0.1851 | 0.636 | 0.052 |
| except10000 | 10000 | 3.4818 | 0.0263 | N/A | N/A | 3.3247 | 0.0218 | 126.414 | 0.829 |
| match20000 | 20000 | 28.6821 | 30.7814 | N/A | N/A | 10.5271 | 0.0719 | 0.342 | 0.002 |
| closure20000 | 20000 | 19.1817 | 19.8106 | N/A | N/A | 7.9455 | 0.0289 | 0.401 | 0.001 |
| inherit1000 | 1000 | 1.7919 | 1.8807 | N/A | N/A | 0.9698 | 0.0093 | 0.516 | 0.005 |
| set5000 | 5000 | 4.5157 | 4.8861 | N/A | N/A | 1.7997 | 0.0335 | 0.368 | 0.007 |
| enum10000 | 10000 | 6.8292 | 6.8828 | N/A | N/A | 4.7560 | 0.0128 | 0.691 | 0.002 |
| modimport2000 | 2000 | 0.3795 | 0.0044 | N/A | N/A | 0.3277 | 0.0027 | 74.477 | 0.614 |
| nanbox20000 | 20000 | 12.2438 | 12.7278 | N/A | N/A | 7.4345 | 0.0165 | 0.584 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1014.7230 | 2.8475 | 2.8571 | 2.8707 | 2361.8238 | 9.5954 | 829.438 | 3.370 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 6.8950 | 0.0182 | 0.0114 | 0.0120 | 16.6489 | 0.0465 | 914.775 | 2.555 |
| sieve30000 | 30000 | 38.6056 | 2.4780 | 43.0000 | 0.2721 | 14.4209 | 0.2128 | 5.820 | 0.086 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 200.0136 | 0.6844 | 148.8800 | 0.7626 | 140.8520 | 3.8210 | 205.804 | 5.583 |
| fib24 | 1 | 136.7688 | 1.9554 | 125.2057 | 0.2122 | 3.2366 | 0.2676 | 1.655 | 0.137 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 2.5142 | 0.2606 | 2.4286 | 0.1947 | 5.4632 | 0.1470 | 20.964 | 0.564 |
| class1000 | 1000 | 2.6288 | 0.5647 | 1.9014 | 0.0608 | 1.3020 | 0.0075 | 2.306 | 0.013 |
| rle1994 | 1994 | 5.0989 | 5.1902 | N/A | N/A | 2.3485 | 0.0393 | 0.452 | 0.008 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 12.3442 | 0.8460 | 14.4286 | 0.5231 | 0.8866 | 0.0918 | 1.048 | 0.109 |
| mergesort1000 | 1000 | 29.8341 | 1.5149 | 31.4286 | 0.5097 | 2.2615 | 0.0922 | 1.493 | 0.061 |
| matrix48 | 110592 | 47.0038 | 11.3645 | N/A | N/A | 21.9631 | 0.2150 | 1.933 | 0.019 |
| math10000 | 10000 | 13.6270 | 0.0262 | 0.0243 | 0.0257 | 4.7264 | 0.0625 | 180.397 | 2.385 |
| sin20000 | 20000 | 15.8243 | 0.5381 | 0.5457 | 0.5503 | 5.8666 | 0.1505 | 10.902 | 0.280 |
| regex6000 | 6000 | 1.2293 | 1.2338 | N/A | N/A | 2.6478 | 0.0728 | 2.146 | 0.059 |
| iter10000 | 30000 | 13.5765 | 14.3959 | 24.4286 | 0.2291 | 4.6916 | 0.1762 | 0.326 | 0.012 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.726 | 0.000 |
| fib24 | 0.876 | 0.000 |
| native_fib24 | 0.626 | 0.000 |
| nested_sum_200 | 0.634 | 0.000 |
| qsort500 | 0.155 | 0.007 |
| mergesort1000 | 0.244 | 0.015 |
| rle1994 | 1.123 | 0.000 |
| dict5000 | 0.987 | 0.000 |
| iter10000 | 0.836 | 0.001 |
| math10000 | 0.433 | 0.000 |
| class1000 | -0.924 | 0.000 |
| sieve30000 | 1.949 | 0.000 |
| matrix48 | 1.270 | 0.000 |
| sin20000 | 0.899 | 0.000 |
| call200000 | 0.771 | 0.000 |
| regex6000 | 1.491 | 0.000 |
| str_unicode | 1.354 | 0.000 |
| except10000 | 0.874 | 0.000 |
| match20000 | 1.215 | 0.000 |
| closure20000 | 0.757 | 0.000 |
| inherit1000 | 0.464 | 0.000 |
| set5000 | 0.520 | 0.000 |
| enum10000 | 0.731 | 0.000 |
| modimport2000 | 0.112 | 0.000 |
| nanbox20000 | 0.753 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **4.424x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **15.429배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 829.438x, call200000은 205.804x(Int 함수의 native 직접 호출), nested_sum_200은 914.775x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.370x, call200000 5.583x, nested_sum_200 2.555x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261124-143033\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | +0.2% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | +0.3% | 보통 |
| int_ops | python | -0.9% | 보통 |
| int_ops | node | +0.2% | 보통 |
| fib24 | unjit | +0.3% | 보통 |
| fib24 | compile | +0.2% | 보통 |
| fib24 | build | +0.9% | 보통 |
| fib24 | python | +0.0% | 보통 |
| fib24 | node | +0.3% | 보통 |
| native_fib24 | unjit | -2.1% | 보통 |
| native_fib24 | compile | +1.6% | 보통 |
| native_fib24 | build | +0.4% | 보통 |
| native_fib24 | python | -6.6% | 보통 |
| native_fib24 | node | -11.7% | 개선 |
| nested_sum_200 | unjit | +0.6% | 보통 |
| nested_sum_200 | compile | -11.1% | 개선 |
| nested_sum_200 | build | +4.6% | 보통 |
| nested_sum_200 | python | +5.1% | 보통 |
| nested_sum_200 | node | -0.4% | 보통 |
| qsort500 | unjit | -9.1% | 보통 |
| qsort500 | compile | -1.9% | 보통 |
| qsort500 | build | -0.5% | 보통 |
| qsort500 | python | +3.8% | 보통 |
| qsort500 | node | -3.9% | 보통 |
| mergesort1000 | unjit | +0.0% | 보통 |
| mergesort1000 | compile | +0.0% | 보통 |
| mergesort1000 | build | -1.5% | 보통 |
| mergesort1000 | python | +7.6% | 보통 |
| mergesort1000 | node | +48.9% | 회귀 |
| rle1994 | unjit | +0.3% | 보통 |
| rle1994 | python | -5.7% | 보통 |
| rle1994 | node | +9.5% | 보통 |
| dict5000 | unjit | -0.3% | 보통 |
| dict5000 | compile | -5.6% | 보통 |
| dict5000 | build | +4.0% | 보통 |
| dict5000 | python | -6.5% | 보통 |
| dict5000 | node | -16.7% | 개선 |
| iter10000 | unjit | -0.7% | 보통 |
| iter10000 | compile | -2.3% | 보통 |
| iter10000 | build | -6.7% | 보통 |
| iter10000 | python | +3.6% | 보통 |
| iter10000 | node | -21.7% | 개선 |
| math10000 | unjit | +0.8% | 보통 |
| math10000 | compile | +0.0% | 보통 |
| math10000 | build | -3.4% | 보통 |
| math10000 | python | +0.4% | 보통 |
| math10000 | node | -0.2% | 보통 |
| class1000 | unjit | -0.6% | 보통 |
| class1000 | compile | +0.2% | 보통 |
| class1000 | build | -7.7% | 보통 |
| class1000 | python | +2.8% | 보통 |
| class1000 | node | +4.2% | 보통 |
| sieve30000 | unjit | +0.2% | 보통 |
| sieve30000 | compile | +0.0% | 보통 |
| sieve30000 | build | +2.0% | 보통 |
| sieve30000 | python | +1.1% | 보통 |
| sieve30000 | node | +7.0% | 보통 |
| matrix48 | unjit | -0.3% | 보통 |
| matrix48 | python | +2.2% | 보통 |
| matrix48 | node | +0.7% | 보통 |
| sin20000 | unjit | -0.1% | 보통 |
| sin20000 | compile | -0.3% | 보통 |
| sin20000 | build | +0.2% | 보통 |
| sin20000 | python | -7.2% | 보통 |
| sin20000 | node | -5.6% | 보통 |
| call200000 | unjit | -0.9% | 보통 |
| call200000 | compile | +0.6% | 보통 |
| call200000 | build | +0.5% | 보통 |
| call200000 | python | +1.1% | 보통 |
| call200000 | node | -1.0% | 보통 |
| regex6000 | unjit | +1.0% | 보통 |
| regex6000 | python | -1.1% | 보통 |
| regex6000 | node | -9.0% | 보통 |
| str_unicode | unjit | -0.2% | 보통 |
| str_unicode | python | +0.6% | 보통 |
| str_unicode | node | -21.2% | 개선 |
| except10000 | unjit | -0.8% | 보통 |
| except10000 | python | -0.6% | 보통 |
| except10000 | node | +1.9% | 보통 |
| match20000 | unjit | +2.0% | 보통 |
| match20000 | python | +0.7% | 보통 |
| match20000 | node | +22.7% | 회귀 |
| closure20000 | unjit | -1.2% | 보통 |
| closure20000 | python | +0.6% | 보통 |
| closure20000 | node | +2.1% | 보통 |
| inherit1000 | unjit | -3.0% | 보통 |
| inherit1000 | python | -1.4% | 보통 |
| inherit1000 | node | -18.4% | 개선 |
| set5000 | unjit | +6.6% | 보통 |
| set5000 | python | -3.0% | 보통 |
| set5000 | node | +1.5% | 보통 |
| enum10000 | unjit | -0.6% | 보통 |
| enum10000 | python | -2.3% | 보통 |
| enum10000 | node | +3.2% | 보통 |
| modimport2000 | unjit | +0.0% | 보통 |
| modimport2000 | python | -3.1% | 보통 |
| modimport2000 | node | +3.8% | 보통 |
| nanbox20000 | unjit | +5.5% | 보통 |
| nanbox20000 | python | +1.1% | 보통 |
| nanbox20000 | node | +0.6% | 보통 |

## 15. Historical Comparison

이전 결과(20261124-143033) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

