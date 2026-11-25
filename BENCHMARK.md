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

- 측정 일시: 2026-11-25 18:57:42 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|088973b
- 측정 커밋: 088973b
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **3.688x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **18.534x 느리다**.
- Un VM은 Python보다 기하평균 **2.421x 느리다**.
- Un VM은 Node.js보다 기하평균 **165.483x 느리다**.
- Python은 Node.js보다 기하평균 **68.361x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 3.688x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.054x  | >1 = Un이 빠름 |
| Un VM / Python | 0.413x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.006x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1072.7243 | 2.8589 | 2.8571 | 2.9559 | 2484.4537 | 9.5987 | 869.024 | 3.357 |
| fib24 | 1 | 137.4402 | 1.9381 | 126.9400 | 0.2118 | 3.2752 | 0.2670 | 1.690 | 0.138 |
| native_fib24 | 1 | 133.9377 | 131.0944 | 132.6986 | 0.2097 | 3.2129 | 0.2667 | 0.025 | 0.002 |
| nested_sum_200 | 40000 | 6.9464 | 0.0181 | 0.0114 | 0.0118 | 16.2927 | 0.0208 | 900.149 | 1.149 |
| qsort500 | 500 | 11.6611 | 0.8783 | 14.8571 | 0.5253 | 0.9074 | 0.1182 | 1.033 | 0.135 |
| mergesort1000 | 1000 | 30.8618 | 1.5237 | 33.5714 | 0.5113 | 2.1394 | 0.0910 | 1.404 | 0.060 |
| rle1994 | 1994 | 5.5193 | 5.2676 | N/A | N/A | 2.4419 | 0.0295 | 0.464 | 0.006 |
| dict5000 | 10000 | 2.4204 | 0.2644 | 2.5714 | 0.1829 | 5.3309 | 0.1438 | 20.162 | 0.544 |
| iter10000 | 30000 | 14.4709 | 15.7556 | 25.0000 | 0.2256 | 4.7579 | 0.2073 | 0.302 | 0.013 |
| math10000 | 10000 | 14.2819 | 0.0280 | 0.0257 | 0.0265 | 4.6632 | 0.0624 | 166.543 | 2.229 |
| class1000 | 1000 | 3.1843 | 0.5920 | 2.0086 | 0.0596 | 1.2968 | 0.0060 | 2.191 | 0.010 |
| sieve30000 | 30000 | 47.1070 | 2.4677 | 43.2857 | 0.2799 | 14.2667 | 0.2335 | 5.781 | 0.095 |
| matrix48 | 110592 | 47.2371 | 11.4913 | N/A | N/A | 22.2225 | 0.2171 | 1.934 | 0.019 |
| sin20000 | 20000 | 21.9935 | 0.5385 | 0.5471 | 0.5499 | 5.8279 | 0.1535 | 10.822 | 0.285 |
| call200000 | 200000 | 204.5603 | 0.6876 | 162.3586 | 0.7623 | 145.5230 | 3.9094 | 211.639 | 5.686 |
| regex6000 | 6000 | 1.2085 | 1.3757 | N/A | N/A | 2.7348 | 0.0973 | 1.988 | 0.071 |
| str_unicode | 2000 | 3.6893 | 3.6338 | N/A | N/A | 2.3844 | 0.2384 | 0.656 | 0.066 |
| except10000 | 10000 | 3.4712 | 0.0263 | N/A | N/A | 3.4412 | 0.0216 | 130.844 | 0.821 |
| match20000 | 20000 | 29.9319 | 31.0493 | N/A | N/A | 10.3211 | 0.0538 | 0.332 | 0.002 |
| closure20000 | 20000 | 20.3665 | 21.7189 | N/A | N/A | 7.7430 | 0.0280 | 0.357 | 0.001 |
| inherit1000 | 1000 | 1.8555 | 1.9324 | N/A | N/A | 0.9646 | 0.0165 | 0.499 | 0.009 |
| set5000 | 5000 | 4.7706 | 4.6414 | N/A | N/A | 1.7662 | 0.0402 | 0.381 | 0.009 |
| enum10000 | 10000 | 7.1394 | 7.0926 | N/A | N/A | 4.7942 | 0.0124 | 0.676 | 0.002 |
| modimport2000 | 2000 | 0.3760 | 0.0043 | N/A | N/A | 0.3325 | 0.0026 | 77.326 | 0.605 |
| nanbox20000 | 20000 | 12.4007 | 12.5975 | N/A | N/A | 7.5047 | 0.0166 | 0.596 | 0.001 |
| dict_str5000 | 10000 | 5.5326 | 5.3455 | N/A | N/A | N/A | N/A | N/A | N/A |
| bytes_ops | 10000 | 15.2371 | 14.5635 | N/A | N/A | N/A | N/A | N/A | N/A |
| slice_ops | 4000 | 4.2612 | 4.3407 | N/A | N/A | N/A | N/A | N/A | N/A |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1072.7243 | 2.8589 | 2.8571 | 2.9559 | 2484.4537 | 9.5987 | 869.024 | 3.357 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 6.9464 | 0.0181 | 0.0114 | 0.0118 | 16.2927 | 0.0208 | 900.149 | 1.149 |
| sieve30000 | 30000 | 47.1070 | 2.4677 | 43.2857 | 0.2799 | 14.2667 | 0.2335 | 5.781 | 0.095 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 204.5603 | 0.6876 | 162.3586 | 0.7623 | 145.5230 | 3.9094 | 211.639 | 5.686 |
| fib24 | 1 | 137.4402 | 1.9381 | 126.9400 | 0.2118 | 3.2752 | 0.2670 | 1.690 | 0.138 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 2.4204 | 0.2644 | 2.5714 | 0.1829 | 5.3309 | 0.1438 | 20.162 | 0.544 |
| class1000 | 1000 | 3.1843 | 0.5920 | 2.0086 | 0.0596 | 1.2968 | 0.0060 | 2.191 | 0.010 |
| rle1994 | 1994 | 5.5193 | 5.2676 | N/A | N/A | 2.4419 | 0.0295 | 0.464 | 0.006 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 11.6611 | 0.8783 | 14.8571 | 0.5253 | 0.9074 | 0.1182 | 1.033 | 0.135 |
| mergesort1000 | 1000 | 30.8618 | 1.5237 | 33.5714 | 0.5113 | 2.1394 | 0.0910 | 1.404 | 0.060 |
| matrix48 | 110592 | 47.2371 | 11.4913 | N/A | N/A | 22.2225 | 0.2171 | 1.934 | 0.019 |
| math10000 | 10000 | 14.2819 | 0.0280 | 0.0257 | 0.0265 | 4.6632 | 0.0624 | 166.543 | 2.229 |
| sin20000 | 20000 | 21.9935 | 0.5385 | 0.5471 | 0.5499 | 5.8279 | 0.1535 | 10.822 | 0.285 |
| regex6000 | 6000 | 1.2085 | 1.3757 | N/A | N/A | 2.7348 | 0.0973 | 1.988 | 0.071 |
| iter10000 | 30000 | 14.4709 | 15.7556 | 25.0000 | 0.2256 | 4.7579 | 0.2073 | 0.302 | 0.013 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.011 | 0.000 |
| fib24 | 0.873 | 0.000 |
| native_fib24 | 0.873 | 0.000 |
| nested_sum_200 | 0.767 | 0.000 |
| qsort500 | 0.125 | 0.007 |
| mergesort1000 | 0.260 | 0.015 |
| rle1994 | 1.080 | 0.000 |
| dict5000 | 1.003 | 0.000 |
| iter10000 | 0.749 | 0.001 |
| math10000 | 0.764 | 0.000 |
| class1000 | -0.495 | 0.000 |
| sieve30000 | 1.823 | 0.000 |
| matrix48 | 1.230 | 0.000 |
| sin20000 | 0.697 | 0.000 |
| call200000 | 0.907 | 0.000 |
| regex6000 | 1.238 | 0.000 |
| str_unicode | 1.345 | 0.000 |
| except10000 | 0.728 | 0.000 |
| match20000 | 1.089 | 0.000 |
| closure20000 | 0.773 | 0.000 |
| inherit1000 | -0.254 | 0.000 |
| set5000 | 0.339 | 0.000 |
| enum10000 | 0.727 | 0.000 |
| modimport2000 | -0.497 | 0.000 |
| nanbox20000 | 0.911 | 0.000 |
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

- 전반적으로 Un JIT은 Python보다 기하평균 **3.688x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **18.534배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 869.024x, call200000은 211.639x(Int 함수의 native 직접 호출), nested_sum_200은 900.149x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.357x, call200000 5.686x, nested_sum_200 1.149x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

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

