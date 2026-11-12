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
| 6 Scope/Context | **완료** | `6f92fe4` | `cargo test --release` green. slot table에서 `Set`은 declaring scope에 write(`for` body의 `sum += v` 축적), `SetLocal`/`SetLocalOrDeclare`는 local 전용, `Declare`는 덮지 않음, `Remove`는 name만 제거(slot index 유지), missing `Get` → `variable '{key}' not found`, `PopFrame` empty → `panic : stack is empty`, `PrintError` trace는 `Frames.Skip(1).Reverse()` + 10-line cap + `truncated [{Count-11}+]` |
| 7c 함수 계열 | **완료** | `f966a1f` | `cargo test --release` 108/108 green, warning 0. `Bind`의 4가지 메시지, positional fast path, `*`/`**` swallow, spread unpack의 name 상속, recursion limit(1000) 증감 순서, defer/using LIFO drain, `Property` backing key `"$"+name`, `FieldRef`, `fn` display/repr, clone이 closure/Args cell 공유. ground truth는 `src/Core/Object/Function/{Fn,LFn,NFn,Arg}.cs`와 `Base/Property.cs` |
| 8a bytecode contract + world/global/native | **완료** | `3f053f0`, `e97bc8c`, `a064047`, `f88c580` | 124 tests green, warning 0. opcode/Proto/Program/validate, `World`/`Global`/native `io` + builtins 14 |
| 8b compiler + stack VM + `run`/`dis` | **완료** | `633ae20`, `248338c`, `000870c`, `600d8cb`, `508b51d` | `cargo test --release` 167/167, build 0 warnings. differential `cmp3.ps1` 29 probes: PASS 27 / DIFF 2(p11,p28 = .NET stack trace only, D-034). reference baseline 불변 |
| 9 control flow | **완료** | `9921c75` | `cargo test --release` 167/167 green, warning 0. `if`/`while`/`for`/`break`/`skip` 구현 + validator의 worklist CFG 깊이 검증. differential `cmp3.ps1` 27 PASS / 2 DIFF 유지 |
| 10 functions/lambda/return | **완료** | `849d224` | `cargo test --release` 189/189 green, warning 0. nested proto + `MAKE_FUNC`/`RETURN` + `begin_call`/`end_call` + 중첩 VM. differential 42 probes: 41 PASS / 1 DIFF(호스트 예외 stack trace만 차이) |
| 11 file modules | **완료** | `d83eee0` | `use`와 파일 모듈(파일 / 디렉터리의 `mod.un` / 디렉터리의 모든 `.un`), 캐시 + `circular import detected`, `test/integration/modules.un` reference와 동일 |
| 12 classes/exceptions/native modules | **완료** | `fca187e`, `6568c37`, `81ff74a`, `5e520e7`, `18fd0b9` | class/enum/property/match/defer/using/decorator + native 12개 모듈 + stream/future/pool/lock. differential 34개 파일: 33 PASS / 1 DIFF(출력 동일, 실행 시간만 차이) |
| 13~17 defer/using/stream/tagged/go-wait/.unbc | **완료** | `18fd0b9`, `43a8f7e`, `fab032b` | `defer`/`using`/`stream`/tagged template/`go`/`wait`/`.unbc`까지 구현. differential 34개 파일: 33 PASS / 1 DIFF, negative 42개 파일: 38 PASS / 4 DIFF(문서화된 차이) |
| 18 random object / http / ws | **완료** | `91cf755`, `1f30012`, `bbdb954` | `random()` 생성 객체(공유/개별 생성기) + `http` client와 `ws` socket payload(plain TCP, RFC 6455 handshake, TLS는 범위 밖 D-057). network probe 13개: 13 PASS / 0 DIFF, differential 34개 파일: 34 PASS / 0 DIFF, negative 38 PASS / 4 DIFF, reference baseline 불변 |
| 19 set literal / binding 없는 `for` / `atomic` / `time` / `go` 지연 | **완료** | `231058b`, `afe2c1b`, `0ff268b`, `4f38a21`, `0a0ab98`, `bd773fa` | `main` merge(`89f2805`)로 최신 reference를 받고 Port: `[1,2,3]`이 set, binding 없는 `for`, `atomic.int`/`atomic.bool`, `time` 확장(8개 함수 + `elapsed_ms` + `TICKS_PER_SEC`), `go`가 `go` 시점의 scope chain을 captured한 deferred future. differential 37개 파일: 37 PASS / 0 DIFF, negative 38 PASS / 4 DIFF, cargo 196 tests, reference baseline Positive 36 files / 1330 passed |
| 20 `re` 엔진 교체(.NET 호환) | **완료** | | .NET `RegexParser`→`RegexWriter`→`RegexInterpreter` 이식(`rust/src/native/regex/`), probes 25 PASS / 0 DIFF, differential 37 PASS / 0 DIFF, cargo 199 tests, reference baseline Positive 36 files / 1330 passed / Negative 42 matched |

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

## Phase 7b 정합 데이터 (members / native iter / spread / error headers)

ground truth는 probe `probes10/{p10..p52}.un`(runner `probes10/run.ps1`, stdout/exit를
파일로 분리)와 reference source bytes에서 취했다. Rust의 member dispatch는 VM phase 작업이라
아직 Rust에서 테스트되지 못했다 → 이 사실들은 VM 멤버 테이블 구현의 spec으로 쓴다.

- 멤버 dispatch는 **멤버 테이블 lookup**이고 virtual dispatch가 아니다(D-029). 멤버에 없는
  이름은 `error: '{Type}' object has no attribute '{name}'` + position block로 abort(exit 1).
  검증된 멤버 집합(`[Native(Name=...)]` verbatim):
  `list`: `add extend insert extend_insert remove remove_at index_of contains order sort
  reverse binary_search lower_bound upper_bound hpush hpop pop map resize`;
  `dict`: `add remove get contains_key contains_value clear keys values`;
  `set`: `add remove contains clear clone union intersect difference`;
  `Iters`: `take skip count to_list to_tuple map filter any all sum first last`;
  builtin(`BuiltinModule("builtin")`): `len exit type hash throw throw_if throw_unless
  chr ord bin oct hex repr ascii`. `next` builtin은 **없고** `list`/`str`/`tuple`/`dict`/
  `set`/`range`/`counter`에 `iter`/`next`/`copy`/`clone` 멤버가 없다.
  `set(1, 2).clone()` → `{1, 2}`가 유일한 clone 멤버다.
- `iter.range(0, 4)` 멤버 실측: `to_list` `[0, 1, 2, 3]`, `to_tuple` `(0, 1, 2, 3)`,
  `count` `4`, `take(2).to_list()` `[0, 1]`, `first` `0`, `last` `3`, `sum` `6`,
  `any(pred)` `true`, `all(pred)` `false`, `map(fn)` `[0, 2, 4, 6]`,
  `filter(fn)` `[2, 3]`. 빈 range은 `sum`/`first`/`last` → `none`, `any` → `false`,
  `all` → `true`. `len(range)`는 element 수. `r.next()` → `error: 'range' object has no
  attribute 'next'`. `skip`은 키워드라 `r.skip(2)`/`iter.skip(...)`는 parse
  `error: expected identifier`.
