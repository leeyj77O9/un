# ROADMAP.md — 17단계 이행 계획

각 Phase는 **작동하는 증분(slipping)**을 남긴다. Phase 완료 기준은 항상
`dotnet build -c Release`(C# 참조) + Rust 빌드 + 관련 테스트 통과 + differential 비교다.

기준선(2026-11-10 `main` 실측): Positive **33 files / 1265 passed / 0 failed / 0 crashed**,
Negative **42 matched / 0 mismatched**, Known **1 file(`test/regression/defects.un`, 6 assertions)**.

| Phase | 이름 | 산출물 | 통과 기준 |
|---|---|---|---|
| 1 | Lexer | `lexer.rs`, `token.rs`, `source.rs` | 83개 `test/**.un` 토큰화 동일, `test/negative/` 4개 lexer 오류 문자열 동일 |
| 2 | Parser | `ast.rs`, `parser.rs`, CLI `ast` | 83개 파싱 동일, 6개 negative parse stderr/exit 동일, 17개 문법 probe 동일 |
| 3 | Desugarer | `desugar.rs`, CLI `desugar` | 83개 트리가 reference와 동일 구조, `+=` 확장/`if-elif` 정규화 확인 |
| 4 | Optimizer | `optimize.rs`, CLI `opt` | fixed point 수렴, 64-pass panic, const folding panic → node 유지 |
| 5 | Object model core | `runtime/obj.rs`, `types.rs`, `primitive.rs` | Int/Float/Str/Bool/None/Null, `Type` 이름, intern/is identity, `ToStr`/`Repr`/`ToBool` |
| 6 | Scope / Context | `runtime/scope.rs`, `runtime/context.rs` | 대입 라우팅(함수 스코프=신규 로컬, block 스코프=parent 전파), `for` 루프 스코프 |
| 7 | Collections | `runtime/collections.rs` | List/Dict/Set/Tup/Json/Spreads, `Copy`/`Clone`, 음수 index, `In` semantics |
| 8 | VM core (expressions) | `compiler/`, `vm/` | 리터럴/연산자/속성/index/slice/call 실행, magic dunder + super fallback |
| 9 | Control flow | `if`/`while`/`for`/`break`/`skip`/ternary/and/or/`??` | `test/unit/runtime/`, `test/integration/` 통과 |
| 10 | Functions | closure, `Arg` binding, recursion limit | 인자 바인딩 5종 메시지, `->` return, defers/usings flush, 재귀 한도 1000 |
| 11 | Classes | class, base, `self`/`super`, Property, `FieldRef` | `test/unit/runtime/property*`, `test/negative/property*` 4개 통과 |
| 12 | Exceptions | try/catch/finally, `?`, `PropagateFlow` | `uncaught`, `uncaught.un`, `catch_clobbers.un`, `stack_overflow.un` 통과 |
| 13 | Modules | import, cache, circular, sandbox | `test/fixtures/*`, `circular_main.un`, `use_member/use_missing*` 통과 |
| 14 | Native stdlib | `native/` 모듈들 | `test/unit/native/*` 통과, 네이티브 메시지(오타 포함) 동일 |
| 15 | Advanced syntax | enum, f-string, tagged template, annotations | `test/unit/runtime/`, `test/integration/` 통과 |
| 16 | `go` / `wait` | cooperative scheduler | `go_loop_var.un` 통과, shared scope semantics 확인 |
| 17 | `.unbc` | `bytecode/writer.rs`, `reader.rs`, CLI `compile` | Round-trip 검증, validation 규칙, source mapping 유지 |

Phase 순서 원칙:
- **의미론을 먼저, 성능을 나중에.** VM이 C#와 같은 값을 내는 것이 8~15의 목표다.
- **Native stdlib(14)는 객체 모델(5~7) 위에 얹는다.** `NFn`은 VM의 call boundary를 재활용한다.
- **`go`/`wait`(16)는 마지막 쪽에 둔다.** shared scope semantics를 검증하려면 함수/클래스가 완성돼야 한다.
- **`.unbc`(17)는 구조 변화 없이 추가한다.** 모든 테이블이 count-prefixed여야 하는 이유(BYTECODE.md).

Phase마다 `PROGRESS.md`에 완료 기록을 남긴다.

## Phase별 검증 방법

1. **AST stage(3, 4)**: reference CLI에 AST dump가 없으므로, Rust dump가 **트리의 구조적 동치**를
   만족하는지 `unvm desugar`/`unvm opt` 덤프의 self-consistency + 후단계 differential run으로 확인한다.
   Phase 3/4의 종착 검증은 Phase 8 이후 **전체 `test/**.un` 실행 결과 비교**다.
2. **VM stage(5~16)**: `scripts/rust_test.ps1`이 `test/MANIFEST.txt`와
   `test/negative/manifest.txt`를 Rust VM으로 실행하고, C# 참조 실행과
   exit code / stdout / stderr substring을 비교한다.
3. **Phase별 스모크**: `test/test_suite.un` (109 assertions)이 각 Phase에서 가능한 범위까지
   통과해야 한다.

## 보고 의무

기존 기능의 제거/의미 변경이 필요한 경우(예: `go`의 병렬성, `.NET` 예외 문자열,
exit code `-1073741571`) 사용자에게 **도입 전에** 보고한다. `DECISIONS.md`에 결정과
영향을 남긴다.
