# Benchmark log

> **Legacy document.** These entries were measured against the C# / .NET 8
> implementation, which has since been replaced by the Rust port (`rust/`).
> The recorded numbers describe that implementation and this machine at the
> time they were taken; they are kept as history, not as current reference.
> For current numbers, see `docs/BENCHMARK_GUIDE.md` and re-measure with
> `scripts/bench.ps1`.

이 문서는 벤치마크 정리 과정과 실측 결과를 기록한다.
절차/형식 규칙은 `docs/BENCHMARK_GUIDE.md`가 정하고, 이 문서는 그 절차로 측정한 값과
그 측정의 한계를 기록한다.

## 1. 정리 이전

기존 벤치마크는 `src/main.un`에 가까운 스크래치 스크립트로 존재했다.
재현·검증에 문제가 있는 형태였다.

- **1회 실행**: 반복 실행이 없어 하나의 값이 JIT/GC·시스템 상태에 크게 의존했다.
- **`sw.ms()` 사용**: 밀리초 단위 해상도가 너무 성글다. 짧은 작업은 0에 가깝게 끊어지고
  값의 개략 변화만 남는다.
- **웜업 없음**: 첫 실행이 측정 대상에 포함된다.
- **통계 없음**: 최솟값·중위값·분산이 없어 "얼마나 빠른가"를 말하지 못한다.
- **출력이 측정에 포함**: 시간 측정을 반복 출력과 함께 수행한다. 출력과 형식 변환이 작업과
  함께 측정되고, stdout이 느려지고 로그가 커진다.
- **검증 없음**: 계산 결과가 옳은지 확인하지 않는다. 연산을 단순화한 스크립트가 통과하는
  형태로 변질될 여지가 있다.
- **비교 대상 없음**: 언어 간 수치는 같은 입력·알고리즘·측정 절차를 쓰는 상대가 필요하다.
  기존 스크립트는 그런 상한이 없어 언어 간 수치를 만들 수 없었다.
- **저장·출력 형식 없음**: 결과가 파일로 남지 않아 실행 간 비교가 불가능하다.

## 2. 정리한 것

- **웜업**: 같은 작업을 측정 전에 실행한다(`warmup=2`).
- **반복**: `iters` 반복으로 1회 시간을 줄이고, `reps` 반복으로 표본을 늘린다(reps=7,
  `iters`는 작업마다 100~10000).
- **해상도**: `sw.tick() / 10000.0`를 써서 단위 밀리초보다 성글지 않게 한다.
- **통계**: 최솟값·중위값·평균·최댓값·표준편차와 연산 수(`iters / time`)를 계산한다.
- **출력 분리**: 측정은 stdout 없이 수행하고, 측정이 끝난 뒤 한 줄을 stdout에 남긴다.
- **검증**: 각 작업은 미리 정한 기대값과 비교하고, 검사 실패는 `assert`가 기록한다.
  모든 벤치마크 스크립트에서 검사가 통과해야 한다.
- **저장**: 결과를 `bench/results/<timestamp>/results.csv`와 `results.md`에 남긴다.
  첫 줄에 `ENV` 행으로 환경(`os`, `plat`, `arch`, `cpu`, 64bit, CPU depth, Node 모델)을 남긴다.
- **비교 상한**: 같은 입력·알고리즘·웜업·반복을 쓰는 Node.js 스크립트 8개를
  `bench/node/`에 두었다. 비교는 실측한 두 실행 환경을 기록한 값으로만 한다.
- **측정 대상 분리**: 짧은 작업이 많은 스크립트, 긴 작업이 많은 스크립트, 출력량이 많은
  스크립트를 측정 대상에서 뺐다. 짧은 작업은 표본이 `iters`에 고정되고, 긴 작업은
  한 번에 수십 초를 쓴다.

## 3. 실측 결과

`scripts/bench.ps1 -Group all`로 저장한 실행은 두 번이다(warmup=2, reps=7, median 기준,
Un/Node 검사 모두 통과, runner exit 0).

- **run A** `bench/results/20261106-213545/` — 제품 수정(`->`가 루프 본문에서 함수를
  반환하도록 한 수정, 다중 상속 수정) 이전
- **run B** `bench/results/20261107-165948/` — 같은 기계, 같은 입력, 두 수정 이후

ENV 행은 두 실행이 같다.

```text
ENV|un|os=Microsoft Windows NT 10.0.19045.0|plat=windows|arch=X64|cpu=16|64=true|depth=1000
ENV|node|os=Windows 10 Home|plat=win32|arch=x64|cpu=16|64=true|model=AMD Ryzen 7 9800X3D 8-Core Processor
```

`sys.version()`는 OS 버전을 반환한다. ENV의 Un 버전 값은 OS 버전이고, Un의 언어 버전과
커밋 해시는 ENV에 남지 않는다(4장 참조). Un은 net8.0 앱이 .NET 10 런타임에서 실행된다
(`DOTNET_ROLL_FORWARD=Major`).

