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

- 측정 일시: 2026-11-23 03:42:46 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|e09a397
- 측정 커밋: e09a397
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **4.092x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **17.068x 느리다**.
- Un VM은 Python보다 기하평균 **3.292x 느리다**.
- Un VM은 Node.js보다 기하평균 **229.929x 느리다**.
- Python은 Node.js보다 기하평균 **69.850x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 4.092x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.059x  | >1 = Un이 빠름 |
| Un VM / Python | 0.304x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.004x  | >1 = Un이 빠름 |
| Python / Node.js | 0.014x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1623.0232 | 2.8472 | 3.0000 | 2.9973 | 2356.0419 | 9.5752 | 827.494 | 3.363 |
| fib24 | 1 | 187.3876 | 1.9379 | 166.7043 | 1.3813 | 3.1695 | 0.2709 | 1.636 | 0.140 |
| native_fib24 | 1 | 191.9972 | 1.9368 | 174.3786 | 1.3678 | 3.2035 | 0.2722 | 1.654 | 0.141 |
| nested_sum_200 | 40000 | 8.7444 | 0.0176 | 0.0114 | 0.0127 | 16.3337 | 0.0467 | 928.051 | 2.653 |
| qsort500 | 500 | 15.8681 | 0.8654 | 20.2857 | 0.5359 | 0.8560 | 0.0871 | 0.989 | 0.101 |
| mergesort1000 | 1000 | 43.3812 | 1.5054 | 47.4286 | 0.5857 | 2.1022 | 0.0889 | 1.396 | 0.059 |
| rle1994 | 1994 | 7.0191 | 7.1530 | N/A | N/A | 2.4489 | 0.0490 | 0.342 | 0.007 |
| dict5000 | 10000 | 3.7418 | 0.2617 | 3.8571 | 0.3799 | 5.3194 | 0.1423 | 20.326 | 0.544 |
| iter10000 | 30000 | 16.9783 | 4.1186 | 35.0000 | 0.2519 | 4.8762 | 0.2307 | 1.184 | 0.056 |
| math10000 | 10000 | 18.5035 | 0.0520 | 0.0257 | 0.0252 | 4.4765 | 0.0628 | 86.087 | 1.208 |
| class1000 | 1000 | 4.5089 | 0.5692 | 3.0271 | 0.3531 | 1.3067 | 0.0065 | 2.296 | 0.011 |
| sieve30000 | 30000 | 58.9005 | 2.4767 | 65.1429 | 0.4071 | 14.0983 | 0.2287 | 5.692 | 0.092 |
| matrix48 | 110592 | 72.9298 | 11.3109 | N/A | N/A | 21.8974 | 0.2021 | 1.936 | 0.018 |
| sin20000 | 20000 | 22.1537 | 0.5398 | 0.5429 | 0.5436 | 5.6997 | 0.1561 | 10.559 | 0.289 |
| call200000 | 200000 | 270.2584 | 0.6915 | 227.7071 | 1.8482 | 203.9391 | 4.2141 | 294.923 | 6.094 |
| regex6000 | 6000 | 2.3654 | 2.2300 | N/A | N/A | 5.0408 | 0.0988 | 2.260 | 0.044 |
| str_unicode | 2000 | 7.8869 | 8.8709 | N/A | N/A | 4.4273 | 0.4296 | 0.499 | 0.048 |
| except10000 | 10000 | 8.0783 | 0.0260 | N/A | N/A | 4.6633 | 0.0224 | 179.358 | 0.862 |
| match20000 | 20000 | 51.8576 | 51.8793 | N/A | N/A | 11.3589 | 0.0506 | 0.219 | 0.001 |
| closure20000 | 20000 | 28.1464 | 38.8059 | N/A | N/A | 8.4865 | 0.0283 | 0.219 | 0.001 |
| inherit1000 | 1000 | 2.8301 | 2.8721 | N/A | N/A | 0.9639 | 0.0101 | 0.336 | 0.004 |
| set5000 | 5000 | 7.5777 | 7.6186 | N/A | N/A | 1.8505 | 0.0441 | 0.243 | 0.006 |
| enum10000 | 10000 | 10.9660 | 11.7640 | N/A | N/A | 4.9891 | 0.0124 | 0.424 | 0.001 |
| modimport2000 | 2000 | 0.5822 | 0.0039 | N/A | N/A | 0.3322 | 0.0027 | 85.179 | 0.692 |
| nanbox20000 | 20000 | 18.0300 | 17.4853 | N/A | N/A | 7.6171 | 0.0161 | 0.436 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1623.0232 | 2.8472 | 3.0000 | 2.9973 | 2356.0419 | 9.5752 | 827.494 | 3.363 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 8.7444 | 0.0176 | 0.0114 | 0.0127 | 16.3337 | 0.0467 | 928.051 | 2.653 |
| sieve30000 | 30000 | 58.9005 | 2.4767 | 65.1429 | 0.4071 | 14.0983 | 0.2287 | 5.692 | 0.092 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 270.2584 | 0.6915 | 227.7071 | 1.8482 | 203.9391 | 4.2141 | 294.923 | 6.094 |
| fib24 | 1 | 187.3876 | 1.9379 | 166.7043 | 1.3813 | 3.1695 | 0.2709 | 1.636 | 0.140 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 3.7418 | 0.2617 | 3.8571 | 0.3799 | 5.3194 | 0.1423 | 20.326 | 0.544 |
| class1000 | 1000 | 4.5089 | 0.5692 | 3.0271 | 0.3531 | 1.3067 | 0.0065 | 2.296 | 0.011 |
| rle1994 | 1994 | 7.0191 | 7.1530 | N/A | N/A | 2.4489 | 0.0490 | 0.342 | 0.007 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 15.8681 | 0.8654 | 20.2857 | 0.5359 | 0.8560 | 0.0871 | 0.989 | 0.101 |
| mergesort1000 | 1000 | 43.3812 | 1.5054 | 47.4286 | 0.5857 | 2.1022 | 0.0889 | 1.396 | 0.059 |
| matrix48 | 110592 | 72.9298 | 11.3109 | N/A | N/A | 21.8974 | 0.2021 | 1.936 | 0.018 |
| math10000 | 10000 | 18.5035 | 0.0520 | 0.0257 | 0.0252 | 4.4765 | 0.0628 | 86.087 | 1.208 |
| sin20000 | 20000 | 22.1537 | 0.5398 | 0.5429 | 0.5436 | 5.6997 | 0.1561 | 10.559 | 0.289 |
| regex6000 | 6000 | 2.3654 | 2.2300 | N/A | N/A | 5.0408 | 0.0988 | 2.260 | 0.044 |
| iter10000 | 30000 | 16.9783 | 4.1186 | 35.0000 | 0.2519 | 4.8762 | 0.2307 | 1.184 | 0.056 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.728 | 0.000 |
| fib24 | 0.731 | 0.000 |
| native_fib24 | 0.876 | 0.000 |
| nested_sum_200 | 0.243 | 0.000 |
| qsort500 | 0.159 | 0.007 |
| mergesort1000 | 0.229 | 0.015 |
| rle1994 | 0.518 | 0.000 |
| dict5000 | 1.307 | 0.000 |
| iter10000 | 0.833 | 0.001 |
| math10000 | 0.887 | 0.000 |
| class1000 | 0.191 | 0.000 |
| sieve30000 | 1.885 | 0.000 |
| matrix48 | 1.164 | 0.000 |
| sin20000 | 0.896 | 0.000 |
| call200000 | 0.897 | 0.000 |
| regex6000 | 1.404 | 0.000 |
| str_unicode | 1.429 | 0.000 |
| except10000 | 0.734 | 0.000 |
| match20000 | 1.173 | 0.000 |
| closure20000 | 0.756 | 0.000 |
| inherit1000 | -0.254 | 0.000 |
| set5000 | 1.151 | 0.000 |
| enum10000 | 0.878 | 0.000 |
| modimport2000 | 0.155 | 0.000 |
| nanbox20000 | 0.731 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1623.0232 | 2.8472 | 3.0000 | 2.9973 | 2356.0419 | 9.5752 | 827.494 | 3.363 |
| fib24 | 1 | 187.3876 | 1.9379 | 166.7043 | 1.3813 | 3.1695 | 0.2709 | 1.636 | 0.140 |
| native_fib24 | 1 | 191.9972 | 1.9368 | 174.3786 | 1.3678 | 3.2035 | 0.2722 | 1.654 | 0.141 |
| nested_sum_200 | 40000 | 8.7444 | 0.0176 | 0.0114 | 0.0127 | 16.3337 | 0.0467 | 928.051 | 2.653 |
| qsort500 | 500 | 15.8681 | 0.8654 | 20.2857 | 0.5359 | 0.8560 | 0.0871 | 0.989 | 0.101 |
| mergesort1000 | 1000 | 43.3812 | 1.5054 | 47.4286 | 0.5857 | 2.1022 | 0.0889 | 1.396 | 0.059 |
| rle1994 | 1994 | 7.0191 | 7.1530 | N/A | N/A | 2.4489 | 0.0490 | 0.342 | 0.007 |
| dict5000 | 10000 | 3.7418 | 0.2617 | 3.8571 | 0.3799 | 5.3194 | 0.1423 | 20.326 | 0.544 |
| iter10000 | 30000 | 16.9783 | 4.1186 | 35.0000 | 0.2519 | 4.8762 | 0.2307 | 1.184 | 0.056 |
| math10000 | 10000 | 18.5035 | 0.0520 | 0.0257 | 0.0252 | 4.4765 | 0.0628 | 86.087 | 1.208 |
| class1000 | 1000 | 4.5089 | 0.5692 | 3.0271 | 0.3531 | 1.3067 | 0.0065 | 2.296 | 0.011 |
| sieve30000 | 30000 | 58.9005 | 2.4767 | 65.1429 | 0.4071 | 14.0983 | 0.2287 | 5.692 | 0.092 |
| matrix48 | 110592 | 72.9298 | 11.3109 | N/A | N/A | 21.8974 | 0.2021 | 1.936 | 0.018 |
| sin20000 | 20000 | 22.1537 | 0.5398 | 0.5429 | 0.5436 | 5.6997 | 0.1561 | 10.559 | 0.289 |
| call200000 | 200000 | 270.2584 | 0.6915 | 227.7071 | 1.8482 | 203.9391 | 4.2141 | 294.923 | 6.094 |
| regex6000 | 6000 | 2.3654 | 2.2300 | N/A | N/A | 5.0408 | 0.0988 | 2.260 | 0.044 |
| str_unicode | 2000 | 7.8869 | 8.8709 | N/A | N/A | 4.4273 | 0.4296 | 0.499 | 0.048 |
| except10000 | 10000 | 8.0783 | 0.0260 | N/A | N/A | 4.6633 | 0.0224 | 179.358 | 0.862 |
| match20000 | 20000 | 51.8576 | 51.8793 | N/A | N/A | 11.3589 | 0.0506 | 0.219 | 0.001 |
| closure20000 | 20000 | 28.1464 | 38.8059 | N/A | N/A | 8.4865 | 0.0283 | 0.219 | 0.001 |
| inherit1000 | 1000 | 2.8301 | 2.8721 | N/A | N/A | 0.9639 | 0.0101 | 0.336 | 0.004 |
| set5000 | 5000 | 7.5777 | 7.6186 | N/A | N/A | 1.8505 | 0.0441 | 0.243 | 0.006 |
| enum10000 | 10000 | 10.9660 | 11.7640 | N/A | N/A | 4.9891 | 0.0124 | 0.424 | 0.001 |
| modimport2000 | 2000 | 0.5822 | 0.0039 | N/A | N/A | 0.3322 | 0.0027 | 85.179 | 0.692 |
| nanbox20000 | 20000 | 18.0300 | 17.4853 | N/A | N/A | 7.6171 | 0.0161 | 0.436 | 0.001 |

