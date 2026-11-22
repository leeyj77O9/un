# BENCHMARK.md

Un 벤치마크 결과입니다. 실행: `scripts\bench.ps1 -Group all -Report` (Windows)

## 1. Benchmark Overview

Un(Rust 바이트코드 컴파일러 + 레지스터 VM + Cranelift JIT)을 CPython 3.13과 Node.js 26과
비교하기 위한 벤치마크다. 각 테스트는 bench/<name>.un, bench/node/<name>.mjs,
bench/python/<name>.py의 세 파일이 **같은 알고리즘, 같은 입력, 같은 검증**을 공유한다.

| 노드 | 내용 |
|---|---|
| 측정 언어 | Un VM (UV_JIT=0), Un JIT(기본), Compile Mode(un native), standalone exe(un build), CPython, Node.js |
| 측정 방식 | 워밍업 2회 → reps=7회 개별 측정, Median 비교 |
| 저장 위치 | bench/results/<timestamp>/results.csv, results.json, results.md |
| 보고서 | 이 문서(BENCHMARK.md)은 -Report로 자동 생성된다 |
| 비교 지표 | 중앙값, 종합은 기하평균(∏ rᵢ^(1/n)) |

## 2. Environment

- 측정 일시: 2026-11-23 02:52:42 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|990626f
- 측정 커밋: 990626f
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **3.978x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **16.541x 느리다**.
- Un VM은 Python보다 기하평균 **3.424x 느리다**.
- Un VM은 Node.js보다 기하평균 **225.262x 느리다**.
- Python은 Node.js보다 기하평균 **65.794x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 3.978x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.060x  | >1 = Un이 빠름 |
| Un VM / Python | 0.292x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.004x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1645.7494 | 2.8514 | 2.8571 | 2.9067 | 2355.6369 | 9.5800 | 826.133 | 3.360 |
| fib24 | 1 | 186.7154 | 1.9300 | 172.1400 | 2.6649 | 3.4079 | 0.2657 | 1.766 | 0.138 |
| native_fib24 | 1 | 198.4579 | 1.9316 | 166.9157 | 2.6613 | 3.1533 | 0.2682 | 1.632 | 0.139 |
| nested_sum_200 | 40000 | 8.6056 | 0.0175 | 0.0129 | 0.0135 | 16.4665 | 0.0475 | 940.943 | 2.714 |
| qsort500 | 500 | 15.8258 | 0.8557 | 20.5714 | 0.5599 | 0.8590 | 0.1005 | 1.004 | 0.117 |
| mergesort1000 | 1000 | 43.4652 | 1.5042 | 45.8571 | 0.6664 | 2.1029 | 0.1201 | 1.398 | 0.080 |
| rle1994 | 1994 | 7.3556 | 7.2614 | N/A | N/A | 2.2900 | 0.0359 | 0.315 | 0.005 |
| dict5000 | 10000 | 3.6868 | 0.2600 | 4.0000 | 0.6214 | 5.3227 | 0.1438 | 20.472 | 0.553 |
| iter10000 | 30000 | 17.1841 | 3.9759 | 35.0000 | 0.3140 | 4.4515 | 0.2144 | 1.120 | 0.054 |
| math10000 | 10000 | 19.9479 | 0.0548 | 0.0257 | 0.0240 | 4.3527 | 0.0626 | 79.429 | 1.142 |
| class1000 | 1000 | 4.4871 | 0.5760 | 3.0443 | 0.4032 | 1.3198 | 0.0058 | 2.291 | 0.010 |
| sieve30000 | 30000 | 58.1888 | 2.4842 | 63.1429 | 0.4333 | 14.4611 | 0.2068 | 5.821 | 0.083 |
| matrix48 | 110592 | 73.5668 | 11.3407 | N/A | N/A | 21.5566 | 0.1953 | 1.901 | 0.017 |
| sin20000 | 20000 | 21.9954 | 0.5418 | 0.5414 | 0.5485 | 5.6508 | 0.1503 | 10.430 | 0.277 |
| call200000 | 200000 | 271.3518 | 0.6840 | 203.9657 | 3.5812 | 137.2445 | 3.8289 | 200.650 | 5.598 |
| regex6000 | 6000 | 1.4133 | 1.2220 | N/A | N/A | 2.7320 | 0.0790 | 2.236 | 0.065 |
| str_unicode | 2000 | 4.5261 | 4.4561 | N/A | N/A | 2.2527 | 0.1943 | 0.506 | 0.044 |
| except10000 | 10000 | 5.5121 | 0.0259 | N/A | N/A | 3.4271 | 0.0214 | 132.320 | 0.826 |
| match20000 | 20000 | 47.8576 | 48.4298 | N/A | N/A | 9.7310 | 0.0517 | 0.201 | 0.001 |
| closure20000 | 20000 | 26.7436 | 28.6721 | N/A | N/A | 7.3500 | 0.0281 | 0.256 | 0.001 |
| inherit1000 | 1000 | 2.8135 | 2.7515 | N/A | N/A | 0.9813 | 0.0195 | 0.357 | 0.007 |
| set5000 | 5000 | 7.8659 | 7.8159 | N/A | N/A | 1.8224 | 0.0339 | 0.233 | 0.004 |
| enum10000 | 10000 | 10.6313 | 10.4867 | N/A | N/A | 4.7468 | 0.0124 | 0.453 | 0.001 |
| modimport2000 | 2000 | 0.5828 | 0.0040 | N/A | N/A | 0.3272 | 0.0027 | 81.800 | 0.675 |
| nanbox20000 | 20000 | 17.1423 | 17.2999 | N/A | N/A | 7.4490 | 0.0161 | 0.431 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1645.7494 | 2.8514 | 2.8571 | 2.9067 | 2355.6369 | 9.5800 | 826.133 | 3.360 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 8.6056 | 0.0175 | 0.0129 | 0.0135 | 16.4665 | 0.0475 | 940.943 | 2.714 |
| sieve30000 | 30000 | 58.1888 | 2.4842 | 63.1429 | 0.4333 | 14.4611 | 0.2068 | 5.821 | 0.083 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 271.3518 | 0.6840 | 203.9657 | 3.5812 | 137.2445 | 3.8289 | 200.650 | 5.598 |
| fib24 | 1 | 186.7154 | 1.9300 | 172.1400 | 2.6649 | 3.4079 | 0.2657 | 1.766 | 0.138 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 3.6868 | 0.2600 | 4.0000 | 0.6214 | 5.3227 | 0.1438 | 20.472 | 0.553 |
| class1000 | 1000 | 4.4871 | 0.5760 | 3.0443 | 0.4032 | 1.3198 | 0.0058 | 2.291 | 0.010 |
| rle1994 | 1994 | 7.3556 | 7.2614 | N/A | N/A | 2.2900 | 0.0359 | 0.315 | 0.005 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 15.8258 | 0.8557 | 20.5714 | 0.5599 | 0.8590 | 0.1005 | 1.004 | 0.117 |
| mergesort1000 | 1000 | 43.4652 | 1.5042 | 45.8571 | 0.6664 | 2.1029 | 0.1201 | 1.398 | 0.080 |
| matrix48 | 110592 | 73.5668 | 11.3407 | N/A | N/A | 21.5566 | 0.1953 | 1.901 | 0.017 |
| math10000 | 10000 | 19.9479 | 0.0548 | 0.0257 | 0.0240 | 4.3527 | 0.0626 | 79.429 | 1.142 |
| sin20000 | 20000 | 21.9954 | 0.5418 | 0.5414 | 0.5485 | 5.6508 | 0.1503 | 10.430 | 0.277 |
| regex6000 | 6000 | 1.4133 | 1.2220 | N/A | N/A | 2.7320 | 0.0790 | 2.236 | 0.065 |
| iter10000 | 30000 | 17.1841 | 3.9759 | 35.0000 | 0.3140 | 4.4515 | 0.2144 | 1.120 | 0.054 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.726 | 0.000 |
| fib24 | 0.186 | 0.000 |
| native_fib24 | 0.731 | 0.000 |
| nested_sum_200 | 0.750 | 0.000 |
| qsort500 | 0.127 | 0.007 |
| mergesort1000 | 0.266 | 0.015 |
| rle1994 | 1.075 | 0.000 |
| dict5000 | 1.327 | 0.000 |
| iter10000 | 0.831 | 0.001 |
| math10000 | 0.875 | 0.000 |
| class1000 | 0.437 | 0.000 |
| sieve30000 | 1.884 | 0.000 |
| matrix48 | 1.188 | 0.000 |
| sin20000 | 0.894 | 0.000 |
| call200000 | 0.747 | 0.000 |
| regex6000 | 1.339 | 0.000 |
| str_unicode | 1.356 | 0.000 |
| except10000 | 0.730 | 0.000 |
| match20000 | 1.140 | 0.000 |
| closure20000 | 0.928 | 0.000 |
| inherit1000 | -0.134 | 0.000 |
| set5000 | 0.442 | 0.000 |
| enum10000 | 0.885 | 0.000 |
| modimport2000 | 0.342 | 0.000 |
| nanbox20000 | 0.730 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1645.7494 | 2.8514 | 2.8571 | 2.9067 | 2355.6369 | 9.5800 | 826.133 | 3.360 |
| fib24 | 1 | 186.7154 | 1.9300 | 172.1400 | 2.6649 | 3.4079 | 0.2657 | 1.766 | 0.138 |
| native_fib24 | 1 | 198.4579 | 1.9316 | 166.9157 | 2.6613 | 3.1533 | 0.2682 | 1.632 | 0.139 |
| nested_sum_200 | 40000 | 8.6056 | 0.0175 | 0.0129 | 0.0135 | 16.4665 | 0.0475 | 940.943 | 2.714 |
| qsort500 | 500 | 15.8258 | 0.8557 | 20.5714 | 0.5599 | 0.8590 | 0.1005 | 1.004 | 0.117 |
| mergesort1000 | 1000 | 43.4652 | 1.5042 | 45.8571 | 0.6664 | 2.1029 | 0.1201 | 1.398 | 0.080 |
| rle1994 | 1994 | 7.3556 | 7.2614 | N/A | N/A | 2.2900 | 0.0359 | 0.315 | 0.005 |
| dict5000 | 10000 | 3.6868 | 0.2600 | 4.0000 | 0.6214 | 5.3227 | 0.1438 | 20.472 | 0.553 |
| iter10000 | 30000 | 17.1841 | 3.9759 | 35.0000 | 0.3140 | 4.4515 | 0.2144 | 1.120 | 0.054 |
| math10000 | 10000 | 19.9479 | 0.0548 | 0.0257 | 0.0240 | 4.3527 | 0.0626 | 79.429 | 1.142 |
| class1000 | 1000 | 4.4871 | 0.5760 | 3.0443 | 0.4032 | 1.3198 | 0.0058 | 2.291 | 0.010 |
| sieve30000 | 30000 | 58.1888 | 2.4842 | 63.1429 | 0.4333 | 14.4611 | 0.2068 | 5.821 | 0.083 |
| matrix48 | 110592 | 73.5668 | 11.3407 | N/A | N/A | 21.5566 | 0.1953 | 1.901 | 0.017 |
| sin20000 | 20000 | 21.9954 | 0.5418 | 0.5414 | 0.5485 | 5.6508 | 0.1503 | 10.430 | 0.277 |
| call200000 | 200000 | 271.3518 | 0.6840 | 203.9657 | 3.5812 | 137.2445 | 3.8289 | 200.650 | 5.598 |
| regex6000 | 6000 | 1.4133 | 1.2220 | N/A | N/A | 2.7320 | 0.0790 | 2.236 | 0.065 |
| str_unicode | 2000 | 4.5261 | 4.4561 | N/A | N/A | 2.2527 | 0.1943 | 0.506 | 0.044 |
| except10000 | 10000 | 5.5121 | 0.0259 | N/A | N/A | 3.4271 | 0.0214 | 132.320 | 0.826 |
| match20000 | 20000 | 47.8576 | 48.4298 | N/A | N/A | 9.7310 | 0.0517 | 0.201 | 0.001 |
| closure20000 | 20000 | 26.7436 | 28.6721 | N/A | N/A | 7.3500 | 0.0281 | 0.256 | 0.001 |
| inherit1000 | 1000 | 2.8135 | 2.7515 | N/A | N/A | 0.9813 | 0.0195 | 0.357 | 0.007 |
| set5000 | 5000 | 7.8659 | 7.8159 | N/A | N/A | 1.8224 | 0.0339 | 0.233 | 0.004 |
| enum10000 | 10000 | 10.6313 | 10.4867 | N/A | N/A | 4.7468 | 0.0124 | 0.453 | 0.001 |
| modimport2000 | 2000 | 0.5828 | 0.0040 | N/A | N/A | 0.3272 | 0.0027 | 81.800 | 0.675 |
| nanbox20000 | 20000 | 17.1423 | 17.2999 | N/A | N/A | 7.4490 | 0.0161 | 0.431 | 0.001 |

