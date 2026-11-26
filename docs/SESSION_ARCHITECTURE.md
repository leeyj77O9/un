# SESSION_ARCHITECTURE.md — 다중 세션 운영을 위한 프로젝트 분석·설계

> 작성일: 2026-11-23. 작성자 관점: Senior Software Architect / Development Team Architect.
> **이 문서는 분석과 설계 결과물이다. 코드·설정·기존 문서는 수정하지 않았다.**
> 모든 판단은 실제 소스 파일, Cargo 매니페스트, 테스트 매니페스트, CI 워크플로,
> 스크립트 본문을 직접 조사한 근거에 기반한다. 문서의 주장과 코드가 다르면 차이를 명시했다.
>
> **(2026-11-27 각주 — historical 스냅샷 보존)** 아래 파일 수·테스트 수·스텝 수 등은 2026-11-23 분석 시점 값이다.
> 현재 게이트 기준은 `cargo test` **430 passed**, `scripts/test.ps1` **51 files / 1638 passed / 0 failed / 0 crashed**,
> `compile-check` **42/0**, `exe-diff` **78/0**, `differential` **102/0** (`docs/project/REFACTOR_SUMMARY.md`).
> CI 스텝 수는 고정하지 않고 `.github/workflows/build.yml`을 기준으로 한다(P08 `488bf0b`가 opt-in 게이트 스텝 추가).

---

## 1. 프로젝트 분석 요약

### 1.1 정체성

Un은 들여쓰기 기반 문법·패턴 매칭·명시적 `go`/`wait`를 제공하는 동적 타입 스크립트
언어다. 현재 구현은 Rust 단일 언어로 되어 있고, 다음을 모두 포함한다.

- 바이트코드 컴파일러 + 레지스터 VM (`unvm` 라이브러리, `un` CLI)
- Cranelift 기반 바이트코드 JIT (Int/Float 부분집합)
- 정적 타입 검사 → TIR → Cranelift 코드젠으로 이어지는 Compile Mode
- VM을 전혀 포함하지 않는 독립 실행 파일 생성 (`libnrt.a`, 약 5MB)
- VM을 내장하는 `--embed` 실행 파일 (`libunvm.a`)
- stdio LSP 서버, WASM playground, 6레인 벤치마크 파이프라인

원래 C#/.NET 8 트리 워킹 인터프리터였으나 Rust 포트가 differential로 byte-identical을
확인한 뒤 대체됐고 C# 소스는 git 이력에만 있다.

### 1.2 검증된 규모

| 항목 | 실제 값 | 근거 |
|---|---|---|
| `rust/src` `.rs` 파일 | 90 | 셸 재귀 열거 (AGENTS.md와 일치) |
| `rust/nrt` 파일 | 22 | `rust/nrt/src` 열거 |
| 코드 라인 | 64,378 (`src` 57,383 + `nrt` 6,995) | AGENTS.md 2026-11-23 재측정 |
| `.un` 테스트 | 175 | `test/` 재귀 열거 (compile 64, negative 48, unit 44, ...) |
| Rust `#[test]` | 223(`src`) + 3(`nrt`) + 8(`playground`) = 234 | 정적 열거 |
| 워크플로 스텝 | 12 (build.yml) + 별도 dispatch-playground.yml | 파일 직접 확인 |
| 벤치 워크로드 | 24 트리플렛 + 7 edge 케이스 | `scripts/bench.ps1` `$Pairs`, `bench/edge/` |

### 1.3 개발 상태 요약 (추측 없이 근거 제시)

| 기능 | 상태 | 근거 |
|---|---|---|
| Lexer/Parser/Optimizer | 완성 | `runner.rs:21-28` 파이프라인, 단일 패스(`optimize.rs:14-20`), 50개 Positive 테스트 |
| 바이트코드/레지스터 VM | 완성 | `bytecode/mod.rs::validate`, `vm/mod.rs`, `.unbc` 왕복 50/50 |
| 바이트코드 JIT | 완성(부분집합) | `jit_bc.rs::precheck_fn`, tag 0/1/2/3/5/6, strict canary 2종, differential 101/0 |
| 제네릭 함수·클래스·제약 | 완성 | `typeck.rs::Ty::Generic`, `tir.rs::expand_generics`, `__mono_` 인스턴스, `test/compile/generics*.un` |
| enum 데이터 variant | 완성(VM/Compile) | `bytecode::Pattern::Variant`, `vm/mod.rs::match_pattern`, `test/compile/enum_data.un` |
| Compile Mode `check`/`native` | 완성 | `main.rs:330/420`, `compile-check.ps1` 33개 |
| `.obj` 생성 (`un obj`) | 완성(Int core + 소스 경로) | `objify.rs`, `natcode::obj_compile`, `obj-demo.ps1` |
| plain `un build` (no-VM) | 완성(제한 범위) | `nrt_supported`, `libnrt.a`, `exe-diff.ps1` 42개 |
| `un build --embed` | 완성 | `unrt.rs`, `libunvm.a`, exe-diff Part A |
| 제네릭 클래스의 TIR 모노모피제이션 | 미구현 | `body_is_core`가 제네릭 클래스 프로그램을 TIR에서 제외, `nrt_supported` 경계 |
| HKT / 명시적 variance | 미구현 | 코드에 대응 타입·문법 없음 |
| 데이터 variant의 plain build | 미구현(거부) | `nrt_supported`가 제외, `--embed` 안내 |
| 상위 8개 Rust 테스트의 CI 실행 | 부분(불일치) | CI 스텝 5가 `--workspace` 생략 → playground 8개 미실행 |
| 사용자 모듈/순환 import | 완성 | `module.rs`, `test/fixtures/`, negative manifest |
| 네이티브 모듈 23개 | 완성 | `native/mod.rs:33-60` 등록, `test/unit/native/` 20개 |
| 메트릭/프로파일링 | 사용 여부 불명확 | `native/inspect.rs`, `native/stopwatch.rs`는 있으나 전용 게이트 없음 |

