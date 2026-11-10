# DECISIONS.md — 결정 로그

형식: `D-NNN | 제목 | 상태 | 결정 | 영향`
- 상태: **Accepted** = 채택되어 코드에 반영, **Proposed** = 채택되었으나 코드 반영 전, **Pending** = 검증 필요

## D-001 | C# 코드는 수정하지 않는다 | Accepted

Reference Implementation(`src/`, `test/`, `.csproj`)은 읽기 전용 기준선(oracle)이다.
Rust 코드와 문서는 `rust/`, `docs/rust-migration/`에만 추가한다. 커밋/푸시는 사용자 승인 후.

## D-002 | 단일 crate `unvm` | Accepted

`rust/Cargo.toml`에 bin `unvm` + lib `unvm` 하나. workspace/다중 crate 분리하지 않는다.
이유: reference가 단일 프로젝트이고, module 경계가 `src/` 폴더 구조와 1:1로 대응한다.

## D-003 | 빌드 도구chain: `cargo +stable-x86_64-pc-windows-gnu` | Accepted

이 개발 머신에 MSVC `link.exe`가 없다. rustc/cargo 1.99.1(gnu)으로 빌드한다.
Rust는 PATH에 없다: `$env:Path += ";$env:USERPROFILE\.cargo\bin"`.
C#는 `$env:DOTNET_ROLL_FORWARD='Major'` 후 `.\bin\Release\net8.0\Un.exe`.

## D-004 | Desugarer/Optimizer는 AST 단계에 남는다 | Accepted

컴파일러는 **desugar+optimize된 AST만** 소비한다. reference가 AST 단계에서 하는 변환을
VM/컴파일러에서 재구현하면 동작이 갈린다(예: `and/or` const folding 제외, `if` true fold 시
`body`를 그대로 반환하는 것). 64-pass fixed point와 pass 중 `Panic` → node 유지도 AST 단계의
계약이다.

## D-005 | Node identity는 `changed` 플래그로 재현한다 | Accepted

reference는 `ReferenceEquals(before, root)`로 fixed point를 판정한다. Rust는 값 타입 `Node`를
쓰므로 `optimize_pass`가 `(Node, changed: bool)`를 반환한다. 상위에 변경이 전파되는 구조
(unchanged subtree는 공유)이므로 **top-level changed == reference의 ReferenceEquals 검사**다.
pass 수는 `Runner.LastOptimization`에만 노출되고 출력에는 나타나지 않으므로 pass 수 자체는 정합 대상이 아니다.

## D-006 | `null` children은 `NodeKind::Null` | Accepted

reference는 생략된 slice bound를 `null` 자식으로 표현한다(`a[:2]`, `a[1:]`).
Rust는 `Option<Node>`를 쓰지 않고 `NodeKind::Null` 노드를 쓴다. VM은 이를 `Obj.None`으로 평가한다.
`a[1:2:3]`는 영향 없다. `a[..2]`는 reference에서 `unexpected primary token DotDot`로 실패한다.

## D-007 | 문자열/타입 이름/off-by-one은 reference 문자열을 verbatim 재현한다 | Accepted

모든 오류 메시지(`'{T}' object has no attribute '{n}'`, `undefined variable '{n}'`,
`no match case`, `tuple unpack mismatch`, …)와 네이티브 오타(`invaild`, `invalid argumnets: index`)는
그대로 재현한다. 테스트가 이 문자열에 의존한다.

## D-008 | `.NET` 예외 이름을 재현해야 하는 케이스 | Accepted

`test/negative/mod_zero.un`은 exit 1 + `System.DivideByZeroException`을 기대하고,
`replace_missing.un`은 `System.NullReferenceException`, `replace_empty.un`은
`System.ArgumentException`을 기대한다. Rust 네이티브/연산 구현은 이 문자열을
**동일한 위치에서** 낸다(header는 `System.X` 형태로 ToString 형태를 유지).

## D-009 | stack overflow exit code | Proposed

