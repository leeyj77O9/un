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

- 측정 일시: 2026-11-24 13:19:38 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|7fa0f8a
- 측정 커밋: 7fa0f8a
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **4.342x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **15.323x 느리다**.
- Un VM은 Python보다 기하평균 **2.371x 느리다**.
- Un VM은 Node.js보다 기하평균 **157.771x 느리다**.
- Python은 Node.js보다 기하평균 **66.531x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 4.342x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.065x  | >1 = Un이 빠름 |
| Un VM / Python | 0.422x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.006x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1009.3853 | 2.8710 | 3.0000 | 3.0367 | 2394.4018 | 9.5756 | 833.996 | 3.335 |
| fib24 | 1 | 144.4983 | 1.9386 | 127.3657 | 0.2085 | 3.1642 | 0.2692 | 1.632 | 0.139 |
| native_fib24 | 1 | 140.8746 | 1.9318 | 128.5671 | 0.2091 | 3.1978 | 0.2821 | 1.655 | 0.146 |
| nested_sum_200 | 40000 | 6.7434 | 0.0182 | 0.0114 | 0.0118 | 16.4365 | 0.0492 | 903.104 | 2.703 |
| qsort500 | 500 | 12.7095 | 0.9300 | 14.2857 | 0.4936 | 0.8743 | 0.0908 | 0.940 | 0.098 |
| mergesort1000 | 1000 | 30.5328 | 1.5141 | 32.0000 | 0.5140 | 2.1859 | 0.0723 | 1.444 | 0.048 |
| rle1994 | 1994 | 5.2472 | 5.2094 | N/A | N/A | 2.5279 | 0.0348 | 0.485 | 0.007 |
| dict5000 | 10000 | 2.4824 | 0.2607 | 2.7143 | 0.1896 | 5.6369 | 0.1872 | 21.622 | 0.718 |
| iter10000 | 30000 | 14.1055 | 14.9394 | 24.5714 | 0.2319 | 4.8003 | 0.2070 | 0.321 | 0.014 |
| math10000 | 10000 | 13.9004 | 0.0262 | 0.0257 | 0.0249 | 4.4642 | 0.0626 | 170.389 | 2.389 |
| class1000 | 1000 | 2.7650 | 0.5704 | 1.9300 | 0.0600 | 1.2563 | 0.0129 | 2.202 | 0.023 |
| sieve30000 | 30000 | 38.7949 | 2.5049 | 44.1429 | 0.2576 | 14.1623 | 0.2238 | 5.654 | 0.089 |
| matrix48 | 110592 | 44.8610 | 11.7344 | N/A | N/A | 22.2745 | 0.2036 | 1.898 | 0.017 |
| sin20000 | 20000 | 16.1562 | 0.5395 | 0.5457 | 0.5435 | 5.7692 | 0.1508 | 10.694 | 0.280 |
| call200000 | 200000 | 204.3468 | 0.6845 | 150.8500 | 0.7640 | 142.8206 | 3.8308 | 208.650 | 5.596 |
| regex6000 | 6000 | 1.4311 | 1.2164 | N/A | N/A | 2.7913 | 0.0916 | 2.295 | 0.075 |
| str_unicode | 2000 | 3.7255 | 3.6812 | N/A | N/A | 2.4872 | 0.1802 | 0.676 | 0.049 |
| except10000 | 10000 | 3.4981 | 0.0277 | N/A | N/A | 3.4486 | 0.0213 | 124.498 | 0.769 |
| match20000 | 20000 | 28.5850 | 30.8227 | N/A | N/A | 9.9067 | 0.0573 | 0.321 | 0.002 |
| closure20000 | 20000 | 19.2104 | 24.1938 | N/A | N/A | 7.2748 | 0.0282 | 0.301 | 0.001 |
| inherit1000 | 1000 | 1.8102 | 1.8946 | N/A | N/A | 0.9555 | 0.0092 | 0.504 | 0.005 |
| set5000 | 5000 | 4.6615 | 4.6432 | N/A | N/A | 1.7879 | 0.0336 | 0.385 | 0.007 |
| enum10000 | 10000 | 6.9137 | 6.9966 | N/A | N/A | 4.6290 | 0.0125 | 0.662 | 0.002 |
| modimport2000 | 2000 | 0.3726 | 0.0046 | N/A | N/A | 0.3228 | 0.0027 | 70.174 | 0.587 |
| nanbox20000 | 20000 | 12.1002 | 11.9186 | N/A | N/A | 7.4126 | 0.0165 | 0.622 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1009.3853 | 2.8710 | 3.0000 | 3.0367 | 2394.4018 | 9.5756 | 833.996 | 3.335 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 6.7434 | 0.0182 | 0.0114 | 0.0118 | 16.4365 | 0.0492 | 903.104 | 2.703 |
| sieve30000 | 30000 | 38.7949 | 2.5049 | 44.1429 | 0.2576 | 14.1623 | 0.2238 | 5.654 | 0.089 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 204.3468 | 0.6845 | 150.8500 | 0.7640 | 142.8206 | 3.8308 | 208.650 | 5.596 |
| fib24 | 1 | 144.4983 | 1.9386 | 127.3657 | 0.2085 | 3.1642 | 0.2692 | 1.632 | 0.139 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 2.4824 | 0.2607 | 2.7143 | 0.1896 | 5.6369 | 0.1872 | 21.622 | 0.718 |
| class1000 | 1000 | 2.7650 | 0.5704 | 1.9300 | 0.0600 | 1.2563 | 0.0129 | 2.202 | 0.023 |
| rle1994 | 1994 | 5.2472 | 5.2094 | N/A | N/A | 2.5279 | 0.0348 | 0.485 | 0.007 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 12.7095 | 0.9300 | 14.2857 | 0.4936 | 0.8743 | 0.0908 | 0.940 | 0.098 |
| mergesort1000 | 1000 | 30.5328 | 1.5141 | 32.0000 | 0.5140 | 2.1859 | 0.0723 | 1.444 | 0.048 |
| matrix48 | 110592 | 44.8610 | 11.7344 | N/A | N/A | 22.2745 | 0.2036 | 1.898 | 0.017 |
| math10000 | 10000 | 13.9004 | 0.0262 | 0.0257 | 0.0249 | 4.4642 | 0.0626 | 170.389 | 2.389 |
| sin20000 | 20000 | 16.1562 | 0.5395 | 0.5457 | 0.5435 | 5.7692 | 0.1508 | 10.694 | 0.280 |
| regex6000 | 6000 | 1.4311 | 1.2164 | N/A | N/A | 2.7913 | 0.0916 | 2.295 | 0.075 |
| iter10000 | 30000 | 14.1055 | 14.9394 | 24.5714 | 0.2319 | 4.8003 | 0.2070 | 0.321 | 0.014 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.239 | 0.000 |
| fib24 | 0.876 | 0.000 |
| native_fib24 | 0.876 | 0.000 |
| nested_sum_200 | 0.741 | 0.000 |
| qsort500 | 1.048 | 0.007 |
| mergesort1000 | 0.245 | 0.015 |
| rle1994 | 1.119 | 0.000 |
| dict5000 | 1.030 | 0.000 |
| iter10000 | 0.894 | 0.001 |
| math10000 | 0.769 | 0.000 |
| class1000 | 1.164 | 0.000 |
| sieve30000 | 1.754 | 0.000 |
| matrix48 | 1.175 | 0.000 |
| sin20000 | 0.894 | 0.000 |
| call200000 | 0.877 | 0.000 |
| regex6000 | 1.145 | 0.000 |
| str_unicode | 1.385 | 0.000 |
| except10000 | 0.737 | 0.000 |
| match20000 | 1.095 | 0.000 |
| closure20000 | 0.768 | 0.000 |
| inherit1000 | -0.107 | 0.000 |
| set5000 | 1.162 | 0.000 |
| enum10000 | 0.731 | 0.000 |
| modimport2000 | 0.045 | 0.000 |
| nanbox20000 | 0.075 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **4.342x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **15.323배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 833.996x, call200000은 208.650x(Int 함수의 native 직접 호출), nested_sum_200은 903.104x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.335x, call200000 5.596x, nested_sum_200 2.703x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261124-122657\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | +0.1% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | +1.3% | 보통 |
| int_ops | python | +1.0% | 보통 |
| int_ops | node | -1.2% | 보통 |
| fib24 | unjit | +0.1% | 보통 |
| fib24 | compile | +1.9% | 보통 |
| fib24 | build | -1.5% | 보통 |
| fib24 | python | -3.6% | 보통 |
| fib24 | node | +0.9% | 보통 |
| native_fib24 | unjit | +0.2% | 보통 |
| native_fib24 | compile | +2.0% | 보통 |
| native_fib24 | build | -0.8% | 보통 |
| native_fib24 | python | +1.4% | 보통 |
| native_fib24 | node | +5.9% | 보통 |
| nested_sum_200 | unjit | +0.6% | 보통 |
| nested_sum_200 | compile | -11.1% | 개선 |
| nested_sum_200 | build | +1.8% | 보통 |
| nested_sum_200 | python | +2.7% | 보통 |
| nested_sum_200 | node | +61.8% | 회귀 |
| qsort500 | unjit | +7.5% | 보통 |
| qsort500 | compile | +1.0% | 보통 |
| qsort500 | build | -5.4% | 보통 |
| qsort500 | python | -4.1% | 보통 |
| qsort500 | node | +6.0% | 보통 |
| mergesort1000 | unjit | +0.5% | 보통 |
| mergesort1000 | compile | +1.4% | 보통 |
| mergesort1000 | build | -4.1% | 보통 |
| mergesort1000 | python | +0.4% | 보통 |
| mergesort1000 | node | +7.6% | 보통 |
| rle1994 | unjit | -3.5% | 보통 |
| rle1994 | python | +3.0% | 보통 |
| rle1994 | node | -20.5% | 개선 |
| dict5000 | unjit | +0.2% | 보통 |
| dict5000 | compile | +11.8% | 회귀 |
| dict5000 | build | +6.4% | 보통 |
| dict5000 | python | +1.6% | 보통 |
| dict5000 | node | +31.9% | 회귀 |
| iter10000 | unjit | +2.0% | 보통 |
| iter10000 | compile | -1.7% | 보통 |
| iter10000 | build | +0.3% | 보통 |
| iter10000 | python | +5.1% | 보통 |
| iter10000 | node | +14.0% | 회귀 |
| math10000 | unjit | -0.4% | 보통 |
| math10000 | compile | +0.0% | 보통 |
| math10000 | build | -2.7% | 보통 |
| math10000 | python | -6.4% | 보통 |
| math10000 | node | +0.0% | 보통 |
| class1000 | unjit | -1.2% | 보통 |
| class1000 | compile | +0.4% | 보통 |
| class1000 | build | -1.0% | 보통 |
| class1000 | python | -6.7% | 보통 |
| class1000 | node | +69.7% | 회귀 |
| sieve30000 | unjit | +1.2% | 보통 |
| sieve30000 | compile | +4.7% | 보통 |
| sieve30000 | build | -3.0% | 보통 |
| sieve30000 | python | -1.0% | 보통 |
| sieve30000 | node | -11.0% | 개선 |
| matrix48 | unjit | -1.7% | 보통 |
| matrix48 | python | +0.3% | 보통 |
| matrix48 | node | +15.7% | 회귀 |
| sin20000 | unjit | +0.2% | 보통 |
| sin20000 | compile | -0.8% | 보통 |
| sin20000 | build | -0.7% | 보통 |
| sin20000 | python | -3.6% | 보통 |
| sin20000 | node | -8.0% | 보통 |
| call200000 | unjit | -0.1% | 보통 |
| call200000 | compile | +3.7% | 보통 |
| call200000 | build | +0.8% | 보통 |
| call200000 | python | -0.2% | 보통 |
| call200000 | node | -0.8% | 보통 |
| regex6000 | unjit | -0.4% | 보통 |
| regex6000 | python | +3.0% | 보통 |
| regex6000 | node | +8.5% | 보통 |
| str_unicode | unjit | +1.6% | 보통 |
| str_unicode | python | +4.7% | 보통 |
| str_unicode | node | -3.9% | 보통 |
| except10000 | unjit | +5.7% | 보통 |
| except10000 | python | -4.5% | 보통 |
| except10000 | node | -0.5% | 보통 |
| match20000 | unjit | +3.9% | 보통 |
| match20000 | python | +1.6% | 보통 |
| match20000 | node | -5.0% | 보통 |
| closure20000 | unjit | +20.9% | 회귀 |
| closure20000 | python | -1.7% | 보통 |
| closure20000 | node | +0.4% | 보통 |
| inherit1000 | unjit | +1.4% | 보통 |
| inherit1000 | python | -1.2% | 보통 |
| inherit1000 | node | -36.6% | 개선 |
| set5000 | unjit | -3.1% | 보통 |
| set5000 | python | +0.5% | 보통 |
| set5000 | node | -4.3% | 보통 |
| enum10000 | unjit | +2.6% | 보통 |
| enum10000 | python | -1.2% | 보통 |
| enum10000 | node | +1.6% | 보통 |
| modimport2000 | unjit | +7.0% | 보통 |
| modimport2000 | python | -3.9% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | +0.1% | 보통 |
| nanbox20000 | python | +1.6% | 보통 |
| nanbox20000 | node | +0.0% | 보통 |

## 15. Historical Comparison

이전 결과(20261124-122657) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