---

## 2. 실제 아키텍처 개요

### 2.1 실행 파이프라인 (코드 기준)

```text
Source → lex(lexer.rs) → parse(parser.rs) → optimize(optimize.rs)
   ├─ run / compile     → compiler.rs → Program/Proto → bytecode/unbc.rs → vm::VM (+ jit_bc)
   ├─ check             → typeck::check_program → 진단
   ├─ native            → typeck → tir::lower → natcode::native_compile → jit_cache::TIR_FNS → VM
   ├─ obj(source)       → typeck → tir::lower → natcode::obj_compile → .obj
   ├─ obj(.unbc)        → objify::emit → .obj
   ├─ build(plain)      → typeck → tir::lower → nrt_supported → obj_compile → driver.rs → rustc + libnrt.a
   └─ build(--embed)    → compile(.unbc) → driver.rs → rustc + libunvm.a → unvm_run_embedded
```

`runner.rs:21-28`가 프론트엔드 공통 경로를 캡슐화한다. `main.rs:19-56`이 14개 서브커맨드를
디스패치하는 단일 진입점이고, 64MB 스택의 worker 스레드에서 실행 후
`jit_cache::clear()`로 종료한다.

### 2.2 순환 의존 (중요)

소스 코드에서 확인된 순환은 세 가지다.

1. `vm/mod.rs → module.rs → vm/mod.rs` (모듈 import가 VM을 호출)
2. `vm/mod.rs → jit_fast.rs → jit_bc.rs → vm::{Ins, ProtoPlan}` (JIT가 VM plan을 consuming)
3. `bytecode/mod.rs:278 → vm::transfer::analyze → bytecode` (validate가 VM transfer 사용)

추가로 런타임은 거대한 강한 연결 요소(SCC)다:
`obj.rs ⇄ collections.rs ⇄ function.rs ⇄ context.rs ⇄ scope.rs ⇄ world/* ⇄ obj.rs`.
`obj.rs`의 `Payload`가 `native::flow/stream/net/random/stopwatch` 상태 타입을 직접 이름으로
가지고 있고(`obj.rs:35-51`), 각 native 모듈은 다시 `runtime::obj`를 import한다 → 양방향 편집.

### 2.3 공통으로 사용하는 시스템

- **`World`** (`runtime/world/mod.rs:19-64`): 문서에는 프로세스 전역이라 적혀 있으나
  실제로는 `thread_local!`이다. `native::init`, playground, 테스트가 `reset()`으로 상태를 복원.
- **`Context`/`Scope`**: 모든 호출 경로가 `Rc<RefCell<ContextData>>`를운반.
- **`jit_cache`**: `JIT_FNS`, `TIR_FNS`, `JIT_MODULES` 두 맵 + `UV_JIT` 플래그.
  VM, JIT, Compile Mode, playground가 모두 사용.
- **`Op`/`Instr`/`Program`/`Proto`**: `#[repr(u8)]` 고정 ABI. 값 변경 시 `.unbc` 깨짐.
- **`Ty`/`CheckedProgram`**, **`TirTy`/`TirProgram`**, **`JitEntry`**, **`EmitOutcome`**:
  모듈 경계를 넘는 핵심 자료구조.

---

## 3. 주요 모듈 및 의존성

### 3.1 계층별 의존 관계

| 계층 | 모듈 | 의존 | 독립성 |
|---|---|---|---|
| 리프 | `source.rs`, `token.rs` | 없음 | 높음 |
| 프론트 | `error.rs`, `ast.rs`, `lexer.rs`, `parser.rs`, `optimize.rs` | 상위 리프만 | 높음(단, `TokenType`은 8개 파일이 사용) |
| 바이트코드 | `bytecode/{instr,mod,unbc}.rs`, `compiler.rs`, `runner.rs` | 프론트 + `vm::transfer` 역간선 | 중간 |
| 정적분석 | `typeck.rs` | 프론트만 | 높음(단, LSP/TIR이 소비) |
| TIR/코드젠 | `tir.rs`, `natcode.rs`, `objify.rs`, `unrt.rs` | typeck, bytecode, jit_cache, native::math | 낮음(4개 파일 동시 변경) |
| VM | `vm/*`, `runtime/*` | 전 계층 + JIT 콜백 | 낮음(SCC) |
| JIT | `jit_bc.rs`, `jit_fast.rs`, `jit_cache.rs` | bytecode, VM plan, runtime function | 낮음 |
| 독립 런타임 | `rust/nrt/**` | **외부 의존 0** | 높음(단, ABI는 natcode/driver와 공유) |
| 표준 라이브러리 | `native/**` | runtime/SCC | 중간(regex는 폐쇄 서브트리) |
| 도구 | `lsp.rs`, `playground/*` | 프론트+typeck(+World) | 높음 |
| 검증 | `scripts/*`, `test/*` | `un.exe`, node, python | 게이트별 상이 |

### 3.2 공동 소유가 필요한 파일 (핫스팟)

