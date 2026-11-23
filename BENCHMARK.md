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

- 측정 일시: 2026-11-24 00:47:12 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|d8bd29f
- 측정 커밋: d8bd29f
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **3.882x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **17.634x 느리다**.
- Un VM은 Python보다 기하평균 **3.054x 느리다**.
- Un VM은 Node.js보다 기하평균 **209.071x 느리다**.
- Python은 Node.js보다 기하평균 **68.449x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 3.882x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.057x  | >1 = Un이 빠름 |
| Un VM / Python | 0.327x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.005x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1317.9881 | 2.8390 | 2.8571 | 2.8654 | 2378.0961 | 9.5756 | 837.653 | 3.373 |
| fib24 | 1 | 174.0251 | 1.9310 | 153.2771 | 0.1785 | 3.1535 | 0.2666 | 1.633 | 0.138 |
| native_fib24 | 1 | 167.7405 | 1.9349 | 152.6171 | 0.1795 | 3.1842 | 0.2674 | 1.646 | 0.138 |
| nested_sum_200 | 40000 | 8.0246 | 0.0187 | 0.0114 | 0.0131 | 16.3865 | 0.0231 | 876.283 | 1.235 |
| qsort500 | 500 | 14.8474 | 0.8847 | 18.7143 | 0.5254 | 0.8620 | 0.0918 | 0.974 | 0.104 |
| mergesort1000 | 1000 | 40.4314 | 1.5041 | 44.7143 | 0.5239 | 2.2424 | 0.1099 | 1.491 | 0.073 |
| rle1994 | 1994 | 6.3102 | 6.5895 | N/A | N/A | 2.3081 | 0.0490 | 0.350 | 0.007 |
| dict5000 | 10000 | 3.2235 | 0.2608 | 3.2857 | 0.1956 | 5.5048 | 0.1492 | 21.107 | 0.572 |
| iter10000 | 30000 | 16.0222 | 16.8135 | 31.0000 | 0.2319 | 4.9658 | 0.2268 | 0.295 | 0.013 |
| math10000 | 10000 | 18.6304 | 0.0272 | 0.0243 | 0.0249 | 4.4230 | 0.0627 | 162.610 | 2.305 |
| class1000 | 1000 | 4.4783 | 0.5665 | 2.8943 | 0.0631 | 1.2882 | 0.0068 | 2.274 | 0.012 |
| sieve30000 | 30000 | 49.9186 | 2.4766 | 56.1429 | 0.2660 | 14.7444 | 0.2172 | 5.953 | 0.088 |
| matrix48 | 110592 | 62.8321 | 11.3456 | N/A | N/A | 21.6087 | 0.1964 | 1.905 | 0.017 |
| sin20000 | 20000 | 20.5175 | 0.5400 | 0.5457 | 0.5439 | 5.6897 | 0.1598 | 10.536 | 0.296 |
| call200000 | 200000 | 248.4862 | 0.6855 | 193.8814 | 0.7655 | 142.4633 | 3.8231 | 207.824 | 5.577 |
| regex6000 | 6000 | 1.2420 | 1.3732 | N/A | N/A | 2.6865 | 0.0889 | 1.956 | 0.065 |
| str_unicode | 2000 | 4.2610 | 5.4021 | N/A | N/A | 2.4851 | 0.1873 | 0.460 | 0.035 |
| except10000 | 10000 | 4.4565 | 0.0269 | N/A | N/A | 3.2901 | 0.0213 | 122.309 | 0.792 |
| match20000 | 20000 | 45.4177 | 45.5365 | N/A | N/A | 10.4953 | 0.0485 | 0.230 | 0.001 |
| closure20000 | 20000 | 26.9531 | 28.6077 | N/A | N/A | 7.8449 | 0.0285 | 0.274 | 0.001 |
| inherit1000 | 1000 | 2.6899 | 2.8325 | N/A | N/A | 0.9462 | 0.0126 | 0.334 | 0.004 |
| set5000 | 5000 | 7.1457 | 7.2284 | N/A | N/A | 1.8288 | 0.0354 | 0.253 | 0.005 |
| enum10000 | 10000 | 10.2189 | 10.0145 | N/A | N/A | 4.6329 | 0.0124 | 0.463 | 0.001 |
| modimport2000 | 2000 | 0.4850 | 0.0050 | N/A | N/A | 0.3266 | 0.0027 | 65.320 | 0.540 |
| nanbox20000 | 20000 | 15.9518 | 15.3843 | N/A | N/A | 8.0077 | 0.0162 | 0.521 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1317.9881 | 2.8390 | 2.8571 | 2.8654 | 2378.0961 | 9.5756 | 837.653 | 3.373 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 8.0246 | 0.0187 | 0.0114 | 0.0131 | 16.3865 | 0.0231 | 876.283 | 1.235 |
| sieve30000 | 30000 | 49.9186 | 2.4766 | 56.1429 | 0.2660 | 14.7444 | 0.2172 | 5.953 | 0.088 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 248.4862 | 0.6855 | 193.8814 | 0.7655 | 142.4633 | 3.8231 | 207.824 | 5.577 |
| fib24 | 1 | 174.0251 | 1.9310 | 153.2771 | 0.1785 | 3.1535 | 0.2666 | 1.633 | 0.138 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 3.2235 | 0.2608 | 3.2857 | 0.1956 | 5.5048 | 0.1492 | 21.107 | 0.572 |
| class1000 | 1000 | 4.4783 | 0.5665 | 2.8943 | 0.0631 | 1.2882 | 0.0068 | 2.274 | 0.012 |
| rle1994 | 1994 | 6.3102 | 6.5895 | N/A | N/A | 2.3081 | 0.0490 | 0.350 | 0.007 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 14.8474 | 0.8847 | 18.7143 | 0.5254 | 0.8620 | 0.0918 | 0.974 | 0.104 |
| mergesort1000 | 1000 | 40.4314 | 1.5041 | 44.7143 | 0.5239 | 2.2424 | 0.1099 | 1.491 | 0.073 |
| matrix48 | 110592 | 62.8321 | 11.3456 | N/A | N/A | 21.6087 | 0.1964 | 1.905 | 0.017 |
| math10000 | 10000 | 18.6304 | 0.0272 | 0.0243 | 0.0249 | 4.4230 | 0.0627 | 162.610 | 2.305 |
| sin20000 | 20000 | 20.5175 | 0.5400 | 0.5457 | 0.5439 | 5.6897 | 0.1598 | 10.536 | 0.296 |
| regex6000 | 6000 | 1.2420 | 1.3732 | N/A | N/A | 2.6865 | 0.0889 | 1.956 | 0.065 |
| iter10000 | 30000 | 16.0222 | 16.8135 | 31.0000 | 0.2319 | 4.9658 | 0.2268 | 0.295 | 0.013 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.726 | 0.000 |
| fib24 | 0.876 | 0.000 |
| native_fib24 | 0.876 | 0.000 |
| nested_sum_200 | 0.760 | 0.000 |
| qsort500 | 0.139 | 0.007 |
| mergesort1000 | 0.237 | 0.015 |
| rle1994 | 1.054 | 0.000 |
| dict5000 | 1.495 | 0.000 |
| iter10000 | 0.735 | 0.001 |
| math10000 | 0.875 | 0.000 |
| class1000 | 0.293 | 0.000 |
| sieve30000 | 1.884 | 0.000 |
| matrix48 | 1.252 | 0.000 |
| sin20000 | 0.896 | 0.000 |
| call200000 | 0.978 | 0.000 |
| regex6000 | 2.218 | 0.000 |
| str_unicode | 1.443 | 0.000 |
| except10000 | 0.732 | 0.000 |
| match20000 | 1.119 | 0.000 |
| closure20000 | 0.034 | 0.000 |
| inherit1000 | -0.126 | 0.000 |
| set5000 | 1.151 | 0.000 |
| enum10000 | 0.742 | 0.000 |
| modimport2000 | 0.363 | 0.000 |
| nanbox20000 | 0.731 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **3.882x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **17.634배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 837.653x, call200000은 207.824x(Int 함수의 native 직접 호출), nested_sum_200은 876.283x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.373x, call200000 5.577x, nested_sum_200 1.235x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261123-223654\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | -0.4% | 보통 |
| int_ops | compile | +0.0% | 보통 |
| int_ops | build | -1.6% | 보통 |
| int_ops | python | +2.4% | 보통 |
| int_ops | node | +0.2% | 보통 |
| fib24 | unjit | -1.3% | 보통 |
| fib24 | compile | -1.2% | 보통 |
| fib24 | build | -1.1% | 보통 |
| fib24 | python | -1.2% | 보통 |
| fib24 | node | -0.9% | 보통 |
| native_fib24 | unjit | +0.3% | 보통 |
| native_fib24 | compile | -1.7% | 보통 |
| native_fib24 | build | +0.0% | 보통 |
| native_fib24 | python | +2.2% | 보통 |
| native_fib24 | node | +0.2% | 보통 |
| nested_sum_200 | unjit | +1.1% | 보통 |
| nested_sum_200 | compile | +0.0% | 보통 |
| nested_sum_200 | build | +1.0% | 보통 |
| nested_sum_200 | python | +0.8% | 보통 |
| nested_sum_200 | node | -53.3% | 개선 |
| qsort500 | unjit | +2.6% | 보통 |
| qsort500 | compile | +0.0% | 보통 |
| qsort500 | build | +4.3% | 보통 |
| qsort500 | python | +0.0% | 보통 |
| qsort500 | node | -5.3% | 보통 |
| mergesort1000 | unjit | +0.6% | 보통 |
| mergesort1000 | compile | +4.0% | 보통 |
| mergesort1000 | build | +1.9% | 보통 |
| mergesort1000 | python | +3.4% | 보통 |
| mergesort1000 | node | +52.4% | 회귀 |
| rle1994 | unjit | +4.1% | 보통 |
| rle1994 | python | -5.1% | 보통 |
| rle1994 | node | +48.9% | 회귀 |
| dict5000 | unjit | -0.5% | 보통 |
| dict5000 | compile | +4.5% | 보통 |
| dict5000 | build | +2.5% | 보통 |
| dict5000 | python | +2.3% | 보통 |
| dict5000 | node | +5.3% | 보통 |
| iter10000 | unjit | -3.3% | 보통 |
| iter10000 | compile | -0.5% | 보통 |
| iter10000 | build | +1.1% | 보통 |
| iter10000 | python | +7.4% | 보통 |
| iter10000 | node | +4.0% | 보통 |
| math10000 | unjit | -3.9% | 보통 |
| math10000 | compile | -5.6% | 보통 |
| math10000 | build | -0.4% | 보통 |
| math10000 | python | -2.8% | 보통 |
| math10000 | node | +0.3% | 보통 |
| class1000 | unjit | -2.4% | 보통 |
| class1000 | compile | +2.6% | 보통 |
| class1000 | build | +6.0% | 보통 |
| class1000 | python | -0.6% | 보통 |
| class1000 | node | +17.2% | 회귀 |
| sieve30000 | unjit | +0.2% | 보통 |
| sieve30000 | compile | -1.5% | 보통 |
| sieve30000 | build | +2.5% | 보통 |
| sieve30000 | python | +3.8% | 보통 |
| sieve30000 | node | -10.7% | 개선 |
| matrix48 | unjit | +0.1% | 보통 |
| matrix48 | python | -0.8% | 보통 |
| matrix48 | node | -8.9% | 보통 |
| sin20000 | unjit | +0.1% | 보통 |
| sin20000 | compile | -1.0% | 보통 |
| sin20000 | build | -0.1% | 보통 |
| sin20000 | python | -0.6% | 보통 |
| sin20000 | node | -33.3% | 개선 |
| call200000 | unjit | +0.1% | 보통 |
| call200000 | compile | +4.9% | 보통 |
| call200000 | build | +1.4% | 보통 |
| call200000 | python | +2.8% | 보통 |
| call200000 | node | -1.0% | 보통 |
| regex6000 | unjit | +14.9% | 회귀 |
| regex6000 | python | -1.8% | 보통 |
| regex6000 | node | +17.9% | 회귀 |
| str_unicode | unjit | +26.9% | 회귀 |
| str_unicode | python | +5.6% | 보통 |
| str_unicode | node | -2.1% | 보통 |
| except10000 | unjit | +0.4% | 보통 |
| except10000 | python | +0.7% | 보통 |
| except10000 | node | +0.0% | 보통 |
| match20000 | unjit | -1.2% | 보통 |
| match20000 | python | +6.0% | 보통 |
| match20000 | node | +0.8% | 보통 |
| closure20000 | unjit | +12.8% | 회귀 |
| closure20000 | python | +5.6% | 보통 |
| closure20000 | node | -6.6% | 보통 |
| inherit1000 | unjit | +3.7% | 보통 |
| inherit1000 | python | +0.6% | 보통 |
| inherit1000 | node | +31.3% | 회귀 |
| set5000 | unjit | +3.1% | 보통 |
| set5000 | python | -1.1% | 보통 |
| set5000 | node | -12.4% | 개선 |
| enum10000 | unjit | +2.5% | 보통 |
| enum10000 | python | -3.3% | 보통 |
| enum10000 | node | +0.0% | 보통 |
| modimport2000 | unjit | +0.0% | 보통 |
| modimport2000 | python | +0.4% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | +0.9% | 보통 |
| nanbox20000 | python | +8.3% | 보통 |
| nanbox20000 | node | -1.8% | 보통 |

## 15. Historical Comparison

이전 결과(20261123-223654) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

