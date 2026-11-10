# PROGRESS.md — 진행 상태

브랜치: `feature/rust-bytecode-vm` (from `main` @ `fe05894`). C# 코드는 변경하지 않는다.
Rust는 `rust/`, 문서는 `docs/rust-migration/`에만 추가한다. 푸시는 사용자 승인 후.

## 빌드/실행 환경

```powershell
$env:Path += ";$env:USERPROFILE\.cargo\bin"
cargo +stable-x86_64-pc-windows-gnu build --release    # MSVC link.exe 없음 -> gnu toolchain
cargo +stable-x86_64-pc-windows-gnu test --release
# 산출물: rust\target\release\unvm.exe

$env:DOTNET_ROLL_FORWARD='Major'
.\bin\Release\net8.0\Un.exe run <file>                 # C# reference
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test.ps1
```

- Rust 1.99.1 (rustc/cargo) + `stable-x86_64-pc-windows-gnu`.
- C#는 .NET 10 런타임으로 roll-forward 실행.
- `cargo build/test --release`는 warning 0개. 새 warning을 추가하지 않는다.

## Phase 진행

| Phase | 상태 | 커밋 | 검증 |
|---|---|---|---|
| 1 Lexer | **완료** | `9dcaa87` (scaffold), `2419e9e` (lexer) | 83개 `test/**.un` 토큰화 정합, `test/negative/` lexer 오류 4개 문자열 동일 |
| 2 Parser | **완료** | `cace0a0` | 83개 파싱 정합(6개 실패는 `test/negative/`만), 6개 negative parse stderr/exit byte-identical, 17개 문법 probe 동일, AST shape dump 확인 |
| 3 Desugarer | **완료** | `90226d8` | 83개 파일 desugar 정합, `compound assign` 확장/미확장 케이스 동일(`6`,`4`,`big`,`[1, 2]`,`[2, 3]`), `Operator` 노드 자식 비움 확인 |
| 4 Optimizer | **완료** | `90226d8` | 83개 파일 pass 카운트 `1x69, 2x8` 동일, 상수 폴딩/오류 문자열 probe 일치, `9223372036854775807 + 1` → `System.OverflowException` exit 1 |
| 5 Runtime 객체 모델 | **완료** | `a2d2d02` | `cargo test --release` 18/18 green, warning 0. 정수 overflow/wrapping, `pow`, display, UTF-16 index/slice, str add/sub, interning, none/type identity, compare fallback, union canonicalize, attribute lookup |
| 6~17 | 미착수 | — | — |

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

## Phase 3/4 정합 데이터

- Desugarer 규칙(`rust/src/desugar.rs`):
  - compound assign(`+=` 등)은 `Assign(target, BinOp(target, value))`로 확장하고
    lhs는 방문하지 않는다(이중 평가 방지).
  - `Branch`/`ElIf`는 `IfCase` + `Else`로 정규화한다.
  - `Operator` 노드는 자식을 비운다(`?` 연산자 트리는 `Operator`가 아니다).
  - `?=`과 `:=`은 확장 대상이 아니다.
  - `Node::with_new_children`(자식 교체)과 `Node::with_children`(자식 교체+재방문)을 분리한다.
- Optimizer 규칙(`rust/src/optimize.rs`):
  - 최대 64 pass, 각 pass는 `changed` flag로 fixed point를 판단한다(reference는
    `ReferenceEquals` node identity 기준 — D-005).
  - 모든 fold 실패는 `Panic`이고 `catch (Panic)` 후 노드를 보존한다:
    `division by zero`, `integer pow overflow: a ** b`, `float pow overflow: a ** b`,
    `unsupported optimize types: X, Y`, `invalid optimize operator 'X' for long/double`.
  - 상수 폴딩 산술은 **checked**(`Un.csproj`에 `/checked`는 없고 `Optimize`가
    `checked` 연산 사용) → `System.OverflowException: Arithmetic operation resulted in an
    overflow.` + exit 1.
  - `Int` 상수 `**`(float 지수 포함)는 double 계산 후 trunc, overflow 시
    `int pow overflow: {a} ** {b}`.
  - `and`/`or`/`xor`는 폴딩 대상이 아니다. `Float` `/` 0은 `inf`를 만든다(폴딩 성공).
