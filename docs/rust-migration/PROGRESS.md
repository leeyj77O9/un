# PROGRESS.md — 진행 상태

브랜치: `feature/rust-bytecode-vm` (from `main` @ `fe05894`). C# 코드는 변경하지 않는다.
Rust는 `rust/`, 문서는 `docs/rust-migration/`에만 추가한다. 푸시는 사용자 승인 후.

## 빌드/실행 환경

```powershell
$env:Path += ";$env:USERPROFILE\.cargo\bin"
cargo +stable-x86_64-pc-windows-gnu build --release    # MSVC link.exe 없음 -> gnu toolchain
# 산출물: rust\target\release\unvm.exe

$env:DOTNET_ROLL_FORWARD='Major'
.\bin\Release\net8.0\Un.exe run <file>                 # C# reference
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test.ps1
```

- Rust 1.99.1 (rustc/cargo) + `stable-x86_64-pc-windows-gnu`.
- C#는 .NET 10 런타임으로 roll-forward 실행.

## Phase 진행

| Phase | 상태 |커밋 | 검증 |
|---|---|---|---|
| 1 Lexer | **완료** | `9dcaa87` (scaffold), `2419e9e` (lexer) | 83개 `test/**.un` 토큰화 정합, `test/negative/` lexer 오류 4개 문자열 동일 |
| 2 Parser | **완료** | `cace0a0` | 83개 파싱 정합(6개 실패는 `test/negative/`만), 6개 negative parse stderr/exit byte-identical, 17개 문법 probe 동일, AST shape dump 확인 |
| 3 Desugarer | 진행 중 | — | — |
| 4 Optimizer | 진행 중 | — | — |
| 5~17 | 미착수 | — | — |

## Phase 2 정합 데이터

- 83개 `test/**.un` 전부 파싱 정합. 파싱 실패 6개는 `test/negative/`의
  `bad_indent`, `tab_indent`, `bad_string`, `unclosed`, `missing_body`, `semicolon`이며
  stderr와 exit code가 C# 참조와 **byte-identical**.
- 추가 문법 probe 17개(`C:\Users\User\AppData\Local\Temp\opencode\case_*.un`) 동일 메시지:
  `sugar_get`, `optional_chain`, `tagged`, `annot`, `annot_on_if`, `is_not`, `coalesce`,
  `nested_interp`, `unterminated_interp`, `chained_assign`, `bad_target`, `set_pair`,
  `using_plain`, `large_int`, `bad_float`, `dict_named`, `ternary`.
- AST shape 확인: Slice의 `Null` 자식(`a[:2]`, `a[1:]`), Parameter default/`*args`/`**kw`,
  ClassBases, Enum, Match case, f-string 조각+interpolated expr, Accessor get/set, Tagged.

## Phase 2에서 확정한 문법 사실 (fix하지 말 것)

- `return`은 키워드가 아니고 `->`가 `TokenType::Return`이다.
- `match`는 expression 전용, `{ }` body 필수.
- `fn`에 return type 절 없다.
- 생략 slice bound는 `:`(`a[:2]`). `a[..2]`는 `unexpected primary token DotDot`.
- accessor sugar은 한 줄 `get x -> e` / `set x = e`, 아니면 indent block.
  top-level accessor는 파싱되고 **평가 시** `unsupported node kind Accessor`로 실패한다.
- 중첩 interpolation의 leading space(`{ b{c} }`)는 `unexpected primary token Indent`
  (reference 공유 quirk, 재현 대상).

## 기준선 (C# reference, 2026-11-10 실측)

- Positive: 33 files / 1265 passed / 0 failed / 0 crashed
- Negative: 42 matched / 0 mismatched
- Known: `test/regression/defects.un` (6 assertions) KNOWN FAIL
- `test/test_suite.un`: `109 passed, 0 failed`

## 현재 작업

- Phase 3 `rust/src/desugar.rs`, Phase 4 `rust/src/optimize.rs`(CLI `desugar`/`opt` dump 포함).
- 네이티브 모듈 인벤토리는 `docs/rust-migration/NATIVE_INVENTORY.md`에 수집 중(미완).

## 다음 단계

1. Desugarer/Optimizer 정합: 83개 파일에 대해 desugar/opt이 reference와 같은 트리 구조를
   내는지 dump self-consistency + 64-pass panic 확인.
2. Phase 5 객체 모델: `Obj`/primtive/intern/type table.
3. Phase 8~9 VM core: 표현식/문장 실행, `test/unit/runtime/` 스모크.
4. differential harness(`scripts/rust_test.ps1`) 도입 — C# 기준선과 stdout/exit/stderr 비교.

## 보고해야 하는 semantic 차이 (도입 전)

1. `go`의 single-threaded cooperative scheduling(`time.sleep` 블로킹, Phase 16).
2. `.NET` 예외 문자열 재현 범위(`System.DivideByZeroException`, `System.NullReferenceException`,
   `System.ArgumentException`) — `test/negative/`가 요구.
3. stack overflow exit code `-1073741571` 재현 전략(`depth_limit_crash.un`, `property_recursion.un`).
4. KnownFailures(6 assertions)는 reference 실제 동작을 기준선으로 삼는다.