| label | iters | Un median ms (A → B) | Node median ms (A → B) | ratio (A → B) | Un OPS (A → B) |
|---|---:|---:|---:|---:|---:|
| `fib24` | 1 | 661.4375 → 704.3707 | 0.3015 → 0.2682 | 2193.8 → 2626.3 | 1.5 → 1.4 |
| `nested_sum_200` | 40000 | 17.5797 → 19.9404 | 0.1874 → 0.0238 | 93.8 → 837.8 | 2275351.7 → 2005977.8 |
| `qsort500` | 500 | 62.3955 → 60.9185 | 0.3801 → 0.0944 | 164.2 → 645.3 | 8013.4 → 8207.7 |
| `rle1994` | 1994 | 14.6394 → 14.9616 | 0.0396 → 0.0729 | 369.7 → 205.2 | 136207.8 → 133274.5 |
| `dict5000` | 10000 | 5.5780 → 5.8017 | 0.1432 → 0.1496 | 39.0 → 38.8 | 1792757.3 → 1723632.7 |
| `iter10000` | 30000 | 73.5840 → 77.3544 | 0.2995 → 0.2336 | 245.7 → 331.1 | 407697.3 → 387825.4 |
| `math10000` | 10000 | 55.6325 → 61.0479 | 0.1514 → 0.0634 | 367.5 → 962.9 | 179751.0 → 163805.8 |
| `class1000` | 1000 | 7.6491 → 8.3202 | 0.0104 → 0.0066 | 735.5 → 1260.6 | 130734.3 → 120189.4 |

`ratio`는 Un median / Node median이고, `OPS`는 `iters / (median ms / 1000)`다. Un의
메모리 열은 `n/a`다. Node heap 증가는 A run `-1.542 ~ 0.583 MB`, B run
`-0.320 ~ 0.560 MB`다. 음수 값은 GC 시점을 제어하지 않아 측정 대상 힙이 측정 전에
이미 줄어든 실행이다.

Un median의 A→B 변화는 `qsort500` -2.4%를 빼면 +2.2% ~ +13.4%다. `class1000`
(+8.8%)는 다중 상속 수정이 모든 객체에 `Parents` 목록을 추가한 것과 직접 관련된다.
나머지 변화는 Node median이 같은 방향·같은 크기로 흔들리는 위치(4장)에서 두 수정의
비용이라고 단정하지 못한다. Node median은 짧은 작업에서 A→B로 1.8배 느려진 것과
8배 빨라진 것이 같이 있고, ratio는 2~9배로 흔들린다. ratio는 한 실행으로 결론짓지
말아야 한다.

`20261106-212936`은 CSV에 웜업과 반복 수 열이 빠진 첫 실행이고, 이후 runner를 고쳐
20열 표로 저장했다.

## 4. 측정의 한계

- **해상도**: Un의 타이머는 0.1 ms 단위다. `iters`를 늘려도 짧은 작업(Node median
  0.0238~0.1514 ms)은 표본이 성글다. Un의 짧은 작업 값은 연산 수의 하한으로 봐야 한다.
- **메모리**: Un에 메모리 조사 API가 없어 `MEM` 열을 낼 수 없다. 메모리 열은 Node에만 있다.
  Un의 메모리 값은 `n/a`다.
- **언어 버전 기록**: Un에 언어 버전을 얻는 API가 없고, `sys.version()`는 OS 버전이다.
  ENV 행의 Un 버전은 OS 버전이며, Un의 버전을 나타내지 못한다. 커밋 해시를 남기는 방법도
  CLI에 없다.
- **표본 개수**: 짧은 작업은 반복을 늘려도 표본이 짧다. 짧은 작업의 중위값은 실행마다
  크게 달라진다(두 실행의 `nested_sum_200` Node median 0.1874 → 0.0238 ms,
  `math10000` Node median 0.1514 → 0.0634 ms). 여러 실행의 중위값의 중위값을 써서
  보고해야 한다.
- **GC**: Un은 GC를 강제로 수행하는 방법이 없다. GC를 포함한 값이 저장된다.
- **출력**: stdout이 측정 밖으로 나갔다.
- **시스템 상태**: 백그라운드 프로그램, 전원 설정, AV 스캔이 표본을 바꾸는 위치는 기록하지
  않았다.
- **비교의 비대칭**: Un은 인터프리터, Node는 V8 기반이다. 이 값은 같은 입력·알고리즘을
  같은 절차로 측정한 값이며, 언어의 우열을 말하지 않는다.
- **작업의 개수**: 8개의 작업은 Un의 기능을 넓게 대표하지 않는다. 문자열 생성·정규식·JSON
  변환·비동기 실행·파일 I/O는 측정 대상이 없다.

## 5. 실행 방법

```powershell
# Un만
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group un
# Un과 Node를 같은 표로
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group all
# 빌드 후 실행
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Rebuild
# 파일을 저장하지 않음
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group all -NoSave
```

runner는 `[BAD]`(검사 실패)가 있으면 exit 1로 끝난다. 각 그룹의 `[OK]` 행은 검사가
통과했음을 나타낸다.
