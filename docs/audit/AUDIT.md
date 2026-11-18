# AUDIT.md — Un 프로젝트 Phase 1: 프로젝트 조사

작성: 2026-11-18. 기준 브랜치 `main`, HEAD = `2f67935` (직전 3 커밋은 사용자의 커밋,
워킹 트리에는 미커밋 변경 있음 — 이 감사에서는 되돌리지 않음).

## 0. Git 상태 (작업 시작 시점)

- 브랜치: `main`, `origin/main` 보다 3 커밋 앞섬.
- 스테이징된 변경: `bench/string.un` (11줄, 사용자 작업 — 유지).
- 미커밋 변경 (사용자 작업, 유지): `AGENTS.md`, `BENCHMARK.md`, `README.md`,
  `docs/ARCHITECTURE.md`, `docs/README.md`, `rust/src/jit_bc.rs` (595줄),
  `rust/src/jit_cache.rs`, `rust/src/jit_fast.rs`, `rust/src/native/math.rs`,
  `rust/src/native/regex/mod.rs`, `rust/src/runtime/collections.rs`,
  `rust/src/runtime/int_dict.rs`, `rust/src/runtime/int_list.rs`,
  `scripts/bench.ps1`, `test/unit/native/re.un`, `test/unit/runtime/functions.un`.
- 미추적 파일: `docs/CODE_NORMALIZATION_REPORT.md` (사용자 문서).
- **이 감사 작업은 위 파일들을 덮어쓰지 않는다.** 수정이 필요한 경우 별도 보고 후 진행.

## 1. 빌드/테스트 기준선 (미커밋 변경 포함, 그대로 측정)

| 항목 | 결과 |
| --- | --- |
| `cargo build --release` | 성공 (6.67s, 증분) — 워킹 트리 상태 |
| `cargo test --release` | **205 passed / 0 failed** |
| `scripts/test.ps1` | Positive **40 files / 1419 passed / 0 failed / 0 crashed**; Negative **41 matched / 0 mismatched / 0 divergences**; Known 0; exit 0 |
| Clippy | `scripts/clippy` 실행 중 (결과는 AUDIT_FINDINGS/PROGRESS에 기록) |

참고: 최근 커밋 3건(`perf: nested-list iteration`, `perf: regex DFA scan`,
`feat(jit): IntVec list intrinsics`)은 JIT/IntVec/regex 성능 작업으로 보이며,
이 감사에서는 그 경계에 추가 의존성을 만들지 않는다.

## 2. 디렉터리 구조

```text
C:\dev\un
├─ rust/                    # Cargo workspace: unvm (lib+bin) + unvm-playground
│  ├─ Cargo.toml            # cranelift-* 0.137.1 + crates.io, region/cranelift-jit/icache-coherence는 vendor/ 패치
│  ├─ src/                  # 83 .rs 파일, ~36.5k lines (wc -l)
│  │  ├─ main.rs            # CLI: run/compile/tokens/ast/desugar/opt/dis/dis-reg/plan/help
│  │  ├─ lexer.rs / token.rs / source.rs
│  │  ├─ parser.rs / ast.rs / optimize.rs (desugar+fold 통합)
│  │  ├─ compiler.rs        # AST → stack bytecode (Proto)
│  │  ├─ bytecode/          # Op (#[repr(u8)] 고정), Instruction, Program, unbc 왕복
│  │  ├─ vm/                # register VM (mod.rs) + transfer.rs (stack→register 번역) + tests.rs
│  │  ├─ jit_bc.rs / jit_cache.rs / jit_fast.rs   # Cranelift JIT (Int/Float 부분집합)
│  │  ├─ runtime/           # obj, collections(+tests), iters, function(+tests), scope, world/,
│  │  │                     # int_dict, int_list, context, modules, types, clock, date
│  │  └─ native/            # io fs re math iter time os sys random http ws atomic flow inspect
│  │                        # builtin members net date_members stream stopwatch codes, regex/ (DFA)
│  └─ playground/           # WASM 진입점 (unvm-playground)
├─ test/                    # 88 .un 스크립트 (unit/integration/regression/negative/fixtures)
│  ├─ MANIFEST.txt          # group|path 행 (suite 1, unit 34, integration 4, regression 1)
│  ├─ negative/manifest.txt # path|exit|substring
│  ├─ KnownFailures.txt     # 비어 있음
│  └─ KnownDivergent.txt    # 비어 있음
├─ scripts/                 # test.ps1, differential.ps1, unbc-roundtrip.ps1, bench.ps1
├─ bench/                   # 17 .un 벤치마크 (+node/, +python/ 대응)
├─ vendor/                  # 패치된 region / cranelift-jit / icache-coherence (+THIRD_PARTY.md)
├─ docs/                    # PROJECT_AUDIT.md, REFACTOR_SUMMARY/LOG, ARCHITECTURE, DECISIONS(75k),
│                           # TECHNICAL_DEBT, BYTECODE, TEST_GUIDE, BENCHMARK_GUIDE, CODE_NORMALIZATION_REPORT(미추적)
└─ tools/                   # (내용은 AUDIT_FINDINGS에서 확인)
```

