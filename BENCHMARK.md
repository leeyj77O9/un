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

- 측정 일시: 2026-11-24 12:32:35 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|b45ebd5
- 측정 커밋: b45ebd5
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **4.439x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **15.566x 느리다**.
- Un VM은 Python보다 기하평균 **2.345x 느리다**.
- Un VM은 Node.js보다 기하평균 **162.027x 느리다**.
- Python은 Node.js보다 기하평균 **69.092x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 4.439x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.064x  | >1 = Un이 빠름 |
| Un VM / Python | 0.426x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.006x  | >1 = Un이 빠름 |
| Python / Node.js | 0.014x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 993.1807 | 2.8684 | 3.0000 | 2.9967 | 2371.7320 | 9.6934 | 826.848 | 3.379 |
| fib24 | 1 | 131.9892 | 1.9368 | 124.9586 | 0.2116 | 3.2816 | 0.2668 | 1.694 | 0.138 |
| native_fib24 | 1 | 142.1812 | 1.9289 | 126.0886 | 0.2107 | 3.1531 | 0.2663 | 1.635 | 0.138 |
| nested_sum_200 | 40000 | 6.9636 | 0.0181 | 0.0129 | 0.0116 | 16.0059 | 0.0304 | 884.304 | 1.680 |
| qsort500 | 500 | 11.7586 | 0.8653 | 14.1429 | 0.5217 | 0.9114 | 0.0857 | 1.053 | 0.099 |
| mergesort1000 | 1000 | 29.6896 | 1.5064 | 31.5714 | 0.5360 | 2.1769 | 0.0672 | 1.445 | 0.045 |
| rle1994 | 1994 | 5.4573 | 5.3981 | N/A | N/A | 2.4543 | 0.0438 | 0.455 | 0.008 |
| dict5000 | 10000 | 2.4443 | 0.2601 | 2.4286 | 0.1781 | 5.5498 | 0.1419 | 21.337 | 0.546 |
| iter10000 | 30000 | 13.9233 | 14.6479 | 25.0000 | 0.2311 | 4.5671 | 0.1815 | 0.312 | 0.012 |
| math10000 | 10000 | 13.5624 | 0.0263 | 0.0257 | 0.0256 | 4.7712 | 0.0626 | 181.414 | 2.380 |
| class1000 | 1000 | 2.6290 | 0.5774 | 1.9214 | 0.0606 | 1.3461 | 0.0076 | 2.331 | 0.013 |
| sieve30000 | 30000 | 37.7375 | 2.4744 | 42.1429 | 0.2654 | 14.3093 | 0.2516 | 5.783 | 0.102 |
| matrix48 | 110592 | 44.8714 | 11.9354 | N/A | N/A | 22.2127 | 0.1759 | 1.861 | 0.015 |
| sin20000 | 20000 | 15.7427 | 0.5383 | 0.5500 | 0.5474 | 5.9852 | 0.1639 | 11.119 | 0.304 |
| call200000 | 200000 | 199.0118 | 0.6849 | 145.5129 | 0.7579 | 143.0982 | 3.8621 | 208.933 | 5.639 |
| regex6000 | 6000 | 1.4514 | 1.2216 | N/A | N/A | 2.7092 | 0.0844 | 2.218 | 0.069 |
| str_unicode | 2000 | 3.6186 | 3.6220 | N/A | N/A | 2.3747 | 0.1875 | 0.656 | 0.052 |
| except10000 | 10000 | 3.4512 | 0.0262 | N/A | N/A | 3.6103 | 0.0214 | 137.798 | 0.817 |
| match20000 | 20000 | 28.0855 | 29.6683 | N/A | N/A | 9.7474 | 0.0603 | 0.329 | 0.002 |
| closure20000 | 20000 | 19.2947 | 20.0104 | N/A | N/A | 7.3976 | 0.0281 | 0.370 | 0.001 |
| inherit1000 | 1000 | 2.1533 | 1.8686 | N/A | N/A | 0.9670 | 0.0145 | 0.517 | 0.008 |
| set5000 | 5000 | 4.5034 | 4.7915 | N/A | N/A | 1.7798 | 0.0351 | 0.371 | 0.007 |
| enum10000 | 10000 | 7.2748 | 6.8178 | N/A | N/A | 4.6863 | 0.0123 | 0.687 | 0.002 |
| modimport2000 | 2000 | 0.3790 | 0.0043 | N/A | N/A | 0.3359 | 0.0027 | 78.116 | 0.628 |
| nanbox20000 | 20000 | 12.0507 | 11.9011 | N/A | N/A | 7.2984 | 0.0165 | 0.613 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 993.1807 | 2.8684 | 3.0000 | 2.9967 | 2371.7320 | 9.6934 | 826.848 | 3.379 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 6.9636 | 0.0181 | 0.0129 | 0.0116 | 16.0059 | 0.0304 | 884.304 | 1.680 |
| sieve30000 | 30000 | 37.7375 | 2.4744 | 42.1429 | 0.2654 | 14.3093 | 0.2516 | 5.783 | 0.102 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 199.0118 | 0.6849 | 145.5129 | 0.7579 | 143.0982 | 3.8621 | 208.933 | 5.639 |
| fib24 | 1 | 131.9892 | 1.9368 | 124.9586 | 0.2116 | 3.2816 | 0.2668 | 1.694 | 0.138 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 2.4443 | 0.2601 | 2.4286 | 0.1781 | 5.5498 | 0.1419 | 21.337 | 0.546 |
| class1000 | 1000 | 2.6290 | 0.5774 | 1.9214 | 0.0606 | 1.3461 | 0.0076 | 2.331 | 0.013 |
| rle1994 | 1994 | 5.4573 | 5.3981 | N/A | N/A | 2.4543 | 0.0438 | 0.455 | 0.008 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 11.7586 | 0.8653 | 14.1429 | 0.5217 | 0.9114 | 0.0857 | 1.053 | 0.099 |
| mergesort1000 | 1000 | 29.6896 | 1.5064 | 31.5714 | 0.5360 | 2.1769 | 0.0672 | 1.445 | 0.045 |
| matrix48 | 110592 | 44.8714 | 11.9354 | N/A | N/A | 22.2127 | 0.1759 | 1.861 | 0.015 |
| math10000 | 10000 | 13.5624 | 0.0263 | 0.0257 | 0.0256 | 4.7712 | 0.0626 | 181.414 | 2.380 |
| sin20000 | 20000 | 15.7427 | 0.5383 | 0.5500 | 0.5474 | 5.9852 | 0.1639 | 11.119 | 0.304 |
| regex6000 | 6000 | 1.4514 | 1.2216 | N/A | N/A | 2.7092 | 0.0844 | 2.218 | 0.069 |
| iter10000 | 30000 | 13.9233 | 14.6479 | 25.0000 | 0.2311 | 4.5671 | 0.1815 | 0.312 | 0.012 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.727 | 0.000 |
| fib24 | 0.875 | 0.000 |
| native_fib24 | 0.876 | 0.000 |
| nested_sum_200 | 0.733 | 0.000 |
| qsort500 | 0.170 | 0.007 |
| mergesort1000 | 1.176 | 0.015 |
| rle1994 | 1.031 | 0.000 |
| dict5000 | 1.297 | 0.000 |
| iter10000 | 0.300 | 0.001 |
| math10000 | 0.875 | 0.000 |
| class1000 | -0.745 | 0.000 |
| sieve30000 | 1.949 | 0.000 |
| matrix48 | 1.257 | 0.000 |
| sin20000 | 0.896 | 0.000 |
| call200000 | 0.759 | 0.000 |
| regex6000 | 2.269 | 0.000 |
| str_unicode | 1.416 | 0.000 |
| except10000 | 0.879 | 0.000 |
| match20000 | 1.149 | 0.000 |
| closure20000 | 0.883 | 0.000 |
| inherit1000 | -0.135 | 0.000 |
| set5000 | 0.525 | 0.000 |
| enum10000 | 0.886 | 0.000 |
| modimport2000 | 0.183 | 0.000 |
| nanbox20000 | 0.731 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **4.439x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **15.566배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 826.848x, call200000은 208.933x(Int 함수의 native 직접 호출), nested_sum_200은 884.304x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.379x, call200000 5.639x, nested_sum_200 1.680x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261124-111920\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | unjit | +0.6% | 보통 |
| int_ops | compile | +5.0% | 보통 |
| int_ops | build | +4.5% | 보통 |
| int_ops | python | +1.5% | 보통 |
| int_ops | node | +1.2% | 보통 |
| fib24 | unjit | -3.9% | 보통 |
| fib24 | compile | -1.1% | 보통 |
| fib24 | build | +4.5% | 보통 |
| fib24 | python | +4.6% | 보통 |
| fib24 | node | -0.5% | 보통 |
| native_fib24 | unjit | -3.9% | 보통 |
| native_fib24 | compile | -0.3% | 보통 |
| native_fib24 | build | +0.4% | 보통 |
| native_fib24 | python | +0.9% | 보통 |
| native_fib24 | node | -1.2% | 보통 |
| nested_sum_200 | unjit | +0.0% | 보통 |
| nested_sum_200 | compile | +12.5% | 회귀 |
| nested_sum_200 | build | +1.1% | 보통 |
| nested_sum_200 | python | +0.8% | 보통 |
| nested_sum_200 | node | -36.7% | 개선 |
| qsort500 | unjit | +1.5% | 보통 |
| qsort500 | compile | +0.0% | 보통 |
| qsort500 | build | -3.6% | 보통 |
| qsort500 | python | +5.3% | 보통 |
| qsort500 | node | -10.4% | 개선 |
| mergesort1000 | unjit | -2.0% | 보통 |
| mergesort1000 | compile | -0.5% | 보통 |
| mergesort1000 | build | +7.0% | 보통 |
| mergesort1000 | python | -1.4% | 보통 |
| mergesort1000 | node | -21.0% | 개선 |
| rle1994 | unjit | +5.9% | 보통 |
| rle1994 | python | +7.5% | 보통 |
| rle1994 | node | -10.2% | 개선 |
| dict5000 | unjit | -0.8% | 보통 |
| dict5000 | compile | -5.6% | 보통 |
| dict5000 | build | -6.8% | 보통 |
| dict5000 | python | +4.1% | 보통 |
| dict5000 | node | +0.4% | 보통 |
| iter10000 | unjit | +0.2% | 보통 |
| iter10000 | compile | +1.2% | 보통 |
| iter10000 | build | -1.3% | 보통 |
| iter10000 | python | -1.9% | 보통 |
| iter10000 | node | +11.0% | 회귀 |
| math10000 | unjit | -6.1% | 보통 |
| math10000 | compile | +0.0% | 보통 |
| math10000 | build | +1.3% | 보통 |
| math10000 | python | +7.9% | 보통 |
| math10000 | node | -0.5% | 보통 |
| class1000 | unjit | -1.7% | 보통 |
| class1000 | compile | +2.7% | 보통 |
| class1000 | build | +1.2% | 보통 |
| class1000 | python | +0.2% | 보통 |
| class1000 | node | +31.0% | 회귀 |
| sieve30000 | unjit | -0.4% | 보통 |
| sieve30000 | compile | -1.0% | 보통 |
| sieve30000 | build | -2.6% | 보통 |
| sieve30000 | python | +1.2% | 보통 |
| sieve30000 | node | +12.4% | 회귀 |
| matrix48 | unjit | +5.1% | 보통 |
| matrix48 | python | +2.1% | 보통 |
| matrix48 | node | -27.4% | 개선 |
| sin20000 | unjit | +0.0% | 보통 |
| sin20000 | compile | +1.0% | 보통 |
| sin20000 | build | +1.6% | 보통 |
| sin20000 | python | +6.6% | 보통 |
| sin20000 | node | +7.1% | 보통 |
| call200000 | unjit | +0.0% | 보통 |
| call200000 | compile | -1.0% | 보통 |
| call200000 | build | +1.4% | 보통 |
| call200000 | python | +3.7% | 보통 |
| call200000 | node | +1.3% | 보통 |
| regex6000 | unjit | +0.8% | 보통 |
| regex6000 | python | +1.2% | 보통 |
| regex6000 | node | +17.4% | 회귀 |
| str_unicode | unjit | +2.2% | 보통 |
| str_unicode | python | +3.3% | 보통 |
| str_unicode | node | -0.5% | 보통 |
| except10000 | unjit | -0.4% | 보통 |
| except10000 | python | +10.1% | 회귀 |
| except10000 | node | +0.0% | 보통 |
| match20000 | unjit | -0.9% | 보통 |
| match20000 | python | -0.7% | 보통 |
| match20000 | node | +31.9% | 회귀 |
| closure20000 | unjit | +1.8% | 보통 |
| closure20000 | python | +0.0% | 보통 |
| closure20000 | node | -1.1% | 보통 |
| inherit1000 | unjit | -0.2% | 보통 |
| inherit1000 | python | -0.3% | 보통 |
| inherit1000 | node | +51.0% | 회귀 |
| set5000 | unjit | +4.9% | 보통 |
| set5000 | python | +1.1% | 보통 |
| set5000 | node | +2.0% | 보통 |
| enum10000 | unjit | -2.3% | 보통 |
| enum10000 | python | -1.2% | 보통 |
| enum10000 | node | -0.8% | 보통 |
| modimport2000 | unjit | -2.3% | 보통 |
| modimport2000 | python | +3.4% | 보통 |
| modimport2000 | node | +0.0% | 보통 |
| nanbox20000 | unjit | +1.1% | 보통 |
| nanbox20000 | python | +0.0% | 보통 |
| nanbox20000 | node | +0.6% | 보통 |

## 15. Historical Comparison

이전 결과(20261124-111920) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

