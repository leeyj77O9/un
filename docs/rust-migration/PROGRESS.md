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
| 7 컬렉션 | **완료** | `b0d96d7` | `cargo test --release` 42/42 green, warning 0. `List`/`Dict`/`Set`/`Tup`/`Spread`/`Json`/`Stru`/`Enu`/`TObj` + JSON 문서 parser. reference probe(`probes7/p3..p8.un`)로 display/메시지 확인 |
| 7a Iterator | **완료** | `47a600e` | `cargo test --release` 63/63 green, warning 0. `Range`/`Counter`/`Repeat`/`Reverse`/`Iters` + `List`/`Tup`/`Dict`/`Str`의 `Iter` source. ground truth는 probe `probes7/p9.un`(exit 0)과 `src/Core/Object/Iters/*.cs` |
| 6, 8~17 | 미착수 | — | — |

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

## Phase 7 정합 데이터 (컬렉션)

`rust/src/runtime/collections.rs` + `collections/tests.rs`(42 tests green, warning 0).
ground truth는 reference source bytes와 probe `probes7/{p3,p4,p5,p7,p8}.un`에서 취했다.

- collection dispatch 순서: dunder → `valid_super` → payload override → base error.
  `init`은 dunder chain보다 payload override를 **먼저** 실행한다.
- `KeyMap`은 insertion-ordered `Vec<(u64 hash, key, value)>`. key 동일성은 `key_eq`
  (`Eq()`를 bool로 coerce, bool이 아닌 결과는 매칭 아님), bucket은 `hash_code`.
- `hash_code` arms: `Int`=value, `Str`=FNV(UTF-16), `Float`=bits, `Bool`=0/1,
  `Date`=ticks, `Enu`=N, `List`/`Tup`=element combine(seed 0,
  `combine_hash(seed,v)=rotl13(seed)^v` → `hash_code([1])==1`, empty==0), `Json`=inner.
  `Stru`/`Dict`/`Set`/`Spreads`/`TObj`는 reference에 `GetHashCode()` override가 없다
  → type-name FNV stand-in.
- `hash()` builtin: base `Obj.Hash()`는 `cannot hashable object`이고 `Str`만 override한다.
  `Int`/`Enu`는 `GetHashCode()`만 override하므로 `hash(1)`, `hash(enu)`는 에러다.
- `List` override: `Init/Eq/GetItem/SetItem/Slice/In/Len/ToBool/ToList/ToTuple/Iter/Copy/
  Clone/ToStr/Spread/GetHashCode`. `List.Copy`는 `this`, `ToList`는 element `Copy`(deep),
  `ToTuple`은 이름이 빈 tuple. `Str` 원소는 `'...'`로 감싼다. `List.In` 메시지는
  `cannot check if '{ot}' is in '{Type}'`; `Overlap`은 모든 needle 원소가 `Eq().As<Bool>()`
  로 발견되어야 true.
- `Dict` override: `Init/Len/GetItem/SetItem/In/Copy/Clone/ToStr/ToList/ToTuple/Iter/Spread`.
  `GetItem` 실패는 `key '{name}' not found in dictionary`(`Str.To(key)`), `Copy`는 `this`,
  `In`은 dict 상대 `Overlap`·그 외 key membership. `ToList/ToTuple/Iter/Spread`는
  `Tup([key,value], ["key","value"])`를 만들고 생성자가 이름을 지우므로 display는 `[(a, 1)]` /
  `((a, 1))`(probe). `Init`은 Tup의 `Name`을 key로 쓰므로 `dict(tuple(key = 1, value = 2))`는
  `{key:1, value:2}`. 오타 `invaild 'dict' initialize` 유지.
- `Set` override: `Len/GetItem/Copy/Clone/ToStr/Spread/Add/Sub/BXor`뿐. `In`/`ToBool`/
  `Iter`/`ToList`/`Hash`는 base 에러. `GetItem` 실패는 `key {Str.To(key)} not found in set`
  (따옴표 없음), 연산자 메시지의 `'set'`은 hard-coded, display는 `{a, b}`, `repr`는 `set`,
  `Init`은 dedup(`set(1, 1, 3)` → `{1, 3}`).
- `Spreads`: `Spread() => this`, `ToStr`/`Len`/`ToBool` override 없음 → display는 base
  `Repr()` = `spread`, `len` 메시지에 타입 이름이 붙지 않는다
  (`unsupported operand type(s) for len()`).