- `iter` native module의 arg order: `map(transform, iterable)`, `filter(predicate, iterable)`,
  `count(predicate, iterable)`, `any(predicate, iterable)`, `all(predicate, iterable)`,
  `take(count, iterable)`, `skip(count, iterable)`. `count`/`any`/`all`을 1Arg로 부르면
  `error: missing required argument 'iterable'`. predicate가 bool을 안 반환하면
  `error: expected 'predicate' to return a boolean`; `iter.max`의 non-bool 비교는
  `error: a < 1 is not a boolean`. `iter.sum/max/min(iterable)`은 단Arg.
- list를 반환하는 native: `chain`(`[1, 2]`), `flatten`(`[1, 2]`), `zip`(`[(1, 3), (2, 4)]`),
  `sorted`(`[1, 2, 3]`), `take`(`[0, 1]`). `enumerate`는 `tuple(index, value)` **이름 있는**
  pair의 iter라 display가 `(0, 5), (1, 6)`.
- `iter.array(value, dims)`: `iter.array(1)`과 `iter.array(1, [])` →
  `error: expected 'size' argument to have at least one dimension`; `iter.array(1, 2)` →
  `[1, 1]`(Int clone은 값 보존); `iter.array(1, [2, 3])` → `[[1, 1, 1], [1, 1, 1]]`;
  extra positional은 무시(`iter.array(1, 2, 3)` → `[[1, 1, 1], [1, 1, 1]]`);
  `iter.array(1, 0)`과 `iter.array(1, [0, 3])` → `[]`, `iter.array(1, [1, 2])` → `[[1, 1]]`.
- `iter.range(a, b, 0)` → `error: step argument must not be zero`. raw constructor는
  Un에서 도달 불가: `range(0, 5, 0)` → `error: undefined variable 'range'`(exit 1).
- `iter`는 builtin type constructor이고 `use iter`는 모듈을 바인딩해 shadow한다:
  `use iter` 후 `iter([1, 2])` → `error: unsupported operand type(s) for (): 'module'`.
  `set`/`list`/`dict`/`tuple`/`str`는 callable builtin이고 `use set`은 실패한다.
- `list(x)`/`tuple(x)`는 `List.Init` 규칙(0 args → `this`, 1 arg의 `ToList()`가 List면
  그 list, 그 외 `args.ToList()`) → `list(iter.repeat([1, 2]))`는 `[repeat]`,
  `tuple(...)`는 `(repeat)`. `to_list`/`to_tuple` **멤버**는 static
  `new List([.. self.Value])`라 unbounded source에서 무한 growth → `TargetInvocationException`
  → inner `System.IndexOutOfRangeException` + `.NET` stack trace, exit 1(stdout).
  즉 멤버 materialize는 virtual이 아니라 **static** materialize다.
- call-arg spread: `f(*x)`는 `Spreads`로 unpack → `io.write("A", *[1, 2])` → `A 1 2`,
  `io.write("A", *iter.range(0, 3))` → `A 0 1 2`. unbounded source는 `*` 노드에
  `Error: cannot unpack Counter`(capital header) + position block, exit 1.
  `EvalArguments`는 `Err`를 `header: err.Header`로 재포장하고(`Error:`), `Unwrap`는
  lowercase `error`를 쓴다(D-030).
- number literal에 `.clone()`/`.copy()`를 붙이는 것은 파싱에서 `error: invalid number
  format`(`1.copy()`). variable에 붙이면 member-lookup error(`x = 3` 후 `x.clone()` →
  `error: 'int' object has no attribute 'clone'`).
- `dict(a = 1)` → `error: invaild 'dict' initialize`(오탈자 유지);
  `dict(tuple(a = 1))` → `{a:1}`.
- reference hazard: `for x in l` 안에서 `l.add(3)`하면 **무한 루프**로 멈추지 않는다
  (300s timeout 실측). Rust의 live `Source::Live` cell은 같은 동작 → divergence 아님.
- `iter.enumerate([5, 6]).map(fn(x, i) -> x + i)`는 lambda body 자리에서 inner
  `error: unsupported operand type(s) for +: 'tuple' and 'int'`가 나고
  `TargetInvocationException`으로 감싸진다(exit 1, stdout) → Rust는 inner 문자열만
  Panic으로 전파한다(D-024). `map`은 item을 **단일 arg**로 넘긴다.

## Phase 7c 정합 데이터 (functions / properties / fields)

`rust/src/runtime/function.rs` + `function/tests.rs`(22 tests, 총 108 green, warning 0).
ground truth는 `src/Core/Object/Function/{Fn,LFn,NFn,Arg}.cs`, `Base/Property.cs`,
`LFn.cs`/`NFn.cs`의 source bytes에서 취했다. D-032 참조.

- `Fn`은 `Obj(UnType.Func)`이고 `self`/`super`/`members`는 `Obj`에 있다. Rust는
  `Payload::Func(FnData cell)`로 표현하고, closure `Context` / `Args` list /
  `ReturnType` / `Property` link는 cell에 넣는다. `LFn`은 AST body, `NFn`은 host closure.
- `Bind`는 `self`, `super`를 local로 먼저 넣고, `Property`가 있으면 `field`에
  `new FieldRef(Self, Property)`를 넣는다. 파라미터 중 `self`/`super` 이름은
  **arg 바인딩에서 건너뛴다**(이미 local로 들어갔기 때문).
- `TryBindSimplePositional`은 args에 keyword name이 없고 `Spreads`가 없으며 파라미터에
  `*`/`**`가 없을 때만 적용된다. fast path가 적용되면 general binding은 실행되지 않는다.
- general binding 메시지 verbatim: `missing required argument '{arg.Name}'`,
  `too many positional arguments`, `multiple values for argument '{name}'`,
  `unexpected keyword argument '{keyword.Keys.First()}'`.
  **required parameter 검사가 keyword leftover 검사보다 먼저** 돈다:
  `f(k=1, j=2)`에 파라미터 `a`가 essential이면 `missing required argument 'a'`다.
- `*args`는 남은 positional을 `Tup`로, `**kwargs`는 남은 keyword를 `Dict`(key는 `Str`)로
  받는다. `UnpackArgs`는 `Spreads` item을 flatten하고 각 unpacked 값이 **원래 arg의
  name을 상속**한다(`f(*x)`의 name는 비어 있다).
- recursion limit: `Global.MAXRECURSIONDEPTH = 1000`이고 depth는 **closure의
  `CallDepth`**다. `LFn.Call`은 `CallDepth >= limit`를 먼저 검사하고 increment한다.
  `NFn.Call`은 `CallDepth++ >= limit`를 검사하고 실패 경로에서도 decrement한다.
  둘 다 같은 1000에서 `Err("maximum recursion depth exceeded")`를 준다.
  `Clone()`은 closure cell과 `Args` list를 **공유**하므로 clone은 limit을 공유한다.
