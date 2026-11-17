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
- ENV|un_commit|c13483a
- 측정 커밋: c13483a
- Un 실행 파일: `rust\target\release\unvm.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **2.393x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **21.556x 느리다**.
- Un VM은 Python보다 기하평균 **4.465x 느리다**.
- Un VM은 Node.js보다 기하평균 **230.361x 느리다**.
- Python은 Node.js보다 기하평균 **51.591x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(Int 부분집합, floor/sqrt·sin intrinsic, IntDict, 클래스 Int 필드 메서드)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 2.393x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.046x  | >1 = Un이 빠름 |
| Un VM / Python | 0.224x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.004x  | >1 = Un이 빠름 |
| Python / Node.js | 0.019x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 3194.2635 | 3.1862 | 2378.8122 | 9.5858 | 746.599 | 3.009 |
| fib24 | 1 | 181.0742 | 1.3214 | 3.1819 | 0.2664 | 2.408 | 0.202 |
| nested_sum_200 | 40000 | 13.4505 | 0.0130 | 16.1893 | 0.0509 | 1245.331 | 3.915 |
| qsort500 | 500 | 15.7085 | 15.6321 | 0.9108 | 0.0860 | 0.058 | 0.006 |
| mergesort1000 | 1000 | 43.1067 | 43.2643 | 2.1430 | 0.0683 | 0.050 | 0.002 |
| rle1994 | 1994 | 11.4743 | 10.9280 | 2.3519 | 0.0487 | 0.215 | 0.004 |
| dict5000 | 10000 | 4.2206 | 3.9148 | 5.6630 | 0.1451 | 1.447 | 0.037 |
| iter10000 | 30000 | 17.4055 | 5.0321 | 4.8395 | 0.2057 | 0.962 | 0.041 |
| math10000 | 10000 | 22.3873 | 0.0162 | 4.4214 | 0.0628 | 272.926 | 3.877 |
| class1000 | 1000 | 5.1491 | 0.7157 | 1.3135 | 0.0063 | 1.835 | 0.009 |
| sieve30000 | 30000 | 53.7873 | 54.4792 | 14.5002 | 0.2841 | 0.266 | 0.005 |
| matrix48 | 110592 | 71.8235 | 71.8682 | 22.4635 | 0.2099 | 0.313 | 0.003 |
| sin20000 | 20000 | 26.6678 | 26.5288 | 5.8995 | 0.1539 | 0.222 | 0.006 |
| call200000 | 200000 | 347.6074 | 0.6993 | 145.0798 | 3.8191 | 207.464 | 5.461 |
| regex6000 | 6000 | 20.3500 | 20.5342 | 2.6731 | 0.0888 | 0.130 | 0.004 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 3194.2635 | 3.1862 | 2378.8122 | 9.5858 | 746.599 | 3.009 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 13.4505 | 0.0130 | 16.1893 | 0.0509 | 1245.331 | 3.915 |
| sieve30000 | 30000 | 53.7873 | 54.4792 | 14.5002 | 0.2841 | 0.266 | 0.005 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 347.6074 | 0.6993 | 145.0798 | 3.8191 | 207.464 | 5.461 |
| fib24 | 1 | 181.0742 | 1.3214 | 3.1819 | 0.2664 | 2.408 | 0.202 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 4.2206 | 3.9148 | 5.6630 | 0.1451 | 1.447 | 0.037 |
| class1000 | 1000 | 5.1491 | 0.7157 | 1.3135 | 0.0063 | 1.835 | 0.009 |
| rle1994 | 1994 | 11.4743 | 10.9280 | 2.3519 | 0.0487 | 0.215 | 0.004 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 15.7085 | 15.6321 | 0.9108 | 0.0860 | 0.058 | 0.006 |
| mergesort1000 | 1000 | 43.1067 | 43.2643 | 2.1430 | 0.0683 | 0.050 | 0.002 |
| matrix48 | 110592 | 71.8235 | 71.8682 | 22.4635 | 0.2099 | 0.313 | 0.003 |
| math10000 | 10000 | 22.3873 | 0.0162 | 4.4214 | 0.0628 | 272.926 | 3.877 |
| sin20000 | 20000 | 26.6678 | 26.5288 | 5.8995 | 0.1539 | 0.222 | 0.006 |
| regex6000 | 6000 | 20.3500 | 20.5342 | 2.6731 | 0.0888 | 0.130 | 0.004 |
| iter10000 | 30000 | 17.4055 | 5.0321 | 4.8395 | 0.2057 | 0.962 | 0.041 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.157 | 0.000 |
| fib24 | 0.219 | 0.000 |
| nested_sum_200 | 0.161 | 0.000 |
| qsort500 | -0.457 | 0.000 |
| mergesort1000 | 0.590 | 0.000 |
| rle1994 | 0.536 | 0.000 |
| dict5000 | 0.544 | 0.000 |
| iter10000 | 0.280 | 0.001 |
| math10000 | 0.162 | 0.000 |
| class1000 | -1.043 | 0.000 |
| sieve30000 | 1.198 | 0.000 |
| matrix48 | 0.629 | 0.000 |
| sin20000 | 0.161 | 0.000 |
| call200000 | 0.159 | 0.000 |
| regex6000 | 1.780 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 3194.2635 | 3.1862 | 2378.8122 | 9.5858 | 746.599 | 3.009 |
| fib24 | 1 | 181.0742 | 1.3214 | 3.1819 | 0.2664 | 2.408 | 0.202 |
| nested_sum_200 | 40000 | 13.4505 | 0.0130 | 16.1893 | 0.0509 | 1245.331 | 3.915 |
| qsort500 | 500 | 15.7085 | 15.6321 | 0.9108 | 0.0860 | 0.058 | 0.006 |
| mergesort1000 | 1000 | 43.1067 | 43.2643 | 2.1430 | 0.0683 | 0.050 | 0.002 |
| rle1994 | 1994 | 11.4743 | 10.9280 | 2.3519 | 0.0487 | 0.215 | 0.004 |
| dict5000 | 10000 | 4.2206 | 3.9148 | 5.6630 | 0.1451 | 1.447 | 0.037 |
| iter10000 | 30000 | 17.4055 | 5.0321 | 4.8395 | 0.2057 | 0.962 | 0.041 |
| math10000 | 10000 | 22.3873 | 0.0162 | 4.4214 | 0.0628 | 272.926 | 3.877 |
| class1000 | 1000 | 5.1491 | 0.7157 | 1.3135 | 0.0063 | 1.835 | 0.009 |
| sieve30000 | 30000 | 53.7873 | 54.4792 | 14.5002 | 0.2841 | 0.266 | 0.005 |
| matrix48 | 110592 | 71.8235 | 71.8682 | 22.4635 | 0.2099 | 0.313 | 0.003 |
| sin20000 | 20000 | 26.6678 | 26.5288 | 5.8995 | 0.1539 | 0.222 | 0.006 |
| call200000 | 200000 | 347.6074 | 0.6993 | 145.0798 | 3.8191 | 207.464 | 5.461 |
| regex6000 | 6000 | 20.3500 | 20.5342 | 2.6731 | 0.0888 | 0.130 | 0.004 |

## 13. Performance Analysis

- **전반적으로 Un JIT은 Python보다 기하평균 **2.393x 빠르다**.**
- **전반적으로 Un JIT은 Node.js보다 기하평균 **21.556배 느리다**.**
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 746.599x, math10000은 272.926x(floor/sqrt intrinsic), call200000은 207.464x(Int 함수의 native 직접 호출), nested_sum_200은 1245.331x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.009x, call200000 5.461x, nested_sum_200 3.915x, math10000 3.877x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, Float-start 누산기 루프(sin20000, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261117-131231\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | +7.9% | 보통 |
| int_ops | python | -1.2% | 보통 |
| int_ops | node | -0.1% | 보통 |
| fib24 | unjit | -0.1% | 보통 |
| fib24 | python | -3.0% | 보통 |
| fib24 | node | -0.4% | 보통 |
| nested_sum_200 | unjit | +0.0% | 보통 |
| nested_sum_200 | python | +0.1% | 보통 |
| nested_sum_200 | node | +8.1% | 보통 |
| qsort500 | unjit | -0.9% | 보통 |
| qsort500 | python | +3.1% | 보통 |
| qsort500 | node | +1.7% | 보통 |
| mergesort1000 | unjit | +2.8% | 보통 |
| mergesort1000 | python | -1.6% | 보통 |
| mergesort1000 | node | +4.6% | 보통 |
| rle1994 | unjit | +4.6% | 보통 |
| rle1994 | python | -1.4% | 보통 |
| rle1994 | node | -12.4% | 개선 |
| dict5000 | unjit | -1.6% | 보통 |
| dict5000 | python | +4.3% | 보통 |
| dict5000 | node | +1.5% | 보통 |
| iter10000 | unjit | +15.7% | 회귀 |
| iter10000 | python | +4.0% | 보통 |
| iter10000 | node | +2.9% | 보통 |
| math10000 | unjit | +2.5% | 보통 |
| math10000 | python | -5.0% | 보통 |
| math10000 | node | +0.0% | 보통 |
| class1000 | unjit | -85.8% | 개선 |
| class1000 | python | -4.4% | 보통 |
| class1000 | node | +5.0% | 보통 |
| sieve30000 | unjit | +1.3% | 보통 |
| sieve30000 | python | +1.6% | 보통 |
| sieve30000 | node | +31.0% | 회귀 |
| matrix48 | unjit | +3.4% | 보통 |
| matrix48 | python | +3.6% | 보통 |
| matrix48 | node | +4.1% | 보통 |
| sin20000 | unjit | -1.5% | 보통 |
| sin20000 | python | +3.5% | 보통 |
| sin20000 | node | -3.5% | 보통 |
| call200000 | unjit | +1.6% | 보통 |
| call200000 | python | +0.5% | 보통 |
| call200000 | node | +0.1% | 보통 |
| regex6000 | unjit | -2.4% | 보통 |
| regex6000 | python | +1.0% | 보통 |
| regex6000 | node | +14.0% | 회귀 |

## 15. Historical Comparison

이전 결과(20261117-131231) 대비 **10% 초과 변동 항목만** 요약한다. 전체 추적은 ench/results/<timestamp>/results.csv와 14절을 참고.

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| rle1994 | node | -12.4% | 개선 |
| iter10000 | unjit | +15.7% | 회귀 |
| class1000 | unjit | -85.8% | 개선 |
| sieve30000 | node | +31.0% | 회귀 |
| regex6000 | node | +14.0% | 회귀 |

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