- `Tup`: 생성자는 `Name[i]`를 `string.Empty`로 지운다 → 이름은 construction 후에 부여된다
  (`tuple(key = 1, value = 2)`는 arguments tuple에서 이름을 유지하고 `t.key`는 1).
  `GetItem`는 int(`(int)` cast, 음수는 `index + Count`)/str(`tuple has no attribute '{s}'`).
  `Slice`의 음수 step은 `for i=s; i>e; i+=st`이고 end는 ≥0으로 clamp → tuple 역화는 불가.
  `s`가 `len`으로 clamp되면 호스 내부에서 `System.IndexOutOfRangeException`(`t[2:0:-1]`).
  `Copy`는 element `Copy`, `Clone`은 element `Clone`; `Int.Clone()`는 base `new Obj(Type)`
  → clone된 int cell의 display는 타입 이름(`[1].clone()` → `[int]`).
- `TObj` 라우팅: `x in y`는 `right.In(left)`, `x is y`는 어느 한쪽이 `TObj`이면
  `right.Is(left)` 그 외 `left.Is(right)`(`Evaluator.cs`). `TObj.In`은 원소가 target과
  identity거나 target을 포함하는 union일 때만 true → `int in int|str` true,
  `int|str in int` false, `1 in int|str` true, `1 is int|str` false, `1 is int` true.
  `int|str|float` display는 `<type: float | int | str>`(member name 정렬).
- `Json`은 `UnType.Create("json")`로만 등록된다 → Un 코드에서 `json(...)`는
  `error: undefined variable 'json'`. display는 `Stringfy`로만 검증(indent `3*depth+1`,
  list compact, None→`null`, `TrimEnd(',', '\n')`), parse된 문서는 `Json` 래퍼라서
  display가 inner의 `ToStr`가 아니라 `Stringfy`다(`json("null")` → `null`,
  `json("\"a\\tb\\u0041\"")` → `"a\tbA"`).
- 호스 예외(D-024) 재현 문자열: dict duplicate key
  `System.ArgumentException: An item with the same key has already been added. Key: {clr}`
  (bool key는 `True`), .NET dict miss
  `System.Collections.Generic.KeyNotFoundException: The given key '{name}' was not present
  in the dictionary.`, slice index
  `System.IndexOutOfRangeException: Index was outside the bounds of the array.`
  `System.Text.Json.JsonException`은 message의 byte-offset text를 재현하지 않는다.
- str-key collection 순서는 D-025에 따라 parity target이 아니다. int-key dict는
  `Val<long>.GetHashCode() == value`라 reference도 insertion 순서 → parity target.

## Phase 7a 정합 데이터 (iterators)

`rust/src/runtime/iters.rs` + `iters/tests.rs`(21 tests, 총 63 green, warning 0).
ground truth는 `src/Core/Object/Iters/{Iter,Range,Counter,Repeat,Reverse}.cs`의 source
bytes와 probe `probes7/p9.un`(exit 0, `io.write` display)에서 취했다. D-028 참조.

- reference는 `Iters : Ref<IEnumerable<Obj>>`이고 `Value`가 re-enumerable이라
  `foreach`/`Count()`/`ToList()`는 매번 새 enumerator다. Rust는 immutable 정의 트리
  (`StageDef`) + mutable cursor cell(`Stage`)로 같은 재현성을 준다.
- `Iter()`는 같은 cell, `Copy()`는 `this`, `Clone()`은 fresh cursors.
  `Range.Clone()`은 `new(start, stop, step)` → 시작부터 다시.
  `Counter.Clone()`은 `new(current)`인데 `current`는 enumerator가 진행하지 않는 생성자
  필드 → clone은 **원래 시작값**에서 시작한다(cursor가 아님).
- display: `Range`는 `start..stop`(step 1) / `start..stop..step`; `Counter`/`Repeat`는
  `ToStr()`가 에러라 `Repr()`(`counter`/`repeat`); `Reverse`와 `seq`는 base `Iters.ToStr()`
  의 `", "` join(`"3, 2, 1"`). `List.ToStr()`는 `Str` 원소를 `'...'`로 감싼다.
- `Len()`: `Range`는 공집합 0 아니면 `(stop - start + step - (step>0?1:-1)) / step`;
  `Counter`는 `long.MaxValue`; `Repeat`는 `source.LongCount() * count`(count<0면 음수);
  base `Iters`는 실제 enumeration 수.
- `Range`의 step 0은 생성자에서 throw하지 않고 첫 `MoveNext`에서
  `panic : step cannot be zero`(lazy). `Len()`은 host 안에서
  `System.DivideByZeroException: Attempted to divide by zero.`.
  `iter.range(a, b, 0)`은 native module가 construction 시 `Err("step argument must not
  be zero")`를 반환한다(다른 메시지). `iter.range`은 `start > end && step > 0`이면
  start/end를 swap한다(`iter.range(10, 0, 1)` → `0..10`, len 10).