## 3. 구성 요소 매핑 (어디에 무엇이 있는가)

| # | 구성 요소 | 위치 | 책임 / 데이터 흐름 |
| --- | --- | --- | --- |
| 1 | Lexer | `lexer.rs` (508줄) | 들여쓰기 기반 토큰; `Indent`/`Dedent`/`NewLine` 생성; `Source`를 mutate해 무시된 개행 기록 |
| 2 | Token | `token.rs` | `TokenType`(22개 variant는 never-constructed, display용) |
| 3 | Parser | `parser.rs` (1584줄) | 재귀 하강 + 우선순위 테이블(~1505행); `Rc<Source>` 공유 |
| 4 | AST | `ast.rs` | `Node{kind,…}`, 14 compound-assignment kind 등 dead variant는 2026-11-16에 삭제됨 |
| 5 | Desugarer | `optimize.rs`의 `desugar_local` | 단일 bottom-up pass로 desugar+fold 통합 (2026-11-16 이후 desugar.rs 없음) |
| 6 | Optimizer | `optimize.rs` | 상수 폴딩 + dead-branch 제거 |
| 7 | Compiler | `compiler.rs` (1669줄) | stack machine 바이트코드 생성; 슬롯 테이블; `FusedIntMulAdd` peephole |
| 8 | Bytecode | `bytecode/{mod,instr,unbc}.rs` | `Op` explicit `#[repr(u8)]` (`.unbc` 계약), `unbc::read` 검증 |
| 9 | VM | `vm/mod.rs` (1697줄), `vm/transfer.rs` | 레지스터 VM; dispatch는 `match Op`; 로컬은 스코프 슬롯 테이블, 레지스터는 임시값; `ArgFrame` 스택 별도 |
| 10 | JIT | `jit_bc.rs` (3888줄), `jit_cache.rs`, `jit_fast.rs` | `.unbc` plan을 Cranelift IR로 번역; Int(i64)/Float(IEEE-754)/Bool 부분집합; hidden pointer로 (value,tag) 반환; tag=2는 런타임 bail → 인터프리터 fallback; `UV_JIT=0` opt-out |
| 11 | Runtime | `runtime/mod.rs`, `world/` | `World` = 프로세스 전역 상태(thread-local), `world/` 하위 모듈로 분할됨 |
| 12 | Object System | `runtime/obj.rs` (2260줄) | `Obj = Rc<RefCell<ObjData>>`; `Payload` 31 variants; int/str/bool interning; attribute lookup 체인 |
| 13 | Type System | `runtime/types.rs` + `ObjData.{type_name,declared,types}` | 런타임 동적 타입; 5중 표현 (TD-18) |
| 14 | Scope/Env | `runtime/scope.rs`, `context.rs` | 부모 체인 슬롯 테이블; `Set`(선언 스코프에 write) vs `SetLocal`; 클로저는 `Context` 전체를 Rc로 캡처 (upvalue 없음, `LoadUpv`/`StoreUpv`는 ABI placeholder) |
| 15 | Stdlib | `native/` (명시적 등록, `native::init`) | io/fs/re/math/iter/time/os/sys/random/http/ws/atomic/flow/inspect/builtin/members 등 |
| 16 | Async | `native/flow.rs`, `atomic.rs`, VM의 future/pool 경로 | **단일 스레드, cooperative** (D-012/D-054): `go`/`wait`/`lock`/`pool`은 calling thread에서 실행; `atomic`은 `Rc<RefCell>` shim |
| 17 | Error | `error.rs`, `vm` TryEntry 스택 | Un 예외는 `LangError`; Rust panic은 잡히지 않음 (TD-40) |
| 18 | Memory | no GC; Rc 참조 카운트 | cycle 가능 (TD-22); `.unbc`/JIT 코드는 프로세스 수명 (TD-05) |

## 4. 주요 실행 경로