## 13. Performance Analysis

- **전반적으로 Un JIT은 Python보다 기하평균 **4.092x 빠르다**.**
- **전반적으로 Un JIT은 Node.js보다 기하평균 **17.068배 느리다**.**
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 827.494x, call200000은 294.923x(Int 함수의 native 직접 호출), nested_sum_200은 928.051x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.363x, call200000 6.094x, nested_sum_200 2.653x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261123-031934\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | +0.0% | 보통 |
| int_ops | compile | +5.0% | 보통 |
| int_ops | build | +3.3% | 보통 |
| int_ops | python | -0.4% | 보통 |
| int_ops | node | -3.3% | 보통 |
| fib24 | unjit | -0.6% | 보통 |
| fib24 | compile | -4.2% | 보통 |
| fib24 | build | -0.1% | 보통 |
| fib24 | python | +0.0% | 보통 |
| fib24 | node | +1.1% | 보통 |
| native_fib24 | unjit | -0.2% | 보통 |
| native_fib24 | compile | +3.8% | 보통 |
| native_fib24 | build | +0.6% | 보통 |
| native_fib24 | python | -5.7% | 보통 |
| native_fib24 | node | +1.8% | 보통 |
| nested_sum_200 | unjit | +0.0% | 보통 |
| nested_sum_200 | compile | +0.0% | 보통 |
| nested_sum_200 | build | -0.7% | 보통 |
| nested_sum_200 | python | +3.6% | 보통 |
| nested_sum_200 | node | +0.9% | 보통 |
| qsort500 | unjit | +1.1% | 보통 |
| qsort500 | compile | +0.7% | 보통 |
| qsort500 | build | +3.4% | 보통 |
| qsort500 | python | -2.0% | 보통 |
| qsort500 | node | -10.1% | 개선 |
| mergesort1000 | unjit | -1.8% | 보통 |
| mergesort1000 | compile | +4.4% | 보통 |
| mergesort1000 | build | +2.8% | 보통 |
| mergesort1000 | python | +0.0% | 보통 |
| mergesort1000 | node | +26.8% | 회귀 |
| rle1994 | unjit | +1.1% | 보통 |
| rle1994 | python | +0.2% | 보통 |
| rle1994 | node | +44.5% | 회귀 |
| dict5000 | unjit | +1.2% | 보통 |
| dict5000 | compile | +0.0% | 보통 |
| dict5000 | build | +43.2% | 회귀 |
| dict5000 | python | -1.0% | 보통 |
| dict5000 | node | -0.3% | 보통 |
| iter10000 | unjit | +3.8% | 보통 |
| iter10000 | compile | +0.8% | 보통 |
| iter10000 | build | -3.0% | 보통 |
| iter10000 | python | +2.1% | 보통 |
| iter10000 | node | +33.9% | 회귀 |
| math10000 | unjit | -0.6% | 보통 |
| math10000 | compile | +5.9% | 보통 |
| math10000 | build | +3.8% | 보통 |
| math10000 | python | -0.3% | 보통 |
| math10000 | node | +0.0% | 보통 |
| class1000 | unjit | -0.2% | 보통 |
| class1000 | compile | -0.9% | 보통 |
| class1000 | build | -0.6% | 보통 |
| class1000 | python | +0.1% | 보통 |
| class1000 | node | -27.8% | 개선 |
| sieve30000 | unjit | -0.1% | 보통 |
| sieve30000 | compile | +4.1% | 보통 |
| sieve30000 | build | +2.7% | 보통 |
| sieve30000 | python | -0.5% | 보통 |
| sieve30000 | node | +2.4% | 보통 |
| matrix48 | unjit | -0.8% | 보통 |
| matrix48 | python | +1.7% | 보통 |
| matrix48 | node | +5.5% | 보통 |
| sin20000 | unjit | -1.5% | 보통 |
| sin20000 | compile | +0.5% | 보통 |
| sin20000 | build | +0.7% | 보통 |
| sin20000 | python | -0.6% | 보통 |
| sin20000 | node | -2.6% | 보통 |
| call200000 | unjit | +1.1% | 보통 |
| call200000 | compile | -3.2% | 보통 |
| call200000 | build | +1.7% | 보통 |
| call200000 | python | +49.6% | 회귀 |
| call200000 | node | +10.4% | 회귀 |
| regex6000 | unjit | +85.7% | 회귀 |
| regex6000 | python | +91.3% | 회귀 |
| regex6000 | node | +22.6% | 회귀 |
| str_unicode | unjit | +59.7% | 회귀 |
| str_unicode | python | +75.7% | 회귀 |
| str_unicode | node | +134.5% | 회귀 |
| except10000 | unjit | +0.8% | 보통 |
| except10000 | python | +41.6% | 회귀 |
| except10000 | node | +5.2% | 보통 |
| match20000 | unjit | +4.6% | 보통 |
| match20000 | python | +14.9% | 회귀 |
| match20000 | node | +6.1% | 보통 |
| closure20000 | unjit | +39.0% | 회귀 |
| closure20000 | python | +15.3% | 회귀 |
| closure20000 | node | +0.7% | 보통 |
| inherit1000 | unjit | -0.7% | 보통 |
| inherit1000 | python | -1.5% | 보통 |
| inherit1000 | node | -32.7% | 개선 |
| set5000 | unjit | -9.0% | 보통 |
| set5000 | python | -29.3% | 개선 |
| set5000 | node | -37.0% | 개선 |
| enum10000 | unjit | +9.8% | 보통 |
| enum10000 | python | +6.9% | 보통 |
| enum10000 | node | -0.8% | 보통 |
| modimport2000 | unjit | +0.0% | 보통 |
| modimport2000 | python | -1.2% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | +2.0% | 보통 |
| nanbox20000 | python | +3.8% | 보통 |
| nanbox20000 | node | -1.2% | 보통 |