- `Counter`/`Repeat`(count<0)는 `ToStr`/`ToList`/`ToTuple`/`Spread`가
  `counter is infinite` / `repeat is infinite`. `Reverse`는 materialized snapshot이라
  원본 list에 append해도 다음 walk가 바뀌지 않는다(lazy LINQ reverse와 divergence).
- source cell: `List.Iter()`는 **live** cell이라 append가 다음 round에 보인다;
  `Tup.Iter()`는 values; `Dict.Iter()`는 `tuple(key, value)` **이름 있는** pair
  (`p.key`/`p.value`.resolve); `Str.Iter()`는 UTF-16 code unit당 `Str` 하나.
- `Range.Eq`는 3필드를 `Range`와만 비교하고 그 외는 base chain →
  `range == list`는 `unsupported operand type(s) for ==: 'range' and 'list'`.
  `x in range`는 int면 step 고려 membership, 그 외
  `unsupported operand type(s) for in: 'range'`. `1..4`는 lower inclusive/upper exclusive.
- init 오탈자 유지: `invaild 'iter' initialize`, `invalid 'counter' initialize`,
  `invalid 'repeat' initialize`, `invalid 'reverse' initialization`.
- `iter` native module(21 함수) 사실: `take`/`skip`/`chain`/`flatten`/`map`/`filter`/
  `sorted`/`array`는 **list**를 반환(`io.write`가 `[0, 1]` 출력), `enumerate`는
  `tuple(index, value)` 이름 있는 pair를 iter로 반환. `counter(start?)` 기본 0,
  `repeat(obj, count?)` 기본 −1. `max`/`min`은 `'{a} < {b} is not a boolean`,
  `filter`는 `expected 'predicate' to return a boolean`. `array(value, size)`는
  `size`가 최소 1 dimension이어야 한다.
- `take`/`skip`/`clone`/`count`/`to_list`/`to_tuple`/`map`/`filter`/`any`/`all`/`sum`/
  `first`/`last`는 **`Iters` 멤버**이고, `skip`은 키워드라 `iter.skip(...)`/`r.skip(2)`는
  parse error(`error: expected identifier`). `counter.count()`는 host
  `System.OverflowException`(`Enumerable.Count`)을 `TargetInvocationException`으로 감싼다.
  `clone`은 `set`에만 있는 멤버다. `str`에는 `to_list`/`to_tuple`/`take`/`count`가 없다.

## 확정한 표현 규칙 (fix하지 말 것)

- `Str` payload는 UTF-16 pair를 보존하므로 `"한글😀"[2:4]`는 round-trip한다.
  pair를 분리하는 slice는 lone surrogate가 되고, Rust는 U+FFFD, .NET는 lone surrogate를
  `string`에 저장하고 콘솔에 `?`로 출력한다 → 문서화된 divergence.
- `str_hash`는 UTF-16 code unit에 대한 FNV-1a. .NET `string.GetHashCode`는 프로세스마다
  랜덤이라 str-key `Dict`/`Set` 순서는 이미 구현 간 재현 불가능 → deterministic stand-in.
- `use builtin`은 reference에서 실패한다:
  `panic : module 'C:\dev\un\builtin' not found` (builtin은 모듈이 아니다).

## 기준선 (C# reference, 2026-11-11 실측)

- Positive: 33 files / 1265 passed / 0 failed / 0 crashed
- Negative: 42 matched / 0 mismatched
- Known: `test/regression/defects.un` (6 assertions) KNOWN FAIL
- `test/test_suite.un`: `109 passed, 0 failed`

## 현재 작업

- Phase 7a iterator 완료(`47a600e`). 남은 stub는 함수 계열뿐이다:
  `rust/src/runtime/obj.rs`의 `invoke`, `is_bound_fn`, `is_property`, `property_get`,
  `property_set`, `call`은 `Payload` variant(`Fn`/`LFn`/`NFn`/`Arg`/`Property`/
  `FieldRef`/`Future`)가 없어서 stub이다. `range_to`와 `Payload::Iter` 계열은 실제
  구현으로 대체됐다.
- `Iters` self-methods `map`/`filter`/`any`/`all`/`sum`/`first`/`last`와
  `map`/`filter` stage는 `Fn` payload + evaluator가 필요해서 보류했다.
- Phase 6 `Scope`/`Context`와 Phase 8 VM core는 미착수(`rust/src/vm/` 없음).
- Phase 11/14 scope의 미탐사 source: `Flow/{Lock,Pool}.cs`,
  `IO/{Stream,HttpClient,WebSocket}.cs`, `Util/{Random,Stopwatch}.cs`,
  `Base/Template.cs`, `Type/{BaseType,CollectionType}.cs`, `Evaluator.cs`의
  Try/Match/Class/Enum/Assign, `Runner.cs`.