- `finally` drain: defers는 LIFO로 evaluator에 넘기고, usings는 LIFO로 `Exit()`를
  호출한다. drain 중에 flow가 올라오면 reference의 `finally`를 **탈출**해서 그 값이
  call 결과가 된다(`defer`의 `return`이 body return을 덮치는 quirk → 유지).
- `Property`는 `BackingKey = "$" + Name`이고 `Read`는 backing member를
  `Default.Clone()`로 **lazy materialize**한다. getter/setter는 `Self`/`Super`/
  `Property`가 바인딩된 채로 `Tup.Empty`/`Tup([value])`로 호출되고, `field` local을
  통해 backing member에 write-through한다. `Property.Clone()`/`Copy()`는 `this` →
  모든 instance가 같은 property object를 본다.
- `FieldRef`는 owner의 backing member에 대한 view이고 `Clone()`/`Copy()`는 `this`다.
  `Evaluator.Unwrap`은 `value is FieldRef`면 `field.Value`로 unwrap한다(VM phase 작업).
- display(`ToStr()`)은 `fn({arg types}) -> {return type}`, `Repr()`는
  `fn {Name}({arg name}: {arg type}) -> {return type}` → `fn(int) -> any` /
  `fn add(a: int) -> any`.
- `LFn`/`NFn`은 `Call`을 override했기 때문에 `__call__` dunder를 **참조하지 않는다**.
  Rust는 payload dispatch로 같은 순서를 만든다(`Func` payload면 바로 `Fn.Call`).
  callable이 아닌 object는 `unsupported operand type(s) for (): '{Type}'`.
- `Payload::Func/Property/FieldRef` 추가에 따라 `clone_obj`에 primitive arm을 넣었다:
  `Int.Clone()`/`Bool.Clone()`는 `From(Value)`(cached cell 유지),
  `Str/Float/Date Clone()`는 pool을 우회한 fresh object → D-026 교정을 code에 적용했다.
  stale한 `collections` test expectation(`[int]`)을 `[1]`으로 고쳤다.
- `Future` payload는 Phase 16(scheduler)에 추가한다. `Future.cs`는 `Task<Obj>` wrapper이고
  `Run()`/`Wait()`만 있다.
- VM이 없어도 `LFn`을 테스트할 수 있게 `world`에 executor hook
  (`set_executor(Option<Executor>)`, `Executor = Rc<dyn Fn(&Context, &[Node]) -> Flow>`)를
  넣었다. `reset()`은 hook을 다른 global 상태와 함께 clears한다.
- `Context`의 frames는 shared `Rc<RefCell<Vec<Frame>>>` cell이다(reference는
  `new Context(scope, Closure.Source, Closure.Frames)`로 frame stack을 물려준다) →
  nested call이 전체 frame chain을 본다.

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

## Phase 8a 정합 데이터 (bytecode contract + global/native 레이어)

`rust/src/bytecode/`(`3f053f0`), `rust/src/runtime/world.rs` 확장(`e97bc8c`),
`rust/src/runtime/global.rs` + `rust/src/native/`(`a064047`). 총 **124 tests green,
warning 0**. ground truth는 `src/Interpreter/Evaluator/Evaluator.cs`,
`src/Program.cs`, `src/Interpreter/Runner/Runner.cs`, `src/Core/Global.cs`,
`src/Core/Native/{Builtin,IO}.cs`의 source bytes에서 취했다. D-033, D-034 참조.

### bytecode contract가 code에 고정됐다

- `Op`는 explicit `#[repr(u8)]` discriminant로 고정된다: loads/stores `0x01..0x0B`,
  stack shuffle `0x0C..0x11`, builders `0x12..0x18`, arithmetic `0x19..0x25`,
  comparisons `0x26..0x2F`(`XOR`가 마지막), unary `0x30..0x35`,
  `JMP_IF 0x36 / JMP 0x37 / PUSH_TRY_PROP 0x38 / PROPAGATE 0x39`,
  attr/item/slice `0x3A..0x3F`, args `0x40..0x45` + `UNWRAP 0x46`,
  `NEW_SCOPE 0x47 / POP_SCOPE 0x48`, loop `0x49..0x4D`, try/defer/using `0x4E..0x54`,
  class/enum `0x55..0x5C`, `MAKE_FUNC 0x5D / USE 0x5E / UNSUPPORTED 0x5F`.
  `0x60..0x7F`는 §16 확장 영역, `0x00`은 unused sentinel.
- `Instr { op: u8, flags: u16, operand: i32 }`는 7-byte LE 인코딩이고
  `Proto`는 `code/consts/names/protos/call_names/source_map/exc/flags/locals/params/
  arity/name`를 owns, `Program`은 `source_path/names/consts/protos/msgs/root`를 owns.
  `source_map.len() == code.len()`이 validate의 1번 규칙이고 `stack_effect`는 `i64`를
  돌려주며 `JMP_IF`는 `FLAG_JUMP_POP`이 세트일 때만 pop한다(`and`/`or`는 short-circuit
 한 쪽을 남기기 때문).
- name index는 operand에, count/flag는 flags에 들어간다:
  `GET_ATTR/SET_ATTR/ARG_KW/ADD_MEMBER/MAKE_CLASS/LOAD_BASE_CLASS/REGISTER_CLASS/
  MAKE_ENUM`. `BUILD_TUPLE_NAMED`는 names-table index를 flags에, count를 operand에
  둔다. `CALL`은 flags = `call_names` index, operand = arg count.
- `Program.msgs`는 `UNSUPPORTED` 메시지 테이블이고 module-level append-only → `.unbc`
  container도 append-only로 유지된다. `Param`(`name/type_name/flags/default`)는 Phase 10
  restructure를 피하려고 지금 넣었다.

### world/global/native

- `World`는 `path`, `global_scope`, `natives`, `stdout`, `allowed`를 owns한다.
  root `Context`의 scope는 global scope 그 자체다(reference는
  `new Context(Global.GetGlobalScope(), null, null)`).
- `native::init(path)` = `world::reset()` + `set_path` + `init_classes()` +
  `register_native("io", io::members())` + builtins 14개 global scope bind +
  `__name__ = "__main__"`. `Global.Init`는 `io`를 bind하지 않는다 → 모든 스크립트는
  `use io`로 시작해야 한다.
- `builtin`은 `[BuiltinModule]`이고 `[NativeModule]`이 아니다 → `natives`에 없고
  `use builtin`은 `Global.Import`로 가서 file-not-found Panic가 난다(확정된 동작).
- `init_classes`는 `[BuiltinType]` instance(`int` Int(0), `float`, `bool` false,
  `str` "", `date` 0, `list`, `tuple`, `set`, `dict`, `iter` seq([]))만 등록하고
  `originalClasses` entry는 **빈 member table**으로 등록한다. builtin `[Native]` member
  테이블(`list.add`, `str.to_upper`, ...)은 native-member 단계에서 채우므로, 지금은
  `Property`가 `'str' object has no attribute 'to_upper'`를 준다 → 기록된 Phase 8
  divergence.
