# 독립 전체 감사 — Un 언어 저장소 (2026-11-20)

> 범위: `C:\dev\un` 전체(소스·테스트·CI·문서·빌드 산출물). 사용자 소유 WIP
> (`main.un`, `rust/src/main.rs`, `rust/src/lsp.rs`, `README.md`, `docs/README.md`,
> `docs/guides/CLI.md`, `docs/guides/CLI.ko.md`, `bench/**` 및 미커밋분 전부)는
> 읽기만 했고 수정·삭제·스테이징하지 않았다. 커밋/푸시도 하지 않았다.
> 이 문서와 `docs/audit/evidence/*`가 유일한 변경 사항이다.
>
> **현재 상태 (2026-11-23, HEAD `f161f11`):** 아래 Part I~IV는 감사 시점 스냅샷이다.
> P1(AUDIT-001)은 `edeabff`에서 해결됐고 AUDIT-003~008도 대부분 해결됐다. 남은
> 항목은 AUDIT-010(생성 exe의 `dead_code` 경고, P3), AUDIT-011(간헐 빌드 실패,
> P3/SUSPECTED), WASM 동적 confine(BLOCKED)뿐이다. 숫자를 현재값으로 덮어쓰지
> 않고 "당시 → 현재" 대비로 보존한다. 현재 기준은 `docs/project/REFACTOR_SUMMARY.md`
> 및 아래 Part V를 참조한다.
>
> **(2026-11-27 각주 — historical 스냅샷 보존)** Part I~IV의 표와 위 2026-11-23 상태는 당시 값으로 그대로 둔다.
> 현재 기준은 `cargo test` **430 passed**, `scripts/test.ps1` **51 files / 1638 passed / 0 failed / 0 crashed**,
> `compile-check` **42/0**, `exe-diff` **78/0**, `differential` **102/0**
> (`docs/project/REFACTOR_SUMMARY.md` "Current baseline"). GOWAIT opt-in 병렬은 D-099/P08 `488bf0b`로 완료,
> 기본 deferred는 유지. opt-in 차이는 `TECHNICAL_DEBT.md` TD-54/TD-55/TD-56 참조.

## 0. 요약

| 항목 | 결과 |
|---|---|
| 정적 빌드 / cargo test / WASM | 통과 (경고 0, 218 passed, wasm OK) |
| `scripts/test.ps1` | 통과 — **Positive 48 files / 1570 passed / 0 failed / 0 crashed**, Negative 50 matched / 0 divergences, Known 0 |
| `differential.ps1` | 통과 — **98 files, 0 differences** |
| `unbc-roundtrip.ps1 -IncludeNegative` | 통과 — Positive 48/48, Negative 41 identical (+9 compile-rejected skip) |
| `UV_JIT=2` strict canary ×2 | 통과 (exit 0, 두 마커 모두 출력) |
| compile-check / exe-diff / obj-demo / compile-bench | 통과 (23/0, 14/0, PASS, 실행됨) |
| **P0** | 없음 |
| **P1** | **1건 — JIT가 두 번째 호출부터 인터프리터로 조용히 퇴화 (성능 회귀, 문서와 불일치)** |
| **P2** | 3건 — 기준 수치 광범위한 stale, BYTECODE.md가 실제 ISA와 불일치, 한국어 README 예제 코드 손상 |
| **P3** | 4건 — ARCHITECTURE 세부 stale, main 이력의ビルド 불가 커밋, 샌드박스 confine 무테스트, 저장소 위생 |

가장 중요한 결론: **모든 바이트 비교 게이트가 green이지만, JIT는 현재 "실질적으로 꺼진" 상태다.**
`differential`(JIT on/off 바이트 동치)과 `UV_JIT=2`(컴파일 성공 여부만 검사)는 이 회귀를
구조적으로 잡을 수 없다. `bench.ps1`의 "un JIT" 레인도 2026-11-19 19:20 이후
인터프리터를 측정해 왔다.

---

## 1. 측정 기준

- `rustc 1.100.0 (0ef1bc386 2026-11-09)`, `git HEAD 36aa40d`, 브랜치 `main`.
- `git status --porcelain` (감사 시작 시): 사용자 WIP 7개 — `README.md`,
  `docs/README.md`, `docs/guides/CLI.ko.md`, `docs/guides/CLI.md`, `main.un`,
  `rust/src/main.rs`, `?? rust/src/lsp.rs`. 감사 중 `main.un`과 `lsp.rs`가 사용자가
  직접 다시 수정함(03:07경 lsp.rs 일시 빌드 불가 → 03:14 재수정으로 해소). 모두 사용자 소유.
- `un.exe --version`은 미지원(`Unknown command: --version`); CLI 커맨드는
  `run/compile/check/native/obj/build/help/lsp` (사용자 WIP의 `un lsp` 포함).
- `test/MANIFEST.txt` 유효 항목 48행 (suite 1 / unit 42 / integration 4 / regression 1),
  `test/negative/manifest.txt` 유효 항목 50행. `test/**/*.un` 135개.
  `rust/src` 90개 .rs / 51,558줄 (+ playground 1개).
- `docs/` 재편은 실제와 일치: `spec/ARCHITECTURE.md`, `spec/BYTECODE.md`,
  `guides/{CLI,CLI.ko,TEST_GUIDE,BENCHMARK_GUIDE}.md`,
  `project/{REFACTOR_SUMMARY,PROJECT_AUDIT,REFACTOR_LOG,TECHNICAL_DEBT,DECISIONS}.md`,
  `compile/` 7개 문서. DECISIONS는 D-001…D-092(92개, 중복 없음).

자세한 명령/원문 출력은 `docs/audit/evidence/` 참조.

---

## 2. 게이트 실행 결과 (현재 HEAD 바이너리로 재확인 포함)

| 게이트 | 명령 | 결과 (원문 발췌) |
|---|---|---|
| release build | `cargo build --release --manifest-path rust/Cargo.toml` | exit 0, warning 없음. 단, 한 시점에 `un.exe`가 OS 잠금(os error 5)으로 강제 clean/build가 막혀 `cargo check --release --workspace`로 경고 0 재확인 |
| unit tests | `cargo test --release --workspace` | `test result: ok. 213 passed` (lib) + `5 passed` (playground) = 218, 0 failed |
| WASM | `cargo build --release --target wasm32-unknown-unknown ... -p unvm-playground` | exit 0 |
| test suite | `scripts/test.ps1` | `Positive: 48 files, 1570 passed, 0 failed, 0 crashed` / `Negative: 50 matched, 0 mismatched, 0 documented divergences` / `Overall exit code: 0` |
| JIT on/off | `scripts/differential.ps1` | `Differential: 98 files, 0 differences` |
| .unbc | `scripts/unbc-roundtrip.ps1 -IncludeNegative` | `RoundTrip(positive): 48 files, 48 identical, 0 mismatched` / `RoundTrip(negative): 41 identical, 0 mismatched, 9 skipped(compile-rejected)` |
| strict canary | `UV_JIT=2 un.exe run test/jit/strict.un` | `jit-strict canary OK`, exit 0 |
| float canary | `UV_JIT=2 un.exe run test/jit/strict_float.un` | `jit-float-strict canary OK`, exit 0 |
| compile mode | `scripts/compile-check.ps1` | `compile-check: 23 passed, 0 failed` |
| exe byte-compare | `scripts/exe-diff.ps1` | `exe-diff: 14 passed, 0 failed` |
| obj/build demo | `scripts/obj-demo.ps1` | `obj-demo: PASS` (`fib = 55`, `un build`, `--embed`, class 37) |
| compile bench | `scripts/compile-bench.ps1` | 3모드 정상 실행(205 / 198 / 177 ms), 결과만 gitignored 경로에 저장 |
| bench spot | `scripts/bench.ps1 -Group un -NoSave` | 완료(exit 0). **수치는 P1-1 참조** — JIT 레인이 인터프리터 속도 |

3레인(UV_JIT=0 / 기본 JIT / UV_JIT=2) + `un native` 바이트 비교도 주석 처리된 Int 프로그램
(`collatz`, `sumsq`)에서 수행: 네 레인 모두 `111 / 328350` 동일 출력, exit 0.
`un native`는 `math.hypot` 사용 시 `tir: ... is not supported in the core`로 거부하는데,
이는 문서화된 "compile mode는 core subset만" 정책에 부합한다.

---

## 3. 발견사항

### P1-1. JIT가 두 번째 호출부터 조용히 인터프리터로 퇴화 (성능 회귀 / 문서 불일치)

**사실.** `try_call_jit()`은 호출 시 `get_tir_entry()`를 먼저 조회하는데
(`rust/src/jit_fast.rs:72`), `get_tir_entry()`와 bytecode JIT의
`get_jit_entry()`가 **동일한 `JIT_FNS` 맵**(`rust/src/jit_cache.rs:70`, 등록:
`:119`/`:131`, 조회: `:140`/`:147`)을 읽는다. bytecode JIT 엔트리는
`arg_kinds: Vec::new()`이므로, 등록 이후 두 번째 호출부터 이 엔트리가
`try_call_tir_entry()`로 잘못 라우팅되고 `entry.arg_kinds.len() != args.values.len()`
(`jit_fast.rs:20`) 검사에 걸려 `None` → 인터프리터 fallback 된다. 최초 1회만
엔트리 등록 전이라 정상 JIT된다.

**증거.**

- 계측(임시 워크트리 `9d3283b`, `jit_fast.rs`에 `UV_JIT_TRACE` 삽입):
  ```text
  JITTRACE try proto=1 ... / entry proto=1 / res proto=1 tag=0   ← 1회차 네이티브
  JITTRACE try proto=1 ... / tir-hit proto=1                     ← 2회차: TIR 오인 → None
  ```
  전체 로그: `docs/audit/evidence/jit-bisect.md`.
- 시간 측정(HEAD): 동일 함수 1회 호출 21.6–29.6 ms(UV_JIT=0: 2362 ms) vs
  2회 호출 2381–2393 ms(UV_JIT=0: 4725 ms) vs 3회 4700 ms → 2회차부터 인터프리터.
- `bench/int_ops.un`: HEAD `1776 ms/rep`, BENCHMARK.md 생성 커밋 `4b4440d`
  `2.851 ms/rep` (같은 머신, 같은 파일, 약 620x 차이). `differential.ps1`은
  바이트만 비교하므로 98/0 그대로, `UV_JIT=2`는 컴파일 성공만 확인하므로 통과.

**영향.** Compile Mode 도입 커밋 `655a08a`(2026-11-19 19:20) 이후 JIT가 실질적으로
비활성. `BENCHMARK.md`가 주장하는 "Un JIT / Python 기하평균 7.527x" 등은
`4b4440d` 시점 수치이며 현재 트리에서는 재현되지 않는다. 정답은 바뀌지 않는다
(fallback이 의미론을 보존).

**최소 재현.**
```powershell
# 1회는 JIT, 2회부터 인터프리터
.\rust\target\release\un.exe run bench\int_ops.un
# 기준: 4b4440d 체크아웃에서 같은 명령 → BENCH|int_ops|...|2.85 ms
```

**제안(승인 후).** bytecode JIT 엔트리와 TIR 엔트리를 **별도 맵**으로 분리하거나
`JitEntry`에 엔트리 종류를 명시하는 필드를 두어 `get_tir_entry()`가 bytecode
엔트리를 반환하지 않게 한다. 수정 범위: `jit_cache.rs`, `jit_fast.rs` 2개 파일,
리스크 낮음(라우팅만 분리). 병행: 게이트 강화 — "JIT가 실제로 발동했는지"를
보는 타이밍/카운터 기반 canary를 추가해야 재발을 잡을 수 있다(기존 byte-diff와
UV_JIT=2로는 불가). BENCHMARK.md 재생성은 수정 후.

---

### P2-1. 기준 수치/CLI 목록이 광범위하게 stale

**사실/증거.** 실제 실행값 vs 문서:

