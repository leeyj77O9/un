# JIT 회귀 이분탐색 / 계측 증거 (2026-11-20)

## 재현 스크립트 (임시 파일, 저장소 외부 TEMP)

```un
use io
use iter

fn int_ops(n)
    total = 0
    for i in iter.range(1, n + 1)
        total += (i * 1103515245 + 12345) % 10007
        total += (i * 7) // 3
        total += i & 255
        total += i ^ 0x5a5a
    -> total

f = fn(n) -> int_ops(n)
io.write(f(3000000))
```

## 버전별 측정 (같은 머신, release, UV_JIT 기본값)

| 커밋 | 바이너리 | 위 람다 프로그램 | 직접 `int_ops(3000000)` |
|---|---|---|---|
| `4b4440d` (BENCH.md 생성점) | `unvm.exe` | 34–44 ms (JIT) | 37–38 ms |
| `06a1ddd` (Compile Mode 직전) | `un.exe` | 32–40 ms (JIT) | — |
| `b3a3297` | `un.exe` | 242 ms (최초JIT 컴파일 포함) | 34 ms |
| `9d3283b` | `un.exe` | 5073–5171 ms (인터프리터) | 38 ms |
| `36aa40d` (HEAD) | `un.exe` | 5246–5333 ms (인터프리터) | 22–41 ms |

`bench/int_ops.un` 자체(동일 파일):

```text
4b4440d: BENCH|int_ops|1000000|7|2|2.8384|2.8512|...  (JIT)
HEAD   : BENCH|int_ops|1000000|7|2|1722.5551|1776|... (인터프리터 속도)
```

## 호출 횟수별 측정 (HEAD, 위 함수의 단순 버전, 3,000,000회/호출)

```text
호출 1회:  21.6 / 29.6 ms   (UV_JIT=0: 2362.6 ms)  → JIT 활성
호출 2회:  2381 / 2383 / 2393 ms (UV_JIT=0: 4724.7 ms) → 2회차부터 인터프리터
호출 3회:  4699.8 ms                                   → 3회차도 인터프리터
```

## 계측 (9d3283b 워크트리의 `jit_fast.rs`에 `UV_JIT_TRACE` 로그 임시 삽입, 빌드 후 실행)

두 호출 프로그램(`io.write(int_ops(3e6)); io.write(int_ops(3e6));`)에서 관측:

```text
JITTRACE try proto=1 id=1 names=[""]
JITTRACE entry proto=1 arity=1 id=1
JITTRACE res proto=1 value=4500634868800 tag=0 ints=[3000000]      ← 1회차: 네이티브 실행
JITTRACE try proto=1 id=1 names=[""]
JITTRACE tir-hit proto=1 arity=1                                    ← 2회차: TIR 조회가 먼저 히트
```

람다 프로그램에서 관측:

```text
JITTRACE try proto=2 id=1 names=[""]        ← lambda
JITTRACE entry proto=2 arity=1 id=1
JITTRACE res proto=2 value=0 tag=2           ← 내부 int_ops 호출의 파라미터 가드 bail → lambda 전체 bail
JITTRACE try proto=1 id=1 names=[""]        ← 인터프리터 재실행 중 int_ops 호출
JITTRACE tir-hit proto=1 arity=1             ← 여기서 TIR 경로로 오인 → None → 인터프리터
```

## 원인 코드 (HEAD)

- `rust/src/jit_cache.rs:70` — `static JIT_FNS: HashMap<program_id, HashMap<proto_index, JitEntry>>`
  bytecode JIT(`register_jit_entry`, :119)와 Compile Mode TIR(`register_tir_entry`, :131)이 **같은 맵**을 공유.
- `rust/src/jit_cache.rs:147` — `get_tir_entry()`도 같은 맵을 읽으며 `UV_JIT` 게이트가 없음.
- `rust/src/jit_fast.rs:72` — `try_call_jit()`이 `get_tir_entry()`를 **먼저** 조회.
- `rust/src/jit_fast.rs:20` — `try_call_tir_entry()`의 `entry.arg_kinds.len() != args.values.len()` 검사에서
  bytecode JIT 엔트리는 `arg_kinds: Vec::new()`(len 0)이므로 인자가 1개 이상이면 즉시 `None` 반환 → 인터프리터 fallback.

최초 1회만 엔트리가 등록되기 전이라 `get_jit_entry` 경로로 진입해 정상 JIT되고, 두 번째 이후는 항상 tir-hit → `None` → 인터프리터.

## 도입 시점

- `b3a3297`까지 정상(32–242 ms), `9d3283b`부터 비정상(5.1 s).
- 그 사이 커밋: `655a08a`(Compile Mode, 19:20) → `b1c9c6f` → `6da0432`(문서) → `9d3283b`(20:41).
- `655a08a`는 `jit_fast.rs`에 `try_call_tir_entry` + TIR 우선 조회를, `jit_cache.rs`에 `register_tir_entry`/`get_tir_entry`와 공유 맵을 도입함.
- 참고: `655a08a`와 `b1c9c6f`는 커밋 자체가 빌드되지 않음(중복 필드 `trap_bad_key_blk`, 없는 `NodeKind::Await`, 비완전 매치 등 6개 컴파일 에러) → 이 구간은 이후 커밋에서 정리됨.
