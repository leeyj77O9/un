# BENCHMARK.md

Un 벤치마크 결과입니다. 실행: `scripts\bench.ps1 -Group all -Report` (Windows)

## 1. Benchmark Overview

Un(Rust 바이트코드 컴파일러 + 레지스터 VM + Cranelift JIT)을 CPython 3.13과 Node.js 26과
비교하기 위한 벤치마크다. 각 테스트는 bench/<name>.un, bench/node/<name>.mjs,
bench/python/<name>.py의 세 파일이 **같은 알고리즘, 같은 입력, 같은 검증**을 공유한다.

| 노드 | 내용 |
|---|---|
| 측정 언어 | Un VM (UV_JIT=0), Un JIT(기본), CPython, Node.js |
| 측정 방식 | 워밍업 2회 → reps=7회 개별 측정, Median 비교 |
| 저장 위치 | bench/results/<timestamp>/results.csv, results.json, results.md |
| 보고서 | 이 문서(BENCHMARK.md)은 -Report로 자동 생성된다 |
| 비교 지표 | 중앙값, 종합은 기하평균(∏ rᵢ^(1/n)) |

## 2. Environment

- ENV|un|os=Microsoft Windows NT 10.0.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|e61a6d8
- 측정 커밋: e61a6d8
- Un 실행 파일: `rust\target\release\unvm.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **4.540x 느리다**.
- Un JIT은 Node.js보다 기하평균 **223.335x 느리다**.
- Un VM은 Python보다 기하평균 **4.467x 느리다**.
- Un VM은 Node.js보다 기하평균 **219.764x 느리다**.
- Python은 Node.js보다 기하평균 **49.194x 느리다**.
- VM과 JIT의 차이가 거의 없는 것은 현재 JIT이 Int 부분집합(floor/sqrt, sin, IntDict, 클래스 Int 필드)만
  native 컴파일하기 때문이며 인터프리터 fallback이 지배적이라는 뜻이다(아래 9/12절 참고).

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 0.220x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.004x  | >1 = Un이 빠름 |
| Un VM / Python | 0.224x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.005x  | >1 = Un이 빠름 |
| Python / Node.js | 0.020x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 3254.4617 | 3236.5490 | 2331.7106 | 9.5656 | 0.720 | 0.003 |
| fib24 | 1 | 172.4702 | 175.9579 | 3.1274 | 0.2673 | 0.018 | 0.002 |
| nested_sum_200 | 40000 | 14.2097 | 14.5450 | 16.1510 | 0.0465 | 1.110 | 0.003 |
| qsort500 | 500 | 15.4988 | 15.6515 | 0.8725 | 0.0816 | 0.056 | 0.005 |
| mergesort1000 | 1000 | 42.2743 | 42.1043 | 2.1846 | 0.0875 | 0.052 | 0.002 |
| rle1994 | 1994 | 10.5742 | 10.5136 | 2.3450 | 0.0466 | 0.223 | 0.004 |
| dict5000 | 10000 | 4.0121 | 4.2664 | 5.8343 | 0.1523 | 1.367 | 0.036 |
| iter10000 | 30000 | 16.8227 | 17.4242 | 4.7034 | 0.2081 | 0.270 | 0.012 |
| math10000 | 10000 | 22.4880 | 22.8916 | 4.4518 | 0.0627 | 0.194 | 0.003 |
| class1000 | 1000 | 4.9995 | 5.3244 | 1.3070 | 0.0098 | 0.245 | 0.002 |
| sieve30000 | 30000 | 55.3885 | 54.8928 | 14.2084 | 0.2212 | 0.259 | 0.004 |
| matrix48 | 110592 | 70.7123 | 71.9863 | 21.7137 | 0.2467 | 0.302 | 0.003 |
| sin20000 | 20000 | 27.4858 | 27.4842 | 5.6211 | 0.1591 | 0.205 | 0.006 |
| call200000 | 200000 | 351.3461 | 361.8851 | 138.8861 | 4.0139 | 0.384 | 0.011 |
| regex6000 | 6000 | 19.2901 | 19.0640 | 2.6833 | 0.0846 | 0.141 | 0.004 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 3254.4617 | 3236.5490 | 2331.7106 | 9.5656 | 0.720 | 0.003 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 14.2097 | 14.5450 | 16.1510 | 0.0465 | 1.110 | 0.003 |
| sieve30000 | 30000 | 55.3885 | 54.8928 | 14.2084 | 0.2212 | 0.259 | 0.004 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 351.3461 | 361.8851 | 138.8861 | 4.0139 | 0.384 | 0.011 |
| fib24 | 1 | 172.4702 | 175.9579 | 3.1274 | 0.2673 | 0.018 | 0.002 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 4.0121 | 4.2664 | 5.8343 | 0.1523 | 1.367 | 0.036 |
| class1000 | 1000 | 4.9995 | 5.3244 | 1.3070 | 0.0098 | 0.245 | 0.002 |
| rle1994 | 1994 | 10.5742 | 10.5136 | 2.3450 | 0.0466 | 0.223 | 0.004 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 15.4988 | 15.6515 | 0.8725 | 0.0816 | 0.056 | 0.005 |
| mergesort1000 | 1000 | 42.2743 | 42.1043 | 2.1846 | 0.0875 | 0.052 | 0.002 |
| matrix48 | 110592 | 70.7123 | 71.9863 | 21.7137 | 0.2467 | 0.302 | 0.003 |
| math10000 | 10000 | 22.4880 | 22.8916 | 4.4518 | 0.0627 | 0.194 | 0.003 |
| sin20000 | 20000 | 27.4858 | 27.4842 | 5.6211 | 0.1591 | 0.205 | 0.006 |
| regex6000 | 6000 | 19.2901 | 19.0640 | 2.6833 | 0.0846 | 0.141 | 0.004 |
| iter10000 | 30000 | 16.8227 | 17.4242 | 4.7034 | 0.2081 | 0.270 | 0.012 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.157 | 0.000 |
| fib24 | 0.157 | 0.000 |
| nested_sum_200 | 0.160 | 0.000 |
| qsort500 | -0.460 | 0.000 |
| mergesort1000 | -0.305 | 0.000 |
| rle1994 | 0.465 | 0.000 |
| dict5000 | 0.740 | 0.000 |
| iter10000 | 0.280 | 0.001 |
| math10000 | 0.162 | 0.000 |
| class1000 | -0.796 | 0.000 |
| sieve30000 | 1.216 | 0.000 |
| matrix48 | 0.623 | 0.000 |
| sin20000 | 0.161 | 0.000 |
| call200000 | 0.159 | 0.000 |
| regex6000 | 0.805 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 3254.4617 | 3236.5490 | 2331.7106 | 9.5656 | 0.720 | 0.003 |
| fib24 | 1 | 172.4702 | 175.9579 | 3.1274 | 0.2673 | 0.018 | 0.002 |
| nested_sum_200 | 40000 | 14.2097 | 14.5450 | 16.1510 | 0.0465 | 1.110 | 0.003 |
| qsort500 | 500 | 15.4988 | 15.6515 | 0.8725 | 0.0816 | 0.056 | 0.005 |
| mergesort1000 | 1000 | 42.2743 | 42.1043 | 2.1846 | 0.0875 | 0.052 | 0.002 |
| rle1994 | 1994 | 10.5742 | 10.5136 | 2.3450 | 0.0466 | 0.223 | 0.004 |
| dict5000 | 10000 | 4.0121 | 4.2664 | 5.8343 | 0.1523 | 1.367 | 0.036 |
| iter10000 | 30000 | 16.8227 | 17.4242 | 4.7034 | 0.2081 | 0.270 | 0.012 |
| math10000 | 10000 | 22.4880 | 22.8916 | 4.4518 | 0.0627 | 0.194 | 0.003 |
| class1000 | 1000 | 4.9995 | 5.3244 | 1.3070 | 0.0098 | 0.245 | 0.002 |
| sieve30000 | 30000 | 55.3885 | 54.8928 | 14.2084 | 0.2212 | 0.259 | 0.004 |
| matrix48 | 110592 | 70.7123 | 71.9863 | 21.7137 | 0.2467 | 0.302 | 0.003 |
| sin20000 | 20000 | 27.4858 | 27.4842 | 5.6211 | 0.1591 | 0.205 | 0.006 |
| call200000 | 200000 | 351.3461 | 361.8851 | 138.8861 | 4.0139 | 0.384 | 0.011 |
| regex6000 | 6000 | 19.2901 | 19.0640 | 2.6833 | 0.0846 | 0.141 | 0.004 |

## 13. Performance Analysis

- **전반적으로 Un이 Python보다 기하평균 4.540배 느리다.**
- **전반적으로 Un이 Node.js보다 기하평균 223.335배 느리다.**
- Un JIT ≈ Un VM인 이유: 현재 JIT 범위는 Int subset(뺄셈/나눗셈은 상수 제수만)과 한정된 intrinsic만이라 대부분이 인터프리터로 실행된다. JIT native 컴파일 범위를 넓히면 int loop, Map/클래스 필드 접근 등에서 직접적인 이득이 예상된다.
- Un이 Python을 이기는 지점: nested_sum_200, dict5000 — IntDict 가속/제제곱 나눗셈이 직접 효과를 내는 구간이다.
- Un이 가장 크게 지는 지점: call200000 — 인터프리터 호출/생성 바인딩 비용이 지배적이다.
- Python이 강한 지점: 촘촘한 정수/비트 루프(int_ops), 짧은 재귀(fib24).
- Node가 강하고 Un이 약한 지점: 문자열/정규식/객체 메서드 다발(Quick Sort, regex).

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261117-094751\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | +0.0% | 보통 |
| int_ops | python | -0.6% | 보통 |
| int_ops | node | -5.4% | 보통 |
| fib24 | unjit | -0.3% | 보통 |
| fib24 | python | -1.5% | 보통 |
| fib24 | node | +0.4% | 보통 |
| nested_sum_200 | unjit | +3.8% | 보통 |
| nested_sum_200 | python | +0.5% | 보통 |
| nested_sum_200 | node | -8.5% | 보통 |
| qsort500 | unjit | -1.2% | 보통 |
| qsort500 | python | +1.6% | 보통 |
| qsort500 | node | -6.4% | 보통 |
| mergesort1000 | unjit | +0.6% | 보통 |
| mergesort1000 | python | -6.4% | 보통 |
| mergesort1000 | node | +22.9% | 회귀 |
| rle1994 | unjit | -1.7% | 보통 |
| rle1994 | python | +2.4% | 보통 |
| rle1994 | node | +37.5% | 회귀 |
| dict5000 | unjit | +4.2% | 보통 |
| dict5000 | python | +1.7% | 보통 |
| dict5000 | node | +4.7% | 보통 |
| iter10000 | unjit | -0.5% | 보통 |
| iter10000 | python | +2.7% | 보통 |
| iter10000 | node | -18.4% | 개선 |
| math10000 | unjit | -0.4% | 보통 |
| math10000 | python | -5.4% | 보통 |
| math10000 | node | +0.3% | 보통 |
| class1000 | unjit | +2.1% | 보통 |
| class1000 | python | -1.8% | 보통 |
| class1000 | node | -43.0% | 개선 |
| sieve30000 | unjit | -0.2% | 보통 |
| sieve30000 | python | +1.1% | 보통 |
| sieve30000 | node | -11.0% | 개선 |
| matrix48 | unjit | +1.4% | 보통 |
| matrix48 | python | +0.6% | 보통 |
| matrix48 | node | +20.8% | 회귀 |
| sin20000 | unjit | -0.1% | 보통 |
| sin20000 | python | -0.3% | 보통 |
| sin20000 | node | +2.2% | 보통 |
| call200000 | unjit | -0.7% | 보통 |
| call200000 | python | -0.1% | 보통 |
| call200000 | node | +5.2% | 보통 |
| regex6000 | unjit | -2.0% | 보통 |
| regex6000 | python | +1.8% | 보통 |
| regex6000 | node | +11.5% | 회귀 |

## 15. Historical Comparison

이전 결과(20261117-094751) 대비 **10% 초과 변동 항목만** 요약한다. 전체 추적은 ench/results/<timestamp>/results.csv와 14절을 참고.

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| mergesort1000 | node | +22.9% | 회귀 |
| rle1994 | node | +37.5% | 회귀 |
| iter10000 | node | -18.4% | 개선 |
| class1000 | node | -43.0% | 개선 |
| sieve30000 | node | -11.0% | 개선 |
| matrix48 | node | +20.8% | 회귀 |
| regex6000 | node | +11.5% | 회귀 |

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
- JIT은 Int 부분집합(floor/sqrt, sin intrinsic, IntDict, 클래스 Int 필드 메서드)만 native 컴파일하고 나머지는 인터프리터로 정확히 실행한다.
- Python의 시스템 PATH가 broken WindowsApps stub일 수 있다. 이 러너는 경로를 자동 탐지한다.
- Python의 tracemalloc은 Python heap만 잰다. Node의 heapUsed는 V8 heap만 잰다. 둘 다 RSS 비교가 아니다.
- 동시성 모델이 다르므로 동시성 벤치는 측정하지 않는다(TD-39).

