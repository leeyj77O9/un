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

- 측정 일시: 2026-11-20 18:03:17 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|02ca4fe
- 측정 커밋: 02ca4fe
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **6.695x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **6.881x 느리다**.
- Un VM은 Python보다 기하평균 **3.821x 느리다**.
- Un VM은 Node.js보다 기하평균 **176.019x 느리다**.
- Python은 Node.js보다 기하평균 **46.071x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 6.695x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.145x  | >1 = Un이 빠름 |
| Un VM / Python | 0.262x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.006x  | >1 = Un이 빠름 |
| Python / Node.js | 0.022x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1589.1781 | 2.8450 | 2.8571 | 2.8571 | 2324.1238 | 9.5604 | 816.915 | 3.360 |
| fib24 | 1 | 183.5278 | 1.9360 | 164.7800 | 0.1771 | 3.2230 | 0.3164 | 1.665 | 0.163 |
| native_fib24 | 1 | 183.9516 | 1.9340 | 165.7614 | 0.1771 | 3.3466 | 0.2673 | 1.730 | 0.138 |
| nested_sum_200 | 40000 | 8.3731 | 0.0175 | 0.0114 | 0.0157 | 16.3634 | 0.0487 | 935.051 | 2.783 |
| qsort500 | 500 | 15.4954 | 0.8754 | N/A | N/A | 0.8463 | 0.0925 | 0.967 | 0.106 |
| mergesort1000 | 1000 | 43.2097 | 1.6525 | N/A | N/A | 2.0809 | 0.0885 | 1.259 | 0.054 |
| rle1994 | 1994 | 7.0516 | 7.1148 | N/A | N/A | 2.2953 | 0.0430 | 0.323 | 0.006 |
| dict5000 | 10000 | 3.5805 | 3.6261 | N/A | N/A | 5.3467 | 0.1388 | 1.475 | 0.038 |
| iter10000 | 30000 | 16.8874 | 3.9267 | N/A | N/A | 4.7213 | 0.1808 | 1.202 | 0.046 |
| math10000 | 10000 | 18.5337 | 0.0508 | N/A | N/A | 4.7579 | 0.0626 | 93.659 | 1.232 |
| class1000 | 1000 | 4.3444 | 0.5674 | N/A | N/A | 1.3048 | 0.0072 | 2.300 | 0.013 |
| sieve30000 | 30000 | 57.4243 | 2.4745 | N/A | N/A | 14.5366 | 0.2391 | 5.875 | 0.097 |
| matrix48 | 110592 | 71.6462 | 11.6969 | N/A | N/A | 21.6014 | 0.1948 | 1.847 | 0.017 |
| sin20000 | 20000 | 21.5564 | 0.5389 | N/A | N/A | 5.5990 | 0.1712 | 10.390 | 0.318 |
| call200000 | 200000 | 260.6966 | 0.6836 | N/A | N/A | 137.5123 | 3.8174 | 201.159 | 5.584 |
| regex6000 | 6000 | 1.4597 | 1.2357 | N/A | N/A | 2.6916 | 0.0860 | 2.178 | 0.070 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1589.1781 | 2.8450 | 2.8571 | 2.8571 | 2324.1238 | 9.5604 | 816.915 | 3.360 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 8.3731 | 0.0175 | 0.0114 | 0.0157 | 16.3634 | 0.0487 | 935.051 | 2.783 |
| sieve30000 | 30000 | 57.4243 | 2.4745 | N/A | N/A | 14.5366 | 0.2391 | 5.875 | 0.097 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 260.6966 | 0.6836 | N/A | N/A | 137.5123 | 3.8174 | 201.159 | 5.584 |
| fib24 | 1 | 183.5278 | 1.9360 | 164.7800 | 0.1771 | 3.2230 | 0.3164 | 1.665 | 0.163 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 3.5805 | 3.6261 | N/A | N/A | 5.3467 | 0.1388 | 1.475 | 0.038 |
| class1000 | 1000 | 4.3444 | 0.5674 | N/A | N/A | 1.3048 | 0.0072 | 2.300 | 0.013 |
| rle1994 | 1994 | 7.0516 | 7.1148 | N/A | N/A | 2.2953 | 0.0430 | 0.323 | 0.006 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 15.4954 | 0.8754 | N/A | N/A | 0.8463 | 0.0925 | 0.967 | 0.106 |
| mergesort1000 | 1000 | 43.2097 | 1.6525 | N/A | N/A | 2.0809 | 0.0885 | 1.259 | 0.054 |
| matrix48 | 110592 | 71.6462 | 11.6969 | N/A | N/A | 21.6014 | 0.1948 | 1.847 | 0.017 |
| math10000 | 10000 | 18.5337 | 0.0508 | N/A | N/A | 4.7579 | 0.0626 | 93.659 | 1.232 |
| sin20000 | 20000 | 21.5564 | 0.5389 | N/A | N/A | 5.5990 | 0.1712 | 10.390 | 0.318 |
| regex6000 | 6000 | 1.4597 | 1.2357 | N/A | N/A | 2.6916 | 0.0860 | 2.178 | 0.070 |
| iter10000 | 30000 | 16.8874 | 3.9267 | N/A | N/A | 4.7213 | 0.1808 | 1.202 | 0.046 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.159 | 0.000 |
| fib24 | 0.157 | 0.000 |
| native_fib24 | 0.157 | 0.000 |
| nested_sum_200 | 0.161 | 0.000 |
| qsort500 | -0.437 | 0.000 |
| mergesort1000 | -0.297 | 0.000 |
| rle1994 | 0.476 | 0.000 |
| dict5000 | 0.740 | 0.000 |
| iter10000 | -0.220 | 0.001 |
| math10000 | 0.162 | 0.000 |
| class1000 | -1.542 | 0.000 |
| sieve30000 | 1.181 | 0.000 |
| matrix48 | 0.676 | 0.000 |
| sin20000 | 0.161 | 0.000 |
| call200000 | 0.222 | 0.000 |
| regex6000 | 0.805 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1589.1781 | 2.8450 | 2.8571 | 2.8571 | 2324.1238 | 9.5604 | 816.915 | 3.360 |
| fib24 | 1 | 183.5278 | 1.9360 | 164.7800 | 0.1771 | 3.2230 | 0.3164 | 1.665 | 0.163 |
| native_fib24 | 1 | 183.9516 | 1.9340 | 165.7614 | 0.1771 | 3.3466 | 0.2673 | 1.730 | 0.138 |
| nested_sum_200 | 40000 | 8.3731 | 0.0175 | 0.0114 | 0.0157 | 16.3634 | 0.0487 | 935.051 | 2.783 |
| qsort500 | 500 | 15.4954 | 0.8754 | N/A | N/A | 0.8463 | 0.0925 | 0.967 | 0.106 |
| mergesort1000 | 1000 | 43.2097 | 1.6525 | N/A | N/A | 2.0809 | 0.0885 | 1.259 | 0.054 |
| rle1994 | 1994 | 7.0516 | 7.1148 | N/A | N/A | 2.2953 | 0.0430 | 0.323 | 0.006 |
| dict5000 | 10000 | 3.5805 | 3.6261 | N/A | N/A | 5.3467 | 0.1388 | 1.475 | 0.038 |
| iter10000 | 30000 | 16.8874 | 3.9267 | N/A | N/A | 4.7213 | 0.1808 | 1.202 | 0.046 |
| math10000 | 10000 | 18.5337 | 0.0508 | N/A | N/A | 4.7579 | 0.0626 | 93.659 | 1.232 |
| class1000 | 1000 | 4.3444 | 0.5674 | N/A | N/A | 1.3048 | 0.0072 | 2.300 | 0.013 |
| sieve30000 | 30000 | 57.4243 | 2.4745 | N/A | N/A | 14.5366 | 0.2391 | 5.875 | 0.097 |
| matrix48 | 110592 | 71.6462 | 11.6969 | N/A | N/A | 21.6014 | 0.1948 | 1.847 | 0.017 |
| sin20000 | 20000 | 21.5564 | 0.5389 | N/A | N/A | 5.5990 | 0.1712 | 10.390 | 0.318 |
| call200000 | 200000 | 260.6966 | 0.6836 | N/A | N/A | 137.5123 | 3.8174 | 201.159 | 5.584 |
| regex6000 | 6000 | 1.4597 | 1.2357 | N/A | N/A | 2.6916 | 0.0860 | 2.178 | 0.070 |

## 13. Performance Analysis

- **전반적으로 Un JIT은 Python보다 기하평균 **6.695x 빠르다**.**
- **전반적으로 Un JIT은 Node.js보다 기하평균 **6.881배 느리다**.**
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 816.915x, call200000은 201.159x(Int 함수의 native 직접 호출), nested_sum_200은 935.051x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.360x, call200000 5.584x, nested_sum_200 2.783x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

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