`depth_limit_crash.un`과 `property_recursion.un`은 exit `-1073741571`(`0xC00000FD`) +
stderr `Stack overflow`를 기대한다. Rust VM은 재귀 한도 1000(`maximum recursion depth exceeded`)
을 **먼저** 발동시켜야 `stack_overflow.un`(exit 1, `maximum recur`)가 통과하고,
`depth_limit_crash.un`/`property_recursion.un`은 **한도를 우회하는 경로**(property/dunder 재귀)로
native stack overflow를 실제로 일으켜야 한다.
→ Phase 12에서 **Rust에서 실제 stack overflow를 발생시키는 전략**(stack growth 검사 + process abort)
   또는 exit code 재현 전략을 결정하고 사용자에게 보고한다.

## D-010 | KnownFailures 정책 | Accepted

`test/regression/defects.un`(6 assertions)는 **reference의 실제 동작**을 기준선으로 삼는다.
Rust VM이 reference와 같은 동작(같은 실패)을 내면 통과로 간주한다. 문서를 따라 동작을
"고치지" 않는다.

## D-011 | 메모리 모델: `Rc<RefCell<_>>` + `Weak` | Proposed

- 가변 컨테이너(List/Dict/Set/Tup/user object/Property/Fn/Scope)는 `Rc<RefCell<_>>`.
- `Super`/`Parents`는 `Weak<RefCell<Obj>>`(타입 객체는 Global registry가 소유하므로 lifetime 안전).
- `Self`는 `Rc` 공유(C#는 `Fn.Self = receiver` 대입으로 공유하며, receiver는 객체다).
  `Fn → self → members → Fn` cycle은 C# GC가 수집하고 Rust는 **누수**한다.
  accepted trade-off: 테스트 코퍼스에서 수명은 유한하고, cycle collector는 필요할 때 추가한다.
- arena/GC는 채택하지 않는다: `Copy()`가 alias를 반환하는 semantics(`List.Copy()` alias,
  `Dict.Copy()` alias)와 `FieldRef`의 owner 참조 semantics에 `Rc`가 가장 가깝다.
- `Int` intern(-5..256), `Str` intern pool(10000 entries / 1000 chars, 만료 시 전체 clear)는
  `is` identity를 관측하게 하므로 **explicit intern table**로 유지한다.

## D-012 | `go`/`wait`는 single-threaded cooperative scheduler | Proposed

C#는 `go`에서 **같은 `Context`**를 새 `Evaluator`에 넣어 다른 스레드에서 실행한다.
`Scope`/`Obj`에 락이 없어 경쟁 조건이 가능하고, `test/negative/go_loop_var.un`(exit 1,
`undefined variable 'i'`)는 루프 변수 가시성 semantics를 고정한다.
- Rust Phase 16: **coroutine frame 목록**으로 cooperative scheduling. shared mutable scope를
  data race 없이 보존한다.
- ** semantic 차이 보고 항목**: `go` 안에서 블로킹 네이티브(`time.sleep`)를 `wait` 없이 호출하면
  single-threaded VM은 전체가 stop한다(C#는 병렬). 채택 전 사용자에게 보고한다.

## D-013 | scope depth cache는 같은 function body 안에서만 유효 | Proposed

`LOAD_UPV depth<<16|slot` 최적화는 런타임 scope chain을 따라 간다. `match`/`catch`의
scope 전환(`context.Scope = scope`)은 chain을 변형하므로, depth cache는 **같은 function
body안**에서만 쓰고 함수 경계를 넘으면 chain lookup으로 복귀한다. 안전성은 Phase 11의
differential test로 검증하고, 실패하면 plain chain lookup으로 되돌린다.

## D-014 | `.unbc` layout은 count-prefixed + little-endian + fixed-size instruction | Accepted

`BYTECODE.md` §13. 재구축 없이 field append 가능. magic `UNB1`. load 시 validation 실패는
침묵하지 않는다.

## D-015 | 초기 단계에 JIT 없음 | Accepted

`Proto` bytecode는 interpreter loop로 실행한다. JIT은 performance layer로만 붙이고
동작 기준은 VM semantics에 고정한다.

## D-016 | C# 코드 컨벤션 준수는 Rust에 적용하지 않는다 | Accepted

Rust는 Rust 관례(`rustfmt`, snake_case, module-per-concern)를 따른다. 단 **필드명은
reference에 맞춤**(`start`, `length`, `typ`)해서 error/AST 매핑이 추적 가능하게 한다.

## D-017 | Rust CLI는 reference CLI와 `run` 출력만 정합한다 | Accepted

`unvm tokens`/`unvm ast`/`unvm desugar`/`unvm opt`/`unvm dis`는 Rust 전용 진단 명령이고
`un run <file>`의 stdout/exit/stderr가 정합 대상이다. reference에 `ast` dump가 없으므로
Phase 2 정합은 parse 성공/오류 문자열 일치 + 17개 probe로 확인했다(ANALYSIS.md §11).

## D-018 | `Obj`는 `Rc<RefCell<ObjData>>`, payload는 enum | Accepted

`Obj = Rc<RefCell<ObjData>>`이고 `Payload`는 `Plain`/`Error`/`Panic`/`Int`/`Float`/`Bool`/
`Str`/`Date`를 담는다(Phase 6/7에서 `Fn`/`List`/`Dict`/… variant 추가). `type_name`은
`World.type_cache`가 interning한다. `Members`는 insertion-ordered `Vec<(String, Obj)>`다
(C# `Attributes`는 `Dictionary`지만 순서 의존 동작이 존재하므로 순서를 보존한다).

## D-019 | `Super`/`Parents`는 strong `Rc`로 둔다 (D-011의 `Weak` 제안 기각) | Accepted

D-011은 `Super`/`Parents`를 `Weak`로 제안했다. Phase 5 실측: 타입 객체는 `Global`
registry가 프로세스 lifetime 동안 소유하고, `Fn → self → members → Fn` cycle은 reference의
C# GC가 수집하는 것과 같은 주기를 Rust가 leaks 한다. 테스트 코퍼스는 수명이 유한하고
exit 즉시 종료하므로 leak은 관측되지 않는다. `Weak`는 `Super` 접근마다 `upgrade` 실패
 케이스를 만들어 reference의 `Super`가 항상 유효하다는 전제를 깨뜨린다.
 → strong `Rc` 유지. cycle collector는 필요할 때만 추가한다.

## D-020 | `Str` payload는 UTF-8, 문자열 연산은 UTF-16 code unit | Accepted

reference는 `string`을 `char`(UTF-16 code unit)로 index/slice하고 `Length`는 UTF-16 개수다.
Rust는 `Rc<str>`(UTF-8)에 저장하고 `encode_utf16()`로 연산한다. 결과적으로
`len("한글😀") == 4`, `s[0] == "한"`, `s[2:4]`는 surrogate pair를 보존한다.
pair를 분리하는 slice는 lone surrogate가 되고 Rust는 U+FFFD, .NET는 `string`에 lone
surrogate를 저장하고 콘솔에 `?`로 출력한다. **문서화된 divergence**(surrogate pair를
째는 스크립트는 코퍼스에 없음). UTF-16 `Vec<u16>` payload는 이 divergence를 없애지만
모든 네이티브 문자열 연산이 UTF-16 경유가 필요하고, Rust의 `str` 기반 natives
(`re`, `json`, `fs` path)와 붙을 때마다 변환해야 한다. Phase 14에서 재검토한다.
Rust의 `String`은 lone surrogate를 저장할 수 없어서 `str_units`의 surrogate fallback
branch는 `Str` payload에서 도달 불가능하고, `"\u{D83D}"`는 Rust literal로 표기할 수 없다
→ divergence는 pair를 째는 slice에서만 발생한다.

## D-021 | hash는 `Str`만 있고 FNV-1a를 쓴다 | Accepted

reference는 `Str.Hash()`만 override한다(`Int`/`Float`/`Bool`/`Date`는 없음).
`Obj.GetHashCode = Hash().As<Int>() ? hash : Type.GetHashCode()`이므로 `hash(5)`는
`error: cannot hashable object`다. str-key `Dict`/`Set`의 hash는 .NET이 프로세스마다
randomize하므로 구현 간 순서가 이미 재현 불가능하다. FNV-1a(UTF-16 units)를 deterministic
stand-in으로 쓴다. **str-key collection의 iteration order는 정합 대상이 아니다.**

## D-022 | `Date` payload는 .NET tick count, 달력은 Hinnant | Accepted

`Date`는 100ns 단위 tick(`TICKS_PER_DAY = 864_000_000_000`, epoch 0001-01-01)을 저장한다.
`date + date`는 `other.Day`(day-of-month)만큼 더하는 reference quirk를 유지한다.
`date + str`은 `"yyyy-MM-dd"`, `date - date`는 tick diff, display는
`"yyyy-MM-dd HH:mm:ss.fff"`. unix epoch 변환은 62135596800 s 오프셋. 달력 계산은
Howard Hinnant의 `civil_from_days`/`days_from_civil`(proleptic Gregorian, .NET와 동일)을
쓰며 `yoe` 항은 `doe/146096`이다(`doe/146097`은 2000-02-29가 2000-03-01으로 어긋난다).

## D-023 | 호스트 예외는 `Payload::Panic` 값으로 흐른다 | Accepted

reference는 `throw`한다. Rust에 unwinding이 없으므로 `System.DivideByZeroException` 같은
호스트 예외는 `Payload::Panic { name, message }`를 반환하고, VM은 문장 경계에서
Panic을 만나면 stdout에 `\npanic : {msg}`(`name == "panic"`) 또는 `System.X: msg`를
출력하고 exit 1한다. `Panic::named(name, msg)`가 display를 결정한다.

## D-024 | `.NET` stack trace와 `TargetInvocationException` 래퍼는 재현하지 않는다 | Accepted

reference는 네이티브 호출의 `TargetInvocationException`을 벗겨 inner 예외만 출력하고,
inner 예외 뒤에는 `.NET` stack trace(`at Un.Object... in C:\dev\un\src\...cs:line N`)가
따른다. stack trace는 빌드 산출물 경로에 의존하는 문자열이라 재현 대상이 아니다.
Rust는 inner 예외를 `Payload::Panic { name, message }`로 저장하고 `{name}: {message}`만
출력한다. 재현한 inner 문자열: dict duplicate key
`System.ArgumentException: An item with the same key has already been added. Key: {clr}`
(bool key는 CLR의 `True`), .NET dict miss
`System.Collections.Generic.KeyNotFoundException: The given key '{name}' was not present
in the dictionary.`, slice index
`System.IndexOutOfRangeException: Index was outside the bounds of the array.`,
`System.DivideByZeroException: Attempted to divide by zero.`,
`System.OverflowException: Arithmetic operation resulted in an overflow.`.
`System.Text.Json.JsonException`은 name만 재현하고 message의 byte offset text는 재현하지
않는다(기록된 divergence). `counter.count()`/`repeat.count()`처럼 무한 sequence의
`Enumerable.Count()`도 같은 `System.OverflowException` inner 문자열을 낸다.

## D-025 | str-key collection의 iteration order는 parity target이 아니다 | Accepted

reference `Dict`/`Set`은 `Dictionary<Obj,Obj>`/`HashSet<Obj>`이고 `Stru`/`Dict`/`Set`/
`Spreads`/`TObj`는 `GetHashCode()`를 override하지 않아 `Type.GetHashCode()`(process
randomised)로 bucket된다. .NET `string.GetHashCode()`도 randomised라 str-key 순서는
구현 간 재현 불가능하다. Rust는 insertion-ordered `Vec` + FNV-1a(UTF-16 units)로
deterministic 순서를 쓴다. int-key dictionary는 `Val<long>.GetHashCode() == value`라서
reference도 insertion 순서 → **int-key 순서는 parity target**이다.

## D-026 | `Clone()`/`Copy()`의 override 집합과 base quirks를 그대로 재현한다 | Accepted

primitive는 `Clone()`을 override한다(`Int/Float/Bool/Date/Str/Enu`는 **값을 보존**하고
`Stru/Dict/Json/List/Tup/Set/LFn/NFn/Future/Property/FieldRef/Counter/Iters/Range/Repeat/
Reverse/Lock`는 각자의 clone 규칙). base `Obj.Clone()`는 dunder `__clone__` → `valid_super`
→ `new Obj(Type)`이고 payload를 옮기지 않으므로 override 없는 타입의 clone은 **값이 없는
빈 cell**이다. `List.Clone()`/`Tup.Clone()`는 element의 `Clone()`를 부르므로 빈 cell들이
전파된다. `List.Copy()`/`Set.Copy()`/`Dict.Copy()`/`Json.Copy()`는 `this`이고, `Tup.Copy()`는
element의 `Copy()`를 부르므로 list element는 같은 cell로 남는다. reference의 이 동작을
"fix"하지 말고 그대로 재현한다.

Un 코드에서 `.clone()`/`.copy()`는 **멤버 테이블** lookup이라 `list`/`str`/`tuple`/`dict`
등에는 멤버가 없어 member-lookup error(`error: 'list' object has no attribute 'clone'`)이고,
`set(1, 2).clone()`만 성공한다. `1.copy()`는 파싱 단계에서 `error: invalid number format`다.
(이 항목의 초기 판명은 "`Int.Clone()`는 base `new Obj(Type)` → `[1].clone()`가 `[int]`"였다가
probe `probes10/{p25,p28,p33,p38}.un`와 source bytes로 반증됐다.)

## D-027 | `Tup` 생성자의 name erase와 `Slice` 음수 step clamping을 그대로 재현한다 | Accepted

`Tup(Obj[] values)`는 `Name[i] = string.Empty`로 이름을 **지운다**. `Tup(Obj[] values,
string[] names)`는 `Name = names`를 유지하고 `Members[name] = value`를 등록한다. 따라서
`Dict.Iter/ToList/ToTuple/Spread`가 만드는 `Tup([key, value], ["key", "value"])`는
`p.key`/`p.value`가 resolve되고(`Dict` materialize는 display가 `[(a, 1)]`/`((a, 1))`),
`Tup(Obj[])`는 이름이 없다. `Tup.Slice`의 음수 step은 `for i=s; i>e; i+=st`이고 `e`는
≥0으로 clamp되므로 **tuple은 역화되지 않는다**. `s`가 `len`으로 clamp되면 reference는
배열 밖을 index해 `System.IndexOutOfRangeException`가 발생한다(`t[2:0:-1]`). 같은
clamping을 `List.Slice`에도 적용한다.

## D-028 | iterator는 immutable 정의 트리 + mutable cursor cell로 모델링한다 | Accepted

reference는 `Iters : Ref<IEnumerable<Obj>>`이고 `Value`가 re-enumerable이라
`foreach`/`Count()`/`ToList()`/`ToStr()`가 매번 새 enumerator를 만든다. C#의 lazy LINQ
generator를 Rust로 옮기면 borrow 규칙과 충돌하고 재현성이 보장되지 않는다.
Rust는 정의 트리 `StageDef`(`Source`/`Reverse`/`Range`/`Counter`/`Repeat`/`Take`/`Skip`)를
`Rc`로 불변 저장하고, enumeration 위치는 `Stage` cell의 `Cursor`(`Pos`/`Val`/`Rem`/`Round`)에
저장한다. payload는 `Payload::Iter(IterCell)`이고 `Iter()`/`Copy()`는 같은 cell,
`Clone()`은 fresh cursors로 재구성한다.
- `Kind`(`Iters`/`Range`/`Counter`/`Repeat`/`Reverse`)가 C# subclass별로 갈리는
  `ToStr`/`Len`/`Init`/`Clone`/`In`/`Eq` 규칙을 dispatch한다.
- `Counter.Clone()`는 `new(current)`이고 `current`는 enumerator가 진행하지 않는 생성자
  필드 → clone은 원래 시작값에서 시작한다(cursor 아님). `Range.Clone()`도
  `new(start, stop, step)`.
- `Source::Live(Rc<RefCell<Vec<Obj>>>)`는 `List.Iter()`의 live cell(append가 다음 round에
  보임), `Source::Fixed`는 snapshot(`Reverse`의 materialized reverse).
- `Take`/`Skip`는 receiver의 정의를 래핑하므로 receiver의 위치가 보존되고 wrapper는
  re-enumerable이다. `Map`/`Filter` stage는 `Fn` payload가 필요해서 의도적으로 미착수다.
- unbounded chain(`Counter`, `Skip` over unbounded, `Repeat` count<0)을 끝까지
  enumeration하면 Rust는 `panic : iteration is infinite`로 멈추고 reference는 멈추지
  않는다 → 문서화된 divergence(#8). `Counter.Len()`/`Repeat.Len()`은 formula라 정합.
- `Range` step 0은 `MoveNext`에서 lazy `panic : step cannot be zero`(display/clone은
  동작), `Len()`은 `System.DivideByZeroException` inner 문자열. `iter.range` native는
  construction 시 `Err("step argument must not be zero")`를 낸다(다른 메시지).

## D-029 | member dispatch는 멤버 테이블 lookup이고 virtual dispatch가 아니다 | Accepted

Un의 `x.m(args)`는 C#의 `override`가 아니라 `[Native(Name=...)]`로 등록된 **멤버 테이블**
lookup이다. 멤버에 없는 이름은 `error: '{Type}' object has no attribute '{name}'`(position
block 포함, exit 1)로 abort한다. C#의 virtual method를 멤버로 착각하면 안 된다.
실측된 멤버 집합은 `PROGRESS.md` Phase 7b에 verbatim으로 기록돼 있고, 요약하면
`list` 19개(`add`..`resize`), `dict` 8개(`add`..`values`), `set` 8개(`clone` 포함),
`Iters` 12개(`take`..`last`), builtin 15개(`len`..`ascii`)다. `next` builtin은 없고
`list`/`str`/`tuple`/`dict`/`set`/`range`/`counter`에 `iter`/`next`/`copy`/`clone`가 없다.
`skip`은 키워드라 `.skip(...)`는 parse error다. `iter`는 callable builtin type constructor
이고 `use iter`가 모듈로 shadow한다. `to_list`/`to_tuple` 멤버는 static materialize
(`new List([.. self.Value])`)라 virtual `ToList()` override와 결과가 다르다.

## D-030 | `Err`의 capital header와 `Error`의 position block 규약을 유지한다 | Accepted

`Err(message, header = "Error")`의 display는 `"{header}: {message}"` → `Error: msg`.
`new Error(...)`의 기본 header는 lowercase `error`이고 출력은 position block
`<file>, line [n], column [c]` + source line(indent 제거) + caret
(`max(1, Length)` 개 `^`, leading whitespace보정) + `{header}: {message}`다.
`Evaluator.Unwrap(obj, node)`는 `new Error(err.Message, node, source)` → lowercase
`error`(`unsupported operand type(s) for in: 'counter'`). `EvalArguments`의 default
branch는 `new Error(err.Message, arg, source, header: err.Header)` → `Err`가 만든 capital
`Error`가 남는다(`Error: cannot unpack Counter`, `*` 노드 position block). exit code 1은
exception이 throw된 경우에만 설정된다. Rust는 `Payload::Error { message, header, start,
length }`와 Panic/throw 전파로 이 규약을 재현하고, `run`의 error 출력은 position block
`+ header: message` + frame trace(`trace:`)를 재현한다.

## D-031 | 네이티브 호출의 `map`/`filter`는 item을 단일 arg로 넘긴다 | Accepted

`iter.map`/`iter.filter`/`iter.count`/`iter.any`/`iter.all`은 각 item을
`predicate.Call(new Tup([item]))`로 **하나의 positional arg**로 넘긴다. 2-arg lambda에
`enumerate`의 named pair를 넘기면 lambda body의 위치에서 inner
`error: unsupported operand type(s) for +: 'tuple' and 'int'`가 나고 reference는
`TargetInvocationException`으로 감싼다. Rust는 wrapper를 벗기고 inner 문자열만
`Payload::Panic`로 전파한다(D-024). predicate가 bool이 아닌 값을 반환하면
`error: expected 'predicate' to return a boolean`이고, `iter.max`/`iter.min`의 비교가
bool이 아니면 `error: {a} < {b} is not a boolean`이다.