- reference runner 기준선 재실측(2026-11-11): Positive 33 files / 1265 passed /
  0 failed / 0 crashed, Negative 42 matched, Known 1 file, overall exit 0.
- `read`/`grep` 도구가 존재하지 않는 파일의 내용을 지어낼 때가 있다 → ground truth는
  shell(`Get-Content`, `Select-String`)로 확인하고, 한국어 문서 텍스트는 `read`로 읽는다
  (콘솔은 non-ASCII를 `?`로 렌더링). `cargo ... 2>&1`는 성공해도 exit 1을 준다.

## 다음 단계

1. Phase 7 함수 계열: `runtime/function.rs`의 `Arg`, `Fn` payload, `Bind`
   (`multiple values for argument '{name}'`, `missing required argument '{arg.Name}'`,
   `too many positional arguments`, `unexpected keyword argument '{keyword.Keys.First()}'`),
   `LFn`/`NFn` `Call`(`maximum recursion depth exceeded`, defer/using drain),
   `Property`(backing key `"$"+name`), `FieldRef`, `Future` → 커밋.
2. Phase 6 `Scope`/`Context`(slot table, function-scope assign 라우팅, `PopFrame`
   empty → `Panic("stack is empty")`, `maximum recursion depth exceeded`).
3. Phase 8 VM core: 표현식/문장 실행, `unvm run`이 `io.write` 출력, `test/unit/runtime/`
   스모크.
4. differential harness(`scripts/rust_test.ps1` / `scripts/diff_test.ps1`) 도입 — C#
   기준선과 stdout/exit/stderr 비교. AST shape 정합은 harness로 증명한다(reference에는
   AST dump가 없고 C# 코드를 수정할 수 없다).
5. `iter` native module(21 함수) + `Iters` self-methods(`map`/`filter`는 `Fn` 이후) —
   list 반환 semantics(`take`/`skip`/`chain`/`flatten`/`map`/`filter`/`sorted`),
   `enumerate`의 named pair, `array(value, dims)`.
6. Phase 14 네이티브 바인딩 계층(`NATIVE_INVENTORY.md` 기반).

## 네이티브 인벤토리 (완료)

`rust\scripts\rust_inventory.ps1`(읽기 전용 parse)이 `docs/rust-migration/NATIVE_INVENTORY.md`를
생성한다. 실측: 등록 36(`NativeModule` x13, `BuiltinModule` x1, `BuiltinType` x11,
`NativeType` x11), `[Native]` 함수 345, 오류 문자열 553(`new Err` 459 / `new Panic` 27 /
`new Error` 67), parse한 `.cs` 파일 85개. 표는 파일별 `fn`/`ret`/`arg types`/`C# params`/
`desc`/`example`이고, 오류 문자열은 파일·라인 순 verbatim 목록이다.
오탈자(`invaild`, `invalid argumnets: index`)와 `ArgInfo`의 `Name`(`old_value`, `sub_str`)을
그대로 재현해야 한다. `ANALYSIS.md` §10이 이 문서를 가리킨다.

## 보고해야 하는 semantic 차이 (도입 전)

1. `go`의 single-threaded cooperative scheduling(`time.sleep` 블로킹, Phase 16).
2. `.NET` 예외 문자열 재현 범위(`System.DivideByZeroException`,
   `System.ArgumentException`) — `test/negative/`가 요구.
3. stack overflow exit code `-1073741571` 재현 전략(`depth_limit_crash.un`,
   `property_recursion.un`).
4. KnownFailures(6 assertions)는 reference 실제 동작을 기준선으로 삼는다.
5. surrogate pair를 분리하는 slice의 출력(U+FFFD vs `?`).
6. str-key collection 순서(FNV-1a vs .NET randomized hash).
7. `System.Text.Json.JsonException`의 message text(byte offset 정보)는 재현하지 않는다.
   `System.IndexOutOfRangeException`/`System.ArgumentException`의 .NET stack trace는
   재현하지 않는다(내부 예외 문자열만 재현, D-024).
8. unbounded enumeration(`Counter.to_list()`, `Skip` over `Counter`, `Repeat` count<0를
   끝까지 enumeration): reference는 무한 루프(또는 `System.OverflowException`)로 멈추지
   않는다. Rust는 `panic : iteration is infinite`로 멈춘다. `Counter.Len()`은
   `long.MaxValue` override라 정합이고, reference는 clone 재현도 `Enumerable` 위치로
   결정된다.