- `use`의 4가지 binding shape(alias rebuild via `UnType.Create(alias)`,
  `'{k}' already exists in module '{alias}'`, wildcard copy,
  `module '{name}' has no member '{name}'`,
  `module '{name}' is not allowed in this environment`,
  `native module '{name}' not found`)를 verbatim 재현한다. file-module import
  (`Global.Import`)는 Phase 11이다.
- 11 tests: `use io`, `use io as myio`, `use io(write)`, `use io(write) as myio`,
  existing alias rebuild(타입이 alias 이름으로 바뀜), missing member Panic,
  unregistered module Panic, disallowed module Panic, wildcard duplicate Panic,
  builtin class registry, builtin 결과 shape(`type(1)` → `<type: int>`,
  `len([1,2,3])` → `3`, `repr("a")` → `str`, `chr(65)` → `A`,
  `throw("boom")` → header `error`).

## Phase 8b 정합 데이터 (compiler + stack VM)

- `rust/src/compiler.rs`(823줄): AST→Proto, name/const/msg proto-local slot, `code(node)` 값 규약 + statement `UNWRAP`/`POP`/`raises`(D-039), short-circuit/`??`/ternary 인코딩(D-041), optional-chain guard를 `children[0]` 직후 배치(D-038, Index/Slice/Call은 `IsOptionalChain(children[0])`만), `ARG_BEGIN`+ARG accumulator+`FLAG_ARG_CHECK`(D-042), tuple destructuring `UNPACK` forward bind, `?` = `PUSH_TRY_PROP`+`PROPAGATE` exc entry·handler가 try entry 먼저 pop(D-040), `Use` runtime native/file dispatch(D-045), `TO_STR`+`FSTRING`(D-044), `Op::Unpack=0x60`/`Op::ToStr=0x61`.
- `rust/src/vm.rs`(823줄): stack VM run loop, 매 exec 후 top-of-stack `Payload::Panic` 검사로 host exception abort(D-034), `LOAD_CONST` world interning, `TO_BOOL`/`UNWRAP`, `JMP_IF`(TRUE/NONE/POP/STRICT), `UNPACK` reverse push, `SET_ITEM`/`SET_SLICE`(step!=1 → `slice assignment with step != 1 not supported`), `FSTRING` parts→`to_str`, `CALL`/arg accumulator/`FLAG_ARG_CHECK`, `USE` native/file, `PROPAGATE` top-level rethrow(header/span 보존), `Program::name/constant` proto-local resolve.
- `rust/src/runtime/world.rs`: `log` + `log_text()`/`log_clear()`, `flush_stdout`가 drain 버퍼를 log에 append(테스트가 run 전체 stdout 읽기).
- `rust/src/runtime/global.rs`: `import_file` source-dir→`PATH` resolve, missing → `module '{PATH-joined}' not found` Panic, existing file module → `file module '{full}' is not loaded (phase 11)`.
- `rust/src/runtime/collections.rs`: `list_slice` step!=1 → capacity-0 `[]`, bounds check Panic 유지(D-035). `rust/src/runtime/obj.rs`: primitive `Init` dispatch(D-037).
- CLI: `unvm run`(exit 1 on error/panic/propagate, trace 10줄 cap), `unvm dis`(names/consts/protos/instructions+running stack depth+exc). parse error stdout+CRLF(D-033).
- Differential: `C:\Users\User\AppData\Local\Temp\opencode\probes11\cmp3.ps1`(29 probes p01..p29) → **PASS 27 / DIFF 2**. p11(`7 % 0`) / p28(`l[4:0:-1]`)는 메시지·앞 줄·exit 1 정합, `.NET` stack trace만 미재현(D-034).
- Tests: `cargo test --release` **167 passed / 0 failed** (`compiler::tests` 17, `vm::tests` 25), `cargo build --release` 0 errors / 0 warnings.
- Reference baseline 불변(2026-11-11): `scripts/test.ps1` → Positive 33/1265/0/0, Negative 42 matched, Known 1(`test/regression/defects.un`, 6 assertions), exit 0.

### 남은 stub

- Phase 8b의 `compiler`/`vm`/`run`/`dis`는 표현식·문장(Phase 8 scope: literals/Identifier/Assign/Binary/Unary incl. `?`/Ternary/Call/Property/Index/Slice/Tuple/List/Dict/FString/Use/Block)만 지원한다. 함수/람다/if/for/while/match/try/class/enum/defer/go/using/return/break/skip/tagged/set/spread/kwspread는 `UNSUPPORTED`다.
- `stream` payload(`io.open`, `io.lines`, file 함수들)는 Phase 14.
  `stream/lock/pool/http_client/web_socket/random/stopwatch` payload는 미착수.
  `template`는 `Payload` variant가 없다.
- `fn_call`/`obj::call`은 positional `&[Obj]`만 받는다. Phase 10은 `Args`/`Flow`로
  refactor하고 compiled-proto executor hook를 추가해야 `LFn` call이 VM에 연결된다
  (`Break`/`Skip`이 `LFn.Call`를 **잡지 않고** 탈출해야 하므로 필수).

## 현재 작업

