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

- 측정 일시: 2026-11-23 22:07:38 +09:00
- ENV|un|os=Un 0.1.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
- ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor           
- ENV|python|os=Windows-10-10.0.19045-SP0|plat=win32|arch=AMD64|cpu=16|64=True|impl=CPython|version=3.13.17
- ENV|un_commit|d9a6508
- 측정 커밋: d9a6508
- Un 실행 파일: `rust\target\release\un.exe` (UV_JIT=0은 VM, 기본은 JIT)

## 3. Executive Summary

- Un JIT은 Python보다 기하평균 **3.825x 빠르다**.
- Un JIT은 Node.js보다 기하평균 **17.904x 느리다**.
- Un VM은 Python보다 기하평균 **3.437x 느리다**.
- Un VM은 Node.js보다 기하평균 **235.381x 느리다**.
- Python은 Node.js보다 기하평균 **68.482x 느리다**.
- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다.

### 기하평균 요약

| 지표 (기하평균) | 값 | 해석 |
|---|---|---|
| Un JIT / Python | 3.825x | >1 = Un이 빠름 |
| Un JIT / Node.js | 0.056x  | >1 = Un이 빠름 |
| Un VM / Python | 0.291x | >1 = Un이 빠름 |
| Un VM / Node.js  | 0.004x  | >1 = Un이 빠름 |
| Python / Node.js | 0.015x  | >1 = Python이 빠름 |

## 4. Results (all)

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1734.9318 | 2.8415 | 2.8571 | 2.8724 | 2411.4868 | 9.9816 | 848.667 | 3.513 |
| fib24 | 1 | 204.4437 | 1.9348 | 174.6429 | 0.1803 | 3.1806 | 0.2682 | 1.644 | 0.139 |
| native_fib24 | 1 | 197.7601 | 1.9287 | 173.8000 | 0.1807 | 3.1333 | 0.2668 | 1.625 | 0.138 |
| nested_sum_200 | 40000 | 9.1460 | 0.0187 | 0.0114 | 0.0133 | 16.5724 | 0.0255 | 886.225 | 1.364 |
| qsort500 | 500 | 16.5913 | 0.8839 | 20.5714 | 0.5200 | 0.9407 | 0.1017 | 1.064 | 0.115 |
| mergesort1000 | 1000 | 46.1151 | 1.5145 | 48.1429 | 0.5107 | 2.1331 | 0.0649 | 1.408 | 0.043 |
| rle1994 | 1994 | 7.2325 | 7.2170 | N/A | N/A | 2.3292 | 0.0486 | 0.323 | 0.007 |
| dict5000 | 10000 | 3.8689 | 0.2611 | 3.8571 | 0.2053 | 5.7600 | 0.1438 | 22.061 | 0.551 |
| iter10000 | 30000 | 17.8724 | 18.7598 | 35.8571 | 0.2331 | 4.9499 | 0.1780 | 0.264 | 0.009 |
| math10000 | 10000 | 20.4990 | 0.0269 | 0.0257 | 0.0257 | 4.8022 | 0.0627 | 178.520 | 2.331 |
| class1000 | 1000 | 4.5270 | 0.5872 | 3.1529 | 0.0619 | 1.3095 | 0.0069 | 2.230 | 0.012 |
| sieve30000 | 30000 | 62.9276 | 2.4807 | 65.5714 | 0.2583 | 14.2724 | 0.2271 | 5.753 | 0.092 |
| matrix48 | 110592 | 75.3098 | 11.6788 | N/A | N/A | 22.3701 | 0.2709 | 1.915 | 0.023 |
| sin20000 | 20000 | 23.8854 | 0.5394 | 0.5529 | 0.5543 | 5.6646 | 0.1525 | 10.502 | 0.283 |
| call200000 | 200000 | 287.5781 | 0.6882 | 212.9871 | 0.7875 | 147.6954 | 3.9007 | 214.611 | 5.668 |
| regex6000 | 6000 | 1.3289 | 1.5102 | N/A | N/A | 3.1128 | 0.1064 | 2.061 | 0.070 |
| str_unicode | 2000 | 4.6253 | 4.6912 | N/A | N/A | 2.6092 | 0.1822 | 0.556 | 0.039 |
| except10000 | 10000 | 5.7221 | 0.0269 | N/A | N/A | 3.3909 | 0.0256 | 126.056 | 0.952 |
| match20000 | 20000 | 51.2754 | 54.5290 | N/A | N/A | 10.3473 | 0.0480 | 0.190 | 0.001 |
| closure20000 | 20000 | 29.7371 | 29.0052 | N/A | N/A | 7.8119 | 0.0282 | 0.269 | 0.001 |
| inherit1000 | 1000 | 2.8317 | 3.0568 | N/A | N/A | 0.9691 | 0.0119 | 0.317 | 0.004 |
| set5000 | 5000 | 7.7125 | 7.8688 | N/A | N/A | 1.8384 | 0.0534 | 0.234 | 0.007 |
| enum10000 | 10000 | 11.6036 | 11.1271 | N/A | N/A | 4.9794 | 0.0124 | 0.448 | 0.001 |
| modimport2000 | 2000 | 0.5952 | 0.0051 | N/A | N/A | 0.3257 | 0.0027 | 63.863 | 0.529 |
| nanbox20000 | 20000 | 18.3303 | 17.9362 | N/A | N/A | 7.5479 | 0.0165 | 0.421 | 0.001 |

## 5. Basic operations

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| int_ops | 1000000 | 1734.9318 | 2.8415 | 2.8571 | 2.8724 | 2411.4868 | 9.9816 | 848.667 | 3.513 |

