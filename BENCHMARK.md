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

- 측정 일시: 2026-11-23 22:43:47 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|64e4b7e
- 측정 커밋: 64e4b7e
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **3.917x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **17.130x 느리다**.
- Un VM은 Python보다 기하평균 **3.114x 느리다**.
- Un VM은 Node.js보다 기하평균 **208.924x 느리다**.
- Python은 Node.js보다 기하평균 **67.100x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 3.917x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.058x  | >1 = Un이 빠름 |
| Un VM / Python | 0.321x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.005x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1295.9978 | 2.8494 | 2.8571 | 2.9134 | 2322.1507 | 9.5588 | 814.961 | 3.355 |
| fib24 | 1 | 182.0878 | 1.9570 | 155.1057 | 0.1806 | 3.1903 | 0.2691 | 1.630 | 0.138 |
| native_fib24 | 1 | 181.0384 | 1.9290 | 155.2343 | 0.1795 | 3.1170 | 0.2669 | 1.616 | 0.138 |
| nested_sum_200 | 40000 | 7.8443 | 0.0185 | 0.0114 | 0.0130 | 16.2605 | 0.0495 | 878.946 | 2.676 |
| qsort500 | 500 | 14.9171 | 0.8625 | 18.7143 | 0.5037 | 0.8619 | 0.0969 | 0.999 | 0.112 |
| mergesort1000 | 1000 | 41.1514 | 1.4951 | 43.0000 | 0.5143 | 2.1682 | 0.0721 | 1.450 | 0.048 |
| rle1994 | 1994 | 6.4705 | 6.3270 | N/A | N/A | 2.4327 | 0.0329 | 0.384 | 0.005 |
| dict5000 | 10000 | 3.1579 | 0.2620 | 3.1429 | 0.1909 | 5.3820 | 0.1417 | 20.542 | 0.541 |
| iter10000 | 30000 | 16.9714 | 17.3930 | 31.1429 | 0.2293 | 4.6216 | 0.2181 | 0.266 | 0.013 |
| math10000 | 10000 | 18.2303 | 0.0283 | 0.0257 | 0.0250 | 4.5510 | 0.0625 | 160.813 | 2.208 |
| class1000 | 1000 | 4.2907 | 0.5807 | 2.8214 | 0.0595 | 1.2962 | 0.0058 | 2.232 | 0.010 |
| sieve30000 | 30000 | 52.7903 | 2.4713 | 57.0000 | 0.2594 | 14.2080 | 0.2432 | 5.749 | 0.098 |
| matrix48 | 110592 | 64.6742 | 11.3378 | N/A | N/A | 21.7886 | 0.2155 | 1.922 | 0.019 |
| sin20000 | 20000 | 20.6915 | 0.5394 | 0.5514 | 0.5444 | 5.7244 | 0.2395 | 10.613 | 0.444 |
| call200000 | 200000 | 248.3692 | 0.6846 | 184.8586 | 0.7549 | 138.6089 | 3.8624 | 202.467 | 5.642 |
| regex6000 | 6000 | 1.3547 | 1.1951 | N/A | N/A | 2.7369 | 0.0754 | 2.290 | 0.063 |
| str_unicode | 2000 | 4.2208 | 4.2575 | N/A | N/A | 2.3526 | 0.1913 | 0.553 | 0.045 |
| except10000 | 10000 | 4.7143 | 0.0268 | N/A | N/A | 3.2664 | 0.0213 | 121.881 | 0.795 |
| match20000 | 20000 | 45.0045 | 46.0963 | N/A | N/A | 9.9006 | 0.0481 | 0.215 | 0.001 |
| closure20000 | 20000 | 24.5741 | 25.3641 | N/A | N/A | 7.4311 | 0.0305 | 0.293 | 0.001 |
| inherit1000 | 1000 | 2.6769 | 2.7313 | N/A | N/A | 0.9401 | 0.0096 | 0.344 | 0.004 |
| set5000 | 5000 | 7.1637 | 7.0113 | N/A | N/A | 1.8496 | 0.0404 | 0.264 | 0.006 |
| enum10000 | 10000 | 9.9750 | 9.7740 | N/A | N/A | 4.7925 | 0.0124 | 0.490 | 0.001 |
| modimport2000 | 2000 | 0.4816 | 0.0050 | N/A | N/A | 0.3253 | 0.0027 | 65.060 | 0.540 |
| nanbox20000 | 20000 | 15.2258 | 15.2473 | N/A | N/A | 7.3920 | 0.0165 | 0.485 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1295.9978 | 2.8494 | 2.8571 | 2.9134 | 2322.1507 | 9.5588 | 814.961 | 3.355 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 7.8443 | 0.0185 | 0.0114 | 0.0130 | 16.2605 | 0.0495 | 878.946 | 2.676 |
| sieve30000 | 30000 | 52.7903 | 2.4713 | 57.0000 | 0.2594 | 14.2080 | 0.2432 | 5.749 | 0.098 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 248.3692 | 0.6846 | 184.8586 | 0.7549 | 138.6089 | 3.8624 | 202.467 | 5.642 |
| fib24 | 1 | 182.0878 | 1.9570 | 155.1057 | 0.1806 | 3.1903 | 0.2691 | 1.630 | 0.138 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 3.1579 | 0.2620 | 3.1429 | 0.1909 | 5.3820 | 0.1417 | 20.542 | 0.541 |
| class1000 | 1000 | 4.2907 | 0.5807 | 2.8214 | 0.0595 | 1.2962 | 0.0058 | 2.232 | 0.010 |
| rle1994 | 1994 | 6.4705 | 6.3270 | N/A | N/A | 2.4327 | 0.0329 | 0.384 | 0.005 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 14.9171 | 0.8625 | 18.7143 | 0.5037 | 0.8619 | 0.0969 | 0.999 | 0.112 |
| mergesort1000 | 1000 | 41.1514 | 1.4951 | 43.0000 | 0.5143 | 2.1682 | 0.0721 | 1.450 | 0.048 |
| matrix48 | 110592 | 64.6742 | 11.3378 | N/A | N/A | 21.7886 | 0.2155 | 1.922 | 0.019 |
| math10000 | 10000 | 18.2303 | 0.0283 | 0.0257 | 0.0250 | 4.5510 | 0.0625 | 160.813 | 2.208 |
| sin20000 | 20000 | 20.6915 | 0.5394 | 0.5514 | 0.5444 | 5.7244 | 0.2395 | 10.613 | 0.444 |
| regex6000 | 6000 | 1.3547 | 1.1951 | N/A | N/A | 2.7369 | 0.0754 | 2.290 | 0.063 |
| iter10000 | 30000 | 16.9714 | 17.3930 | 31.1429 | 0.2293 | 4.6216 | 0.2181 | 0.266 | 0.013 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.871 | 0.000 |
| fib24 | 0.876 | 0.000 |
| native_fib24 | 0.886 | 0.000 |
| nested_sum_200 | 0.895 | 0.000 |
| qsort500 | 0.185 | 0.007 |
| mergesort1000 | 1.227 | 0.015 |
| rle1994 | 1.074 | 0.000 |
| dict5000 | 1.390 | 0.000 |
| iter10000 | 0.813 | 0.001 |
| math10000 | 0.875 | 0.000 |
| class1000 | 0.437 | 0.000 |
| sieve30000 | 1.885 | 0.000 |
| matrix48 | 1.162 | 0.000 |
| sin20000 | 0.758 | 0.000 |
| call200000 | 0.903 | 0.000 |
| regex6000 | 1.358 | 0.000 |
| str_unicode | 1.402 | 0.000 |
| except10000 | 0.505 | 0.000 |
| match20000 | 1.104 | 0.000 |
| closure20000 | 0.738 | 0.000 |
| inherit1000 | -0.107 | 0.000 |
| set5000 | 1.151 | 0.000 |
| enum10000 | 0.504 | 0.000 |
| modimport2000 | 0.342 | 0.000 |
| nanbox20000 | 0.874 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **3.917x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **17.130배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 814.961x, call200000은 202.467x(Int 함수의 native 직접 호출), nested_sum_200은 878.946x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.355x, call200000 5.642x, nested_sum_200 2.676x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261123-215950\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | +0.3% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | +1.4% | 보통 |
| int_ops | python | -3.7% | 보통 |
| int_ops | node | -4.2% | 보통 |
| fib24 | unjit | +1.1% | 보통 |
| fib24 | compile | -11.2% | 개선 |
| fib24 | build | +0.2% | 보통 |
| fib24 | python | +0.3% | 보통 |
| fib24 | node | +0.3% | 보통 |
| native_fib24 | unjit | +0.0% | 보통 |
| native_fib24 | compile | -10.7% | 개선 |
| native_fib24 | build | -0.7% | 보통 |
| native_fib24 | python | -0.5% | 보통 |
| native_fib24 | node | +0.0% | 보통 |
| nested_sum_200 | unjit | -1.1% | 보통 |
| nested_sum_200 | compile | +0.0% | 보통 |
| nested_sum_200 | build | -2.0% | 보통 |
| nested_sum_200 | python | -1.9% | 보통 |
| nested_sum_200 | node | +94.1% | 회귀 |
| qsort500 | unjit | -2.4% | 보통 |
| qsort500 | compile | -9.0% | 보통 |
| qsort500 | build | -3.1% | 보통 |
| qsort500 | python | -8.4% | 보통 |
| qsort500 | node | -4.7% | 보통 |
| mergesort1000 | unjit | -1.3% | 보통 |
| mergesort1000 | compile | -10.7% | 개선 |
| mergesort1000 | build | +0.7% | 보통 |
| mergesort1000 | python | +1.6% | 보통 |
| mergesort1000 | node | +11.1% | 회귀 |
| rle1994 | unjit | -12.3% | 개선 |
| rle1994 | python | +4.4% | 보통 |
| rle1994 | node | -32.3% | 개선 |
| dict5000 | unjit | +0.3% | 보통 |
| dict5000 | compile | -18.5% | 개선 |
| dict5000 | build | -7.0% | 보통 |
| dict5000 | python | -6.6% | 보통 |
| dict5000 | node | -1.5% | 보통 |
| iter10000 | unjit | -7.3% | 보통 |
| iter10000 | compile | -13.1% | 개선 |
| iter10000 | build | -1.7% | 보통 |
| iter10000 | python | -6.6% | 보통 |
| iter10000 | node | +22.5% | 회귀 |
| math10000 | unjit | +5.2% | 보통 |
| math10000 | compile | +0.0% | 보통 |
| math10000 | build | -2.8% | 보통 |
| math10000 | python | -5.2% | 보통 |
| math10000 | node | -0.3% | 보통 |
| class1000 | unjit | -1.1% | 보통 |
| class1000 | compile | -10.5% | 개선 |
| class1000 | build | -3.8% | 보통 |
| class1000 | python | -1.0% | 보통 |
| class1000 | node | -15.9% | 개선 |
| sieve30000 | unjit | -0.4% | 보통 |
| sieve30000 | compile | -13.1% | 개선 |
| sieve30000 | build | +0.4% | 보통 |
| sieve30000 | python | -0.5% | 보통 |
| sieve30000 | node | +7.1% | 보통 |
| matrix48 | unjit | -2.9% | 보통 |
| matrix48 | python | -2.6% | 보통 |
| matrix48 | node | -20.5% | 개선 |
| sin20000 | unjit | +0.0% | 보통 |
| sin20000 | compile | -0.3% | 보통 |
| sin20000 | build | -1.8% | 보통 |
| sin20000 | python | +1.1% | 보통 |
| sin20000 | node | +57.0% | 회귀 |
| call200000 | unjit | -0.5% | 보통 |
| call200000 | compile | -13.2% | 개선 |
| call200000 | build | -4.1% | 보통 |
| call200000 | python | -6.2% | 보통 |
| call200000 | node | -1.0% | 보통 |
| regex6000 | unjit | -20.9% | 개선 |
| regex6000 | python | -12.1% | 개선 |
| regex6000 | node | -29.1% | 개선 |
| str_unicode | unjit | -9.2% | 보통 |
| str_unicode | python | -9.8% | 보통 |
| str_unicode | node | +5.0% | 보통 |
| except10000 | unjit | -0.4% | 보통 |
| except10000 | python | -3.7% | 보통 |
| except10000 | node | -16.8% | 개선 |
| match20000 | unjit | -15.5% | 개선 |
| match20000 | python | -4.3% | 보통 |
| match20000 | node | +0.2% | 보통 |
| closure20000 | unjit | -12.6% | 개선 |
| closure20000 | python | -4.9% | 보통 |
| closure20000 | node | +8.2% | 보통 |
| inherit1000 | unjit | -10.6% | 개선 |
| inherit1000 | python | -3.0% | 보통 |
| inherit1000 | node | -19.3% | 개선 |
| set5000 | unjit | -10.9% | 개선 |
| set5000 | python | +0.6% | 보통 |
| set5000 | node | -24.3% | 개선 |
| enum10000 | unjit | -12.2% | 개선 |
| enum10000 | python | -3.8% | 보통 |
| enum10000 | node | +0.0% | 보통 |
| modimport2000 | unjit | -2.0% | 보통 |
| modimport2000 | python | -0.1% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | -15.0% | 개선 |
| nanbox20000 | python | -2.1% | 보통 |
| nanbox20000 | node | +0.0% | 보통 |

## 15. Historical Comparison

이전 결과(20261123-215950) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