## 15. Historical Comparison

이전 결과(20261123-031934) 대비 **10% 초과 변동 항목만** 요약한다. 전체 추적은 ench/results/<timestamp>/results.csv와 14절을 참고.

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| qsort500 | node | -10.1% | 개선 |
| mergesort1000 | node | +26.8% | 회귀 |
| rle1994 | node | +44.5% | 회귀 |
| dict5000 | build | +43.2% | 회귀 |
| iter10000 | node | +33.9% | 회귀 |
| class1000 | node | -27.8% | 개선 |
| call200000 | python | +49.6% | 회귀 |
| call200000 | node | +10.4% | 회귀 |
| regex6000 | unjit | +85.7% | 회귀 |
| regex6000 | python | +91.3% | 회귀 |
| regex6000 | node | +22.6% | 회귀 |
| str_unicode | unjit | +59.7% | 회귀 |
| str_unicode | python | +75.7% | 회귀 |
| str_unicode | node | +134.5% | 회귀 |
| except10000 | python | +41.6% | 회귀 |
| match20000 | python | +14.9% | 회귀 |
| closure20000 | unjit | +39.0% | 회귀 |
| closure20000 | python | +15.3% | 회귀 |
| inherit1000 | node | -32.7% | 개선 |
| set5000 | python | -29.3% | 개선 |
| set5000 | node | -37.0% | 개선 |

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