- `x ?= 1`은 **두 구현 모두 파싱 성공**(Un에 `?=` 토큰이 없고 `Question`+`Assign`로
  토큰화). reference는 평가 시 `invalid pattern Unary`로 실패한다(`Evaluator.cs`의
  `invalid pattern {Kind}`, Kind = C# enum 이름). Phase 8에서 같은 문자열로 재현해야 한다.

## Phase 5 정합 데이터

`rust/src/runtime/`는 `Obj` 객체 모델, interning, `Types`, primitive 5종(`Int`/`Float`/
`Str`/`Bool`/`Date`)을 재현한다. 검증은 `cargo test --release`(18 tests)와 reference
probe(`C:\Users\User\AppData\Local\Temp\opencode\probe5.un`)로 했다.

- `Obj = Rc<RefCell<ObjData>>`, `Payload`는 `Plain`/`Error`/`Panic`/`Int`/`Float`/`Bool`/
  `Str`/`Date`를 담는다. 호스트 예외는 unwinding 없이 `Payload::Panic{ name, message }`로
  전달한다. display는 name이 `panic`이면 `\npanic : {msg}`, 아니면 `System.X: msg`.
- dispatch 순서(reproduced): `TryMethod`= `Members[name]` → `Self`/`Super` 설정 →
  `Call(args)`; 실패 시 `ValidSuper()` 재귀; 그다음 payload fallback.
  `GetAttrFrom`: Property → `__getattr__` → Members → `Super.Has` →
  `Global.TryGetOriginalValue(Type.Name, name)` → `Parents[1..]` → error.
  result가 `Fn`이면 `Self = receiver`, `Super = receiver.Super`.
  `SetAttrFrom`: Property → `__setattr__`(값 반환) → Members → Super → primitive 타입명
  (`int|float|str|bool|date|none|null`) → error → set.
- interning: `Int` 캐시 `-5..256`, `Str` pool(10000 entries / 1000 UTF-16 units, 만르면
  통째로 비움), `none`/`null`/`true`/`false` singleton. `Str.Copy/Clone`은 pool을
  거치지 않는다.
- `Int` 런타임 산술은 **unchecked/wrapping**: `int("9223372036854775807")+1` →
  `-9223372036854775808`, `*2` → `-2`, `1000000000*1000000000*1000000000` →
  `-6930898827444486144`. 시프트는 `(int)rhs & 63`(`1 << 64` == 1).
- `Int.Mod` by 0 → `System.DivideByZeroException: Attempted to divide by zero.`
- `Float → long` cast는 saturating(`i64::MIN`).
- Float display는 .NET shortest-round-trip: 지수 `>= 17` 또는 `<= -5`일 때만 과학표기
  (`1e14`→`100000000000000`, `1e17`→`1E+17`, `1e-5`→`1E-05`, `1e-4`→`0.0001`),
  exponent는 2자리 padding, `inf`→`∞`, `NaN`→`NaN`, `-0.0`→`-0`, `1.0`→`1`.
- `Date` payload는 .NET tick(100ns, `TICKS_PER_DAY = 864_000_000_000`), 달력은 Hinnant
  `civil_from_days`/`days_from_civil`. display `"yyyy-MM-dd HH:mm:ss.fff"`.
  `date + date`는 `other.Day`(day-of-month)만큼 더한다(reference quirk 유지).
  `date + str`은 `"yyyy-MM-dd"`를 쓴다. `date - date`는 tick diff.
- `Hash()`를 override하는 타입은 `Str`뿐이다(`Int`/`Float`/`Bool`/`Date`는 없음) →
  `hash(5)`는 `error: cannot hashable object`. `Obj.GetHashCode = Hash().As<Int>() ?
  hash : Type.GetHashCode()`.
- `Str`은 UTF-16 code unit 기준 length/index/slice(C# `string.Length`와 동일).
- 검증된 문자열 quirk: `Str.Sub`는 `Replace(old, "")`라 `"abab" - "ab"`는 **빈 문자열**;
  `Str.NEq`는 `==` 문구를 재사용한다; `Int.NEq`는 `!=`를 쓴다; `Str.Add/Sub`는
  `other.ToStr()`를 쓰므로 `str("a") + true`는 `"atrue"`, `str("a") + none`은 `"anone"`.
  `cannot add 'T' to 'Type'` 문구는 ToStr가 Str도 Err도 아닌 경우에만 도달한다.

## 확정한 표현 규칙 (fix하지 말 것)

- `Str` payload는 UTF-16 pair를 보존하므로 `"한글😀"[2:4]`는 round-trip한다.
  pair를 분리하는 slice는 lone surrogate가 되고, Rust는 U+FFFD, .NET는 lone surrogate를
  `string`에 저장하고 콘솔에 `?`로 출력한다 → 문서화된 divergence.
- `str_hash`는 UTF-16 code unit에 대한 FNV-1a. .NET `string.GetHashCode`는 프로세스마다
  랜덤이라 str-key `Dict`/`Set` 순서는 이미 구현 간 재현 불가능 → deterministic stand-in.
- `use builtin`은 reference에서 실패한다:
  `panic : module 'C:\dev\un\builtin' not found` (builtin은 모듈이 아니다).

## 기준선 (C# reference, 2026-11-10 실측)

- Positive: 33 files / 1265 passed / 0 failed / 0 crashed
- Negative: 42 matched / 0 mismatched
- Known: `test/regression/defects.un` (6 assertions) KNOWN FAIL
- `test/test_suite.un`: `109 passed, 0 failed`

## 현재 작업

- Phase 6/7 객체 확장: `List`/`Dict`/`Set`/`Tup`/`Json`/`Spread`,
  `Fn`/`LFn`/`NFn`/`Arg`/`Future`, `Property`/`FieldRef`/`Stru`/`TObj`/`Enu`,
  `Iter`/`Range`/`Counter`/`Repeat`/`Reverse`.
  `rust/src/runtime/obj.rs`의 `invoke`, `is_bound_fn`, `is_property`, `range_to`,
  Str의 `Iter/ToList/ToTuple`은 TODO stub이다.
## 네이티브 인벤토리 (완료)

`rust\scripts\rust_inventory.ps1`(읽기 전용 parse)이 `docs/rust-migration/NATIVE_INVENTORY.md`를
생성한다. 실측: 등록 36(`NativeModule` x13, `BuiltinModule` x1, `BuiltinType` x11,
`NativeType` x11), `[Native]` 함수 345, 오류 문자열 553(`new Err` 459 / `new Panic` 27 /
`new Error` 67), parse한 `.cs` 파일 85개. 표는 파일별 `fn`/`ret`/`arg types`/`C# params`/
`desc`/`example`이고, 오류 문자열은 파일·라인 순 verbatim 목록이다.
오탈자(`invaild`, `invalid argumnets: index`)와 `ArgInfo`의 `Name`(`old_value`, `sub_str`)을
그대로 재현해야 한다. `ANALYSIS.md` §10이 이 문서를 가리킨다.

## 다음 단계

1. Phase 6 컬렉션 + Phase 7 함수/타입 객체.
2. Phase 8 VM core: 표현식/문장 실행, `unvm run`이 `io.write` 출력, `test/unit/runtime/`
   스모크.
3. differential harness(`scripts/rust_test.ps1` / `scripts/diff_test.ps1`) 도입 — C#
   기준선과 stdout/exit/stderr 비교. AST shape 정합은 harness로 증명한다(reference에는
   AST dump가 없고 C# 코드를 수정할 수 없다).
4. Phase 14 네이티브 바인딩 계층(`NATIVE_INVENTORY.md` 기반).

## 보고해야 하는 semantic 차이 (도입 전)

1. `go`의 single-threaded cooperative scheduling(`time.sleep` 블로킹, Phase 16).
2. `.NET` 예외 문자열 재현 범위(`System.DivideByZeroException`,
   `System.ArgumentException`) — `test/negative/`가 요구.
3. stack overflow exit code `-1073741571` 재현 전략(`depth_limit_crash.un`,
   `property_recursion.un`).
4. KnownFailures(6 assertions)는 reference 실제 동작을 기준선으로 삼는다.
5. surrogate pair를 분리하는 slice의 출력(U+FFFD vs `?`).
6. str-key collection 순서(FNV-1a vs .NET randomized hash).