| 문서 (위치) | 문서 값 | 실제 값 |
|---|---|---|
| `AGENTS.md:11` | Rust 84 files / 40,766 lines, `.un` 101 | 90 files(rust/src)+1(playground) / 51,558 lines, `.un` 135 |
| `AGENTS.md:28` | CLI `.../plan/help` | `plan` 없음; `check/native/obj/build/lsp` 누락 |
| `AGENTS.md:199-200` | Positive 47 / 1538, Negative 48 | 48 / 1570, 50 |
| `README.md:355-356`, `docs/README.md` 동일 구절 | 47 / 1538 / 48 | 48 / 1570 / 50 |
| `docs/guides/TEST_GUIDE.md:78-80,143-145,154` | 47 / 1538 / 48 / roundtrip 39/39 | 48 / 1570 / 50 / 41 identical |
| `docs/compile/COMPILE_TESTS.md:11-17` | 47/1538/48/95 files/39; compile-check 18 checks; exe-diff 7 checks | 48/1570/50/98 files/41; 23; 14 |
| `docs/project/REFACTOR_SUMMARY.md:77-79,107` | 47/1538/48, differential 95, roundtrip 47/47+39/39, D-001–D-077 | 48/1570/50, 98, 48/48+41, D-001–D-092 |
| `docs/spec/ARCHITECTURE.md:196` | differential 95 files | 98 files |
| `docs/project/PROJECT_AUDIT.md:17` | D-001–D-077 | D-001–D-092 |

`REFACTOR_SUMMARY.md`는 "Current baseline (all verified 2026-11-17)"로 날짜를
명시하고 있어 역사적 기록으로는 성실하지만, 현재 상태를 읽는 독자에게는 오해 유발.
`AGENTS.md:90`의 CI 단계 기술은 build.yml과 일치(2개 strict canary 스텝 포함).
`README`의 설치/첫 실행 예제는 실제 실행 확인: `Hello, Un!` 정상 출력(exit 0).

**제안.** baseline 표를 한 곳(예: `docs/guides/TEST_GUIDE.md`)에서 관리하고
나머지는 링크; AGENTS.md의 CLI 목록과 수치를 갱신. 단, README/CLI 문서는 사용자
소유 WIP이므로 **사용자 확인 후** 반영.

---

### P2-2. `docs/spec/BYTECODE.md`가 실제 Op/ISA와 불일치

**사실/증거.**

- 문서는 opcode를 `LOAD_NONE`, `AND`, `OR`, `COALESCE`, `WHILE_NOT`, `ENTER_SCOPE`,
  `JMP_FALSE/TRUE/NONE` 등 C# 참조 구현 이름으로 기술(`BYTECODE.md:29,71-78` 등).
  실제 `rust/src/bytecode/instr.rs`는 `LoadNone=0x01…Await=0x64`, `FusedIntMulAdd=0x7F`
  이고 short-circuit/None-jump는 `JmpIf` + flag(`FLAG_JUMP_TRUE/POP/NONE`)로 구현.
- `:299` "0x65..=0x7F는 비워 둔다"는 실제와 모순 → `0x7F`는 `FusedIntMulAdd` 사용,
  예약 구간은 `0x65..=0x7E`(`instr.rs::reserved_area_is_free` 테스트가 고정).
- `:262` "예약 영역 0x62..0x7F"도 동일 문제.
- `:271,290` 삭제된 C# 소스(`Evaluator.cs:1309/1087`)를 근거로 인용.
- stack-effect/validation 기술(§14-15)도 실제 `bytecode/mod.rs::validate` 및
  `vm/transfer.rs::analyze`와는 개념 수준에서만 유사.

**제안.** 문서를 "참조 구현 개념"이 아니라 "현재 Rust `.unbc`" 기준으로 재작성하고,
opcode 표를 `instr.rs::Op`에서 자동 생성하거나 최소한 `#[repr(u8)]` 값 표를 동기화.
`docs/spec/ARCHITECTURE.md`도 `parser.rs:1505-1534`(실제 precedence table
`1551-1580`) 등 line reference가 드리프트됨.

---

### P2-3. 한국어 README의 실행 예제 코드 블록 손상 (사용자 WIP 파일)

**사실/증거.** `README.md:576` — `\`\`\`un` 펜스 안에
`using pool = flow.spawn(2)`가 있어야 할 자리에 한국어 설명 문장이 들어가 있다.
영어 쌍 `docs/README.md`의 같은 위치는 정상 코드:

```text
KO README.md:571-580  (펜스 내부)
  use io / use flow / use iter
  - map과 submit을 제공하는 flow.spawn(n) 풀 핸들 (...)   ← 코드가 아님
  r = pool.map(...)
EN docs/README.md:594-607
  use io / use flow / use iter
  using pool = flow.spawn(2)
  r = pool.map(...)
```

같은 문장이 `README.md:558`에도 있어 복제된 상태. 복사→실행 시 실패하는 예제이며
한/영 쌍 동기화 규칙에도 어긋난다. **사용자 소유 파일이므로 보고만 한다.**

**제안.** KO 펜스 내부 줄을 `using pool = flow.spawn(2)`로 복원하고 :558의 중복
문장을 영어판의 atomic/단일 스레드 설명에 맞춰 정리(사용자 승인 후).

---

### P3-1. `ARCHITECTURE.md`의 세부 기술 stale

- `:89` "TryEntry.depth는 기록만 되고 읽히지 않는다 (TD-12)" — 현재 필드는
  `acc_depth`이며 `vm/mod.rs:346/354/371`에서 `args.truncate()`에 사용됨.
  `TECHNICAL_DEBT.md`도 TD-12를 해결로 기록 → 문서 간 모순.
- JIT tagservers 기술(`:5`)이 `0/1/2/3`만 나열 — `jit_cache.rs:12-13`의
  `TAG_NONE=5`, `TAG_INTLIST=6` 누락.
- `:199` "213 Rust unit tests"는 lib 기준으로 정확하나 workspace 합계는 218.

### P3-2. main 이력에 빌드되지 않는 커밋

`git worktree`로 `655a08a`, `b1c9c6f`를 각각 체크아웃해 `cargo build --release`
실행 시 동일한 6개 컴파일 에러가 재현됨:
`natcode.rs:500` 중복 필드 `trap_bad_key_blk`, `tir.rs:1299`의 없는
`NodeKind::Await/Wait`, `TirStmt` 비완전 매치 등. 반면 `b3a3297`, `6da0432`,
`9d3283b` 및 HEAD는 빌드됨. 커밋 메시지는 "Verified by user ... green"을
주장하므로 커밋 스냅샷과 검증 시점 작업 트리가 달랐던 것으로 보인다.
`9d3283b`에서 정리됨 → 현재는 문제없으나 이력 기반 재현/감사 시 주의.

### P3-3. 샌드박스 confine 경로에 자동 테스트 없음

- `rust/src/native/io.rs:48 normalize_under()`는 드라이브 문자·UNC·RootDir·
  `..` 탈출을 거부하고 `starts_with_case_insensitive`로 최종 확인 — 정적 검토상
  건전. 단 `world::sandbox_active()`가 true일 때만(`world::set_allowed` 호출 시,
  즉 playground/WASM) 적용되고 CLI `un run`에서는 비활성.
- 저장소 전체에서 `"escapes the sandbox"` 문자열은 구현(`io.rs:121`)에만 있고
  테스트에는 없음. WASM playground 바이너리는 이 저장소에 없어 release/debug
  confine 테스트를 **실행하지 못함** → 정적 동치 추론으로 환원.
- 제안(승인 후): `normalize_under` 단위 테스트(`C:/...`, `\\server\share`,
  `..//`, `sub/../../escape`)를 `native/io.rs`의 `#[cfg(test)]`에 추가.

### P3-4. 저장소 위생

- `.gitignore`는 `/rust/target/`, `*.unbc`, `/bench/results/`, node/python 캐시,
  에이전트 상태 디렉터리를 적절히 제외. 추적되는 바이너리/obj/wasm 없음(368 tracked files).
- `rust/target/release/unvm-audit-old.exe` (11.8 MB, 2026-11-18) 존재 —
  `rust/target/` 아래라 gitignore됨. 삭제 제안만(수정 안 함).
- `bench/results/`에 `-broken` 접미사 디렉터리들이 남아 있음(gitignored).
- `test/fixtures/*`는 negative manifest/tests에서 모두 참조됨.
  `KnownFailures.txt`/`KnownDivergent.txt`는 등록 항목 0으로 문서와 일치.

---

## 4. 보안/안전 검증 결과 (이상 없음)

- **CRLF 주입**: `native/net.rs::parse_uri`가 URI 전체의 CR/LF를 거부,
  `native/http.rs:183-193` `set_header`가 이름의 `:`/CR/LF와 값의 CR/LF 거부.
  WS key는 random 16바이트 base64 → 헤더 주입 불가. 구조적 방책 확인.
- **union tag contract**: `jit_cache.rs:8-13`의 `0=Int,1=Float,2=bail,3=Bool,5=None,6=IntList`를
  `jit_bc.rs`(tag 기록)와 `jit_fast.rs`(tag 소비)가 일치하게 사용. bail 시 호출 측이
  `None`을 반환해 인터프리터가 재실행. class/ctor/dead-handle 시 bail 경로도 동일 계약.
- **IntVec/IntDict 소유권(정적)**: `jit_bc.rs::emit_int_cleanup`이 bail/정상 반환
  양쪽에서 dict/iter/owned-list 핸들을 free하고, `INTLIST_BORROWED`는 caller 소유로
  free하지 않음. `try_call_jit`의 back-sync는 모든 인자를 먼저 검증한 뒤 쓰고,
  bail 시 drain만 수행 → 이중 free 없음. 단 INTLIST 반환 경로는 로컬 리스트 전체를
  `keep_all_intlist=true`로 남겨(process-lifetime arena) 보수적으로 누수 여지가 있음 —
  메모리 안전 문제 아님.
- **JIT class name-checked intrinsic**: 필드 이름/payload 검증 실패 시 tag=2 bail로
  인터프리터 폴백(코드 검토). CRLF/널 크래시 경로 없음.
- **go/wait**: cooperative single-thread 설계(D-012/D-054)와 일치, `wait` 비-future
  오류 메시지/interpreter-JIT 바이트 동치는 differential이 커버.

---

## 5. 승인 요청 — 제안 우선순위 3가지

아래는 **제안만**이며 승인 전 어떤 소스/문서도 수정하지 않는다.

| # | 제안 | 범위 | 비용 | 리스크 |
|---|---|---|---|---|
| 1 | **P1-1 수정**: TIR/bytecode 엔트리 분리 + "JIT 실제 발동" canary 추가 + BENCHMARK.md 재생성 | `jit_cache.rs`, `jit_fast.rs`, `scripts/bench.ps1` 또는 신규 canary, `BENCHMARK.md` | 중간(핵심 2파일은 소규모, 벤치 재측정 포함) | 낮음~중간. 라우팅 분리 자체는 안전하나 TIR zero-arg 엔트리 처리에 주의; 벤치 수치는 머신 의존 |
| 2 | **기준 문서 동기화**: AGENTS/REFACTOR_SUMMARY/TEST_GUIDE/ARCHITECTURE/COMPILE_TESTS의 수치·CLI·TABLE 갱신 (README/CLI 쌍은 사용자 확인 후) | 6~8개 문서 | 낮음(기계적) | 낮음. 단 사용자가 WIP 중인 README/CLI.ko는 충돌 가능 → 별도 승인 |
| 3 | **BYTECODE.md 재작성 + README KO 예제 복구 + sandbox 단위 테스트** | `docs/spec/BYTECODE.md`, 사용자 소유 `README.md`(승인 시), `native/io.rs` 테스트 모듈 | 중간(문서 전면) | 낮음. 문서 전면 재작성은 검토 시간 소요 |

원하시면 1번부터 diff 단위로 준비하겠습니다. 승인 전에는 커밋/푸시하지 않습니다.

---

## 6. 감사 중 수행하지 않은 것 / 한계

- Linux/WSL 실행 불가 항목: WASM playground 직접 실행, `io.open` sandbox confine의
  release/debug 양쪽 테스트 — 정적 분석 및 코드 검토로 환원(명시됨).
- `scripts/bench.ps1 -Group all` (Python/Node 4레인 전체)은 실행하지 않음.
  BENCHMARK.md의 언어 간 비교는 P1-1 수정 전에는 신뢰할 수 없음.
- 강제 실패 canary는 `UV_JIT=2` + `drive(...)` 프로그램으로 exit 2와
  `JITSTRICT: ... failed JIT codegen: 2:drive` 메시지를 확인(실제 소스 유래 후보).