## 6. Control flow

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nested_sum_200 | 40000 | 9.1460 | 0.0187 | 0.0114 | 0.0133 | 16.5724 | 0.0255 | 886.225 | 1.364 |
| sieve30000 | 30000 | 62.9276 | 2.4807 | 65.5714 | 0.2583 | 14.2724 | 0.2271 | 5.753 | 0.092 |

## 7. Function calls

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| call200000 | 200000 | 287.5781 | 0.6882 | 212.9871 | 0.7875 | 147.6954 | 3.9007 | 214.611 | 5.668 |
| fib24 | 1 | 204.4437 | 1.9348 | 174.6429 | 0.1803 | 3.1806 | 0.2682 | 1.644 | 0.139 |

## 8. Collections

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| dict5000 | 10000 | 3.8689 | 0.2611 | 3.8571 | 0.2053 | 5.7600 | 0.1438 | 22.061 | 0.551 |
| class1000 | 1000 | 4.5270 | 0.5872 | 3.1529 | 0.0619 | 1.3095 | 0.0069 | 2.230 | 0.012 |
| rle1994 | 1994 | 7.2325 | 7.2170 | N/A | N/A | 2.3292 | 0.0486 | 0.323 | 0.007 |

## 9. Algorithms

| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| qsort500 | 500 | 16.5913 | 0.8839 | 20.5714 | 0.5200 | 0.9407 | 0.1017 | 1.064 | 0.115 |
| mergesort1000 | 1000 | 46.1151 | 1.5145 | 48.1429 | 0.5107 | 2.1331 | 0.0649 | 1.408 | 0.043 |
| matrix48 | 110592 | 75.3098 | 11.6788 | N/A | N/A | 22.3701 | 0.2709 | 1.915 | 0.023 |
| math10000 | 10000 | 20.4990 | 0.0269 | 0.0257 | 0.0257 | 4.8022 | 0.0627 | 178.520 | 2.331 |
| sin20000 | 20000 | 23.8854 | 0.5394 | 0.5529 | 0.5543 | 5.6646 | 0.1525 | 10.502 | 0.283 |
| regex6000 | 6000 | 1.3289 | 1.5102 | N/A | N/A | 3.1128 | 0.1064 | 2.061 | 0.070 |
| iter10000 | 30000 | 17.8724 | 18.7598 | 35.8571 | 0.2331 | 4.9499 | 0.1780 | 0.264 | 0.009 |

## 10. Runtime & Memory

Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은
인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다.

| Benchmark | Node heap delta (MB) | Python heap delta (MB) |
|---|---:|---:|
| int_ops | 0.704 | 0.000 |
| fib24 | 0.732 | 0.000 |
| native_fib24 | 0.876 | 0.000 |
| nested_sum_200 | 0.731 | 0.000 |
| qsort500 | 1.245 | 0.007 |
| mergesort1000 | 1.198 | 0.015 |
| rle1994 | 1.012 | 0.000 |
| dict5000 | 1.388 | 0.000 |
| iter10000 | 0.412 | 0.001 |
| math10000 | 0.887 | 0.000 |
| class1000 | 0.293 | 0.000 |
| sieve30000 | 1.949 | 0.000 |
| matrix48 | 1.185 | 0.000 |
| sin20000 | 0.762 | 0.000 |
| call200000 | 0.760 | 0.000 |
| regex6000 | 1.737 | 0.000 |
| str_unicode | 1.160 | 0.000 |
| except10000 | 0.875 | 0.000 |
| match20000 | 1.191 | 0.000 |
| closure20000 | 0.034 | 0.000 |
| inherit1000 | -0.279 | 0.000 |
| set5000 | 0.520 | 0.000 |
| enum10000 | 0.735 | 0.000 |
| modimport2000 | 0.276 | 0.000 |
| nanbox20000 | 0.874 | 0.000 |

## 11. Concurrency

- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)
  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다.

## 12. Overall Comparison

6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다.
원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는
bench/report.html에서 확인할 수 있다.

## 13. Performance Analysis

- 전반적으로 Un JIT은 Python보다 기하평균 **3.825x 빠르다**.
- 전반적으로 Un JIT은 Node.js보다 기하평균 **17.904배 느리다**.
- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 848.667x, call200000은 214.611x(Int 함수의 native 직접 호출), nested_sum_200은 886.225x 빠르다.
- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops 3.513x, call200000 5.668x, nested_sum_200 1.364x (1보다 크면 Un이 빠름).
- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖).
- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로.
- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다.

## 14. Performance Regression

측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다.
비교 기준: bench\results\20261123-215501\results.csv (다른 실행).

| Benchmark | 언어 | 변화 | 판정 |
|---|---|---|---|
| int_ops | build | -2.2% | 보통 |
| fib24 | build | +0.2% | 보통 |
| native_fib24 | build | +1.5% | 보통 |
| nested_sum_200 | build | -14.8% | 개선 |
| qsort500 | build | +0.0% | 보통 |
| mergesort1000 | build | -0.5% | 보통 |
| dict5000 | build | +9.2% | 보통 |
| iter10000 | build | -1.4% | 보통 |
| math10000 | build | +1.6% | 보통 |
| class1000 | build | +3.3% | 보통 |
| sieve30000 | build | -1.3% | 보통 |
| sin20000 | build | +1.7% | 보통 |
| call200000 | build | +4.0% | 보통 |

## 15. Historical Comparison

이전 결과(20261123-215501) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로
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