## 13. Performance Analysis

- **전반적으로 Un JIT은 Python보다 기하평균 **3.978x 빠르다**.**
- **전반적으로 Un JIT은 Node.js보다 기하평균 **16.541배 느리다**.**
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 826.133x, call200000은 200.650x(Int 함수의 native 직접 호출), nested_sum_200은 940.943x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.360x, call200000 5.598x, nested_sum_200 2.714x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261123-023254\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | -2.0% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | +1.5% | 보통 |
| int_ops | python | +0.9% | 보통 |
| int_ops | node | +0.1% | 보통 |
| fib24 | unjit | -0.1% | 보통 |
| fib24 | compile | -1.7% | 보통 |
| fib24 | build | +0.8% | 보통 |
| fib24 | python | +8.2% | 보통 |
| fib24 | node | -0.9% | 보통 |
| native_fib24 | unjit | -2.3% | 보통 |
| native_fib24 | compile | -2.1% | 보통 |
| native_fib24 | build | +0.8% | 보통 |
| native_fib24 | python | -1.8% | 보통 |
| native_fib24 | node | -0.4% | 보통 |
| nested_sum_200 | unjit | -0.6% | 보통 |
| nested_sum_200 | compile | +12.5% | 회귀 |
| nested_sum_200 | build | -16.6% | 개선 |
| nested_sum_200 | python | +4.3% | 보통 |
| nested_sum_200 | node | -1.7% | 보통 |
| qsort500 | unjit | -3.8% | 보통 |
| qsort500 | compile | -0.7% | 보통 |
| qsort500 | build | -4.3% | 보통 |
| qsort500 | python | +0.7% | 보통 |
| qsort500 | node | +19.1% | 회귀 |
| mergesort1000 | unjit | -2.2% | 보통 |
| mergesort1000 | compile | -0.6% | 보통 |
| mergesort1000 | build | -9.1% | 보통 |
| mergesort1000 | python | +0.6% | 보통 |
| mergesort1000 | node | -13.4% | 개선 |
| rle1994 | unjit | -7.1% | 보통 |
| rle1994 | python | -1.9% | 보통 |
| rle1994 | node | -13.9% | 개선 |
| dict5000 | unjit | -0.6% | 보통 |
| dict5000 | compile | +7.7% | 보통 |
| dict5000 | build | -30.5% | 개선 |
| dict5000 | python | -5.7% | 보통 |
| dict5000 | node | -21.6% | 개선 |
| iter10000 | unjit | -2.8% | 보통 |
| iter10000 | compile | -2.0% | 보통 |
| iter10000 | build | -57.6% | 개선 |
| iter10000 | python | -5.4% | 보통 |
| iter10000 | node | +1.8% | 보통 |
| math10000 | unjit | +0.4% | 보통 |
| math10000 | compile | +0.0% | 보통 |
| math10000 | build | -3.9% | 보통 |
| math10000 | python | -1.9% | 보통 |
| math10000 | node | +0.0% | 보통 |
| class1000 | unjit | +0.0% | 보통 |
| class1000 | compile | -0.3% | 보통 |
| class1000 | build | -12.0% | 개선 |
| class1000 | python | +1.2% | 보통 |
| class1000 | node | +7.4% | 보통 |
| sieve30000 | unjit | -1.4% | 보통 |
| sieve30000 | compile | -1.3% | 보통 |
| sieve30000 | build | +8.2% | 보통 |
| sieve30000 | python | +2.8% | 보통 |
| sieve30000 | node | +0.2% | 보통 |
| matrix48 | unjit | -0.4% | 보통 |
| matrix48 | python | -0.1% | 보통 |
| matrix48 | node | -5.8% | 보통 |
| sin20000 | unjit | +0.5% | 보통 |
| sin20000 | compile | +0.3% | 보통 |
| sin20000 | build | +1.7% | 보통 |
| sin20000 | python | +0.5% | 보통 |
| sin20000 | node | -3.1% | 보통 |
| call200000 | unjit | -0.2% | 보통 |
| call200000 | compile | -12.3% | 개선 |
| call200000 | build | -0.2% | 보통 |
| call200000 | python | -0.7% | 보통 |
| call200000 | node | +0.1% | 보통 |
| regex6000 | unjit | -21.8% | 개선 |
| regex6000 | python | +4.2% | 보통 |
| regex6000 | node | -1.9% | 보통 |
| str_unicode | unjit | +0.5% | 보통 |
| str_unicode | python | +0.4% | 보통 |
| str_unicode | node | +7.6% | 보통 |
| except10000 | unjit | +0.4% | 보통 |
| except10000 | python | -2.4% | 보통 |
| except10000 | node | +0.0% | 보통 |
| match20000 | unjit | -0.2% | 보통 |
| match20000 | python | -2.4% | 보통 |
| match20000 | node | +10.0% | 보통 |
| closure20000 | unjit | +3.2% | 보통 |
| closure20000 | python | -0.3% | 보통 |
| closure20000 | node | +0.0% | 보통 |
| inherit1000 | unjit | -2.7% | 보통 |
| inherit1000 | python | +3.2% | 보통 |
| inherit1000 | node | +22.6% | 회귀 |
| set5000 | unjit | +0.3% | 보통 |
| set5000 | python | +1.8% | 보통 |
| set5000 | node | -3.7% | 보통 |
| enum10000 | unjit | -3.5% | 보통 |
| enum10000 | python | -0.6% | 보통 |
| enum10000 | node | +0.8% | 보통 |
| modimport2000 | unjit | +0.0% | 보통 |
| modimport2000 | python | -0.8% | 보통 |
| modimport2000 | node | +3.8% | 보통 |
| nanbox20000 | unjit | +0.8% | 보통 |
| nanbox20000 | python | +1.9% | 보통 |
| nanbox20000 | node | -1.8% | 보통 |