- 사용자 WIP의 중간 상태(03:07 lsp.rs 빌드 불가)를 관찰했으나 03:14 재수정으로
  해소되었고 최종 빌드는 성공했다. 이 파일들은 수정하지 않았다.

---
---

# Part II — Independent Full Audit Review (2026-11-20, Read-Only, Second Pass)

> 이 Part는 위 Part I(기존 감사 기록)를 **덮어쓰지 않고 보존한 상태에서 추가**한 독립 재감사 결과다.
> Part I의 발견사항(P1-1 등)을 **그대로 신뢰하지 않고 처음부터 다시 검증**했다.
> 이 감사는 READ-ONLY다. 생성한 파일은 `docs/audit/` 하위의 `r2_*` 로그와
> 재현용 `.un` 파일, 그리고 이 보고서의 Part II 추가분뿐이며, 기존 소스·문서·테스트·CI·WIP
> 파일은 읽기만 했다. 커밋/푸시/스테이징/포맷팅/리팩터링은 하지 않았다.

## 1. Executive Summary

| 항목 | 결과 |
|---|---:|
| 감사 범위 | 저장소 전체(18개 영역), 코드·실행·테스트·CI·문서·성능·위생 |
| 실행/확인한 검사 수 | 32 |
| PASS | 23 |
| FAIL | 5 |
| BLOCKED | 1 |
| NOT VERIFIED | 3 |
| P0 | 0 |
| P1 | 1 (AUDIT-001) |
| P2 | 4 (AUDIT-002/003/004/008) |
| P3 | 3 (AUDIT-005/006/007) |

가장 중요한 결론: **Part I의 핵심 발견(JIT가 첫 호출 이후 조용히 인터프리터로 퇴화한다)을
완전히 독립적으로 재현했다.** 정적 원인(공유 JIT 엔트리 맵)과 동적 증거(1회 27 ms vs 2회 1,265 ms),
그리고 공식 벤치 게이트의 수치(int_ops 1,706 ms, Un JIT/Python 기하평균 0.259)가 서로 일치한다.
모든 바이트 동치 게이트가 green인데도 이 회귀는 잡히지 않는다. **`UV_JIT=2`는 일반 테스트 48개
전체에서 exit 2가 나는 canary 전용 게이트이며, 동치 검증 레인이 아니다**는 점을 새로 명확히 했다.

전체 프로젝트 상태: 기능·테스트·빌드 자체는 건전하나, **핵심 JIT 기능이 실질적으로 비활성**인
상태에서 벤치마크 문서만 성능 우위를 주장하고 있는 "부분 완료(일부 환경 의존 검증 미실시)" 상태다.

## 2. Audit Scope and Environment

| 항목 | 값 / 명령 |
|---|---|
| 실행 환경 | Windows, PowerShell, 작업 디렉터리 `C:\dev\un` |
|rustc / cargo | `rustc 1.100.0 (0ef1bc386 2026-11-09)` / `cargo 1.100.0 (3acc79c0c 2026-11-05)` |
| toolchain | active `stable-x86_64-pc-windows-gnu` (rust-toolchain.toml override), default `stable-x86_64-pc-windows-msvc` |
| 설치 target | `wasm32-unknown-unknown`, `x86_64-pc-windows-gnu`, `x86_64-unknown-linux-gnu` |
| HEAD / branch | `36aa40d` / `main` |
| CLI | `un.exe` (13,598,064 B), 명령: `run/compile/check/native/obj/build/lsp/help` |
| `--version` | 미지원 (`Unknown command: --version`, exit 0) — 문서상 버전 명령 없음 |
| 빌드 산출물 | `rust/target/release/un.exe`, `unvm-audit-old.exe`(11,779,077 B, 2026-11-18) — 모두 gitignore, 추적 바이너리 0 |
| 테스트 부작용 사전 확인 | `test.ps1`/`differential.ps1`는 un.exe만 실행, `unbc-roundtrip.ps1`는 `.unbc`를 생성 후 finally 삭제, `obj-demo`/`exe-diff`는 `%TEMP%\{guid}` 사용, `bench.ps1`/`compile-bench.ps1`는 `-NoSave`로 `bench/**` 미변경 — 사용자 WIP 접근 없음 |

감사 시작 기준점(원문):
```
 M README.md
 M docs/README.md
 M docs/guides/CLI.ko.md
 M docs/guides/CLI.md
 M main.un
 M rust/src/main.rs
?? docs/audit/
?? rust/src/lsp.rs
```
위 7개를 사용자 소유 보호 대상으로 등록했고, 감사 종료 시까지 **변경되지 않았다**(중간 확인 1회 수행).

## 3. Repository Baseline

실제 측정 명령과 값(Part I의 수치를 재사용하지 않고 재측정):

| 항목 | 측정 명령 | 실제 값 | AGENTS.md 문서값 |
|---|---|---:|---:|
| Rust .rs 파일 | `Get-ChildItem rust -Recurse -Filter *.rs \| Where target 제외` | 91 | 84 |
| rust/src 라인 | `(Get-Content).Count` 합 | 54,028 | 40,766 |
| 전체 .rs 라인(target 제외) | 동일, rust 전체 | 54,420 | 40,766 |
| `.un` 파일 | `Get-ChildItem test -Recurse -Filter *.un` | 135 | 101 |
| Positive manifest | `test/MANIFEST.txt` 유효행 | 48 | 47 |
| Negative manifest | `test/negative/manifest.txt` 유효행 | 50 | 48 |
| KnownFailures / KnownDivergent | 유효행 | 0 / 0 | 0 / 0 |
| DECISIONS | `^## D-\d+` | 92 (D-001…D-092, 중복 0) | D-001–D-077 |
| docs 구조 | spec 2 / guides 4 / compile 7 / project 5 | 일치 | 일치 |
| scripts | 8개 (bench, compile-bench, compile-check, differential, exe-diff, obj-demo, test, unbc-roundtrip) | — | AGENTS.md에 `exe-diff.ps1` 누락 |
| 추적 파일 수 | `git ls-files \| Measure-Object` | 368 | — |

## 4. Overall Verification Matrix

| ID | 검사 항목 | 기대 결과 | 실제 결과 | 상태 | 증거 |
|---|---|---|---|---|---|
| CHK-01 | 초기 Git 상태/보호 목록 확보 | 기록됨 | 7개 WIP 기록 | PASS | §2 원문 |
| CHK-02 | 툴체인 정보 | 기록됨 | §2 표 | PASS | `rustc/cargo/rustup show` |
| CHK-03 | Release 빌드 | exit 0, warning 0 | exit 0, warning 0 (0.09 s 캐시) | PASS | §9 |
| CHK-04 | cargo test --workspace | 전부 통과 | 213 + 5 + doc 0 = 218 passed, 0 failed/ignored | PASS | §9 |
| CHK-05 | WASM 빌드 | exit 0 | exit 0, 8.9 s (target 설치됨) | PASS | §9 |
| CHK-06 | 산출물/CLI 열거 | un.exe, 명령 목록 | un.exe 존재, 7명령, 추적 바이너리 0 | PASS | §2 |
| CHK-07 | scripts/test.ps1 | 47/1538 기대(문서) | **48 files / 1570 passed / 0 failed / 0 crashed**, Negative 50, Known 0 | PASS | r2_test_ps1.log |
| CHK-08 | differential.ps1 | 0 differences | **98 files, 0 differences** | PASS | r2_differential.log |
| CHK-09 | unbc-roundtrip -IncludeNegative | 바이트 동일 | 48/48 identical, Negative 41 identical + 9 skip | PASS | r2_unbc_roundtrip.log |
| CHK-10 | strict canary ×2 | exit 0, 마커 | 두 마커 모두, exit 0 | PASS | §10 |
| CHK-11 | compile-check/exe-diff/obj-demo/compile-bench | 통과 | 23/0, 14/0, PASS, 3모드 정상 | PASS | r2_*.log |
| CHK-12 | bench -Group un / all (-NoSave) | 실행 완료 | 76.2 s, 4레인 완료 | PASS | r2_bench_*.log |
| CHK-13 | Manifest 전수 무결성 | 경로/중복/그룹 정상 | 48행, 누락 0, 중복 0, 그룹 1/42/4/1 | PASS | §10 |
| CHK-14 | UV_JIT=0 ↔ default 동치 | 동일 | 144회 스윕 중 이 두 레인은 100% 동일 | PASS | r2_jit_timing.log, §12 |
| CHK-15 | UV_JIT=2 동작 | canary 전용 | 일반 48개 전부 exit 2 (`JITSTRICT: …failed JIT codegen`) — 설계대로 | PASS | §12 |
| CHK-16 | un native 동치 | core subset 동일 | 수락 15개 중 14개 바이트 동일, 1개는 BENCH 타이밍만 차이(의미 동일) | PASS | §12 |
| CHK-17 | JIT 실제 발동 여부 | 2회차도 JIT | 2회차 인터프리터 퇴화 | **FAIL** | AUDIT-001 |
| CHK-18 | AGENTS.md 전수 | 수치/CLI 일치 | 다수 불일치 | **FAIL** | AUDIT-002 |
| CHK-19 | README 한/영 쌍 | 동일 | KO 코드 블록 손상 + form feed | **FAIL** | AUDIT-004 |
| CHK-20 | CLI 문서 한/영 쌍 | 의미 동일 | headings 20/20 동일, 전문 대조 미실시 | NOT VERIFIED | §11 |
| CHK-21 | ARCHITECTURE/BYTECODE 문서 | 코드와 일치 | ISA·TD-12 기술 불일치 | **FAIL** | AUDIT-003/005 |
| CHK-22 | Project 문서(DECISIONS 등) | 현재 상태 반영 | D-077 표기 등 stale | **FAIL** | AUDIT-002 |
| CHK-23 | UNBC 파서/검증기 | 손상 입력 우아한 거부 | magic/version/절단 모두 exit 1, panic 없음 | PASS | §13 |
| CHK-24 | Sandbox 경로 탈출 동적 검증 | 4종 경로 차단 | WASM 호스트 실행 불가 → 정적 검토로 환원 | BLOCKED | §13, §16 |
| CHK-25 | HTTP/WS CRLF 방지 | 프로토콜 경계 차단 | `net.rs:73`, `http.rs:185-191` 정적 확인 | PASS | §13 |
| CHK-26 | go/wait 동시성 심층 | 오류전파/중복대기 등 | 정적 + differential 바이트 동치까지만 | NOT VERIFIED | §13, §16 |
| CHK-27 | JIT rebound parameter(TD-52) | 3레인 동일 | 3레인 `0,1666716667` 동일, 행 안 함 | PASS | §12 |
| CHK-28 | Strict JIT 강제 실패 | exit 2 + JITSTRICT | exit 2, proto 목록 출력 | PASS | §12 |
| CHK-29 | Union tag 계약 | 0/1/2/3/5/6 일치 | `jit_cache.rs:8-13` ↔ 소비자 정적 일치 | PASS | §12 |
| CHK-30 | IntVec/IntDict 소유권 | 이중해제 없음 | 정적 검토만, 메모리 검사기 미실시 | NOT VERIFIED | §16 |
| CHK-31 | 저장소 위생 | 추적 산출물 없음 | 추적 바이너리 0, `.unbc` 잔여 0 | PASS | §15 |
| CHK-32 | 감사 종료 Git 무결성 | baseline과 동일 | baseline과 동일(보고서만 docs/audit 내 추가) | PASS | §18 |

집계: PASS 23 / FAIL 5 / BLOCKED 1 / NOT VERIFIED 3 = 32.

## 5. P0 Findings

없음. 데이터 손실, 메모리 안전성 붕괴, 광범위 실행 불능, 중대 보안 취약점은 확인되지 않았다.

## 6. P1 Findings

### [AUDIT-001] JIT가 인자 있는 proto의 두 번째 호출부터 조용히 인터프리터로 퇴화한다

- Severity: P1
- Category: JIT 정확성/성능 회귀, 문서 불일치
- Status: CONFIRMED (정적 + 동적 + 공식 벤치 게이트三重 재현)
- Location: `rust/src/jit_cache.rs:70,119-148`, `rust/src/jit_fast.rs:20,72-76`, `rust/src/jit_bc.rs:608-614`
- Related Files: `rust/src/vm/mod.rs:1611-1649`, `BENCHMARK.md`
- Related Tests: `test/jit/strict.un`, `test/jit/strict_float.un`, `scripts/differential.ps1`, `scripts/compile-bench.ps1`, `scripts/bench.ps1`
- Related Documents: `AGENTS.md:12,50`, `docs/spec/ARCHITECTURE.md`, `BENCHMARK.md:31,42,52`

