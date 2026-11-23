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

- 측정 일시: 2026-11-24 08:53:21 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|c460fbe
- 측정 커밋: c460fbe
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **4.154x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **16.096x 느리다**.
- Un VM은 Python보다 기하평균 **2.623x 느리다**.
- Un VM은 Node.js보다 기하평균 **175.387x 느리다**.
- Python은 Node.js보다 기하평균 **66.867x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 4.154x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.062x  | >1 = Un이 빠름 |
| Un VM / Python | 0.381x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.006x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 997.3195 | 2.9905 | 2.8571 | 2.9493 | 2393.9356 | 9.5908 | 800.513 | 3.207 |
| fib24 | 1 | 180.8664 | 1.9667 | 145.7443 | 0.1797 | 3.1135 | 0.2686 | 1.583 | 0.137 |
| native_fib24 | 1 | 161.2246 | 1.9623 | 144.4814 | 0.1823 | 3.1663 | 0.2679 | 1.614 | 0.137 |
| nested_sum_200 | 40000 | 7.2013 | 0.0185 | 0.0114 | 0.0136 | 16.3719 | 0.0476 | 884.968 | 2.573 |
| qsort500 | 500 | 14.0834 | 0.8553 | 16.1429 | 0.5563 | 0.8532 | 0.1014 | 0.998 | 0.119 |
| mergesort1000 | 1000 | 35.9254 | 1.5394 | 37.2857 | 0.5036 | 2.2127 | 0.0864 | 1.437 | 0.056 |
| rle1994 | 1994 | 5.3749 | 5.4103 | N/A | N/A | 2.2840 | 0.0469 | 0.422 | 0.009 |
| dict5000 | 10000 | 2.4346 | 0.2602 | 2.4286 | 0.1759 | 5.5840 | 0.1466 | 21.460 | 0.563 |
| iter10000 | 30000 | 15.3685 | 16.8436 | 28.1429 | 0.2304 | 4.8249 | 0.1828 | 0.286 | 0.011 |
| math10000 | 10000 | 16.6305 | 0.0265 | 0.0257 | 0.0257 | 4.4603 | 0.0626 | 168.313 | 2.362 |
| class1000 | 1000 | 3.3029 | 0.5756 | 2.1743 | 0.0624 | 1.2892 | 0.0057 | 2.240 | 0.010 |
| sieve30000 | 30000 | 44.2598 | 2.5026 | 46.0000 | 0.2589 | 14.2474 | 0.2004 | 5.693 | 0.080 |
| matrix48 | 110592 | 50.2660 | 11.5883 | N/A | N/A | 22.1898 | 0.2020 | 1.915 | 0.017 |
| sin20000 | 20000 | 18.6733 | 0.5386 | 0.5486 | 0.5390 | 5.6631 | 0.1724 | 10.514 | 0.320 |
| call200000 | 200000 | 220.8057 | 0.6843 | 163.8329 | 0.7466 | 137.6608 | 3.8240 | 201.170 | 5.588 |
| regex6000 | 6000 | 1.4180 | 1.2003 | N/A | N/A | 2.8947 | 0.0901 | 2.412 | 0.075 |
| str_unicode | 2000 | 3.9908 | 4.2094 | N/A | N/A | 2.4036 | 0.1812 | 0.571 | 0.043 |
| except10000 | 10000 | 3.4017 | 0.0268 | N/A | N/A | 3.2404 | 0.0213 | 120.910 | 0.795 |
| match20000 | 20000 | 31.2055 | 33.1037 | N/A | N/A | 10.0783 | 0.0657 | 0.304 | 0.002 |
| closure20000 | 20000 | 25.5003 | 32.5743 | N/A | N/A | 7.3133 | 0.0287 | 0.225 | 0.001 |
| inherit1000 | 1000 | 2.1535 | 2.0289 | N/A | N/A | 0.9539 | 0.0118 | 0.470 | 0.006 |
| set5000 | 5000 | 5.2787 | 5.3096 | N/A | N/A | 1.8267 | 0.0346 | 0.344 | 0.007 |
| enum10000 | 10000 | 7.6962 | 7.6166 | N/A | N/A | 4.7104 | 0.0124 | 0.618 | 0.002 |
| modimport2000 | 2000 | 0.3781 | 0.0049 | N/A | N/A | 0.3431 | 0.0027 | 70.020 | 0.551 |
| nanbox20000 | 20000 | 11.8576 | 11.8191 | N/A | N/A | 7.3495 | 0.0166 | 0.622 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 997.3195 | 2.9905 | 2.8571 | 2.9493 | 2393.9356 | 9.5908 | 800.513 | 3.207 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 7.2013 | 0.0185 | 0.0114 | 0.0136 | 16.3719 | 0.0476 | 884.968 | 2.573 |
| sieve30000 | 30000 | 44.2598 | 2.5026 | 46.0000 | 0.2589 | 14.2474 | 0.2004 | 5.693 | 0.080 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 220.8057 | 0.6843 | 163.8329 | 0.7466 | 137.6608 | 3.8240 | 201.170 | 5.588 |
| fib24 | 1 | 180.8664 | 1.9667 | 145.7443 | 0.1797 | 3.1135 | 0.2686 | 1.583 | 0.137 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 2.4346 | 0.2602 | 2.4286 | 0.1759 | 5.5840 | 0.1466 | 21.460 | 0.563 |
| class1000 | 1000 | 3.3029 | 0.5756 | 2.1743 | 0.0624 | 1.2892 | 0.0057 | 2.240 | 0.010 |
| rle1994 | 1994 | 5.3749 | 5.4103 | N/A | N/A | 2.2840 | 0.0469 | 0.422 | 0.009 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 14.0834 | 0.8553 | 16.1429 | 0.5563 | 0.8532 | 0.1014 | 0.998 | 0.119 |
| mergesort1000 | 1000 | 35.9254 | 1.5394 | 37.2857 | 0.5036 | 2.2127 | 0.0864 | 1.437 | 0.056 |
| matrix48 | 110592 | 50.2660 | 11.5883 | N/A | N/A | 22.1898 | 0.2020 | 1.915 | 0.017 |
| math10000 | 10000 | 16.6305 | 0.0265 | 0.0257 | 0.0257 | 4.4603 | 0.0626 | 168.313 | 2.362 |
| sin20000 | 20000 | 18.6733 | 0.5386 | 0.5486 | 0.5390 | 5.6631 | 0.1724 | 10.514 | 0.320 |
| regex6000 | 6000 | 1.4180 | 1.2003 | N/A | N/A | 2.8947 | 0.0901 | 2.412 | 0.075 |
| iter10000 | 30000 | 15.3685 | 16.8436 | 28.1429 | 0.2304 | 4.8249 | 0.1828 | 0.286 | 0.011 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.622 | 0.000 |
| fib24 | 0.625 | 0.000 |
| native_fib24 | 0.731 | 0.000 |
| nested_sum_200 | 0.751 | 0.000 |
| qsort500 | 0.152 | 0.007 |
| mergesort1000 | 0.262 | 0.015 |
| rle1994 | 1.141 | 0.000 |
| dict5000 | 1.153 | 0.000 |
| iter10000 | 0.832 | 0.001 |
| math10000 | 0.887 | 0.000 |
| class1000 | -0.702 | 0.000 |
| sieve30000 | 1.884 | 0.000 |
| matrix48 | 1.227 | 0.000 |
| sin20000 | 0.183 | 0.000 |
| call200000 | 0.748 | 0.000 |
| regex6000 | 0.104 | 0.000 |
| str_unicode | 1.389 | 0.000 |
| except10000 | 0.746 | 0.000 |
| match20000 | 1.212 | 0.000 |
| closure20000 | 0.756 | 0.000 |
| inherit1000 | -0.280 | 0.000 |
| set5000 | 1.154 | 0.000 |
| enum10000 | 0.735 | 0.000 |
| modimport2000 | 0.413 | 0.000 |
| nanbox20000 | 0.729 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **4.154x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **16.096배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 800.513x, call200000은 201.170x(Int 함수의 native 직접 호출), nested_sum_200은 884.968x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.207x, call200000 5.588x, nested_sum_200 2.573x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261124-031236\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | +4.0% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | -2.0% | 보통 |
| int_ops | python | -0.7% | 보통 |
| int_ops | node | -3.1% | 보통 |
| fib24 | unjit | +1.7% | 보통 |
| fib24 | compile | -4.2% | 보통 |
| fib24 | build | -2.2% | 보통 |
| fib24 | python | -1.9% | 보통 |
| fib24 | node | -10.6% | 개선 |
| native_fib24 | unjit | +1.7% | 보통 |
| native_fib24 | compile | -4.1% | 보통 |
| native_fib24 | build | +2.5% | 보통 |
| native_fib24 | python | -13.4% | 개선 |
| native_fib24 | node | -0.5% | 보통 |
| nested_sum_200 | unjit | -2.1% | 보통 |
| nested_sum_200 | compile | +0.0% | 보통 |
| nested_sum_200 | build | -9.1% | 보통 |
| nested_sum_200 | python | +4.1% | 보통 |
| nested_sum_200 | node | +69.4% | 회귀 |
| qsort500 | unjit | +0.8% | 보통 |
| qsort500 | compile | -7.4% | 보통 |
| qsort500 | build | +9.1% | 보통 |
| qsort500 | python | -2.0% | 보통 |
| qsort500 | node | +20.7% | 회귀 |
| mergesort1000 | unjit | +0.8% | 보통 |
| mergesort1000 | compile | -13.3% | 개선 |
| mergesort1000 | build | -7.1% | 보통 |
| mergesort1000 | python | +3.9% | 보통 |
| mergesort1000 | node | +29.5% | 회귀 |
| rle1994 | unjit | -9.6% | 보통 |
| rle1994 | python | -16.9% | 개선 |
| rle1994 | node | +4.2% | 보통 |
| dict5000 | unjit | -0.6% | 보통 |
| dict5000 | compile | -22.7% | 개선 |
| dict5000 | build | -1.6% | 보통 |
| dict5000 | python | +1.5% | 보통 |
| dict5000 | node | +4.0% | 보통 |
| iter10000 | unjit | +1.7% | 보통 |
| iter10000 | compile | -9.2% | 보통 |
| iter10000 | build | -2.7% | 보통 |
| iter10000 | python | +0.0% | 보통 |
| iter10000 | node | -10.5% | 개선 |
| math10000 | unjit | -7.0% | 보통 |
| math10000 | compile | +0.0% | 보통 |
| math10000 | build | -1.3% | 보통 |
| math10000 | python | -7.5% | 보통 |
| math10000 | node | -0.2% | 보통 |
| class1000 | unjit | -0.5% | 보통 |
| class1000 | compile | -24.9% | 개선 |
| class1000 | build | +1.4% | 보통 |
| class1000 | python | +0.9% | 보통 |
| class1000 | node | -6.6% | 보통 |
| sieve30000 | unjit | -2.6% | 보통 |
| sieve30000 | compile | -14.1% | 개선 |
| sieve30000 | build | -14.2% | 개선 |
| sieve30000 | python | -5.6% | 보통 |
| sieve30000 | node | -15.1% | 개선 |
| matrix48 | unjit | -2.5% | 보통 |
| matrix48 | python | -1.7% | 보통 |
| matrix48 | node | -2.0% | 보통 |
| sin20000 | unjit | -0.8% | 보통 |
| sin20000 | compile | -1.5% | 보통 |
| sin20000 | build | -3.8% | 보통 |
| sin20000 | python | -4.4% | 보통 |
| sin20000 | node | +10.4% | 회귀 |
| call200000 | unjit | -0.1% | 보통 |
| call200000 | compile | -11.8% | 개선 |
| call200000 | build | -3.0% | 보통 |
| call200000 | python | -2.2% | 보통 |
| call200000 | node | -1.4% | 보통 |
| regex6000 | unjit | -0.2% | 보통 |
| regex6000 | python | +3.7% | 보통 |
| regex6000 | node | +7.0% | 보통 |
| str_unicode | unjit | -1.5% | 보통 |
| str_unicode | python | +2.5% | 보통 |
| str_unicode | node | -4.3% | 보통 |
| except10000 | unjit | +0.4% | 보통 |
| except10000 | python | -6.3% | 보통 |
| except10000 | node | -0.5% | 보통 |
| match20000 | unjit | -27.6% | 개선 |
| match20000 | python | -2.9% | 보통 |
| match20000 | node | +24.2% | 회귀 |
| closure20000 | unjit | +25.3% | 회귀 |
| closure20000 | python | -2.5% | 보통 |
| closure20000 | node | +0.7% | 보통 |
| inherit1000 | unjit | -26.5% | 개선 |
| inherit1000 | python | -1.1% | 보통 |
| inherit1000 | node | -3.3% | 보통 |
| set5000 | unjit | -27.0% | 개선 |
| set5000 | python | +3.7% | 보통 |
| set5000 | node | +3.6% | 보통 |
| enum10000 | unjit | -18.9% | 개선 |
| enum10000 | python | -2.6% | 보통 |
| enum10000 | node | +0.8% | 보통 |
| modimport2000 | unjit | -2.0% | 보통 |
| modimport2000 | python | +3.5% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | -21.1% | 개선 |
| nanbox20000 | python | -3.7% | 보통 |
| nanbox20000 | node | +3.1% | 보통 |

## 15. Historical Comparison

이전 결과(20261124-031236) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