1. `unvm run file.un` → `Runner::load` (CRLF→LF 정규화) → parse → optimize →
   `compile` → `evaluate` (`Runner::RunCore` 대응).
2. `compile` → `StackProgram` → `transfer::compile` → `Program`(register plan)
   → `unbc::write` (선택) → 실행 시 `plan_for`로 plan 캐시 (`PLAN_CACHE`, thread-local,
   `Program.id` 키) → `register_protos`/`try_call_jit` (lazy JIT, proto 당 1회)
   → register VM dispatch 또는 JIT native entry (`F1..F8` extern "C").
3. 인터프리터 ↔ JIT 경계: JIT는 `precheck_fn` 통과 proto만 컴파일; 불가 proto는
   등록되지 않고 호출자가 인터프리터로 fallback. 실행 중 의미론을 검증할 수 없으면
   tag=2로 bail → 해당 호출만 인터프리터가 수행.
4. 테스트: `scripts/test.ps1` → `test/MANIFEST.txt` 행별 실행, stdout의
   `N passed, M failed` 파싱; negative는 stderr substring + exit code 검증.
5. Differential: `scripts/differential.ps1` — 모든 manifest 파일을 `UV_JIT=0`과
   기본 JIT로 두 번 실행해 stdout+exit code 비교. `scripts/unbc-roundtrip.ps1` —
   run vs compile+run 바이트 비교.

## 5. 의존성 / 빌드

- crates.io: cranelift-codegen/frontend/jit/module/native 0.137.1 (JIT 전용).
- crates.io 패치: `region` 3.0.2, `cranelift-jit` 0.137.1,
  `wasmtime-internal-jit-icache-coherence` 50.0.1 (Windows/GNU dlltool 부재 회피,
  `vendor/`에 raw kernel32 FFI로 패치).
- 툴체인: `stable-x86_64-pc-windows-gnu` (rust-toolchain.toml 고정). MSVC linker 없음.
- WASM: `wasm32-unknown-unknown` (unvm-playground; JIT 비활성화).
- CI: `.github/workflows/build.yml` — GNU 빌드 + test.ps1 + cargo test + differential + unbc-roundtrip (2026-11-17 2회 green).

## 6. 테스트/벤치 구조

- Rust unit tests: `cargo test --release` → 205개 (lexer/parser/compiler/runtime/VM/native).
- `.un` 스크립트: 88개 (unit 34 + integration 4 + regression 1 + suite 1 + negative 41 + fixtures).
- MANIFEST 집계: Positive 40 files / 1419 assertions (2026-11-18 측정).
- 벤치: `scripts/bench.ps1 -Group all|un|unvm|unjit|node|python` — warm-up + reps +
  min/median/mean/max/stddev/ops; `-Report`는 BENCHMARK.md 갱신.
- Differential JIT runner: 81 files (ARCHITECTURE 문서 기준) — 이 감사에서 재실행 예정.

## 7. 미해결 알려진 부채 (docs/TECHNICAL_DEBT.md 요약, 이 감사에서 재검증 대상)

high: TD-05 (JIT native code 미해제, 프로세스 경계 필요).
medium: TD-03 (PLAN_CACHE 무한 성장), TD-08 (invoke/call args.to_vec), TD-09
(snapshot deep clone), TD-10 (set_attr_from 3x clone), TD-11 (Source::get_line
O(n)), TD-16 (plan_for .expect), TD-17 (transfer opcode index 하드코딩),
TD-39 (atomic/pool/lock cooperative shim + atomic overflow wrap), TD-40 (Rust
panic escape), TD-51 (JIT 실패 silent stub — strict canary 부재).
low: TD-18/20/21/22/24/26/27/31/38/41/43/44/45/47/48/49/50 등.

## 8. Phase 1 결론 및 Phase 2 진행

- 구조 파악 완료. 이전 대규모 감사(PROJECT_AUDIT.md, 2026-11-16)가 이미 S1~S4를
  처리했으므로, 이 감사는 **독립 재검증 + 신규 결함 탐색**에 집중한다.
- 조사는 영역별 독립 조사관(frontend / compiler+VM / JIT / runtime / native+async /
  tests+tooling)에게 병렬 위임하고, 발견 사항은 이 저장소에서 직접 재확인한 뒤
  `AUDIT_FINDINGS.md`에 P0~P4로 분류한다.
- 워킹 트리의 사용자 미커밋 변경(JIT/IntVec/regex/bench)은 감사 대상이므로 읽되
  **덮어쓰지 않는다**.