**1. 문제 설명.** bytecode JIT와 Compile Mode의 TIR이 **하나의 `JIT_FNS` 맵**을 공유하는데,
`try_call_jit()`이 TIR 조회를 먼저 하고, bytecode JIT가 등록한 엔트리(`arg_kinds: Vec::new()`)를
TIR 엔트리로 오인해 arity 불일치로 `None`을 반환 → 인터프리터 fallback 된다. 첫 호출만 정상 JIT.

**2. 증거.**

정적:
```
jit_cache.rs:70   static JIT_FNS: ... HashMap<u64, HashMap<u32, JitEntry>>
jit_cache.rs:140  get_jit_entry  -> JIT_FNS.lock()...get(&program_id)?.get(&proto_index)
jit_cache.rs:147  get_tir_entry  -> JIT_FNS.lock()...get(&program_id)?.get(&proto_index)   // 동일 맵
jit_bc.rs:608-614 register_jit_entry(..., JitEntry{ ..., arg_kinds: Vec::new() })
jit_fast.rs:72    let tir_entry = get_tir_entry(...);
jit_fast.rs:73-75 if let Some(entry) = tir_entry { return try_call_tir_entry(...); }
jit_fast.rs:20    if entry.arg_kinds.len() != args.values.len() { return None; }  // 0 vs N
```
동적(본 감사 재현, `docs/audit/evidence/r2_jit_timing.log`):
```
one_call defaultJIT ms: 28.4, 27.2, 26.1
two_call defaultJIT ms: 1268.3, 1273.9, 1254.1
one_call UV_JIT=0 ms: 1271.5, 1255.3
two_call UV_JIT=0 ms: 2451.6, 2460.2
```
공식 게이트(`r2_compile_bench.log`): `un run (VM+JIT) 200.682 ms` vs `un run (UV_JIT=0) 200.223 ms`
— JIT 레인이 인터프리터와 동속. `r2_bench_all.log`: `Un JIT / Python 0.259`(BENCHMARK.md는 7.527x).

**3. 기대 동작.** JIT 후보 proto는 등록 이후 모든 호출에서 네이티브 코드로 실행되어야 하고,
`BENCHMARK.md`가 주장하는 int_ops ≈2.84 ms / Un JIT > Python이 재현되어야 한다.

**4. 실제 동작.** 1회차만 ≈27 ms로 네이티브 실행, 2회차부터 ≈1,238 ms 추가 비용(인터프리터 1회분).
두 번째 호출에서 `get_tir_entry()`가 bytecode 엔트리를 반환하고 `arg_kinds` 길이 0 ≠ 인자 수 N으로
`None` → fallback.

**5. 영향 범위.** VM/JIT 런타임 전반. 정답은 바뀌지 않아 differential 98/0은 green이지만,
JIT 성능 주장 전부가 무효화되고 `bench.ps1`의 "un JIT" 레인이 2026-11-19 이후 인터프리터를
측정해 왔다. Compile Mode(`un native`)는 별도 경로라 영향 없음.

**6. 재현 방법.** `docs/audit/jit_repro_one.un`(1회)과 `jit_repro_two.un`(2회)을
`.\rust\target\release\un.exe run` 후 각각wall-clock 비교. 또는
`powershell -File scripts\bench.ps1 -Group un -NoSave`의 `int_ops` 값을 BENCHMARK.md:52와 대조.

**7. 근본 원인.** 확정. 두 등록 함수(`register_jit_entry`/`register_tir_entry`)가 같은 맵에 쓰고,
두 조회 함수가 같은 맵에서 읽으며, entry 종류를 구분하는 필드가 없음.

**8. 제안 조치(승인 후; 지금은 하지 않음).** bytecode JIT 엔트리와 TIR 엔트리를 별도 맵으로 분리하거나
`JitEntry`에 `kind: Unbc|Tir`를 추가해 `get_tir_entry()`가 bytecode 엔트리를 반환하지 않게 한다.
`try_call_jit()`에서 TIR 조회 실패 시 bytecode 조회로 계속 진행하도록 순서도 점검. 병행해
"JIT가 실제 발동했는지"를 보는 카운터/타이밍 canary를 게이트에 추가해야 재발을 잡을 수 있다
(바이트 diff와 `UV_JIT=2`로는 불가). 범위: `jit_cache.rs`, `jit_fast.rs` 2파일 + canary/벤치 재생성.

**9. 검증 방법.** 수정 후 (a) 위 1회/2회 타이밍이 모두 JIT 수준(≈30 ms)인지,
(b) `scripts/bench.ps1 -Group all -NoSave`의 `Un JIT / Python`이 BENCHMARK.md 수치 근처로 회복되는지,
(c) `scripts/differential.ps1` 98/0, `scripts/test.ps1` 48/1570 유지, (d) strict canary ×2 통과.

**10. 관련 기준.** AGENTS.md:12,50(JIT 설명), `.github/workflows/build.yml:44-72`(게이트),
`BENCHMARK.md:31,42,52`, TECHNICAL_DEBT.md TD-51(동일 부류의 "조용한 실패" 기록).

## 7. P2 Findings

### [AUDIT-002] 기준 수치·CLI 목록·DECISIONS 범위가 AGENTS.md 및 다수 문서에서 stale

- Severity: P2
- Category: 문서-구현 불일치, 테스트 기준 신뢰성
- Status: CONFIRMED
- Location: `AGENTS.md:11,28,199-200`, `docs/project/REFACTOR_SUMMARY.md:77-79,107`, `docs/guides/TEST_GUIDE.md:78-79,143,152,154`, `docs/compile/COMPILE_TESTS.md:11-17`, `docs/spec/ARCHITECTURE.md:196`, `docs/project/PROJECT_AUDIT.md:17,175-176`, `README.md:355-356,698`, `docs/README.md:363-364,725`
- Related Files: `test/MANIFEST.txt`, `test/negative/manifest.txt`, `docs/project/DECISIONS.md`
- Related Tests: `scripts/test.ps1`, `scripts/differential.ps1`, `scripts/unbc-roundtrip.ps1`
- Related Documents: 위 Location 전체

**1. 문제 설명.** 문서의 "현재 기준" 수치가 실제 게이트 실행값보다 뒤처져 있다.

**2. 증거.** 문서값 → 실제값: Positive 47/1538 → 48/1570; Negative 48 → 50; differential 95 files → 98;
roundtrip 47/47+39/39 → 48/48+41; Rust 84 files/40,766 lines/.un 101 → 91/54,420/135;
DECISIONS D-001–D-077 → D-001–D-092(중복 0). AGENTS.md:28과 README 쌍의 CLI 목록은
`tokens/ast/desugar/opt/dis/dis-reg/plan/help`라고 적었으나 실제는
`run/compile/check/native/obj/build/lsp/help`이고 `plan` 명령은 존재하지 않는다.
`unvm.exe`로 잘못 기재된 곳은 활성 문서에 없고 Part I evidence 파일(`jit-bisect.md:26`)에만 있다.

**3. 기대 동작.** 기준 문서의 수치·명령 목록이 실제 코드/게이트와 일치.

**4. 실제 동작.** 위 표 참조. `REFACTOR_SUMMARY.md`는 "all verified 2026-11-17"로 날짜를 명시해
역사 기록으로는 성실하나 현재 상태를 읽는 독자에게 오해를 준다.

**5. 영향 범위.** 문서 신뢰성, 온보딩, 감사 기준(AGENTS.md가 기준값으로 쓰이므로 오해 유발).
런타임 동작에는 영향 없음.

**6. 재현 방법.** §3 표의 명령으로 재측정 후 각 문서 라인과 대조.

**7. 근본 원인.** 단일 baseline 원본 없이 여러 문서에 수치를 복제했고 기능 추가(테스트 1개, negative 2개,
DECISIONS 15개, Compile Mode 명령) 이후 갱신하지 않음.

**8. 제안 조치.** baseline 표를 `docs/guides/TEST_GUIDE.md` 한 곳에서 관리하고 나머지는 링크.
AGENTS.md 수치·CLI 목록 갱신. 단 README/CLI.ko/CLI.md/AGENTS.md는 사용자 WIP이므로 **사용자 확인 후** 반영.

**9. 검증 방법.** 갱신 후 `scripts/test.ps1`, `differential.ps1`, `unbc-roundtrip.ps1 -IncludeNegative`
출력을 문서 수치와 재대조.

**10. 관련 기준.** AGENTS.md:11,28,199-200; test/MANIFEST.txt(48); test/negative/manifest.txt(50).

### [AUDIT-003] `docs/spec/BYTECODE.md`가 실제 Rust ISA가 아닌 삭제된 C# 참조 구현을 기술

- Severity: P2
- Category: 문서-구현 불일치
- Status: CONFIRMED
- Location: `docs/spec/BYTECODE.md:29,71-73,98,100,103,262,271,290,297,299`
- Related Files: `rust/src/bytecode/instr.rs:2-3,5,124,126`, `rust/src/bytecode/mod.rs:159-`
- Related Tests: `rust/src/bytecode/unbc.rs` 테스트, `scripts/unbc-roundtrip.ps1`
- Related Documents: `docs/spec/ARCHITECTURE.md`

**1. 문제 설명.** 문서의 opcode 표가 C# 시대 이름(`LOAD_NONE`, `AND`, `OR`, `COALESCE`,
`WHILE_NOT`, `ENTER_SCOPE`, `JMP_FALSE`)을 쓰고 예약 영역을 잘못 기술한다.

**2. 증거.** 실제 `instr.rs:2-3`은 `#[repr(u8)] pub enum Op`이며 `LoadNone=0x01`, `Await=0x64`,
`FusedIntMulAdd=0x7F`다. 문서 `:299`는 "`0x65..=0x7F`는 비워 둔다"고 하지만 `0x7F`는 사용 중이다.
`:262`의 "예약 0x62..0x7F"도 동일 모순. `:271,290`은 삭제된 `Evaluator.cs:1309/1087`을 근거로 인용.
한편 `#[repr(u8)]` 적용 자체는 코드에서 확인되어 이 항목은 PASS(문서만 불일치).

**3. 기대 동작.** 문서 opcode 표가 `Op` enum의 실제 값/이름과 1:1 대응.

**4. 실제 동작.** 이름·값·예약 구간·근거 파일 모두 불일치.

**5. 영향 범위.** `.unbc` 포맷을 문서로 이해하려는 개발자/감사자에게 오해. 런타임 동작 무영향.

**6. 재현 방법.** `BYTECODE.md` opcode 표와 `instr.rs` enum을 라인별 대조.

**7. 근본 원인.** Rust 포트 시 문서를 C# 용어로 남겨 두고 재작성하지 않음.

**8. 제안 조치.** 현재 Rust `.unbc` 기준으로 전면 재작성, opcode 표를 `instr.rs::Op`에서 생성 또는
최소한 `#[repr(u8)]` 값 표 동기화. 코드는 변경하지 않는다.

**9. 검증 방법.** 재작성 후 `scripts/unbc-roundtrip.ps1 -IncludeNegative` 48/48+41 유지 및
`cargo test --release`의 opcode 값 단언 테스트 통과.

**10. 관련 기준.** AGENTS.md "bytecode/ Op enum explicit #[repr(u8)]" 항목; docs/spec/BYTECODE.md.

### [AUDIT-004] 한국어 README의 실행 예제 코드 블록이 산손상되고 form feed 문자를 포함 (사용자 WIP)

- Severity: P2
- Category: 문서-구현 불일치, 사용자 WIP
- Status: CONFIRMED (보고만, 수정하지 않음)
- Location: `README.md:558,576`
- Related Files: `docs/README.md:594-604` (정상 영어 쌍)
- Related Tests: 해당 예제를 복사→실행 시 실패
- Related Documents: README 한/영 쌍 동기화 규칙(AGENTS.md 코드 컨벤션)

