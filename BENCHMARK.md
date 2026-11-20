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

- 측정 일시: 2026-11-21 02:11:16 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|3f678bb
- 측정 커밋: 3f678bb
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **6.610x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **7.459x 느리다**.
- Un VM은 Python보다 기하평균 **3.887x 느리다**.
- Un VM은 Node.js보다 기하평균 **191.674x 느리다**.
- Python은 Node.js보다 기하평균 **49.305x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 6.610x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.134x  | >1 = Un이 빠름 |
| Un VM / Python | 0.257x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.005x  | >1 = Un이 빠름 |
| Python / Node.js | 0.020x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1603.6929 | 2.8903 | 2.8571 | 3.0000 | 2375.1728 | 9.5934 | 821.774 | 3.319 |
| fib24 | 1 | 184.5292 | 1.9360 | 168.7414 | 0.1814 | 3.2095 | 0.2677 | 1.658 | 0.138 |
| native_fib24 | 1 | 184.7977 | 2.0449 | 166.3800 | 0.1771 | 3.2223 | 0.2663 | 1.576 | 0.130 |
| nested_sum_200 | 40000 | 8.5405 | 0.0175 | 0.0129 | 0.0157 | 16.5909 | 0.0467 | 948.051 | 2.669 |
| qsort500 | 500 | 16.1068 | 0.8536 | 20.1429 | 0.7143 | 0.8972 | 0.0807 | 1.051 | 0.095 |
| mergesort1000 | 1000 | 43.2981 | 1.5745 | 45.7143 | 1.4286 | 2.1796 | 0.0673 | 1.384 | 0.043 |
| rle1994 | 1994 | 7.1617 | 7.4478 | N/A | N/A | 2.2746 | 0.0451 | 0.305 | 0.006 |
| dict5000 | 10000 | 3.6319 | 3.8222 | 17.7143 | 12.7143 | 5.4390 | 0.1439 | 1.423 | 0.038 |
| iter10000 | 30000 | 16.9523 | 4.1056 | 33.4286 | 0.7143 | 4.5935 | 0.1861 | 1.119 | 0.045 |
| math10000 | 10000 | 19.3671 | 0.0510 | 0.0257 | 0.0243 | 4.4918 | 0.0627 | 88.075 | 1.229 |
| class1000 | 1000 | 4.6270 | 0.5811 | 3.0743 | 0.8386 | 1.3020 | 0.0056 | 2.241 | 0.010 |
| sieve30000 | 30000 | 58.4268 | 2.4565 | 61.4286 | 2.0000 | 14.1794 | 0.2277 | 5.772 | 0.093 |
| matrix48 | 110592 | 70.6826 | 11.3346 | N/A | N/A | 21.8361 | 0.2218 | 1.926 | 0.020 |
| sin20000 | 20000 | 22.4328 | 0.5391 | 0.5486 | 0.5429 | 5.6117 | 0.1561 | 10.409 | 0.290 |
| call200000 | 200000 | 262.8371 | 0.6840 | 198.2529 | 0.7214 | 137.9320 | 3.8146 | 201.655 | 5.577 |
| regex6000 | 6000 | 1.4619 | 1.3241 | N/A | N/A | 2.6527 | 0.0624 | 2.003 | 0.047 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1603.6929 | 2.8903 | 2.8571 | 3.0000 | 2375.1728 | 9.5934 | 821.774 | 3.319 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 8.5405 | 0.0175 | 0.0129 | 0.0157 | 16.5909 | 0.0467 | 948.051 | 2.669 |
| sieve30000 | 30000 | 58.4268 | 2.4565 | 61.4286 | 2.0000 | 14.1794 | 0.2277 | 5.772 | 0.093 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 262.8371 | 0.6840 | 198.2529 | 0.7214 | 137.9320 | 3.8146 | 201.655 | 5.577 |
| fib24 | 1 | 184.5292 | 1.9360 | 168.7414 | 0.1814 | 3.2095 | 0.2677 | 1.658 | 0.138 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 3.6319 | 3.8222 | 17.7143 | 12.7143 | 5.4390 | 0.1439 | 1.423 | 0.038 |
| class1000 | 1000 | 4.6270 | 0.5811 | 3.0743 | 0.8386 | 1.3020 | 0.0056 | 2.241 | 0.010 |
| rle1994 | 1994 | 7.1617 | 7.4478 | N/A | N/A | 2.2746 | 0.0451 | 0.305 | 0.006 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 16.1068 | 0.8536 | 20.1429 | 0.7143 | 0.8972 | 0.0807 | 1.051 | 0.095 |
| mergesort1000 | 1000 | 43.2981 | 1.5745 | 45.7143 | 1.4286 | 2.1796 | 0.0673 | 1.384 | 0.043 |
| matrix48 | 110592 | 70.6826 | 11.3346 | N/A | N/A | 21.8361 | 0.2218 | 1.926 | 0.020 |
| math10000 | 10000 | 19.3671 | 0.0510 | 0.0257 | 0.0243 | 4.4918 | 0.0627 | 88.075 | 1.229 |
| sin20000 | 20000 | 22.4328 | 0.5391 | 0.5486 | 0.5429 | 5.6117 | 0.1561 | 10.409 | 0.290 |
| regex6000 | 6000 | 1.4619 | 1.3241 | N/A | N/A | 2.6527 | 0.0624 | 2.003 | 0.047 |
| iter10000 | 30000 | 16.9523 | 4.1056 | 33.4286 | 0.7143 | 4.5935 | 0.1861 | 1.119 | 0.045 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.220 | 0.000 |
| fib24 | 0.157 | 0.000 |
| native_fib24 | 0.157 | 0.000 |
| nested_sum_200 | 0.160 | 0.000 |
| qsort500 | -0.441 | 0.000 |
| mergesort1000 | 0.588 | 0.000 |
| rle1994 | 0.529 | 0.000 |
| dict5000 | 0.816 | 0.000 |
| iter10000 | 0.280 | 0.001 |
| math10000 | 0.162 | 0.000 |
| class1000 | -1.658 | 0.000 |
| sieve30000 | 1.178 | 0.000 |
| matrix48 | 0.619 | 0.000 |
| sin20000 | 0.224 | 0.000 |
| call200000 | 0.159 | 0.000 |
| regex6000 | 0.804 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1603.6929 | 2.8903 | 2.8571 | 3.0000 | 2375.1728 | 9.5934 | 821.774 | 3.319 |
| fib24 | 1 | 184.5292 | 1.9360 | 168.7414 | 0.1814 | 3.2095 | 0.2677 | 1.658 | 0.138 |
| native_fib24 | 1 | 184.7977 | 2.0449 | 166.3800 | 0.1771 | 3.2223 | 0.2663 | 1.576 | 0.130 |
| nested_sum_200 | 40000 | 8.5405 | 0.0175 | 0.0129 | 0.0157 | 16.5909 | 0.0467 | 948.051 | 2.669 |
| qsort500 | 500 | 16.1068 | 0.8536 | 20.1429 | 0.7143 | 0.8972 | 0.0807 | 1.051 | 0.095 |
| mergesort1000 | 1000 | 43.2981 | 1.5745 | 45.7143 | 1.4286 | 2.1796 | 0.0673 | 1.384 | 0.043 |
| rle1994 | 1994 | 7.1617 | 7.4478 | N/A | N/A | 2.2746 | 0.0451 | 0.305 | 0.006 |
| dict5000 | 10000 | 3.6319 | 3.8222 | 17.7143 | 12.7143 | 5.4390 | 0.1439 | 1.423 | 0.038 |
| iter10000 | 30000 | 16.9523 | 4.1056 | 33.4286 | 0.7143 | 4.5935 | 0.1861 | 1.119 | 0.045 |
| math10000 | 10000 | 19.3671 | 0.0510 | 0.0257 | 0.0243 | 4.4918 | 0.0627 | 88.075 | 1.229 |
| class1000 | 1000 | 4.6270 | 0.5811 | 3.0743 | 0.8386 | 1.3020 | 0.0056 | 2.241 | 0.010 |
| sieve30000 | 30000 | 58.4268 | 2.4565 | 61.4286 | 2.0000 | 14.1794 | 0.2277 | 5.772 | 0.093 |
| matrix48 | 110592 | 70.6826 | 11.3346 | N/A | N/A | 21.8361 | 0.2218 | 1.926 | 0.020 |
| sin20000 | 20000 | 22.4328 | 0.5391 | 0.5486 | 0.5429 | 5.6117 | 0.1561 | 10.409 | 0.290 |
| call200000 | 200000 | 262.8371 | 0.6840 | 198.2529 | 0.7214 | 137.9320 | 3.8146 | 201.655 | 5.577 |
| regex6000 | 6000 | 1.4619 | 1.3241 | N/A | N/A | 2.6527 | 0.0624 | 2.003 | 0.047 |

## 13. Performance Analysis

- **전반적으로 Un JIT은 Python보다 기하평균 **6.610x 빠르다**.**
- **전반적으로 Un JIT은 Node.js보다 기하평균 **7.459배 느리다**.**
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 821.774x, call200000은 201.655x(Int 함수의 native 직접 호출), nested_sum_200은 948.051x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.319x, call200000 5.577x, nested_sum_200 2.669x (1보다 크면 Un이 빠름).
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