| 파일 | 왜 충돌하는가 |
|---|---|
| `rust/src/main.rs` | 14개 커맨드 디스패치 + obj/build 드라이버 생성 텍스트가 한 파일. 기능 추가 시 모두 충돌 |
| `rust/src/runtime/obj.rs` | `Obj`/`Payload`가 약 30개 파일에서 사용. 새 variant는 전 계층 파급 |
| `rust/src/runtime/world/*` | 전역 상태 facade. JIT depth, native registry, module cache 등이 교차 |
| `rust/src/bytecode/{mod,instr}.rs` | `Op` 고정 ABI. JIT/objify/natcode/VM/QA가 공유 |
| `rust/src/typeck.rs` | `Ty` 변경 시 TIR + LSP 동시 수정 |
| `rust/src/tir.rs` | natcode의 유일 입력 + main의 `nrt_supported` |
| `rust/src/jit_cache.rs` | 태그·UV_JIT·두 등록 맵. VM/JIT/CMP/playground 공유 |
| `test/MANIFEST.txt`, `test/negative/manifest.txt`, `test/compile/errors/manifest.txt` | 게이트 3종이 읽고 모든 기능 세션이 행 추가 |
| `scripts/bench.ps1` | 54KB 단일 파일에 워크로드 표·그룹·CSV 스키마·리포트 생성 |
| `BENCHMARK.md` | `bench.ps1 -Report`가 전체 재생성 |
| `AGENTS.md` + 10개 이상의 문서 | 동일 기준 숫자(1606/234/101/33/42)를 인용 → 일괄 갱신 필요 |
| `rust/Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `vendor/**` | 워크스페이스 공통, 의존성/툴체인 단일 장애 지점 |

---

## 4. 세션 구성표

불필요한 세분화를 피하고, 실제 파일 경계와 전문성이 겹치지 않도록 **11개 세션**을 설계했다.
PM 역할은 총괄에 흡수했고, Optimizer는 프론트엔드에, Code Reviewer는 총괄+QA에 분산했다.

| ID | 세션 | 담당 영역 | 주요 책임 | 의존 세션 |
|---|---|---|---|---|
| `ARC-00` | 총괄 아키텍트 | 문서·CI·빌드설정·vendor·`main.rs` 소유권 | 아키텍처 승인, 우선순위, 인터페이스 조율, 통합 검증 | 전 세션 |
| `FE-01` | 프론트엔드 | lexer/parser/ast/token/source/optimize/module/error | 문법·AST·최적화·모듈 import | ARC-00 |
| `VM-02` | 바이트코드·VM | `bytecode/*`, compiler, `vm/*`, runner | Op 고정, 스택→레지스터, VM 실행 | FE-01, ARC-00 |
| `RT-03` | 런타임 코어 | `runtime/*` (obj, collections, function, context, scope, iters, world, int_* …) | 객체 모델·전역 상태·콜 스택 | VM-02, ARC-00 |
| `JIT-04` | JIT 컴파일러 | `jit_bc`, `jit_cache`, `jit_fast`, `test/jit/*` | Int/Float JIT, bail, strict canary | VM-02, RT-03 |
| `CMP-05` | Compile Mode | `typeck`, `tir`, `natcode`, `objify`, `unrt` | 정적 타입·제네릭·TIR·Cranelift 코드젠 | FE-01, VM-02, RT-03 |
| `NRT-06` | 독립 런타임 | `rust/nrt/**`, `main.rs` build 드라이버 구간 | `libnrt.a`, `unrt_*` ABI, no-VM exe | CMP-05(ABI), ARC-00 |
| `NAT-07` | 표준 라이브러리 | `rust/src/native/**` (regex 포함) | 23개 모듈, 멤버 디스패치 | RT-03, ARC-00 |
| `TOOL-08` | 도구·플랫폼 | `lsp.rs`, `rust/playground/**`, VS Code 확장 | LSP, WASM sandbox, 에디터 연동 | FE-01, CMP-05, NAT-07 |
| `QA-09` | tests/검증 | `scripts` 중 게이트 6종, `test/**` | 회귀 게이트·매니페스트·픽스처 소유 | 전 세션 |
| `BENCH-10` | 성능·벤치 | `bench/**`, `BENCHMARK.md`, bench 3종 스크립트 | 6레인 측정·리포트·회귀 게이트 | QA-09, ARC-00 |

---

## 5. 각 세션의 상세 역할

아래 형식은 그대로 OpenCode Agent 설정 파일의 프롬프트 블록으로 옮길 수 있게 했다.

### `ARC-00` — 총괄 아키텍트

```yaml
session_id: ARC-00
name: 총괄 아키텍트
role: Project Manager + Architect + Integration Owner
purpose: 전체 아키텍처 일관성 유지, 작업 분배, 인터페이스 승인, 최종 통합
priority: 최상
owns:
  dirs: [docs/, vendor/, .github/workflows/]
  files: [AGENTS.md, INFORMATION.md, docs/SESSION_ARCHITECTURE.md, BENCHMARK.md(승인만),
          rust/Cargo.toml, rust/Cargo.lock, rust-toolchain.toml, .gitignore, .gitattributes]
  shared_primary: rust/src/main.rs   # 커맨드별 기능 소유자는 따로 있으나 변경 승인은 이 세션
may_not:
  - 특정 기능 구현을 다른 세션 대신 수행
  - 기능 세션이 소유한 소스 파일 직접 수정(승인 없이)
requests_from: [전 세션]
verification:
  - scripts/test.ps1, compile-check.ps1, exe-diff.ps1, edge-fuzz.ps1,
    differential.ps1, unbc-roundtrip.ps1 -IncludeNegative
  - cargo build --release (오류 0, 경고 0)
  - cargo test --release --workspace (234)
completion: 모든 게이트 green + 문서 기준치 일치 + 충돌 해소 확인
```

### `FE-01` — 프론트엔드

```yaml
session_id: FE-01
name: 프론트엔드
role: Lexer / Parser / AST / Optimizer / Module
owns:
  files: [rust/src/lexer.rs, rust/src/parser.rs, rust/src/ast.rs, rust/src/token.rs,
          rust/src/source.rs, rust/src/optimize.rs, rust/src/module.rs, rust/src/error.rs]
invariants:
  - Node는 (start, length, kind, op, children, value, annotations) 구조 유지
  - 타입 어노테이션은 원본 텍스트를 Const::Str로 보관 (별도 타입 AST 노드 만들지 않음)
  - optimize는 단일 bottom-up 패스 (pass_count == 1)
interfaces: [NodeKind, Const, TokenType, Source, LangError]
verification:
  - scripts/test.ps1 -Group unit
  - test/unit/lexer/literals.un, test/unit/parser/*.un
  - scripts/differential.ps1 (파서 변경 시)
notify: [VM-02, CMP-05, TOOL-08, QA-09]
approval_needed: [TokenType 변경, NodeKind 변경(특히 TypeParams 위치)]
```

### `VM-02` — 바이트코드·VM

```yaml
session_id: VM-02
name: 바이트코드·VM
role: Bytecode Compiler + Register VM
owns:
  dirs: [rust/src/bytecode/, rust/src/vm/]
  files: [rust/src/compiler.rs, rust/src/compiler/tests.rs, rust/src/runner.rs]
  shared: [rust/src/bytecode/instr.rs(ARC 승인), rust/src/main.rs(run/compile/dis/debug 구간)]
invariants:
  - Op은 explicit #[repr(u8)] — opcode 값 변경 금지 (.unbc 호환)
  - 0x65..=0x7E 예약 영역 유지, 0x7F FusedIntMulAdd 유지
  - 레지스터 수 상한 256, PLAN_CACHE_LIMIT = 64
  - bytecode::validate → vm::transfer::analyze 역간선 유지
interfaces: [Program, Proto, Instr, Op, Plan, Outcome, JitEntry(소비)]
verification:
  - scripts/unbc-roundtrip.ps1 -IncludeNegative
  - scripts/differential.ps1
  - scripts/test.ps1 -Group suite -Group integration -Group regression
notify: [JIT-04, CMP-05, QA-09]
```

### `RT-03` — 런타임 코어

```yaml
session_id: RT-03
name: 런타임 코어
role: Object model + Collections + Scope + Context + World
owns:
  dirs: [rust/src/runtime/]  # world/ 포함, unit tests 포함
  files: [rust/src/runtime/mod.rs]
  shared_primary: rust/src/runtime/obj.rs (JIT/NAT/CMP가 참조만, 변경은 이 세션 승인)
invariants:
  - Payload variant 추가는 native 상태 타입(native/flow|stream|net|random|stopwatch)과 양방향 정합
  - World는 thread_local — 테스트/playground는 reset()으로 복원
  - int/str/bool intern 캐시 경계 유지
  - JIT member intrinsic의 # Safety 계약 유지
interfaces: [Obj, ObjData, Payload, Scope, Context, World, KeyMap, Types]
performance: 객체 할당 경로, dict/set 해시, scope slot 조회
verification:
  - scripts/test.ps1 -Group unit (runtime 19개)
  - cargo test --release --manifest-path rust/Cargo.toml (lib 219)
notify: [VM-02, JIT-04, CMP-05, NAT-07, TOOL-08]
approval_needed: [Payload/ObjData 레이아웃 변경, scope slot 의미론 변경]
```

### `JIT-04` — JIT 컴파일러

```yaml
session_id: JIT-04
name: JIT 컴파일러
role: .unbc → Cranelift JIT + TIR entry dispatch
owns:
  files: [rust/src/jit_bc.rs, rust/src/jit_cache.rs, rust/src/jit_fast.rs,
          test/jit/strict.un, test/jit/strict_float.un]
  shared: [rust/src/runtime/int_list.rs, int_dict.rs, int_class.rs (RT-03 소유, FFI 이름만 사용)]
invariants:
  - tag 0 Int / 1 Float(bits) / 2 bail / 3 Bool / 5 None / 6 IntList (4 미사용)
  - bail은 의미론 보존용 fallback — 절대 엔진 오류로 노출하지 않음
  - FFI ABI: (hidden result ptr, param_tags, args...)
  - TIR_FNS와 JIT_FNS 분리(AUDIT-001), TIR entry가 UV_JIT와 무관하게 우선
  - UV_JIT=2 strict canary에서 후보 fallback stub → exit(2)
  - 공유 proto_span 인스턴스는 JIT 등록 건너뜀 (결정적 dispatch)
  - 레지스터 재사용 시 clear_specialized 호출 필수, BuildDict는 preserve
verification:
  - $env:UV_JIT=2; un.exe run test/jit/strict.un / strict_float.un
  - scripts/differential.ps1 (101 files / 0 differences)
notify: [VM-02, RT-03, CMP-05, QA-09]
```

### `CMP-05` — Compile Mode

```yaml
session_id: CMP-05
name: Compile Mode
role: typeck + TIR + natcode + objify + unrt
owns:
  files: [rust/src/typeck.rs, rust/src/tir.rs, rust/src/natcode.rs,
          rust/src/objify.rs, rust/src/unrt.rs]
  shared: [rust/src/main.rs(check/native/obj 구간), rust/src/lsp.rs(typeck 진단 소비)]
invariants:
  - Ty::Generic id는 선언마다 전역 고유
  - Typed IR은 구조화된 tree IR(SSA 별도 생성 없음), MAX_CORE_ARITY = 8
  - nrt_supported는 수동 allowlist — 미추가 시 조용히 --embed로 이동
  - fn_uses_* 워커는 exhaustive match — 새 TirTy 추가 시 전부 갱신
  - TIR entry ABI tag 0/1/3/4/2(bail), param_tags bit 32+p Float, 40+p Bool
  - __mono_ 인스턴스 이름, @dep: 해소로 재귀/교차 호출 연결
interfaces: [Ty, CheckedProgram, ClassInfo, TirTy, TirProgram, EmitOutcome]
verification:
  - scripts/compile-check.ps1 (33)
  - scripts/exe-diff.ps1 (42)
  - scripts/compile-bench.ps1 (성능 참고)
notify: [NRT-06, JIT-04, TOOL-08, QA-09, ARC-00]
approval_needed: [Ty/TirTy variant 추가, nrt_supported 경계 변경, 새 unrt_* 심볼]
```

### `NRT-06` — 독립 런타임

```yaml
session_id: NRT-06
name: 독립 런타임 (no-VM exe)
role: libnrt.a 구현 + un build 드라이버/링크
owns:
  dirs: [rust/nrt/]
  files: [bench/obj_fib_driver.rs]
  shared: [rust/src/main.rs(build_cmd 드라이버 생성 1076-1187, 링크 1193-1248)]
invariants:
  - nrt crate는 외부 의존성 0 (no-VM 보장의 구조적 근거)
  - unrt_* / nrt_* C ABI 시그니처는 natcode.rs declare_import와 1:1
  - NObj tag 0..10, leaked allocation, small-int cache 0..16384
  - class registry는 등록 후 OnceLock 동결, 뮤텍스 밖에서 메서드 호출(교착 방지)
  - NRT regex/는 rust/src/native/regex/의 포크 — 버그 수정은 양쪽
  - native.rs dispatch 미지원 모듈은 명시적 오류
  - plain build는 random/동적 Any 거부 → --embed 안내
performance: trace frame 무할당, int dict/map 직접 경로, fused list get
verification:
  - scripts/exe-diff.ps1 (42)
  - scripts/obj-demo.ps1
  - cargo test --release -p nrt (3)
notify: [CMP-05, ARC-00, QA-09]
```

### `NAT-07` — 표준 라이브러리

```yaml
session_id: NAT-07
name: 표준 라이브러리 / 네이티브 모듈
role: 23개 native 모듈 + regex 엔진 + 멤버 디스패치
owns:
  dirs: [rust/src/native/]
  shared: [rust/src/runtime/obj.rs(Payload 상태 타입 양방향),
           rust/src/typeck.rs(native_module_ret_type/builtin_ty 등록만),
           rust/nrt/src/native.rs(plain build 패리티 요청)]
invariants:
  - native::init에서 명시적 등록 (어트리뷰트 자동 발견 없음)
  - native/members.rs는 의미를 재구현하지 않고 runtime::obj에 위임
  - regex 서브트리는 내부 의존만 (re.rs가 유일 브릿지)
  - 새 모듈 추가 시 lsp.rs native_catalog(434-437) 갱신 필요 → TOOL-08에 요청
verification:
  - scripts/test.ps1 -Group unit (native 20개)
  - test/compile/native_modules.un, test/compile/build/native_modules.un
notify: [RT-03, TOOL-08, NRT-06, CMP-05, QA-09]
```

### `TOOL-08` — 도구·플랫폼

```yaml
session_id: TOOL-08
name: 도구·플랫폼
role: LSP + WASM playground + VS Code 확장
owns:
  files: [rust/src/lsp.rs, rust/playground/src/lib.rs, rust/playground/smoke.mjs,
          C:/dev/un-vscode/extension.js, C:/dev/un-vscode/package.json]  # 확장은 저장소 외부
invariants:
  - LSP는 자체 JSON 레이어(serde 없음), stdio
  - 진단은 compile_mode 옵션일 때만 typeck 호출
  - playground는 entry/exit 모두 world::reset() + jit_cache::clear()
  - WASM JIT 비활성은 cranelift_native::builder() 실패의 결과 — 명시 guard 없음
  - playground::textual_infer는 lsp::infer_types의 복사본 — 동시 갱신
  - 허용 모듈 allowlist 유지
verification:
  - cargo test --release --workspace (playground 8)
  - un lsp 수동 smoke, node rust/playground/smoke.mjs
  - WASM 빌드: cargo build --release --target wasm32-unknown-unknown -p unvm-playground
notify: [FE-01, CMP-05, NAT-07, ARC-00]
```

### `QA-09` — 테스트·검증

```yaml
session_id: QA-09
name: 테스트·검증
role: 게이트 소유 + 매니페스트/픽스처 관리
owns:
  dirs: [test/]
  files: [scripts/test.ps1, scripts/differential.ps1, scripts/unbc-roundtrip.ps1,
          scripts/compile-check.ps1, scripts/exe-diff.ps1, scripts/edge-fuzz.ps1,
          scripts/obj-demo.ps1]
  shared_primary: [test/MANIFEST.txt, test/negative/manifest.txt,
                   test/compile/errors/manifest.txt, test/assert.un]
invariants:
  - 기준: Positive 50 files/1606 passed, Negative 51, compile-check 33, exe-diff 42,
    differential 101, roundtrip 50+42(9 skip), edge-fuzz 7
  - test/assert.un 변경은 1606 카운트 전체에 영향 → 총괄 승인
  - .unbc는 테스트 디렉터리에 임시 생성 후 삭제 (gitignore)
  - UV_JIT 환경변수는 프로세스 단위 — 세션 간 공유 셸 주의
verification: 본 세션이 게이트 자체의 소유자
notify: [전 세션]
```

### `BENCH-10` — 성능·벤치

```yaml
session_id: BENCH-10
name: 성능·벤치
role: 6레인 측정 + 리포트 + 회귀 게이트
owns:
  dirs: [bench/, bench/results/(gitignored)]
  files: [scripts/bench.ps1, scripts/bench-report.ps1, scripts/compile-bench.ps1,
          BENCHMARK.md(생성 산출물), docs/guides/BENCHMARK_GUIDE.md]
invariants:
  - 하네스 프로토콜 BENCH/RESULT/SAMPLE/MEM 유지 (24개 트리플렛이 의존)
  - opcode/RESULT 비교의 unicode_edge 제외 규칙 유지
  - bench/results/<초 단위 stamp> 충돌 주의 — 동시 실행 금지
  - BENCHMARK.md는 bench.ps1 -Report만 재생성 → 단일 reporter 지정
verification:
  - scripts/bench.ps1 -Group all -Report
  - scripts/bench-report.ps1 -Gate -Threshold 10
notify: [ARC-00, QA-09]
```

---

## 6. 세션별 파일 소유권

### 6.1 단일 주 담당자

| 경로 | 주 담당 | 비고 |
|---|---|---|
| `rust/src/lexer.rs`, `parser.rs`, `ast.rs`, `token.rs`, `source.rs`, `optimize.rs`, `module.rs`, `error.rs` | FE-01 | `TokenType`은 8개 파일이 참조 → 변경 시 승인 |
| `rust/src/compiler.rs`, `bytecode/**`, `vm/**`, `runner.rs` | VM-02 | `instr.rs` opcode 고정 |
| `rust/src/runtime/**` | RT-03 | SCC 내부는 이 세션만 수정 |
| `rust/src/jit_*.rs`, `test/jit/**` | JIT-04 |  |
| `rust/src/typeck.rs`, `tir.rs`, `natcode.rs`, `objify.rs`, `unrt.rs` | CMP-05 | 4개 파일 동시 변경 원칙 |
| `rust/nrt/**` | NRT-06 | 외부 의존 0 유지 |
| `rust/src/native/**` | NAT-07 | regex 포크 동기화 의무 |
| `rust/src/lsp.rs`, `rust/playground/**` | TOOL-08 |  |
| `test/**`, 게이트 스크립트 6종 | QA-09 | 매니페스트 단일 소유 |
| `bench/**`, bench 스크립트 3종, `BENCHMARK.md` | BENCH-10 |  |
| `docs/**`, `AGENTS.md`, `INFORMATION.md`, `docs/SESSION_ARCHITECTURE.md`, `.github/workflows/**`, Cargo/t/toolchain/vendor | ARC-00 |  |

### 6.2 공동 관리 대상 (변경 승인 필수)

| 경로 | 1차 소유 | 공동 이해관계자 |
|---|---|---|
| `rust/src/main.rs` | ARC-00 | VM-02(run/compile), CMP-05(check/native/obj), NRT-06(build), TOOL-08(lsp) |
| `rust/src/runtime/obj.rs` | RT-03 | JIT-04(intrinsic), NAT-07(Payload 상태), CMP-05, NRT-06(대응 구현) |
| `rust/src/bytecode/instr.rs` | VM-02 | JIT-04, CMP-05, NRT-06, QA-09 |
| `rust/src/typeck.rs` | CMP-05 | TOOL-08(LSP 진단), NAT-07(시그니처) |
| `rust/src/tir.rs` | CMP-05 | JIT-04, NRT-06, ARC-00 |
| `rust/src/jit_cache.rs` | JIT-04 | VM-02, CMP-05, TOOL-08(clear) |
| `test/MANIFEST.txt` 등 3종 | QA-09 | 전 기능 세션(테스트 행 추가 요청) |
| `test/assert.un` | QA-09 | 전 세션(의미 변경 시 승인) |
| `BENCHMARK.md` | BENCH-10 | ARC-00(문서 기준치), QA-09(게이트 숫자) |
| `rust/Cargo.toml`, `Cargo.lock` | ARC-00 | 전 세션(의존성 추가 요청) |

### 6.3 소유권이 불분명한 파일

- `C:\dev\un\main.un` — 사용자 스크래치. **어떤 세션도 수정하지 않는다.**
- `docs/audit/evidence/*` — 로그 산출물. 감사 세션이 따로 있다면 ARC가 소유 지정.
- `bench/results/*` — gitignore이지만 여러 세션이 생성. BENCH-10 단일 소유로 고정 권장.

---

## 7. 세션 간 협업 및 의존 관계

### 7.1 의존 흐름

```text
                    ARC-00 (승인·통합)
                   /    |     \      \
              FE-01  VM-02  NAT-07  CMP-05
                |      |       |       |
                +→ RT-03 ←─────┘       |
                     |                 |
                   JIT-04 ←────────────┘
                     |                 |
                  QA-09 ←──────────────┘
                     |
                  BENCH-10
                     |
                  TOOL-08 (FE/CMP/NAT 소비)
                     |
                  NRT-06 (CMP ABI 공유)
```

### 7.2 변경 전달 매트릭스

| 변경 | 반드시 통보 |
|---|---|
| `NodeKind` / `TokenType` 추가·변경 | VM-02, CMP-05, TOOL-08 |
| `Op` 값·신규 opcode | JIT-04, CMP-05, NRT-06, QA-09 |
| `Payload` variant / `ObjData` 레이아웃 | RT-03 → JIT-04, NAT-07, CMP-05, NRT-06 |
| `Ty` / `TirTy` variant | CMP-05 → TOOL-08, JIT-04, NRT-06, ARC-00 |
| 새 `unrt_*` / `nrt_register_*` 심볼 | NRT-06 ↔ CMP-05(main 드라이버) |
| 새 native 모듈 | NAT-07 → TOOL-08(lsp catalog), CMP-05(시그니처), NRT-06(패리티) |
| 새 테스트/픽스처 | 기능 세션 → QA-09(매니페스트 등록) |
| 새 워크로드/하네스 필드 | BENCH-10 → QA-09, ARC-00 |
| 의존성/툴체인/vendor 변경 | ARC-00 승인 필수, 전 세션 통보 |

### 7.3 협의가 필요한 사항

- Compile Mode 사양 경계(`nrt_supported`)를 넓힐 때는 CMP-05 + NRT-06 + ARC-00 3자 합의.
- JIT 태그/ABI 변경은 VM-02 + JIT-04 + CMP-05 + QA-09 합의.
--standard library 의미 변경은 VM과 NRT 양쪽 구현을 동시에 갱신해야 하므로 NAT-07 + NRT-06 + QA-09 합의.

---

## 8. 충돌 방지 및 변경 승인 규칙

### 8.1 승인이 필요한 변경 (ARC-00)

1. 공개 인터페이스: `lib.rs` 모듈 목록, `main.rs` 커맨드/플래그, C ABI 시그니처.
2. 핵심 자료구조: `Op`, `Instr`, `ObjData`, `Payload`, `Scope`, `Ty`, `TirTy`, `JitEntry`, `EmitOutcome`.
3. 모듈 간 의존성: 새 `mod`, 새 `use crate::` 방향, 순환 추가.
4. 공통 파일: `main.rs`, `runtime/obj.rs`, `bytecode/instr.rs`, 3개 manifest, `Cargo.toml`, `Cargo.lock`.
5. 아키텍처: `nrt_supported` 경계, bail 규칙, World 상태 모델, `.unbc` 포맷.
6. 빌드 시스템: vendor 패치, 툴체인, 링커 플래그, CI 스텝.

### 8.2 세션 내부 원칙

- 담당 디렉터리 밖 파일은 **읽기만** 하고 수정은 요청한다.
- 공동 파일은 해당 구간만 편집하고, 커밋 전 `git diff`로 다른 세션 변경이 섞이지 않았는지 확인한다.
- 새 파일 추가 시 모듈 루트(`lib.rs`, `runtime/mod.rs`, `native/mod.rs`) 수정은 ARC-00 승인 후 진행.

### 8.3 통합 규칙

1. 기능 세션은 자신의 작업을 독립 게이트로 검증한 뒤 ARC-00에 완료 보고.
2. ARC-00은 변경 파일 목록을 수집하고 소유권 충돌 여부 확인.
3. 통합 순서: FE → VM/RT → JIT/CMP/NAT → NRT/TOOL → QA → BENCH → 문서.
4. 통합 후 CI 동일 게이트를 로컬에서 재실행(당시 12스텝; 현재 스텝 구성은 `.github/workflows/build.yml` 참조).
5. 충돌 시: 해당 파일의 1차 소유 세션이 해결하고, 양쪽 세션이 다시 게이트를 돈다.
6. `cargo build` 경고 0 유지(새 경고 추가 금지).

### 8.4 실제 관측된 리스크 (이 세션에서 경험한 충돌)

이전 작업 중 여러 담당자가 `typeck.rs`, `tir.rs`, `bytecode/mod.rs`, `runtime/obj.rs`를
동시에 편집해 `E0119`(중복 derive), `E0004`(비 exhaustive match), `E0433`(모듈 경로) 등
빌드 오류가 연속 발생했다. 이는 위 8.1 규칙의 필요성을 보여주는 실제 증거다.

---

## 9. 총괄 세션 운영 방식

ARC-00은 다음을 수행하되 전문 구현은 대신하지 않는다.

| 책임 | 구체 행동 |
|---|---|
| 아키텍처 이해 | 11개 세션의 소유권 표와 의존 그래프를 기준으로 판단 |
| 우선순위 결정 | 게이트 회귀 > 빌드 오류 > 기능 > 성능 > 문서 순 |
| 작업 분배 | 작업 ID를 부여하고 선행 조건 명시 |
| 의존성 관리 | 변경 전달 매트릭스(§7.2)에 따라 통보 누락 방지 |
| 인터페이스 조율 | 승인 요청을 1일 내 처리, 거부 시 대안 제시 |
| 중복 방지 | 동일 파일을 두 세션이 동시에 열지 않도록 작업 상태표 확인 |
| 통합·회귀 | §8.3 순서대로 병합 후 전체 게이트 재실행 |
| 문서 관리 | `AGENTS.md`, `INFORMATION.md`, 기준 숫자 10+개 일괄 갱신 |
| 진행 추적 | §10 상태 템플릿 유지 |

금지: 기능 구현을 직접 대신하기, 다른 세션의 파일을 몰래 고치기, 게이트 결과 없이 통합 선언하기.

---

## 10. 작업 상태 관리 체계

서로 다른 시점에 작업해도 현재 상태를 알 수 있도록 아래 템플릿을 공유 상태 파일로
운영할 것을 권장한다. **이번 작업에서는 실제 파일을 만들지 않았다.**
권장 위치: `.opencode/SESSION_STATUS.md` (이미 gitignore됨).

```yaml
- work_id: W-2026-11-23-001
  session: CMP-05
  purpose: "Ty variant 추가 (예: Decimal)"
  prerequisites: [ARC-00 인터페이스 승인, NRT-06 ABI 협의]
  status: in_progress        # planned | in_progress | review | merged | blocked
  changed_files:
    - rust/src/typeck.rs
    - rust/src/tir.rs
    - rust/src/natcode.rs
  verification:
    build: pass              # 오류 0 / 경고 0
    compile_check: pass
    exe_diff: pass
  impact: [TOOL-08, NRT-06, QA-09]
  notes: "nrt_supported allowlist 갱신 필요"
```

최소 기록 필드: 작업 ID, 담당 세션, 목적, 선행 조건, 진행 상태, 변경 파일, 검증 결과,
다른 세션에 미치는 영향. `blocked` 상태는 원인과 해소 조건을 반드시 적는다.

---

## 11. 권장 작업 순서

1. **ARC-00**: 소유권 표 확정, 기준 게이트 재측정, CI `--workspace` 불일치 결정.
2. **FE-01 / VM-02 / RT-03**: 기반 계층을 안정화. 이 셋이 흔들리면 upstairs 전부 영향.
3. **JIT-04 / CMP-05 / NAT-07**: 안정된 인터페이스 위에서 병렬 진행.
4. **NRT-06 / TOOL-08**: CMP-05의 ABI와 FE/CMP/NAT의 공개 시그니처가 확정된 뒤 착수.
5. **QA-09**: 각 단계마다 게이트를 돌려 회귀 즉시 탐지. 매니페스트 단일 소유 유지.
6. **BENCH-10**: 기능 변경이 끝난 뒤 단일 reporter가 `-Report` 실행.
7. **ARC-00**: 최종 통합, CI 재현(당시 12스텝; 현재는 `.github/workflows/build.yml` 참조), 문서 기준치 일괄 갱신.

---

## 12. 분석 과정에서 확인한 위험 요소

1. **런타임 SCC** — `obj ⇄ collections ⇄ function ⇄ context ⇄ scope ⇄ world`. 단일 세션 소유 필수.
2. **VM↔JIT↔bytecode 순환** 3건(§2.2). 인터페이스 변경 시 전방 통보 필수.
3. **Compile 4종 세트** — `typeck→tir→natcode→main`. 한 세션이 묶어서 소유해야 안전.
4. **`main.rs` 단일 허브** — 14개 커맨드와 드라이버 생성 텍스트가 한 파일에 집중.
5. **`Payload` ↔ native 상태 양방향** — `obj.rs:35-51`와 native 모듈을 동시에 고쳐야 함.
6. **`nrt_supported` 수동 allowlist** — 누락 시 조용히 `--embed`로 이동해 회귀가 숨을 수 있음.
7. **WASM JIT 비활성은 우연적 동작** — 명시 guard가 아니라 `cranelift_native::builder()` 실패의 결과.
8. **`World` 문서 불일치** — AGENTS.md는 process-global이라 쓰지만 실제는 `thread_local!`.
9. **regex 이중 구현** — `native/regex/`와 `nrt/regex/` 포크, 수정 시 양쪽.
10. **LSP/playground 추론 중복** — `lsp::infer_types` ↔ `playground::textual_infer`.
11. **CI 게이트 약함** — `cargo test`가 `--workspace` 생략 → playground 8개 미실행(234 중 226만).
12. **문서-코드 불일치 다수** — `ObjData` 스니펫 stale, parser line ref 오류, JIT tag 5/6 누락,
    native 모듈 목록 16개(실제 23), `//!` 모듈 문서 0개.
13. **기준 숫자 10+개 문서 중복 인용** — 한 번에 갱신하지 않으면 문서 간 불일치 발생.
14. **`bench/results/<초 단위 stamp>` 충돌** — 동시 벤치 실행 시 디렉터리 덮어쓰기.
15. **공유 `un.exe` 경쟁** — 여러 세션이 `-Rebuild`를 동시에 돌리면 바이너리 교체 중 실행.
16. **빌드 실패 관측 이력** — 실제 병렬 편집에서 E0119/E0004/E0433 연속 발생(§8.4).

---

## 13. 추가 확인이 필요한 사항

| 항목 | 확인 필요 내용 | 담당 제안 |
|---|---|---|
| VM-02 / RT-03 분리 유지 여부 | SCC 때문에 분리가 어렵다면 하나로 합치는 편이 안전한지 | ARC-00 |
| CMP-05 / NRT-06 ABI 경계 | `unrt_*` 호출자/구현자 분리 소유가 효율적인지, 단일 ABI 오너가 필요한지 | ARC-00 + 양 세션 |
| CI `--workspace` 수정 | 문서 기준 234를 CI에서도 실행할지, 아니면 문서를 226으로 고칠지 | ARC-00 + QA-09 |
| `INFORMATION.md` 소유·커밋 | 신규 문서를 ARC 소유로 편입할지 | ARC-00 |
| 상태 파일 위치 | `.opencode/SESSION_STATUS.md` 대신 저장소 추적 파일을 원하는지 | ARC-00 |
| VS Code 확장 경로 | `C:\dev\un-vscode\` 가 저장소 외부 — TOOL-08 접근 권한 확인 | TOOL-08 |
| `docs/audit/evidence/*` 소유 | 감사 로그를 누가 갱신하는지 | ARC-00 |
| 기준치 재측정 | Positive 1606 / compile-check 33 / exe-diff 42가 현재 HEAD에서 그대로인지 실행 확인 | QA-09 |
| WASM 빌드 게이트 | CI가 WASM 타깃을 설치만 하고 빌드하지 않음 — 게이트 추가 여부 | ARC-00 + TOOL-08 |
| 사용자 커밋 정책 | 에이전트 커밋 금지 규칙 유지 여부(사용자가 직접 수행) | ARC-00 |

---

> 검증 방법: 이 문서에 나온 모든 파일 경로는 실제 저장소 열거 결과이며,
> 라인 번호는 2026-11-23 작업 트리 기준이다. 세션 설계는 §1–§3의 실제 의존성에서
> 직접 도출했다. 문서와 코드가 다른 곳은 §12에 명시했다.