**1. 문제 설명.** ```un 펜스 안에 코드 대신 한국어 설명 문장이 들어가 있고, `flow.spawn` 앞에
리터럴 form feed(0x0C)가 붙어 `low.spawn`으로 깨져 있다.

**2. 증거.** KO `README.md:576` 펜스 내부:
`- map과 submit을 제공하는 \flow.spawn(n) 풀 핸들 (…)` (0x000C 확인: hex `000C 006C 006F 0077`).
EN `docs/README.md:599` 같은 위치: `using pool = flow.spawn(2)`. 동일 문장이 KO `:558`에도 복제됨.

**3. 기대 동작.** KO/EN 예제가 동일한 실행 가능 코드를 담아야 함.

**4. 실제 동작.** KO 블록은 주석 문장이 코드 자리에 있어 복사 실행 시 파싱 실패.

**5. 영향 범위.** 사용자 문서의 예제 신뢰성. 프로덕션 코드 영향 없음.

**6. 재현 방법.** `README.md:571-581` 블록과 `docs/README.md:594-604` 블록 대조.

**7. 근본 원인.** WIP 편집 중 코드 줄이 설명 문장으로 대체되고 `\f` 이스케이프가 리터럴 제어문자로 저장됨.

**8. 제안 조치(사용자 승인 후).** KO `:576`을 `using pool = flow.spawn(2)`로 복원하고 `:558` 중복
문장을 EN의 atomic/single-thread 설명에 맞춰 정리. **사용자 소유 파일이므로本报告에서는 수정하지 않음.**

**9. 검증 방법.** 수정 후 두 README의 해당 코드 블록을 실제 `un run`으로 실행해 같은 출력 확인.

**10. 관련 기준.** AGENTS.md "README.md와 docs/README.md는 같은 내용의 한/영 쌍" 항목.

### [AUDIT-008] `BENCHMARK.md`의 성능 우위 주장이 현재 트리에서 재현되지 않음

- Severity: P2
- Category: 성능 문서 불일치 (AUDIT-001의 직접 결과)
- Status: CONFIRMED
- Location: `BENCHMARK.md:31,42,52,72,140,158-161`
- Related Files: `scripts/bench.ps1`, `scripts/compile-bench.ps1`
- Related Tests: `scripts/bench.ps1 -Group all -NoSave`
- Related Documents: `docs/guides/BENCHMARK_GUIDE.md`

**1. 문제 설명.** 문서는 "Un JIT이 Python보다 기하평균 7.527x 빠르다", int_ops JIT 2.8434 ms를
주장하나 현재 측정값은 정반대다.

**2. 증거.** `r2_bench_all.log`: `Un JIT / Python 0.259`, `Un JIT / Node 0.006`,
`int_ops un jit 1706.891 ms / python 2550.031 ms / node 9.637 ms`.
`r2_bench_un.log` int_ops `1646.710 ms`. `r2_compile_bench.log` VM+JIT ≈ UV_JIT=0.

**3. 기대 동작.** 문서 수치가 현재 구현에서 재현되거나, 생성 시점/commit이 명시되어야 함.

**4. 실제 동작.** 약 600x(int_ops), 기하평균 비율 약 29x 차이. (AUDIT-001 미해결 시 재현 불가)

**5. 영향 범위.** 성능 마케팅/의사결정 오도. 기능 정확성 무영향.

**6. 재현 방법.** `powershell -File scripts/bench.ps1 -Group all -NoSave` 후 `BENCHMARK.md` 대조.

**7. 근본 원인.** AUDIT-001(JIT 퇴화) + 벤치 결과가 특정 커밋(Part I 기준 `4b4440d`) 스냅샷에서
생성된 뒤 미재생성.

**8. 제안 조치.** AUDIT-001 수정 완료 후 BENCHMARK.md를 동일 머신/동일 스크립트로 재생성하고
생성 커밋·날짜·환경을 명기.

**9. 검증 방법.** 재생성 후 `Un JIT / Python`이 문서값 근처인지 재측정.

**10. 관련 기준.** BENCHMARK.md:31,42; docs/guides/BENCHMARK_GUIDE.md.

## 8. P3 Findings

### [AUDIT-005] `ARCHITECTURE.md`의 TD-12 설명이 이미 해결된 코드와 모순

- Severity: P3
- Category: 문서 staleness
- Status: CONFIRMED
- Location: `docs/spec/ARCHITECTURE.md:88-90`
- Related Files: `rust/src/vm/mod.rs:64,346,354,371,793`, `docs/project/TECHNICAL_DEBT.md:47`
- Related Tests: 없음(문서 대조만)
- Related Documents: TECHNICAL_DEBT.md(TD-12 resolved)

**1. 문제 설명.** "TryEntry.depth는 기록만 되고 읽히지 않는다 (TD-12)"라고 적혔으나 실제 필드는
`acc_depth`이며 세 곳에서 `args.truncate()`에 사용된다.

**2. 증거.** `vm/mod.rs:64 pub acc_depth: usize`, `:346/:354/:371 self.args.truncate(entry.acc_depth)`,
`:793 acc_depth: self.args.len()`. TECHNICAL_DEBT.md:47은 TD-12를 resolved로 기록.

**3. 기대 동작.** 문서가 현재 필드명·사용처·해결 상태를 반영.

**4. 실제 동작.** 문서만 과거 상태 유지 → 문서 간 모순.

**5. 영향 범위.** 작은 문서 오해. 런타임 영향 없음.

**6. 재현 방법.** 위 라인 대조.

**7. 근본 원인.** TD-12 해결 시 ARCHITECTURE.md 미갱신.

**8. 제안 조치.** 해당 문장을 "handler 진입 시 `acc_depth`로 args를 truncate한다(TD-12 해결)"로 갱신 제안.

**9. 검증 방법.** 갱신 후 TECHNICAL_DEBT.md와 상호 참조 일치 확인.

**10. 관련 기준.** AGENTS.md 기준문서 목록, TECHNICAL_DEBT.md TD-12.

### [AUDIT-006] Sandbox confine 경로에 자동 테스트가 없고 symlink 정규화가 없음

- Severity: P3
- Category: 보안 테스트 커버리지, 정적 관찰
- Status: CONFIRMED(테스트 부재) / SUSPECTED(symlink 우회 가능성)
- Location: `rust/src/native/io.rs:48-93,117-122`
- Related Files: `test/**` (문자열 미존재), `rust/playground/src/lib.rs`
- Related Tests: `"escapes the sandbox"` 검색 결과 test/ 디렉터리 0건
- Related Documents: docs/guides/CLI.md(sandbox 언급 여부 미확인)

**1. 문제 설명.** `normalize_under()`는 드라이브문자·UNC·RootDir·`..` 탈출을 거부하고
`starts_with_case_insensitive()`로 최종 확인하는 등 정적 구조는 건전하다. 그러나 (a) 이 경로를
검증하는 자동 테스트가 없고, (b) `std::fs::canonicalize`가 없어 베이스 내부 symlink가 외부를
가리키는 경우를 차단하지 못한다(정적 추론).

**2. 증거.** `io.rs:52-55` 드라이브/UNC 거부, `:59-67` Prefix/RootDir/ParentDir 처리,
`:71-75` 최종 prefix 검사, `:121` 오류 `"io.open: path escapes the sandbox root"`.
`grep -R "escapes the sandbox" test/` → 0 matches. 동적 WASM confine 실행은 BLOCKED(§16).

**3. 기대 동작.** 4종 탈출 입력(`C:/`, `\\server\share`, `..//`, 중첩 상대경로)이 모두
`escapes the sandbox` 계열 오류로 거부되고 회귀 테스트가 존재.

**4. 실제 동작.** 구현은 거부하나 테스트 근거 없음. symlink 케이스는 미검증.

**5. 영향 범위.** Playground/WASM sandbox 신뢰성. CLI `un run`은 sandbox 비활성(신뢰 실행).

**6. 재현 방법.** `normalize_under` 단위 테스트를 `native/io.rs`의 `#[cfg(test)]`에 추가 후
위 4종 입력 검증(제안). 현재는 코드 검토만 가능.

**7. 근본 원인.** sandbox가 playground 전용이며 WASM 실행 호스트가 이 저장소에 없어 동적 검증 누락.

**8. 제안 조치.** `#[cfg(test)]` 단위 테스트 추가 + symlink 해석 정책(canonicalize 또는 symlink 거부) 명시.
코드는 승인 후에만 변경.

**9. 검증 방법.** `cargo test --release`에 신규 테스트 포함, 두 모드(JIT on/off)에서 동일 오류 확인.

**10. 관련 기준.** AGENTS.md "PlaygroundRunner의 Rust 대응", TECHNICAL_DEBT.md TD-51 계열.

### [AUDIT-007] 레거시 바이너리·벤치 결과 디렉터리 잔존 (gitignored)

- Severity: P3
- Category: 저장소 위생
- Status: CONFIRMED
- Location: `rust/target/release/unvm-audit-old.exe`, `bench/results/**`
- Related Files: `.gitignore:1-8`
- Related Tests: 없음
- Related Documents: AGENTS.md 알려진 함정

**1. 문제 설명.** `unvm-audit-old.exe`(11.8 MB, 2026-11-18)와 다수의 `bench/results/<timestamp>/`
디렉터리(일부 `-broken` 접미사)가 남아 있다.

**2. 증거.** `git ls-files`로 추적 바이너리 0개 확인. `.gitignore`가 `/rust/target/`, `/bench/results/`,
`*.unbc`를 제외. `Get-ChildItem test -Recurse -Filter *.unbc` → 잔여 0. (본 감사의 bench 실행은
`-NoSave`로 새 디렉터리를 만들지 않음.)

**3. 기대 동작.** gitignored 산출물은 남아 있어도 추적되지 않으면 문제 없음.

**4. 실제 동작.** 기능 영향 없음, 디스크 정리 대상.

**5. 영향 범위.** 없음(정리 제안만).

**6. 재현 방법.** `Get-Item rust\target\release\unvm-audit-old.exe`, `Get-ChildItem bench\results -Directory`.

**7. 근본 원인.** 과거 감사/벤치 산출물 미정리.

**8. 제안 조치(승인 후).** 위 항목 삭제 제안만. **감사 중 삭제하지 않음.**

**9. 검증 방법.** 삭제 후 `git status --porcelain` 변화 없음 확인.

**10. 관련 기준.** AGENTS.md ".gitignore" 항목, "생성된 빌드 산출물을 저장소에 추가하지 않는다".

## 9. Build and Toolchain Results

| 검사 | 명령 | 결과 |
|---|---|---|
| Release build | `rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml` | exit 0, `Finished in 0.09s`, warning 0, error 0 (캐시 히트) |
| Workspace test | `... cargo test --release --manifest-path rust/Cargo.toml --workspace` | lib 213 passed + playground 5 passed + doc 0+0; 0 failed, 0 ignored; 10.9 s |
| WASM | `... cargo build --release --target wasm32-unknown-unknown ... -p unvm-playground` | exit 0, `Finished in 8.89s`; `wasm32-unknown-unknown` 설치됨; unvm + unvm-playground 컴파일 |
| 산출물 | `Get-Item` | un.exe 13,598,064 B; unvm-audit-old.exe 11,779,077 B; 추적 바이너리 0 |

경고는 한 건도 없었다. Part I이 관찰한 `un.exe` OS 잠금(os error 5)은 이번 감사에서 재현되지 않았다.

## 10. Test Integrity Results

| 게이트 | 결과 (원문 발췌) | 판정 |
|---|---|---|
| `scripts/test.ps1` | `Positive: 48 files, 1570 passed, 0 failed, 0 crashed` / `Negative: 50 matched, 0 mismatched, 0 documented divergences` / `Known failures: 0 still failing, 0 unexpectedly passing` / `Overall exit code: 0` | PASS |
| `scripts/differential.ps1` | `Differential: 98 files, 0 differences` | PASS |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | `RoundTrip(positive): 48 files, 48 identical, 0 mismatched` / `RoundTrip(negative): 41 identical, 0 mismatched, 9 skipped(compile-rejected)` | PASS |
| strict canary | `jit-strict canary OK` (exit 0), `jit-float-strict canary OK` (exit 0) | PASS |
| compile-check / exe-diff / obj-demo | `23 passed, 0 failed` / `14 passed, 0 failed` / `obj-demo: fib = 55`, `PASS` | PASS |
| compile-bench (-NoSave) | `un run (VM+JIT) 200.682` / `UV_JIT=0 200.223` / `un native 171.397 ms`, all `2/0` | PASS(실행), AUDIT-001 시사 |

- Manifest 무결성: 48행, 그룹 `suite:1 unit:42 integration:4 regression:1`, 누락/중복 0.
  manifest에 없는 `.un` 파일은 assert 헬퍼·negative fixture·compile fixture·jit canary로 의도된 것들.
- CI `.github/workflows/build.yml:18-72`: Checkout → Install GNU toolchain → Build(release) →
  Test(test.ps1) → Test(cargo test) → Test(differential) → Test(.unbc round-trip) →
  `JIT strict canary (UV_JIT=2)` → `JIT Float strict canary (UV_JIT=2)`. **양 canary 스텝 존재 확인.**
  문서(AGENTS.md:90)의 묘사와 단계 이름·순서 일치. 단 CI는 `cargo test`를 `--workspace` 없이 실행하는데
  이번 로컬 실행은 `--workspace`로 218개를 확인했으므로 CI는 lib 213개만 커버(playground 5개 미포함) — 경미한 차이.

## 11. Documentation Consistency

| 대상 | 결과 |
|---|---|
| AGENTS.md | FAIL — AUDIT-002(수치·CLI·DECISIONS), AUDIT-001(JIT 설명이 실제 동작과 모순) |
| README.md ↔ docs/README.md | FAIL — AUDIT-004(KO 코드 블록), AUDIT-002(수치 47/1538/48) |
| docs/guides/CLI.md ↔ CLI.ko.md | NOT VERIFIED — heading 20/20 동일, 라인 345/331, 전문 의미 대조는 미실시. 다만 양 파일 모두 사용자 WIP |
| docs/spec/ARCHITECTURE.md | FAIL — AUDIT-003 일부, AUDIT-005(TD-12), `:196` differential 95→98 |
| docs/spec/BYTECODE.md | FAIL — AUDIT-003 |
| docs/project/* (5개) | FAIL — AUDIT-002(REFACTOR_SUMMARY/PROJECT_AUDIT), TD-51/52/53 상태 기술은 코드와 대체로 일치 |
| docs/compile/* (7개) | 부분 FAIL — COMPILE_TESTS/COMPILE_PROGRESS의 47/1538/48, 95 files, 39/39 stale |
| `unvm.exe` 오기 | 활성 문서에는 없음(Part I evidence 파일에만 존재) — 해당 점검 PASS |

## 12. VM / JIT / Compiler Correctness

- **결정적 동치(UV_JIT=0 ↔ default):** 48개 positive × 2레인 + differential 98개 파일 0 차이. 통과.
  별도 144회 스윕에서 이 두 레인의 stdout/stderr/exit는 항상 동일했다.
- **`UV_JIT=2`의 성격 재정의:** 일반 48개 파일 전부 exit 2. 예: `test/unit/runtime/operators.un` →
  `JITSTRICT: 9/9 candidate(s) failed JIT codegen: 3:assert_true, … 11:assert_err_eq` +
  `JITSTRICT: curate the workload or unset UV_JIT …`. 따라서 strict 모드는 **canary 전용 게이트**이며
  "기본 JIT와 동치여야 하는 레인"이 아니다. 이를 동치 레인으로 오해하면 오탐이 발생한다(본 감사 초기 스윕이 그랬음).
- **`un native` 동치:** core subset 15개 수락 중 14개 바이트 동일. `bench/native_fib.un` 1건은
  `BENCH|…` 타이밍 줄만 달라지고 `=== Result: 2 passed, 0 failed ===`는 동일 → 의미 동치.
- **JIT 활성화:** AUDIT-001 — 1회차만 네이티브, 2회차부터 인터프리터. 미해결.
- **TD-52(rebound parameter):** 자체 재현 `docs/audit/rebound.un` 3레인 모두 `0,1666716667`, 행 없음 → 해결 확인.
- **Strict 강제 실패:** `docs/audit/jit_force_fail.un` → exit 2 + `JITSTRICT: 9/9 …` → 거부 경로 정상.
- **Union tag 계약:** `jit_cache.rs:8-13`의 `0=Int,1=Float,2=Bail,3=Bool,5=None,6=IntList`를
  `jit_fast.rs:49-55,163-175`가 동일하게 소비. class/dead-handle/bail도 tag=2로 폴백(정적).
- **IntVec/IntDict 소유권:** `jit_bc.rs::emit_int_cleanup`이 bail/정상 경로 모두에서 핸들을 free하고
  borrowed는 caller 소유로 유지한다는 정적 구조 확인. 다만 전용 메모리 검사 미실시 → NOT VERIFIED(§16).

## 13. Security and Runtime Safety

- **UNBC 파서/검증기:** `unbc.rs::Reader`가 `guard()`(선언 개수 ≤ 잔여 바이트)와 `take()`(경계 검사)를
  사용, `MAX_PATTERN_DEPTH=64`, magic/version 검사, `bytecode/mod.rs::validate`가 opcode 디코딩·
  인덱스·jump·register(256)·예외 슬롯 검증. 동적 확인: 손상 magic → `not a .unbc file`(exit 1),
  version 0xFFFF → `unsupported .unbc version 65535`(exit 1), 절단 → `unexpected end`(exit 1).
  Panic/abort 없음. PASS.
- **Sandbox:** 정적 구조 건전, 자동 테스트 부재 + symlink 미정규화 → AUDIT-006. 동적 confine은 BLOCKED.
- **HTTP/WS CRLF:** `net.rs:73`(URI 전체 CR/LF 거부), `http.rs:185-191`(헤더명 `:`/CR/LF, 값 CR/LF 거부),
  WS key = random 16B base64(`net.rs:556`). 프로토콜 경계 방책 확인. PASS(정적).
- **go/wait 동시성:** cooperative single-thread 설계(D-012/D-054)와 일치, differential이 바이트 동치 커버.
  오류 전파·Future 중복 대기·Lock 범위 심층 동적 검증은 미실시 → NOT VERIFIED.
- **JIT name-checked intrinsic:** 필드 이름/payload 검증 실패 시 tag=2 bail → 인터프리터(정적). PASS(정적).

## 14. Performance and Regression

| 항목 | BENCHMARK.md | 현재 측정 | 판정 |
|---|---:|---:|---|
| int_ops (un jit) | 2.8434 ms | 1,706.891 ms (`r2_bench_all.log`) | 심각 악화 (AUDIT-001/008) |
| Un JIT / Python | 7.527x | 0.259x | 주장 반대 |
| fib24 | — | 190.131 ms | 참고 |
| qsort500 / mergesort1000 / sieve30000 / call200000 | — | 16.172 / 46.414 / 61.955 / 323.793 ms (Group un) | 참고 |

`bench.ps1 -Group un`은 실제로 `modes: unjit`이며 "un jit(ms)" 컬럼이 인터프리터 속도를 측정한다.
`compile-bench`도 VM+JIT ≈ UV_JIT=0. **단일 측정이 아니라 3가지 독립 경로(타이밍 재현, compile-bench,
group-all geomean)가 모두 같은 결론을 지지하므로 측정 노이즈가 아닌 실제 회귀다.**
실행 환경은 16코어 Windows, Python 3.13.17, Node v26.13.0, CPU 모델은 BENCHMARK.md 생성 시점과
동일 계열(AMD Ryzen 7 9800X3D)으로 기록되어 직접 비교 가능.

## 15. Repository Hygiene

- `.gitignore`: `/rust/target/`, `*.unbc`, `/bench/results/`, node/python 캐시, 에이전트 상태, OS/editor noise 적절.
- 추적 바이너리/obj/wasm 0개(368 tracked files). `.unbc` 잔여 0개.
- `rust/target/release/unvm-audit-old.exe`, `bench/results/*-broken` 잔존 → AUDIT-007(정리 제안만).
- `scripts/` 8개 전부 역할이 명확하고 CI가 4개(test/differential/unbc-roundtrip + canary 인라인)를 사용.
  `exe-diff.ps1`은 CI 미사용이지만 로컬 게이트로 유효. AGENTS.md가 `exe-diff.ps1`을 목록에서 누락.

## 16. Blocked and Unverified Checks

| 항목 | 상태 | 이유 / 대체 |
|---|---|---|
| WASM playground release/debug sandbox confine 동적 실행 | BLOCKED | 이 저장소에 실행 가능한 WASM 호스트 바이너리 없음. `native/io.rs::normalize_under` 정적 분석으로 대체(§13). 정적 추론만으로 동적 동치 주장하지 않음 |
| CLI 문서 한/영 전문 의미 대조 | NOT VERIFIED | heading 20/20 동일만 확인, 양쪽 모두 사용자 WIP라 변경 없이 전문 검토 생략 |
| go/wait 심층 동시성(오류전파·중복대기·lock/deadlock) | NOT VERIFIED | cooperative 설계 정적 + differential 바이트 동치까지만 확인 |
| IntVec/IntDict 이중해제/use-after-free 동적 검증 | NOT VERIFIED | 소유권 경로 정적 추적만. ASAN/전용 스트레스 미실시 |
| Linux/WSL 전용 검사 | 환경 메모 | Windows 환경에서 WSL 미사용. 감사 항목 중 WSL 전용 필수 검사는 없으므로 §4 매트릭스의 BLOCKED 건수(1건: WASM confine 동적 실행)에는 포함하지 않음 |

## 17. Recommended Actions

모두 **제안**이며 사용자가 명시적으로 승인한 항목만 별도 작업으로 진행한다. 승인 전 어떤 파일도 수정하지 않는다.

### Priority 1 — Critical
1. **AUDIT-001 수정** — 대상 `jit_cache.rs`, `jit_fast.rs`. 범위: bytecode/TIR 엔트리 저장소 분리 또는
   `JitEntry.kind` 추가 + `try_call_jit()` 라우팅 순서 정리. 기대효과: JIT 복활성, 벤치 수치 회복.
   리스크: 낮음~중간(TIR zero-arg 엔트리 처리 주의). 검증: 1회/2회 타이밍, bench group all, differential 98/0,
   strict canary ×2. **선행 조건: "JIT 실제 발동" canary를 게이트에 추가** — 없으면 재발을 다시 놓친다.

### Priority 2 — Important
2. **AUDIT-008 후속** — AUDIT-001 완료 후 `BENCHMARK.md` 재생성 + 생성 커밋/환경 명기. 비용 중간(머신 의존).
3. **AUDIT-002/003 문서 동기화** — 대상 AGENTS.md, REFACTOR_SUMMARY, TEST_GUIDE, ARCHITECTURE,
   COMPILE_TESTS/COMPILE_PROGRESS, BYTECODE.md 재작성. baseline 단일 원본화. 비용 낮음~중간.
   단 README/CLI 쌍과 AGENTS.md는 사용자 WIP → 별도 승인.
4. **AUDIT-004 KO README 복구** — 대상 사용자 소유 `README.md`. 승인 시에만.

### Priority 3 — Cleanup
5. **AUDIT-005/006/007** — ARCHITECTURE TD-12 문장 갱신, `normalize_under` 단위 테스트 추가 + symlink 정책 명시,
   `unvm-audit-old.exe`·`-broken` 벤치 디렉터리 삭제 제안.

## 18. Final Audit Conclusion

- **감사 상태: 부분 완료.** 필수 검증 대부분을 실제로 실행했고 결과를 확정했다. 다만 WASM playground
  동적 confine, go/wait 심층, IntVec/IntDict 메모리 안전성 동적 검증은 환경/도구 한계로 BLOCKED 또는
  NOT VERIFIED다. 이를 모두 완료로 표현하지 않는다.
- **무결성 재확인:** 보고서 작성 후 `git status --porcelain`, `git diff --stat`, `git diff --cached --stat`
  실행 결과는 감사 시작 기준점과 동일했다 — `M README.md`, `M docs/README.md`,
  `M docs/guides/CLI.ko.md`, `M docs/guides/CLI.md`, `M main.un`, `M rust/src/main.rs`,
  `?? docs/audit/`, `?? rust/src/lsp.rs`. `git diff --cached --stat`는 출력 없음(스테이징 없음).
  신규 변경은 `docs/audit/` 내부(이 보고서 Part II와 `r2_*` 로그·재현 `.un`)뿐이다.
- **사용자 WIP 보호:** 7개 보호 대상 파일을 읽기만 했고 수정·삭제·이동·포맷팅·스테이징하지 않았다.
  감사 중 사용자 파일의 추가 변경도 관찰되지 않았다(Part I에서 관찰된 중간 편집은 이번 재감사에서는 없음).
- **최종 판정:** 빌드/테스트/CI 게이트는 모두 green이고 P0는 없다. 그러나 **P1 AUDIT-001(JIT 조용한 퇴화)이
  미해결**이며, 이로 인해 BENCHMARK.md의 성능 주장이 무효화된 상태다. 게이트 구조상 바이트 비교와
  `UV_JIT=2`로는 이 회귀를 잡을 수 없으므로, 수정 승인 시 "JIT 발동 여부" 검증을 반드시 함께 추가해야 한다.

## 부록 — 이 Part에서 실행한主要 명령 목록

```
git status --porcelain / git log --oneline -15 / git branch --show-current
git diff --stat / git diff --cached --stat / git ls-files
rustc --version / cargo --version / rustup show
cargo build --release --manifest-path rust/Cargo.toml
cargo test --release --manifest-path rust/Cargo.toml --workspace
cargo build --release --target wasm32-unknown-unknown --manifest-path rust/Cargo.toml -p unvm-playground
un.exe help / un.exe --version
scripts/test.ps1 ; scripts/differential.ps1 ; scripts/unbc-roundtrip.ps1 -IncludeNegative
scripts/compile-check.ps1 ; scripts/exe-diff.ps1 ; scripts/obj-demo.ps1 ; scripts/compile-bench.ps1 -NoSave
scripts/bench.ps1 -Group un -NoSave ; scripts/bench.ps1 -Group all -NoSave
UV_JIT=2 un.exe run test/jit/strict.un ; UV_JIT=2 un.exe run test/jit/strict_float.un
docs/audit/jit_repro_{one,two}.un / rebound.un / jit_force_fail.un (타이밍·동치·강제실패 재현)
손상 .unbc 3종(magic/version/truncate) run → exit 1 확인
```

모든 원문 로그: `docs/audit/evidence/r2_*.log`, `docs/audit/evidence/r2_jit_timing.log`.

---

# Part III — Remediation Log (2026-11-20, 사용자 승인 후)

아래는 사용자의 "우선순위 순서대로 진행해" 승인에 따라 Part II 권고안을 순서대로
적용한 기록이다. **커밋/푸시/스테이징은 하지 않았다.** 보호 대상 WIP
(`main.un`, `rust/src/main.rs`, `rust/src/lsp.rs`, `README.md`, `docs/README.md`,
`docs/guides/CLI.md`, `docs/guides/CLI.ko.md`, `bench/**`)는 여전히 수정하지 않았다.

## Priority 1 — 완료

### AUDIT-001 (P1) 수정
- `rust/src/jit_cache.rs`: `JIT_FNS`(bytecode JIT)와 `TIR_FNS`(Compile Mode TIR)를
  **별도 맵으로 분리**. `register_tir_entry`/`get_tir_entry`는 `TIR_FNS`만 사용하고,
  `clear_program`/`clear`는 두 맵을 모두 정리.
- `rust/src/jit_fast.rs`는 변경하지 않음 — `get_tir_entry()`가 이제 bytecode 엔트리를
  반환하지 않으므로 기존 라우팅 순서가 그대로 안전해짐.
- `rust/src/vm/mod.rs::jit_call`: `JIT_NATIVE_CALLS` 카운터 증가(relaxed atomic).
- 회귀 테스트 2개 추가:
  - `jit_cache::tests::bytecode_and_tir_entries_live_in_separate_maps`
  - `vm::tests::an_argument_taking_proto_stays_jit_native_on_repeated_calls`
  (둘 다 `cargo test --release`의 기존 CI 스텝에서 자동 실행 → "JIT 실제 발동" 게이트 역할)

**검증 결과**

| 항목 | 수정 전 | 수정 후 |
|---|---:|---:|
| `jit_repro_two.un` 2회 호출 | ~1,265 ms | **~29 ms** |
| `compile-bench` VM+JIT | 200.682 ms | **1.934 ms** |
| `compile-bench` UV_JIT=0 | 200.223 ms | 199.676 ms |
| `bench -Group un` int_ops | 1,646.710 ms | **2.873 ms** |
| `bench -Group all` Un JIT/Python | 0.259x | **6.990x** |
| cargo test | 218 | 220 (이 시점) |

`BENCHMARK.md`의 int_ops 2.8434 ms / 7.527x와 현재 측정(2.882 ms / 6.990x)은
측정 편차 범위(약 1~7%) 내에서 일치하므로 **BENCHMARK.md 재생성은 하지 않았다.**
(`bench.ps1 -Report`는 `bench/results/`에 기록하는데 이 경로는 보호 대상 `bench/**`에 해당함.)

모든 게이트 재확인: build exit 0/warning 0, test.ps1 48/1570/50, differential 98/0,
unbc-roundtrip 48/48+41, compile-check 23/0, exe-diff 14/0, obj-demo PASS,
compile-bench 정상, strict canary ×2 exit 0, WASM build exit 0.

## Priority 2 — 완료(보호 파일 제외)

수치/목록 동기화: `AGENTS.md`(규모 91/54,420/135, CLI 목록, 48/1570/50),
`docs/guides/TEST_GUIDE.md`, `docs/project/REFACTOR_SUMMARY.md`(DECISIONS D-001–D-092 포함),
`docs/project/PROJECT_AUDIT.md`, `docs/compile/COMPILE_TESTS.md`,
`docs/compile/COMPILE_PROGRESS.md`, `docs/compile/COMPILE_FINDINGS.md`,
`docs/spec/ARCHITECTURE.md`(differential 98, cargo test 수).

`docs/spec/BYTECODE.md` 전면 재작성: 실제 `Op` enum의 0x01–0x64 + 0x7F 표,
예약 영역 `0x65..=0x7E` 정정, `JmpIf` + `FLAG_JUMP_*` 단축회로 인코딩,
`#[repr(u8)]` 고정, 현재 `.unbc` 레이아웃/검증 규칙, JIT 엔트리 분리 메모.
C# `Evaluator.cs` 인용과 존재하지 않는 opcode 이름은 제거.

**추가 승인 후 완료:** `README.md` KO 예제 복구(AUDIT-004), README 쌍의 기준 수치
47/1538/48 → 48/1570/50, README 쌍의 CLI 목록(`plan` → 실제 7개 명령)을 반영했다.
`README.md:63`의 세 번째 form feed도 제거해 파일 내 form feed는 0개가 되었다.
복구한 예제는 `docs/audit/readme_ko_pool.un`으로 실행 검증했다
(`[1, 4, 9, 16]` / `21`, exit 0). EN 쌍과 해당 코드 블록이 동일하다.

## Priority 3 — 부분 완료

- AUDIT-005: `ARCHITECTURE.md`의 TD-12 문장을 `TryEntry.acc_depth`가
  `args.truncate()`에 사용된다는 현재 사실로 정정(완료).
- AUDIT-006: `rust/src/native/io.rs`에 `#[cfg(test)]` 단위 테스트 3개 추가 —
  드라이브/UNC, `..` 탈출, 정상 하위 경로. (완료)
  **정정:** Part II에서 "자동 테스트 없음"이라 했으나 playground에 기존
  `io_open_confines_to_sandbox_root` 테스트가 있었다. `.un` 테스트 디렉터리에
  문자열이 없다는 뜻이었으며, native 단위 테스트 추가로 커버리지가 강화되었다.
  symlink canonicalize 우회 가능성도 사용자 승인 후 수정 완료(아래 추가 항목 참조).

- **AUDIT-006 symlink 정책(사용자 승인 후 완료):** `normalize_under()`가 어휘적 검사 이후
  `std::fs::canonicalize`로 루트와 후보 경로의 symlink를 모두 해석해 containment를 재검사하고,
  검사를 통과한 **canonical 경로 자체를 반환**하도록 했다(check/use 사이 symlink 교체도 차단).
  대상이 아직 없는 write 모드는 가장 가까운 기존 조상만 canonicalize한다.
  playground의 가상 루트(`memory://`)는 canonicalize가 불가하므로 기존 어휘 동작을 유지한다.
  신규 테스트 `sandbox_rejects_symlink_pointing_outside_the_base` 추가(권한 부족 시 skip).
  검증: lib 219 passed(신규 symlink 테스트 통과), playground 5 passed, WASM exit 0,
  격리 target 디렉터리에서 release 링크 exit 0/warning 0.
  **남은 제약:** 사용자 편집기의 `un lsp` 프로세스(PID 1092, 01:46:start)가
  `rust/target/release/un.exe`를 점유(os error 5)해 **제자리 바이너리 재링크는 실패**했다.
  사용자의 LSP 세션을 임의로 종료하지 않았다 — 종료 승인 시 재링크 후 게이트 재실행 가능.
- AUDIT-007(레거시 산출물 삭제): 사용자 승인 후 실행 완료 —
  `rust/target/release/unvm-audit-old.exe`(11,779,077 B)와
  `bench/results/20261117-092954-broken`, `bench/results/20261117-093241-broken`
  디렉터리를 삭제했다. 다른 벤치 결과는 보존했다. (`bench/results/`는 gitignored)

## 최종 상태

-build/warning 0, `cargo test --release --workspace` **227 passed**(lib 219 + playground 8).
-감사 후 사용자가 `0c0cf4a`(use import 스코프), `9a05f66`(playground typeck ABI)를 별도 커밋하면서
  기준값이 Negative 50→51, differential 98→99, roundtrip negative 41→42로 이동했다.
  위 문서 수치는 이 변경을 반영한 값이며, Part II의 측정값은 당시 기록으로 보존한다.
-모든 게이트 green, JIT 성능 회복 확인.
-감사 보고서의 원래 발견사항은 보존했고, 이 Part III이 수정 사실을 명시한다.

---
---

# Part IV — Independent Full Re-Audit (2026-11-22, Read-Only)

Part I~III은 보존한다. 이 Part는 사용자의 요청으로 **현재 HEAD(a861113) 기준으로
처음부터 다시 수행한 전체 감사** 결과다. 이번에도 READ-ONLY이며, 생성한 것은
`docs/audit/evidence/r4_*.log` 및 이 보고서의 Part IV 추가분뿐이다.

감사 중 사용자가 `69aeea8`…`a861113`(NRT 도입 5개 커밋)을 진행해 HEAD가 감사 시작 시
`581b5b6`에서 종료 시 `a861113`으로 이동했다. 기준점은 **종료 시 HEAD**로 잡고,
그 시점의 보호 대상은 `main.un` 1개뿐이었다(감사 시작 시의 27개 WIP는 모두 커밋됨).

## 1. Baseline (재측정)

| 항목 | 이전(Part III) | 현재 | 측정 |
|---|---:|---:|---|
| HEAD | 36aa40d → af36ada | **a861113** | `git rev-parse --short HEAD` |
| Rust .rs (target 제외) | 91 | **113** | `Get-ChildItem rust -Recurse -Filter *.rs` |
| rust 전체 라인 | 54,420 | **63,307** | `(Get-Content).Count` 합 |
| rust/src 라인 | 54,028 | **55,734** | 동일 |
| `rust/nrt/` .rs | 0 | **22** | 동일 |
| `.un` 파일 | 135 | **156** | `Get-ChildItem test -Recurse -Filter *.un` |
| Positive / Negative manifest | 48 / 51 | 48 / 51 | 유효행 수 |
| CLI 명령 | 7 | 7 | `un help` |

**AGENTS.md:11은 여전히 "Rust 91 files / 54,420줄 / .un 135개"로 stale** —
현재 113 / 63,307(+ NRT 22개) / 156. 사용자가 NRT 문서화 커밋(`a861113`)을 했지만
규모 수치는 갱신되지 않았다. 새 발견 AUDIT-009.

## 2. Verification Matrix (Part IV)

| ID | 검사 | 기대 | 실제 | 상태 | 증거 |
|---|---|---|---|---|---|
| R4-01 | 초기 Git 상태/보호 목록 | 기록 | 27개 WIP → 종료 시 `main.un` 1개 | PASS | §1 |
| R4-02 | Release build | exit 0 | exit 0, warning 0 | PASS | r4_build.log |
| R4-03 | cargo test --workspace | 통과 | 3(nrt)+219(lib)+8(playground) = **230 passed**, 0 failed | PASS | r4_cargo_test.log |
| R4-04 | WASM build | exit 0 | exit 0, 10.55 s | PASS | r4_wasm.log |
| R4-05 | test.ps1 | green | 48/1570/0/0, Negative **51**, Known 0, exit 0 | PASS | r4_test.log |
| R4-06 | differential | 0 diff | **99 files, 0 differences** | PASS | r4_differential.log |
| R4-07 | unbc-roundtrip | 바이트 동일 | 48/48 + Negative 42 identical + 9 skip | PASS | r4_roundtrip.log |
| R4-08 | compile-check / obj-demo | 통과 | 23/0, PASS | PASS | r4_*.log |
| R4-09 | exe-diff (NRT 포함) | 통과 | **31 passed, 0 failed**, no-VM 마커 + 5.5 MB exe 확인 | PASS | r4_exe_diff.log |
| R4-10 | NRT 동치 (lambda/모듈/기본값/traceback) | byte 동일 | 4종 모두 `un run` ↔ plain exe 동일 | PASS | §4 |
| R4-11 | NRT 거부 경계 | 거부 | 동적 타입→annotation 오류, random→`--embed` 안내 | PASS | §4 |
| R4-12 | strict canary ×2 | exit 0 | 두 마커 모두 | PASS | §4 |
| R4-13 | JIT 2회 호출 | 네이티브 유지 | ~29 ms (회귀 없음) | PASS | §4 |
| R4-14 | bench -Group all -NoSave | 완료 | 6레인 완료, 567.8 s | PASS* | §6, `[BAD]` 1건은 SUSPECTED |
| R4-15 | Manifest 무결성 | 정상 | 48/51, 누락·중복 0, Known 0/0 | PASS | §5 |
| R4-16 | 손상 .unbc | 우아한 거부 | magic/version/trunc 모두 exit 1, panic 없음 | PASS | §5 |
| R4-17 | AGENTS.md 규모 수치 | 일치 | 91/54,420/135 vs 실제 113/63,307/156 | **FAIL** | AUDIT-009 |
| R4-18 | 생성 exe rustc 경고 | 경고 0 | `nrt_register_field` dead_code 경고 1건 | **FAIL** | AUDIT-010 |
| R4-19 | bench `native_class.un` 빌드 | 항상 성공 | 1회 `[BAD] exit=1`, 5회 재현 시도 모두 exit 0 | NOT VERIFIED | AUDIT-011 |
| R4-20 | WASM playground 동적 confine | 동적 확인 | 호스트 바이너리 없음 | BLOCKED | 정적 분석으로 환원 |
| R4-21 | symlink 정책 | 동적 확인 | Part III 단위 테스트 통과, WASM 동적 미실시 | PASS(정적+단위) | §5 |
| R4-22 | 저장소 위생 | 정상 | 추적 바이너리/로그 0, `.unbc` 잔여 0, 레거시 산출물 0 | PASS | §5 |
| R4-23 | 종료 Git 무결성 | baseline 일치 | WIP `main.un` + 감사 산출물 2개뿐 | PASS | §7 |

## 3. Executive Summary

- PASS 18 / FAIL 2 / BLOCKED 1 / NOT VERIFIED 2 = 23건.
- P0 **0**, P1 **0**, P2 **1**(AUDIT-009), P3 **2**(AUDIT-010/011).
- 가장 중요한 결론: **NRT 도입 이후에도 모든 게이트가 green이고 JIT 회복이 유지된다.**
  Part I의 P1(AUDIT-001)은 재발하지 않았다. 새로 발견된 것은 문서 수치 stale과
  생성 exe의 rustc 경고뿐이며, 기능/보안/메모리 안전성 문제는 확인되지 않았다.

## 4. NRT(VM-less plain build) 심층 검증

`69aeea8`(rust/nrt 추가) → `6f493da`(libnrt 링크) → `6973b2a`(31 fixtures) →
`a861113`(문서화) 변경을 실제 실행으로 검증했다.

- **동치**: `lambdas.un`(캡처closure `10/11/15`), `module_globals.un`(`10/10`),
  `defaults.un`(`11/21/12/15`), `nested_traceback.un`(VM과 완전히 동일한 traceback)
  모두 `un run` ↔ plain exe stdout/stderr/exit byte 동일. exe 크기 ~5.54 MB
  (libunvm.a 링크 시 ~17 MB) → no-VM 확인.
- **거부 경계**: 타입 없는 파라미터 → `parameter 'a' requires a type annotation` (exit 1),
  `random` 사용 → `rebuild with un build --embed` 안내 (exit 1), exe 미생성.
- **no-VM 마커**: `exe-diff` Part C가 `no-VM native runtime` 문자열과 10 MB 미만
  크기를 검증 → PASS.
- **구현 구조 정적 확인**: `tir::nrt_supported`가 경계 판정,
  `core_rejection_reason`이 힌트 출력. NRT closure tag=8, 전역 슬롯,
  소스/프레임 등록을 통한 byte-identical traceback은 fixture 실행으로 확인.

## 5. 테스트 진실성 / 보안 / 위생

- 게이트 3종 수치는 문서(48/1570/51, 99/0, 48/48+42)와 **정확히 일치**.
- Manifest 48/51행 모두 파일 존재, 중복 없음, KnownFailures/KnownDivergent 0/0.
- 손상 .unbc 3종 모두 exit 1 + 명확한 오류(`not a .unbc file`,
  `unsupported .unbc version 65535`, `unexpected end`), panic/abort 없음.
- sandbox: Part III에서 추가한 `normalize_under` canonicalize + symlink 단위 테스트는
  lib 219개에 포함되어 통과. WASM 동적 confine 실행은 호스트 부재로 BLOCKED(정적 환원).
- HTTP/WS CRLF 방지(`net.rs:73`, `http.rs:185-191`)는 변경 없이 유지.
- 위생: 추적 바이너리/obj/wasm/로그 0, `.unbc` 잔여 0, `unvm-audit-old.exe` 및
  `-broken` 벤치 디렉터리 없음. `rust/nrt/` 23개 파일 모두 추적됨.

## 6. 성능 / 회귀

| 항목 | 이전(Part III) | 현재 | 판정 |
|---|---:|---:|---|
| `int_ops` JIT | 2.873 ms | **2.843 ms** | 회복 유지 |
| Un JIT/Python | 6.990x | **843.581x** (jit/py 컬럼) / geomean은 로그 확인 | Int 루프 JIT 정상 |
| `un build` exe (int_ops) | n/a | 2.857 ms | JIT와 동일 수준 |
| `fib24` exe | n/a | 5.983 ms | native(unnative) 170.657 ms보다 빠름 |

`bench.ps1`이 이제 6레인(unvm/unjit/python/node/unnative/unbuild)을 측정하며,
`un_commit`은 실행 시점 HEAD(`a861113`)를 정상적으로 기록한다.
`[BAD] build bench/native_class.un exit=1`은 1회만 관찰되고 5회 재현 시도 모두
exit 0이었으므로 **간헐적 환경 요인(SUSPECTED)** 으로 기록하고 기능 결함으로 확정하지 않는다.

## 7. 새 발견사항

### [AUDIT-009] AGENTS.md 규모 수치 stale (P2)
- Location: `AGENTS.md:11`
- 문서: Rust 91 files / 54,420 lines / .un 135개
- 실제: Rust 113 files / 63,307 lines(+NRT 22개 .rs) / .un 156개
- 사용자가 `a861113`에서 NRT를 문서화하면서도 규모 수치는 갱신하지 않음.
- 제안: 한 곳에서 자동 생성하거나 위 값으로 갱신. (이번 감사는 READ-ONLY라 미수정)

### [AUDIT-010] plain exe 생성 시 rustc dead_code 경고 (P3)
- Location: `un build`가 생성하는 `%TEMP%\un_build_driver_*.rs:6`
- 증거: `warning: function 'nrt_register_field' is never used` — `cargo build` 자체는
  경고 0이지만, 생성 드라이버의 `rustc` 단계에서 경고 1건 출력.
- 영향: 기능 없음(사용하지 않는 extern 선언). "새 경고 추가 금지" 정신에 따라 정리 제안.
- 제안: 생성 드라이버에서 미사용 extern 선언을 제거하거나 `#[allow(dead_code)]` 부여.

### [AUDIT-011] bench exe 빌드 간헐 실패 (P3, SUSPECTED)
- Location: `scripts/bench.ps1 -Group all` → `[BAD] build bench/native_class.un exit=1`
- 1회 관찰, 동일 명령 5회 재실행 모두 exit 0. 병렬/임시 경로 경합 추정.
- CONFIRMED로 처리하지 않음. 재현 시 로그 수집 후 원인 규명 권장.

## 8. Final Audit Conclusion (Part IV)

- **감사 상태: 완료(단, WASM 동적 confine 1건 BLOCKED).**
- P0/P1 없음. Part I의 핵심 JIT 회귀는 재발하지 않았고, NRT 도입도 모든 게이트를
  통과했다. 기능·정확성·보안·메모리 안전성 측면에서 새로운 중대 결함은 발견되지 않았다.
- 신규 발견은 문서 수치 1건(P2), 경고 1건(P3), 간헐 실패 1건(P3)뿐이다.
- 감사 중 사용자 커밋 진행으로 HEAD가 이동했으나, 종료 시점 기준으로 재검증했고
  보호 대상은 `main.un`뿐이었다. **어떤 파일도 수정/스테이징하지 않았다.**

---
---

# Part V — Post-audit refresh (2026-11-23, 작성 시점 HEAD 660559b)

이 Part는 Part I~IV를 **수정하지 않고** 끝에 덧붙이는 갱신 메모다. 2026-11-23에
HEAD `660559b` 기준으로 게이트를 다시 실행해 확인한 값이며, 대상은 문서 수치다.
그 후 저장소 HEAD는 `f161f11`로 전진했으므로 아래 표의 "현재 값"은
**작성 시점 기준값**으로 읽는다.

## Part IV 수치 중 지금은 대체된 것

| Part IV 항목 | Part IV 값 | 현재 값 (2026-11-23, HEAD 660559b) |
|---|---:|---:|
| 기준 HEAD | a861113 | **660559b** |
| Rust .rs / 라인 / `.un` (§1) | 113 / 63,307 / 156 | **113 / 64,378 / 175** (rust/src 90 · 57,383줄 + rust/nrt 22 · 6,995줄 + playground 1) |
| `cargo test --release --workspace` (R4-03) | 230 passed | **234 passed** (nrt 3 + lib 219 + un bin/LSP 4 + playground 8) |
| `scripts/test.ps1` (R4-05) | 48 files / 1570 passed | **50 files / 1606 passed**, Negative **51** 매칭, Known 0 |
| `scripts/differential.ps1` (R4-06) | 99 files, 0 differences | **101 files, 0 differences** |
| `unbc-roundtrip -IncludeNegative` (R4-07) | 48/48 + 42 | **50/50** + 42 identical (9 compile-rejected skip) |
| compile-check / exe-diff (R4-08/09) | 23/0, 31/0 | **33/0, 42/0** |
| `BENCHMARK.md` (Part IV §6) | jit/py 843.581x (그 이전 6.990x/7.527x 포함) | **2026-11-25 18:51 +09:00, 커밋 180d3db** 재생성 — 6레인, Un JIT/Python **geomean 3.688x** |

R4-17 / AUDIT-009(AGENTS.md 규모 수치 stale)도 같은 종류의 숫자 드리프트다.
이번 재측정으로 기준값이 한 번 더 이동했을 뿐, 새 결함은 아니다.

## 새로 확인된 것은 회귀가 아니라 도구/CI/벤치 변경

Part IV 이후 추가된 것은 제네릭(generics), 데이터를 담는 enum variant,
plain no-VM `un build`(`libnrt.a` 링크), `dict[int,int]`, `un lsp`,
6레인 벤치, 그리고 LSP 테스트 4개다. CI(`.github/workflows/build.yml`)도
12단계로 늘어 `compile-check.ps1`, `exe-diff.ps1`, `edge-fuzz.ps1` 게이트가
추가되었다(Part I/II의 9단계 기술은 당시 기록으로 유효). 따라서 위 표의 차이는
테스트 커버리지·도구 확장과 벤치 재측정에서 온 것이며, **Part I의 P1
(AUDIT-001 JIT 조용한 퇴화)를 포함한 기능 회귀는 재발하지 않았다.**
Part IV의 판정(P0/P1 없음, 게이트 green)은 그대로 유효하다.