- Phase 8b(compiler + stack VM + CLI `run`/`dis`) 완료(`633ae20`~`508b51d` + docs 커밋).
  `rust/src/compiler.rs`(823줄), `rust/src/vm.rs`(823줄), `compiler::tests` 17, `vm::tests` 25.
  `cargo test --release` **167 passed / 0 failed**, `cargo build --release` 0 errors / 0 warnings.
  differential `cmp3.ps1`(`C:\Users\User\AppData\Local\Temp\opencode\probes11\`, 29 probes p01..p29):
  **PASS 27 / DIFF 2** — p11(`7 % 0`)과 p28(`l[4:0:-1]`)만 `.NET` stack trace 문자열 차이(D-034),
  메시지/앞 줄/exit code 정합. reference baseline(`scripts/test.ps1`) 불변: Positive 33/1265/0/0,
  Negative 42 matched, Known 1 file, exit 0.
- 남는 stub: `fn_call`/`obj::call`은 positional `&[Obj]`만 받고 `Flow`/compiled-proto executor hook 없음(Phase 10),
  `Future` payload 미구현(Phase 16), `stream`/`lock`/`pool`/`http_client`/`web_socket`/`random`/`stopwatch` payload 미착수,
  `template` variant 없음, builtin `[Native]` member 테이블 비어 있음(ground truth 개별 probe 필요).
- `Iters` self-methods `map`/`filter`/`any`/`all`/`sum`/`first`/`last`와 `iter` native module(21 함수)는 VM member dispatch 후.
- file-module import(`Global.Import`/`Runner.Load`, `mod.un`/디렉터리 전체 로드)는 Phase 11. `test/fixtures/*` 미통과 예상.
- Phase 11/14 scope 미탐사: `Flow/{Lock,Pool}.cs`, `IO/{Stream,HttpClient,WebSocket}.cs`,
  `Util/{Random,Stopwatch}.cs`, `Base/Template.cs`, `Type/{BaseType,CollectionType}.cs`,
  `Evaluator.cs`의 Function/Lambda/Match/For/While/If/Try/Defer/Class/Enum/Using/Go/Wait/Annotations, `Runner.cs`.
- `read`/`grep` 도구가 존재하지 않는 파일 내용을 지어낼 때가 있다 → ground truth는 shell로 확인.
  `cargo ... 2>&1`는 성공해도 PowerShell에서 exit 1을 줄 수 있다.

## Phase 10 종합 검증(함수 / 람다 / return)

`rust/src/compiler.rs` / `rust/src/vm.rs` / `rust/src/runtime/function.rs`가 함수 3형태
(`fn name()`, `fn Type.method()`, `fn(a, b) -> expr` lambda)와 `->`(return)를 다룬다.

- 컴파일러: 함수 본문을 **nested proto**로 만든다(`Proto.protos`가 부모 proto의 slot 목록이고,
  `Program.protos`는 module 단위 평탄 목록이다). `MAKE_FUNC`(operand = slot, flags high byte =
  기본값 개수)이 `LFn`을 만들고 이름을 `Scope.Set` 규칙으로 바인딩한다. `RETURN`은 프로토를
  끝낸다. `Program::validate`는 `RETURN`을 terminator로 보고, 그 뒤의 코드는 unreachable이다.
- 파라미터: `Param { name, type_name, flags, default }`는 module name index를 쓴다.
  `Fn.GetArgs` 규칙 그대로 `essential/optional/positional/keyword` flag를 채운다.
  기본값 표현식은 **정의 시점**에 평가되므로 바깥 proto에 코드를 두고 `MAKE_FUNC`이 pop한다.
- 런타임: `Body::Proto`를 가진 `Fn`은 VM이 중첩 VM(`VM::new_proto`)으로 proto를 실행한다.
  `function::begin_call`/`end_call`이 reference `LFn.Call`의 prologue/epilogue를 담당한다
  (재귀 한도 1000, 함수 스코프, 인자 바인딩, defer/using drain, call depth 복원).
  callee context가 closure의 frame list를 공유해서 `?`의 `Frames.Count > 0` 판정이 reference와
  같다. `LFn.Call`이 `PropagateFlow`를 잡는 규칙(호출자가 call site에서 unwrap)도 그대로다.
- 오류 위치: 중첩 VM 덕분에 함수 안에서 raise된 언어 오류의 위치가 유지된다.
  `fn_call` 경로(멤버 호출 등)는 위치를 잃고 `Err` 값으로 떨어뜨린다.
- `unvm`은 64MB 스택의 worker thread에서 실행한다(VM이 Un 호출마다 Rust frame을 쌓는다).
- 문서화한 차이: 함수 안의 `break`/`skip`은 caller의 반복문까지 unwind 하지 않는다(D-049).

### 검증

- `cargo build --release` 0 errors / 0 warnings, `cargo test --release` **189 passed / 0 failed**.
- differential `C:\Users\User\AppData\Local\Temp\opencode\p10\cmp10.ps1`(42 probes):
  **41 PASS / 1 DIFF**. DIFF는 값 없는 `->` 하나이고, reference는
  `System.ArgumentOutOfRangeException`을 .NET stack trace와 함께 출력하는 반면 Rust는 첫 줄만
  출력한다(D-034의 host 예외 규칙).
- 기존 differential `probes11/cmp3.ps1`(29 probes) baseline 유지: **27 PASS / 2 DIFF**.
- reference baseline(`scripts/test.ps1`) 불변: Positive 33/1265/0/0, Negative 42 matched,
  Known 1(`test/regression/defects.un`, 6 assertions).
## 다음 단계

1. Phase 8b working tree 정리: PROGRESS/DECISIONS/BYTECODE 반영 후 의미 있는 단위로 커밋(`src/main.un` 제외).
2. member ground truth 개별 probe(각 case abort): `str` `to_upper`/`split`/`replace`/`copy`,
   `dict` `get`/`keys`/`values`, `list` `pop`/`sort`/`order`/`binary_search`,
   `set` `union`/`intersect`/`difference`. builtin `[Native]` member 테이블 채우기 범위 확정.
3. differential harness를 `scripts/`로 승격(`cmp3.ps1`/probe set 정리)해 stdout/exit/stderr 비교를 재현 가능하게 유지.
4. Phase 9 control flow: 완료(`9921c75`). `if`/`while`/`for`/`break`/`skip` + validator의 jump 깊이 검증.
5. Phase 10 functions/lambda/return: 완료. nested proto + `MAKE_FUNC`/`RETURN` + `begin_call`/`end_call` + 중첩 VM.
6. 남은 Phase: 11(file-module import, `test/fixtures/*`), 12 try/catch, 13 defer/using, 14 class/enum/member dispatch, 16 go/wait, 17 `.unbc`.

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

## Phase 10~12 종합 검증(함수 / 클래스 / 예외 / 네이티브 모듈)

`rust/src/compiler.rs` / `rust/src/vm.rs` / `rust/src/runtime/` / `rust/src/native/`가
언어의 나머지 문법과 네이티브 모듈을 다룬다.

### Phase 10 — 함수 / 람다 / return

- 함수 본문은 **nested proto**가 되고 `MAKE_FUNC`(operand = 부모의 proto slot, flags
  high byte = 기본값 개수)이 `LFn`을 만들어 이름을 `Scope.Set` 규칙으로 바인딩한다.
  `RETURN`은 프로토를 끝내고, `Program::validate`는 `RETURN`을 terminator로 본다.
- 파라미터 기본값은 **정의 시점**에 평가되므로(`Fn.GetArgs` 규칙) 바깥 프로토에 코드가
  놓이고 `MAKE_FUNC`이 스택에서 pop한다.
- `LFn.Call`은 본문을 직접 돌리므로 함수 본문은 `EvalBlock`을 지나지 않는다(문장 값의
  unwrap 없음).
- 컴파일된 호출은 **중첩 VM**에서 proto를 실행한다. 그래서 함수 안에서 raise된 언어
  오류의 위치가 유지된다. callee context는 closure의 frame list를 공유한다(`?`의
  `Frames.Count > 0` 판정이 reference와 같다).

### Phase 11 — 파일 모듈

- 네이티브가 아닌 `use`는 파일 모듈을 읽는다: 파일, 디렉터리의 `mod.un`, 디렉터리의
  모든 `.un`. 모듈은 전역 스코프를 부모로 하는 자기 스코프에서 실행되고, 그 스코프의
  심볼들이 모듈의 멤버가 된다.
- 모듈 맵은 전체 경로로 캐시되고, 읽는 중인 경로가 다시 요청되면
  `circular import detected: '{path}'`.
- 컴파일된 함수는 자기 프로그램을 들고 다닌다(모듈마다 프로그램이 다르므로).

### Phase 12 — 클래스 / 예외 / 네이티브 모듈

- `class Name(params): Base`는 `MAKE_CLASS` / `REGISTER_CLASS` / `LOAD_BASE_CLASS` /
  `SET_BASES` / `ADD_MEMBER` / `ADD_METHOD` / `MAKE_PROPERTY`로 컴파일된다. 타입 객체는
  멤버를 붙이는 동안 스택에 남는다(클래스 표현식의 값이 타입 객체).
- 접근자는 `Property`가 되고, getter/setter는 각자 proto이며 setter는 대입된 값을
  받는다. 기본값이 없는 property는 base class의 property 기본값을 물려받는다.
- `match`는 패턴이 proto의 pattern 표에 있고 `MATCH_PATTERN`(`0x62`)이 스택의 값을
  시험한다. 케이스 스코프는 현재 스코프의 자식이고, 에러 변수는 `Err(message, header)`로
  바인딩된다.
- `defer` 블록은 nested proto이고, 함수의 epilogue가 역순으로 실행한다. `try`의
  `finally` 블록은 return이나 propagation이 블록을 떠나기 전에 실행되고, 대기하던
  에러/return/propagation이 그 뒤에 이어진다.
- 데코레이터(`@name`)는 역순으로 적용되고, 장식된 함수는 데코레이터가 실행된 뒤
  바인딩된다.

### 네이티브 모듈

| 모듈 | 상태 |
|---|---|
| `io` | `write` / `read` / `open` ✓, `stream` payload ✓ |
| `inspect` | 7개 ✓ |
| `math` | 30개 ✓ (.NET의 to-even 중간 반올림) |
| `iter` | 20개 ✓ (`map`/`filter`는 lazy) |
| `os` / `sys` | 18 / 19개 ✓ |
| `fs` | 37개 ✓ |
| `time` | `sleep` / `now` / `stopwatch` ✓ |
| `random` | 17개 ✓ (공유 시드 생성기) |
| `re` | 9개 ✓ ( 자체 backtracking 엔진) |
| `flow` | `spawn` / `lock` ✓, `pool` / `lock` / `future` payload ✓ |

### 검증

- `cargo build --release` 0 errors / 0 warnings, `cargo test --release` **189 passed / 0 failed**.
- differential `C:\Users\User\AppData\Local\Temp\opencode\suite\cmp_suite.ps1`
  (MANIFEST의 34개 파일): **33 PASS / 1 DIFF**. DIFF는 `test/unit/native/set.un`인데
  출력이 같고 실행 시간만 40초 제한을 넘긴 경우다(단독 실행은 0.03초).
- `test/test_suite.un`은 reference와 동일하게 **109 passed, 0 failed**.
- reference baseline(`scripts/test.ps1`) 불변: Positive 33/1265/0/0, Negative 42 matched,
  Known 1(`test/regression/defects.un`, 6 assertions).

## Phase 13~17 종합 검증(나머지 기능)

- **Phase 13** `defer` / `using` ✓: `defer` 블록은 nested proto이고 함수의 epilogue가 역순으로
  실행한다. `using`은 대입된 값의 `__entry__`를 등록하고 함수 종료 때 `__exit__`를 부른다.
- **Phase 14** `stream` payload ✓, `io.open(path, mode)` ✓ (`r`/`w`/`a`/`rw`).
- **Phase 15** tagged template ✓: `tag\`hello ${name}\``은 tag 함수를
  `fn(values, strings)`로 부른다.
- **Phase 16** `go` / `wait` ✓: `MAKE_FUTURE`와 `AWAIT`. 스레드를 쓸 수 없어서(D-054)
  표현식을 바로 계산해서 future에 넣는다. 이 즉시 평가는 Phase 19e에서 deferred future로
  바뀌었다(D-063).
- **Phase 17** `.unbc` 컨테이너 ✓: `bytecode::unbc`가 쓰고 읽는다. magic/version/flag가
  다르면 validation이 실패한다. 소스 텍스트를 함께 저장할 수 있어서(flags bit0) 오류
  위치가 원본 파일의 줄을 가리킨다. CLI는 `unvm compile <file.un>`과
  `unvm run <file.unbc>` ✓.

### 검증

- `cargo build --release` 0 errors / 0 warnings, `cargo test --release` **192 passed / 0 failed**.
- differential suite(MANIFEST 34개 파일): **33 PASS / 1 DIFF**(출력은 같고 실행 시간만 제한
  시간을 넘긴 경우).
- negative suite(42개 파일): **38 PASS / 4 DIFF**. 남은 네 개는 문서화된 차이다:
  .NET 스택 오버플로(Rust는 재귀 한도), .NET `Array.Sort`의 일관성 검사,
  `go`의 racy한 타이밍(D-063, Phase 19e에서 확인).

## Phase 18 종합 검증(random 객체 / http / ws)

- **Phase 18a** `random()` 객체 ✓: `random()`가 생성자 객체를 만들고 타입 이름은 `random`이다.
  모듈 함수는 공유 생성기를 쓰고, 객체의 멤버 16개(`next`/`int`/`float`/`bool`/`bytes`/
  `shuffle`/`choice`/`choices`/`sample`/`chance`/`seed`/`uuid`/`str`/`weighted`/`range`/
  `value`)는 자기 자신의 생성기를 쓴다. `seed`는 양쪽에서 생성기를 교체하므로 같은 시드는
  같은 값을 낸다. `Clone()`은 같은 cell을 공유하므로 시드도 계속 공유된다(D-059).
- **Phase 18b** `http` client ✓: `http.connect()`는 host client payload를 담는다. 타입
  이름은 `http`, display는 `http`, `==`는 host client reference 비교(자기 자신만 true),
  `str`/`bool`/`len`/`()`는 base 메시지를 답한다.
- **Phase 18c** `ws` socket ✓: `ws.connect(url)`은 plain TCP 위에서 RFC 6455 handshake를
  한다(SHA-1 + base64 accept 검증, magic GUID는 RFC의 `258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).
  타입 이름은 `socket`, display는 `<socket {state}>`이고 state는 .NET `WebSocketState`의
  이름(`Open`/`Closing`/`Closed`), `bool`은 `state == Open`, `==`는 socket reference 비교다.
- `ws.connect`의 URI 검증은 `System.Uri`의 문구를 그대로 재현한다(ground truth `refw3.txt`):
  빈 URI(`Invalid URI: The URI is empty.`), 형식 불능, host parse 불가, `ws://`의 hostname,
  invalid port(`Invalid URI: Invalid port specified.`), ws가 아닌 scheme
  (`Only Uris starting with 'ws://' or 'wss://' are supported. (Parameter 'uri')`),
  연결 실패(`Unable to connect to the remote server`).
- TLS(`https`/`wss`)는 범위 밖이다(D-057): std 전용·무의존 구현이라 TLS가 없고, connect
  실패는 reference의 `Unable to connect to the remote server`로 답한다.
- `http_client`/`web_socket` 멤버 표는 등록하지만 도달할 수 없다(D-058): `[NativeType]`
  이름은 `originalClasses`에만 등록되고 객체의 타입 이름은 `http`/`socket`이므로 멤버
  접근은 `'http' object has no attribute 'get'` / `'socket' object has no attribute 'send'`로
  실패하고 `inspect.attr`는 `[]`다. 그래서 request/frame I/O는 Un 코드에서 관측할 수 없고
  parity 대상이 아니다(D-060).

### 검증

- `cargo build --release` 0 errors / 0 warnings, `cargo test --release` **192 passed / 0 failed**.
- network probe suite(`p10\wscmp.ps1`, `ws1..ws6` + `http1..http7`): **13 PASS / 0 DIFF**
  (stdout과 exit code가 byte-identical). `ws` probe는 Node `ws` package의 echo server
  (port 19085)에 실제 handshake와 send/receive를 한다. ground truth는 `refw3.txt`(URI
  메시지표), `refw5.txt`(live socket 객체의 표), `refw6.txt`(interpolation/`str`/`bool`/
  `not`/`==`/globals), `refh7.txt`(http 객체의 표).
- differential suite(MANIFEST 34개 파일): **34 PASS / 0 DIFF**. `test/unit/native/set.un`의
  실행 시간 한도 초과(flake)는 단독 실행이 0.03초에 끝나서 해소됐다.
- negative suite(42개 파일): **38 PASS / 4 DIFF**.
- reference baseline(`scripts/test.ps1`) 불변: Positive 33 files / 1265 passed / 0 failed /
  0 crashed, Negative 42 matched, Known 1(`test/regression/defects.un`, 6 assertions).

## Phase 19 종합 검증(최신 reference 기능)

`main`의 최신 커밋 6개를 `main` merge(`89f2805`)로 받아서 Port했다: `0a6bbe0`(risk fixes),
`ebde101`(binding 없는 `for`), `63cab26`(set literal과 `Set.In`), `42fd0bf`(`atomic`),
`4941988`(`time`), `f43637f`(dead-code 정리).

- **Phase 19a** set literal ✓: set 리터럴의 문법은 `{1, 2, 3}`이고 `[1, 2, 3]`은 list로
  남는다. reference는 `{`를 여는 표현식에서 첫 `:`이 없으면 `NodeKind.Set`을 만들 수 있었지만
  평가 케이스가 없어서 `unsupported node kind Set`로 실패했고, `63cab26`이 `EvalSet`을 추가했다.
  Rust는 같은 `code_container`가 `MAKE_SET`을 쓰므로 이미 평가되고 있었고, Port한 것은 set
  membership이다. 비어 있는 중괄호 `{}`와 `:`이 있는 중괄호 `{"a": 1}`은 dict로 남는다.
  `in`은 `HashSet.Contains`와 같은 comparer(`key_eq`)를 쓴다. `test/unit/native/set.un`의
  `1 in set`이 true다.
- **Phase 19b** binding 없는 `for` ✓: `for expr`은 헤더가 `[Wildcard(iterable), expr, body]`로
  parse되고, compiler의 `NodeKind::Wildcard` target arm이 iterator를 pop한다. `for _ in x`와
  같은 경로다.
- **Phase 19c** `atomic` module ✓: `atomic.int(initial)`/`atomic.bool(initial)`는 interlocked
  counter와 flag를 담고 `get`/`set`/`add`/`inc`/`dec`/`exchange`/`compare_and_swap`를 답한다.
  인자 이름은 reference의 `[ArgInfo]` 이름(`initial`/`amount`/`expected`/`newValue`)을 따르고,
  오류 문구는 reference가 hard-code한 `'value'`/`'new_value'`다. `Clone()`은 counter/flag/
  pool/lock에서 오류, `Copy()`는 네 개 모두 `this`다. 스레드가 없어서(D-012) 값은 한 cell에
  담고, interlocked 연산이 만드는 값과 같은 결과를 낸다.
- **Phase 19d** `time` 확장 ✓: `utc_now`/`epoch`/`epoch_ms`/`from_epoch`/`from_epoch_ms`/
  `monotonic`/`monotonic_ms`와 `stopwatch.elapsed_ms()`를 추가하고, `TICKS_PER_SEC`는 .NET의
  `10_000_000`(100ns tick)이다. `Date`는 UTC instant를 그대로 쓴다(D-032).
- **Phase 19e** `go` 지연 ✓: `go expr`는 `RETURN`으로 끝나는 nested proto를 `MAKE_FUTURE`의
  operand로 담고, future는 그 proto와 `go` 시점의 scope chain 복사본을 가진다. `AWAIT`가
  captured chain의 child scope에서 proto를 실행하고 값을 memoize한다(`Task.Result`의 동작).
  `Future.Clone()`은 같은 task를 공유한다. reference의 `EvalGo` task는 `go` 시점에
  `new Scope(context.Scope)`를 열므로, future는 자기 iteration의 loop var를 본다.
- `f43637f`가 지운 것들(`UnType`의 `TGeneric`/`UGeneric`/`Infinity`/`Skip`/`Break`,
  `ArgInfo.Keyword`, `Native.Async`, `Source.FullName`, `IFileSystem.Enumerate*`,
  `IO.ResetStdout`/`ResetStdin`)은 Rust에 이미 없다. `Native.Async`는 reference의 binder가
  이미 무시했고(D-061), `Scope`의 `SetLocalOrDeclare`/`Declare`/`ContainsKeyInTop`과
  `Snapshot`/`Restore`는 reference가 `SetLocal` 하나로 합쳤다(D-062).

### 검증

- `cargo build --release` 0 errors / 0 warnings, `cargo test --release` **196 passed / 0 failed**.
- differential suite(MANIFEST 37개 파일): **37 PASS / 0 DIFF**(stdout과 exit code가
  byte-identical). `test/unit/native/set.un`, `test/unit/runtime/for_no_bind.un`,
  `test/unit/native/atomic.un`, `test/unit/native/time.un`, `test/integration/concurrency.un`,
  `test/test_suite.un`(`109 passed, 0 failed`) 포함.
- `go` probe suite: `go (n * n)`이 함수 body 안에서 파라미터를 보고(`f= 25`), `go (i * 2)`가
  loop의 각 iteration 값을 유지한다(`out =  [2, 4, 6]`). reference 6회 실행과 같은 출력.
- negative suite(42개 파일): **38 PASS / 4 DIFF**.
- network probe suite: **13 PASS / 0 DIFF** 유지.
- reference baseline(`scripts/test.ps1`): Positive **36 files / 1330 passed / 0 failed /
  0 crashed**, Negative 42 matched, Known 1(`test/regression/defects.un`, 6 assertions).
  `test/MANIFEST.txt`에 `unit|test/unit/runtime/for_no_bind.un`,
  `unit|test/unit/native/atomic.un`, `unit|test/unit/native/time.un`이 추가됐다.

### 남은 차이(총집합)

- 함수 안의 `break`/`skip`이 호출자의 반복문까지 unwind 하지 않는다(D-049).
- `os.tickCount`는 epoch 밀리초(D-051), `fs.hidden`는 점 접두사(D-052).
- 정규식은 자체 엔진(D-053), `go`/`pool`은 단일 스레드(D-054).
- 스택 오버플로: reference는 .NET의 실제 overflow로 exit `-1073741571` + stderr
  `Stack overflow`를 내고, Rust는 재귀 한도(1000)가 `maximum recursion depth exceeded`로
  exit 1을 낸다. `test/negative/depth_limit_crash.un`와 `test/negative/property_recursion.un`
  가 이 차이이고(D-034), .NET의 overflow 값은 parity target이 아니다.
- `go`는 `go` 시점의 scope chain 복사본을 쓴다(D-063). `test/negative/go_loop_var.un`은
  reference가 racy라서 deterministic하게 맞출 수 없다: reference 6회 실행 중 4회가 exit
  `-1`, 2회가 exit `1`이고, manifest가 기대하는 `undefined variable 'i'`는 task가 loop
  scope를 버린 뒤에 시작하는 순서에서 나오는 결과다. Rust는 `sorted =  [1, 4, 9]`로 exit 0이다.
- .NET `Array.Sort`는 비교 일관성 위반을 `inconsistent results`로 throw하고 exit 1을 낸다
  (`test/negative/order_crash.un`). Rust는 같은 정렬에 같은 비교를 쓰므로 throw하지 않는다.
  (exit code 1은 같고 stderr 문구가 없다.)
- TLS(`https`/`wss`)는 지원하지 않는다(D-057). `http`/`ws`의 request/frame I/O는 Un 코드에서
  관측할 수 없어 parity 대상이 아니다(D-060).

## Phase 20 종합 검증(`re` 엔진 교체)

`re`의 엔진이 .NET `System.Text.RegularExpressions`의 자체 호환 backtracking 엔진으로
교체됐다(`rust/src/native/regex/`, D-053 교체). .NET의 알고리즘을 그대로 이식했다:
`RegexParser`(두 패스 capture counting 포함) → `RegexWriter` → `RegexInterpreter`(VM).
node kind/opcode 번호는 .NET과 동일하다. `.NET`의 `Regex.Escape`, `ScanDollar`
(`$` expansion, `${name}`, `$0..$9`는 전체 자릿수 소비 + 슬롯 없으면 literal),
`Regex.Split`(캡처 그룹을 결과에 splice)과 `Matches()` enumeration(빈 매치는 한 글자
bump, len에서 멈춤)도 그대로 재현했다. `IO.write`가 > 0x7F를 `?`로 자르는 특성까지
포함해 25개 probe가 전부 byte-identical이다.

- 13→25 probes(`probe_re*.un`): **25 PASS / 0 DIFF**.
- 엔진 내부: 고정 용량 배열(track/stack/crawl) + `.NET` peek-after-pop 규약,
  `Backtrack()`의 negative marker = `BacktrackingSecond` 규칙(`back`을 교체, OR이 아님),
  `Branchcount`는 `[mark, count]`를 track에 `-codepos` marker로 저장, loop arm은 mark를 track에 저장.
- Unicode category: BMP+astral table(`unicode_table.rs` 계열)을 .NET과 같은 numbering으로
  병합해 생성. astral 문자는 char 하나로 분류되므로(.NET은 surrogate 쌍으로 봄)
  category 매칭이 astral에서도 동작하는 점만 divergence다(D-064).
- case-insensitivity는 culture 테이블이 아니라 Rust의 Unicode lowercase/uppercase/titlecase로
  단순화(D-064). `Matches()` bump 규칙, `Split`의 캡처 splice, `$` expansion의 전체 자릿수
  소비를 재현했다.
- differential 중 발견·수정한 엔진 결함(전부 byte-identical 재검증):
  `scan_group_open`의 Rust `break`가 loop 전체를 탈출해 `(?(x)b)` 같은 조건을
  `Unrecognized grouping construct.`로 만들던 구조 오류(switch는 fall-through,
  shared return으로 연결 + `(?)` early-return 누락 수정), 카테고리 group의 result 초기값
  (`false`가 아닌 `true`로 시작해서 모든 문자가 `\p{...}`에 매칭), `NOT_SPACE_CONST`의
  double negation(`\S`가 공백에 매칭), 클래스 내부 shorthand(`[\D]`/`[\S]`/`[\W]`/
  `[\p{...}]`)가 `ch`(=`\`)를 비교해 항상 부정절로 빠지던 문제, `[\b]`의 `_pos` 이중
  rewinding(ranges `(92)`=`\`,`(98)`=`b`), `Branchcount` track frame에 mark가 빠진 것과
  positive marker로 저장한 것, `backtrack()`의 `back |= BACKTRACKING_SECOND`(384)가
  어떤 arm에도 맞지 않아 capture를 복원하지 못해 goto(31) OOB panic을 만들던 문제
  (교체가 맞다), `is_true_quantifier`의 끝자락 OOB read(`a{2`에서 panic).

### 검증

- `cargo build --release` 0 errors / 0 warnings, `cargo test --release` **199 passed / 0 failed**
  (`char_class_category_groups`, `class_shorthand_escapes`, `match_and_find_all` 추가).
- probes: **25 PASS / 0 DIFF** (`probe_re.un` ~ `probe_re24.un`, `probe_repan.un`).
- differential suite(MANIFEST 37개 파일): **37 PASS / 0 DIFF** 유지.
- negative suite: **38 PASS / 4 DIFF**(문서화된 차이 유지).
- network probe suite: **13 PASS / 0 DIFF** 유지.
- reference baseline(`scripts/test.ps1`) 불변: Positive 36 files / 1330 passed / 0 failed /
  0 crashed, Negative 42 matched, Known 1.
- `src/main.un`은 손대지 않았다. C# 코드(`src/*.cs`), 테스트, build config 변경 없음.

## Phase 20 후속(성능): `KeyMap` bucket-chained hash table(D-065)

- `KeyMap`을 insertion-ordered linear scan에서 **bucket-chained hash table**로 교체했다
  (power-of-two `buckets` + per-entry `next` chain, `Entry { hash, key, value, used }`).
  제거는 tombstone + `next`에 threading된 free list 재사용 → live entry의 삽입 순서가
  보존되고 `remove`도 순서를 바꾸지 않는다. duplicate key는 가장 이른 index를 반환한다.
- `Set`도 같은 `KeyMap`으로 backed 했다(value = `none`). `native/members.rs`의 set
  멤버 함수들도 O(n) scan에서 O(1) lookup으로 바꿨다.
- dict5000 벤치마크: 17.9ms → ~9.3ms (reference ~6.1ms, loop-only는 양쪽 parity).
  differential 37 PASS / 0 DIFF, negcmp 38/4(문서화된 차이), wscmp 13/0, fullcmp 37/0,
  `cargo test --release` **199 passed / 0 failed**, `cargo build --release` 0/0.

### 남은 차이(총집합)

- UTF-16 vs `char`: .NET은 surrogate 2개 code unit, Rust는 char 하나로 센다(D-064).
  BMP 리터럴/클래스는 동일하고 astral 문자의 category 매칭만 동작한다.
- case-insensitivity는 Rust Unicode 대소문자 매핑으로 단순화(Turkish 등 culture 특수
  케이스는 다름, D-064).
- 정규식 timeout은 두 구현 모두 지정되지 않는다(.NET도 `RegexOptions.None` + timeout 기본 off).
  문서화 대상 아님.