## 15. Historical Comparison

이전 결과(20261123-023254) 대비 **10% 초과 변동 항목만** 요약한다. 전체 추적은 ench/results/<timestamp>/results.csv와 14절을 참고.

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| nested_sum_200 | compile | +12.5% | 회귀 |
| nested_sum_200 | build | -16.6% | 개선 |
| qsort500 | node | +19.1% | 회귀 |
| mergesort1000 | node | -13.4% | 개선 |
| rle1994 | node | -13.9% | 개선 |
| dict5000 | build | -30.5% | 개선 |
| dict5000 | node | -21.6% | 개선 |
| iter10000 | build | -57.6% | 개선 |
| class1000 | build | -12.0% | 개선 |
| call200000 | compile | -12.3% | 개선 |
| regex6000 | unjit | -21.8% | 개선 |
| inherit1000 | node | +22.6% | 회귀 |

## 16. Benchmark Methodology

측정 명령:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1                 # All lanes
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group unvm -NoSave
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group unjit -NoSave
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group python -NoSave
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group node -NoSave
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Only qsort500,mergesort1000
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Compare bench\results\<이전results.csv>
```

- 워밍업 2회, 이후 reps=7회 개별 측정해 Min/Median/Average/Max/StdDev(모집단)를 낸다.
- 비교 지표는 **중앙값**(중위수). 종합은 **기하평균** ∏ rᵢ^(1/n).
- 비율은 `baseline` ÷ `candidate` 로 정의한다(1.00x=동일, 2.00x=candidate가 2배 빠름).
- 각 스크립트는 측정 전에 자기검증을 수행한다. 실패하면 측정값이 나오지 않고 실행 실패로 집계한다.
- 시간 측정: Un `time.stopwatch().tick()`(100ns 틱, ms환산), Node `performance.now()`, Python `time.perf_counter()`.
- Un은 UV_JIT=0으로 VM만, 기본값으로 JIT(런타임)을 측정한다.

## 17. Known Limitations

- Un에는 힙 메모리 측정 API가 없다(Node/Python은 tracemalloc/heapUsed 기준 참고치).
- Un의 sys.version()/os.version은 실제 호스트와 무관한 고정 문자열을 반환한다(TD-44).
- JIT은 Int 부분집합(IntVec/IntDict/class-method intrinsic)만 native 컴파일하고 나머지는 인터프리터로 정확히 실행한다.
- Python의 시스템 PATH가 broken WindowsApps stub일 수 있다. 이 러너는 경로를 자동 탐지한다.
- Python의 tracemalloc은 Python heap만 잰다. Node의 heapUsed는 V8 heap만 잰다. 둘 다 RSS 비교가 아니다.
- 동시성 모델이 다르므로 동시성 벤치는 측정하지 않는다(TD-39).

