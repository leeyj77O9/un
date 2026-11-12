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

## D-032 | 함수 계열은 `Payload::Func` cell + `Bind`의 reference 메시지를 그대로 재현한다 | Accepted

- `Fn(Context closure)`는 `Obj(UnType.Func)`이고 `Name` / `Args` / `ReturnType` /
  `Property`를 owns. Rust는 `Payload::Func(FnData cell)`로 표현하고 closure `Context`,
  `Args` list, `ReturnType`, `Property` link, body를 cell에 넣는다. `LFn`은 AST body,
  `NFn`은 host closure. `Clone()`은 `Args`를 **같은 list object로 assign**하므로
  `Args` cell과 closure cell이 공유되고, 따라가서 **recursion limit이 공유**된다.

## D-033 | display newline은 CRLF, `io.write`는 LF, parse error는 stdout | Accepted

reference의 `Error.ToString()`/`Panic.ToString()`는 CRLF로 저장된 `.cs` 파일의 raw string
literal이라 **CRLF separator**이고, `Console.WriteLine`은 `Environment.NewLine`(CRLF)를
추가한다. display 문자열 끝에는 newline이 없다. `io.write`는 literal `"\n"` 문자열을
stream에 써서 **LF**로 끝난다. 두 newline은 바꿀 수 없고, `error::nl()`(`cfg!(windows)`)가
표시 라인 전용이다. parse error는 `Run()` 안에서 catch돼 `Console.WriteLine(e)`로 **stdout**에
나간다(stderr가 아니다). exit code는 exception이 throw될 때만 1, usage error는 2.

## D-034 | uncaught host exception은 `Payload::Panic` 값이고 .NET stack trace는 재현하지 못한다 | Accepted

reference는 `Int.Mod` by 0(`System.DivideByZeroException: Attempted to divide by zero.`),
`List.Slice`의 `this[i]` bounds(`System.IndexOutOfRangeException: Index was outside the
bounds of the array.`)처럼 host exception을 **throw**한다. VM은 unwinding이 없으므로 같은
객체가 stack에 올라오면 run loop가 매 exec 직후 top-of-stack을 검사해 abort하고
`{Name}: {Message}` + exit 1을 stdout에 prints. reference는 같은 출력 뒤 `.NET` stack
trace(`   at Un.Object.Primitive.Int.Mod(Obj other) in C:\dev\un\src\...\Int.cs:line 87`)를
追加한다. Rust에 대응하는 frame 정보가 없으므로 stack trace는 **documented divergence**다
(실측: `p11.un`, `p28.un`의 앞 줄과 exit code는 정합, trace 문자열만 diff).

## D-035 | `List.Slice`의 step != 1은 capacity-0 list를 만들고 bounds check는 남는다 | Accepted

`List.Slice`는 `st == 1`일 때만 `Array.Copy`하고, 그 외에는 `newList = new List()`
(용량 0)에 `this[i]`를 `Add`한다. `List.Add`는 용량이 차면 `new Err("...")`를 **반환**하고
루프가 그 값을 버린다. 그래서 실측 결과는 `[]`다(`[1,2,3,4][3:0:-2]` → `[]`).
`this[i]`의 index check는 `IndexOutOfRangeException`를 throw하므로, `l[4:0:-1]`은 abort한다.
Rust는 같은 순서로 `list_slice`를 구현하고 check는 Panic payload로 낸다.

## D-036 | slice assignment는 step 1만 지원한다 | Accepted

`BindPattern`의 Slice branch는 step이 1이 아니면
`slice assignment with step != 1 not supported`를 낸다. `SET_SLICE`는 bounds + target +
value를 pop하고 value를 pattern result로 stack에 **다시 push**한다(net `-(k+1)`).

## D-037 | primitive 생성자는 `Init` dispatch table을 그대로 재현한다 | Accepted

`int()`/`float()`/`bool()`/`str()`/`date()`는 `TObj.Call` → `Global.GetClass(this).Clone().Init(args)`
이고, `Init`는 arity별 규칙을 가진다: `int` 0→0, 1→`ToInt()`, `(str,int)`→base conversion
(base 2..36, `0b/0x/0o` prefix, 실패 시 `int: Input string was not in a correct format.`),
그 외 `int: expected (str, int) for base conversion`, 그 외 타입은
`cannot convert to 'int'`. `float` 0→0.0, 1→`ToFloat()`, 그 외
`cannot convert to 'float'`. `bool` 0→false, 1→`ToBool()`, extra arg는 무시.
`str` 0→"", 1→`ToStr()`, `(str,int)`→`repeat(min(i, int.MaxValue))`(negative → empty),
그 외 `too many arguments`. `date` 0→`DateTime.Now`(**time dependent, 재현 불가**),
1 Str→`invalid date str`, 그 외 `cannot convert '{type}' to 'date'`.

## D-038 | optional chain guard는 children[0] 평가 직후에 있고 index/slice는 chain이 아니다 | Accepted

`EvalCall`/`EvalSlice`는 `children[0]`을 평가한 뒤 `none`이면 **나머지를 평가하지 않고**
`none`을 반환한다. 그래서 guard는 `code(children[0])` 바로 직후, argument list / bounds
expr 앞에 놓인다. chain 판정은 reference 규칙 그대로: `Property`는
`Operator == QuestionDot || IsOptionalChain(children[0])`, `Index`/`Slice`/`Call`은
`IsOptionalChain(children[0])`만 본다. 따라서 `m?[0]`은 chain이 아니고 `none`에
`unsupported operand type(s) for []: 'none'`가 난다. `JMP_IF(FLAG_JUMP_NONE)`은
`FLAG_JUMP_TRUE`과 짝일 때만 jump를 계산하므로 guard는 항상 두 flag를 함께 쓴다.

## D-039 | expression encoding은 reference의 `Unwrap` 지점을 기준으로 한다 | Accepted

`code(node)`는 node의 값을 stack에 남긴다. `UNWRAP`는 `Evaluator.cs`가
`Unwrap(..., node)`를 호출하는 지점(Unary, Binary(arith), Property, Index, Slice, `CALL`
handler 안)에서만 발생한다. Identifier/literal/FString/Tuple/List/Dict/Assign/Ternary/
and/or/`??`/`Use`는 `UNWRAP`를 발생하지 않는다. statement 규칙은 모든 block child(마지막
제외)에 `code(child); UNWRAP(child span); POP`, 마지막 child는 값이 block result로 남는다.
control transfer(`UNSUPPORTED`/`RETURN` 등)로 끝나는 child는 `UNWRAP`/`POP`를 생략한다 —
`raises` flag. `io.write`의 callee property는 `EvalProperty`가 자기 node에서 unwrap하므로
`UNWRAP` 개수는 statement 수 + chain 표현식 수다.

## D-040 | `?`는 PUSH_TRY_PROP + PROPAGATE + exc entry이고 handler는 try entry를 먼저 pop한다 | Accepted

`code_unary(TokenType::Question)`은 `PUSH_TRY_PROP`, `code(child)`, `PROPAGATE`를 발생하고
`ExcEntry{kind: Propagate, start: push_ip, end: ip, handler: PROPAGATE ip, var_slot: None}`
를 append한 뒤 `push_ip`의 operand에 index를 patch한다. `PROPAGATE`는 자기 try entry를
**먼저** pop한다(`tries.last().handler == ip`). handler ip가 region 안에 있으므로
`handle()`이 region을 다시 매칭하면(`start <= ip < end`) 무한 루프가 난다(실측 TIMEOUT).
`frames > 0`이면 `Propagate`, top level이면 `pending`(catch된 `LangError`)을 재throw해
span/header를 원본 그대로 유지한다(`throw;`와 같다).

## D-041 | short-circuit 인코딩은 reference의 truth test 위치와 같다 | Accepted

- `and` = `code(L); DUP; TO_BOOL(STRICT); JMP_IF(TRUE|POP)→T; JMP→E; T: POP; code(R); E:`
- `or` = 같은 형태에서 `JMP_IF(POP)` (`FLAG_JUMP_TRUE` 없음 → truthy left가 결과)
- `??` = `code(L); JMP_IF(NONE|TRUE)→R; JMP→E; R: POP; code(R); E:`
- ternary = `code(cond); TO_BOOL(no STRICT); JMP_IF(TRUE|POP)→A; code(else); JMP→B; A:
  code(then); B:` — 여분 POP 없음. `EvalTernary`는 cond를 unwrap하지 않으므로 `Err` cond는
  falsy로 **else branch**로 간다(단, cond 자체가 `Err`면 그 node에서 abort).
- `while`은 `JMP_IF`에 `FLAG_STRICT`를 쓴다.

`Unwrap` on non-`Bool`, non-`Err`는 falsy다. `none.ToBool()`는 `Err`라 `none and 3`은
`unsupported operand type(s) for bool(): 'none'`로 abort한다(실측 정합).

## D-042 | ARG accumulator는 값을 stack에 남기고 CALL은 runtime count를 쓴다 | Accepted

`ARG_POS`/`ARG_KW`/`ARG_SPREAD`/`ARG_BEGIN`의 stack effect는 0(값을 push하지 않고
accumulator에 이름만 기록). `ARG_KW_SPREAD`는 `-1`. `CALL`은 operand가 static count,
runtime count는 accumulator frame length(실측: spread가 count를 늘린다).
`FLAG_ARG_CHECK`가 붙은 `ARG_POS`(call site)는 peek한 값이 `Err`면
`Error(msg, arg node)`를 header 보존으로 throw한다. container literal의 `ARG_POS`는 flag를
쓰지 않는다. `ARG_KW`/`Pair`는 검사하지 않는다. tuple literal의 이름은 names table에서
**contiguous**해야 한다(`push_names_contiguous`).

## D-043 | name/constant operand는 proto-local slot이다 | Accepted

`name_index`/`const_index`/`push_name_raw`는 module table index가 아니라 **proto-local slot**
을 반환하고, `Program::name(proto, slot)` / `Program::constant(proto, slot)`가
`proto.names[slot]` / `proto.consts[slot]`로 shared table을 resolve한다. `check_indices`는
`Op::Use` operand를 `proto.names` 경계로 검사한다. `BUILD_TUPLE_NAMED`는 names-table index를
flags에 넣고, `CALL`은 `call_names` index를 flags에, arg count를 operand에 넣는다.

## D-044 | `TO_STR`는 FSTRING opcode의 type check와 같은 span을 쓴다 | Accepted

`EvalFString`는 expression part마다 `Eval(child).ToStr()`를 호출하고 `Str`를 요구한다:
`f-string expression must be a string`는 **FString node** 위치에서 난다. 그래서 compiler는
expression part 직후에 `TO_STR`(span = FString node)를 발생하고, `FSTRING`(operand = part
수)은 plain stack 값을 소비한다(accumulator frame을 pop하지 않는다). literal part는
`Str` constant. `list` part는 display text가 된다(`` `a{[1, 2]}c` `` → `a[1, 2]c`).

## D-045 | `use`의 native/file dispatch는 VM run time에 결정된다 | Accepted

compiler는 file module path를 항상 compile하고 `Op::Use`가 run time에
`world::is_native(last)`로 dispatch한다(native → `global::include`, 그 외 →
`global::import_file(parts, ctx.source.path, alias, imports)`).
`import_file`은 `resolve_module`(source directory 기준 `<p>.un` 파일 || `p` 디렉터리) 후
`Path.Combine(PATH, path)`를 try하고, 없으면
`module '{Path::new(world::path()).join(joined)}' not found` Panic를 낸다(reference는
`Global.Load`의 `Path.Combine(PATH, path)` 문자열). module이 **존재하면** Phase 11까지
`file module '{full}' is not loaded (phase 11)` Panic가 documented divergence다.
`builtin`은 `[BuiltinModule]`라 native registry에 없고 `use builtin`은 file-not-found로 간다.
- `Bind`는 `self`를 local로, `Property`가 있으면 `field`(= `new FieldRef(Self, Property)`)을
  local로 먼저 넣고, 파라미터 중 `self`/`super` 이름은 arg 바인딩에서 **건너뛴다**.
  `TryBindSimplePositional`은 args에 keyword name이 없고 `Spreads`가 없으며 파라미터에
  `*`/`**`가 없을 때만 적용된다.
- general binding 메시지는 verbatim 유지: `missing required argument '{arg.Name}'`,
  `too many positional arguments`, `multiple values for argument '{name}'`,
  `unexpected keyword argument '{keyword.Keys.First()}'`.
  **required parameter 검사가 keyword leftover 검사보다 먼저** 실행된다(파라미터 순회
  우선). `Arg.IsEssential = !optional && !positional && !keyword`.
- `UnpackArgs`는 `Spreads` item을 flatten하고 각 unpacked 값이 **원래 arg의 name을
  상속**한다. `*args`는 남은 positional을 `Tup`로, `**kwargs`는 남은 keyword를
  `Dict`(`Str` key)로 받는다.
- recursion limit: `Global.MAXRECURSIONDEPTH = 1000`이고 depth는 **closure의
  `CallDepth`**다. `LFn.Call`은 `CallDepth >= limit`를 먼저 검사하고 increment하고,
  `NFn.Call`은 `CallDepth++ >= limit`를 검사하고 실패 경로에서도 decrement한다.
  둘 다 같은 1000에서 `Err("maximum recursion depth exceeded")`를 준다.
- `finally` drain: defers는 LIFO로 evaluator에 넘기고, usings는 LIFO로 `Exit()`를
  호출한다. drain 중
  flow가 올라오면 reference의 `finally`를 **탈출**해서 그 값이 call 결과가 된다
  (`defer`의 `return`이 body return을 덮치는 quirk 유지).
- `Property`는 `BackingKey = "$" + Name`이고 `Read`는 backing member를
  `Default.Clone()`로 lazy materialize한다. getter/setter는 `Self`/`Super`/`Property`가
  바인딩된 채로 호출되고 `field` local로 write-through한다. `Property.Clone()/Copy()`와
  `FieldRef.Clone()/Copy()`는 `this` → 모든 instance가 같은 property object를 본다.
  `Evaluator.Unwrap`는 `value is FieldRef`면 `field.Value`로 unwrap한다(VM phase 작업).
- display는 `fn({arg types}) -> {return type}`, `Repr()`는
  `fn {Name}({arg name}: {arg type}) -> {return type}`. `LFn`/`NFn`은 `Call`을
  override해서 `__call__` dunder를 **참조하지 않는다** → Rust는 payload dispatch로 같은
  순서를 만든다. callable이 아닌 object는 `unsupported operand type(s) for (): '{Type}'`.
- `Future` payload는 Phase 16 scheduler에 추가한다(`Future.cs`는 `Task<Obj>` wrapper의
  `Run()`/`Wait()`).
- `world`에는 VM이 없어도 `LFn`을 테스트하는 executor hook
  (`set_executor(Option<Executor>)`)가 있고 `reset()`이 다른 global 상태와 함께 clears한다.
  `Context`의 frames는 shared `Rc<RefCell<Vec<Frame>>>` cell → nested call이 전체 frame
  chain을 본다(reference `new Context(scope, Closure.Source, Closure.Frames)`).
- `Payload::Func/Property/FieldRef` 추가에 따라 `clone_obj`의 primitive arm을 넣었다:
  `Int.Clone()`/`Bool.Clone()`는 `From(Value)`(cached cell 유지),
  `Str/Float/Date Clone()`는 pool을 우회한 fresh object → D-026의 교정을 code에 적용했다.

## D-033 | `chr`의 surrogate range는 inner .NET 문자열만 재현한다 | Accepted

`chr(55296)`은 reference에서 `System.Reflection.TargetInvocationException`으로 감싸진
`System.ArgumentOutOfRangeException`다. inner message는
`A valid UTF32 value is between 0x000000 and 0x10ffff, inclusive, and should not include
surrogate codepoint values (0x00d800 ~ 0x00dfff). (Parameter 'utf32')`이고 wrapper와
stack trace는 재현하지 않는다(D-024). Rust는 `Payload::Panic { name, message }`로 같은
inner 문자열만 저장하고 CLI는 `System.ArgumentOutOfRangeException: {message}`로 출력한다.
`chr`의 range check(`0..1114111`)는 `Err("chr: code point out of range (0..1114111)")`로
먼저 잡히고, surrogate는 그 뒤의 host throw다.

## D-034 | 네이티브 레이어는 공유 closure + world stdout buffer를 쓴다 | Accepted

- 모든 native는 `Context(Global.GetGlobalScope(), null, null)` 위에 만들어진다. Rust는
  `native::init`가 만든 **단일** `Context(global_scope, empty Source, new_frames())`를
  모든 native가 공유 → `CallDepth`가 native 호출 전체의 공유 상태다.
- `Arg.DefaultValue`는 native 인자에 설정되지 않는다. reference는 C# slot이 `null`이라
  `sep ??= " "` 같은 `??=`가 발동하고 `GetArgs`는 `Obj defaultValue = Null`을 쓴다.
  Rust는 unbound optional을 `obj::none()`으로 bind하고 host가 `is_none`으로 `??=`를
  흉내 낸다. native의 `[ArgInfo]` flag는 Essential/Optional/Positional뿐이고
  `IsKeyword`는 항상 false다.
- `*values` positional collector는 `Tup`로 bind되고 `io.write`는 `sep` `" "` /
  `end` `"\n"` / `ToStr()` → `Repr()` fallback / flush per write를 따른다.
  `stream` payload가 Phase 14이므로 explicit `stream`/`input`/`output` 인자는
  reference의 type-check 메시지로 reject한다.
- stdout는 world-owned buffered `String`이고 `world::flush_stdout()`가 reference의
  flush 시점(`cw.Write(...)` 후 `cw.Flush()`)에서 real stdout에 넘긴다 → run의 byte
  순서가 유지되고 sandbox는 buffer를 캡처한다. `exit`는 `process::exit` 전에 flush한다.
- `Global.Init`는 `io`를 bind하지 않는다. 모든 probe/test 스크립트는 `use io`로 시작한다.
- `Str`은 `Repr()`를 override하지 않으므로 `repr("a")`는 base form의 type name `str`이다
  (`type(1)` → `<type: int>`, `len([1,2,3])` → `3`, `chr(65)` → `A`와 실측 비교).

## D-046 | 함수 본문은 `EvalBlock`을 거치지 않는다 | Accepted

- reference `LFn.Call`은 `foreach (Node node in body) evaluator.Eval(node)`로 본문을 돈다.
  module 최상위 블록은 `EvalBlock`을 지나서 각 자식의 `Err`을 unwrap하지만, 함수 본문은
  `EvalBlock`을 지나지 않으므로 **문장 값이 unwrap 되지 않는다**. 컴파일러는 함수 본문을
  `unwrap = false`로 codegen한다(`code_block(node, pop_last, unwrap)`).
- reference `LFn.Call`은 본문 문장 값을 모두 버리고 `None`을 돌려준다. 컴파일러는 함수 본문
  마지막에 `LOAD_NONE` + `RETURN`을 emit해서 같은 결과를 만든다.
- `->`(return)만 예외다. `ReturnFlow`는 `LFn.Call`이 잡아서 값으로 돌려주므로, 컴파일된 형태는
  `RETURN` opcode 하나로 프로토를 끝내고 `Outcome::Return(value)`를 돌려준다.

## D-047 | 컴파일된 함수 호출은 중첩 VM에서 proto를 실행한다 | Accepted

- `Body::Proto(proto_index)`를 가진 `Fn`은 VM이 중첩 VM(`VM::new_proto`)으로 proto를 실행한다.
  world에 `proto_executor`/`current_program` hook을 등록하고, `function::begin_call`/`end_call`이
  reference `LFn.Call`의 prologue/epilogue(재귀 한도 검사, 함수 스코프 생성, 인자 바인딩,
  defer/using drain, call depth 복원)를 담당한다.
- 중첩 VM을 쓰면 함수 안에서 raise된 언어 오류의 **위치**가 그대로 유지된다(`Outcome::Error`가
  그대로 caller로 올라간다). `fn_call` 경로(멤버 호출 등)에서는 위치를 잃으므로 `Err` 값으로
  떨어뜨린다.
- callee context는 closure context의 frame list를 공유한다(reference `new Context(scope,
  Closure.Source, Closure.Frames)`와 동일). 그래서 `?` 연산자의 `Frames.Count > 0` 판정이
  reference와 같이 동작한다.
- `LFn.Call`이 `PropagateFlow`를 잡아서 `Err` 값으로 돌려주는 규칙도 그대로 적용한다. 그래서
  함수 안의 `?`는 caller의 call site에서 unwrap 된다.

## D-048 | 파라미터 기본값은 정의 시점에 평가한다 | Accepted

- `Fn.GetArgs`는 함수 **생성 시점**에 기본값 표현식을 평가한다(closure context). 그래서
  `x = 1; fn f(a = x); x = 2; f()`는 `a == 1`이다.
- 컴파일러는 기본값 표현식을 **바깥 프로토**에 그대로 codegen한다(정의 시점에 평가되므로
  enclosing scope를 봐야 한다). `MAKE_FUNC`은 flags high byte에 기본값 개수를 담아서 스택에서
  pop하고, 만든 함수의 파라미터 기본값으로 넣는다.
- `Param.default` 필드는 module constant index를 담는 자리로 남겨 두었지만, 지금은 항상
  `None`이다(기본값을 상수 풀에 넣으면 정의 시점 평가 규칙과 어긋난다).

## D-049 | 함수 안의 `break`/`skip`은 caller의 반복문까지 unwind 하지 않는다 | Accepted

- reference는 `BreakFlow`/`SkipFlow`가 C# 예외로 호출자 프레임까지 unwind 하므로, 반복문 안에서
  호출한 함수가 `break`를 실행하면 그 반복문이 끝난다(`probes10/k1.un`, `k2.un` 확인).
- 컴파일러는 함수 본문을 별도 proto로 컴파일할 때 loop group을 비우기 때문에, 함수 안의
  `break`/`skip`은 `'break' outside loop` / `'skip' outside loop` 오류가 된다.
- 이건 아직 지원하지 않는 divergence다. 해결하려면 반복문 경계를 runtime에 노출하는
  opcode(loop context push/pop)와 호출 프레임을 넘어서는 jump가 필요하다.

## D-050 | 프로그램은 큰 스택의 worker thread에서 실행한다 | Accepted

- VM은 Un 호출 1개당 Rust frame 1개를 쌓는다. reference는 recursion limit 1000까지 nested call을
  허용하는데, Rust frame은 C# frame보다 크다(Windows 주 스택은 1MB).
- `unvm`은 64MB 스택의 worker thread에서 실행한다(가상 메모리라 예약 비용은 없다). 테스트
  harness의 스택은 2MB라서, 깊은 재귀 테스트는 `assert_abort_deep`가 따로 스택을 확보한다.

## D-051 | `os.tickCount`는 epoch 밀리초를 쓴다 | Accepted

- reference의 `Environment.TickCount64`는 머신의 uptime(부팅 이후 밀리초)이다. Rust
  runtime에는 이 값을 이식성 있게 읽는 방법이 없다(Windows API 호출이 필요하다).
- 그래서 epoch 밀리초를 반환한다. 값은 단조 증가하고 양수라는 점은 같지만, 값 자체는
  reference와 다르다.

## D-052 | `fs.hidden`은 점 접두사로 판정한다 | Accepted

- reference는 Windows의 `FileAttributes.Hidden` 속성을 읽는다. 이 속성은 이식성이
  없고 Rust runtime에는 대응 API가 없다.
- 그래서 파일 이름이 `.`로 시작하면 hidden으로 판정한다. Windows의 dotfile 규칙과
  비슷하지만 reference와 항상 같지는 않다.

## D-053 | 정규식은 자체 backtracking 엔진으로 구현한다 | Superseded by D-064

- reference는 `System.Text.RegularExpressions`를 쓴다. Rust runtime에는 regex crate가
  없고(외부 의존성을 추가하지 않는다), .NET 문법 전체를 재현하는 것도 범위가 아니다.
- 최초 구현(`native/regex.rs`)은 backtracking 매처였고 언어가 쓰는 문법만 지원했다
  (리터럴, `.`, 문자 클래스, 그룹, 대안, 수량자, `^`/`$`).
- **D-064**에서 이 엔진은 .NET 알고리즘의 충실한 이식으로 교체됐다
  (`rust/src/native/regex/`). named group, lookaround, balancing group, `\p{...}`,
  EXPRCOND/BACKCOND, `$` expansion 등 .NET 기능이 그대로 동작한다.

## D-054 | `go`/`wait`과 pool은 호출 스레드에서 실행한다 | Accepted

- reference는 pool의 worker를 실제 스레드로 만들고 `go`는 `Task.Run`으로 표현식을 별도
  스레드에서 평가한다.
- Rust runtime의 객체 모델(`Rc<RefCell<..>>`)은 `Send`가 아니라 스레드로 넘길 수 없다.
  그래서 D-012대로 단일 스레드 cooperative scheduler로 구현한다: `go expr`은 표현식을
  바로 계산해서 future에 넣고, `wait`는 그 값을 꺼낸다. pool의 `map`/`submit`도 호출
  스레드에서 순서대로 실행한다.
- 결과 값은 reference와 같지만 인터리빙(순서)은 다르다.
- `go`의 즉시는 D-063으로 대체했다: `go`는 work를 deferred future에 담고, work는 `go`
  시점의 scope chain 복사본에서 `wait` 시점에 실행된다. pool의 단일 스레드 규칙은 그대로다.

## D-055 | `Str`는 `>`를 override 하지 않는다 | Accepted

- reference의 `Str`는 `Eq`, `NEq`, `Lt`만 override하고 `>`는 base class의 규칙
  (`!lt && !eq`)을 따른다. 그래서 문자열 비교는 culture가 아니라 순서 비교가 된다.
- Rust의 `str_binop`도 `>`를 base class로 보내므로 같은 결과가 나온다.

## D-056 | `Stru.Clone`은 멤버 표를 복사한다 | Accepted

- reference의 `Stru.Clone()`은 `Members = Members.New()`로 멤버 표를 복사한다. Rust
  구현이 멤버를 복사하지 않아서 클래스의 메서드가 인스턴스에서 사라졌다.
- `stru_copy`가 멤버 표를 복사하도록 고쳤다.

## D-057 | 네트워크는 plain TCP만 지원한다(TLS는 범위 밖) | Accepted

- reference는 `System.Net.Http.HttpClient`와 `System.Net.WebSockets.ClientWebSocket`를
  쓴다. `https`와 `wss`는 host의 TLS handshake(SSL 스택)를 탄다.
- Rust runtime은 std 전용·무의존 구현이고(외부 crate를 추가하지 않는다) TLS 스택이 없다.
  그래서 `https`/`wss`는 연결이 실패하고, reference의 연결 실패 문구
  `Unable to connect to the remote server`를 그대로 답한다.
- `http://`와 `ws://`만 parity target이다. 연결/핸드셰이크 실패는 전부 같은 문구로
  답한다(refused, DNS 실패, timeout, TLS 요청 포함).

## D-058 | `http_client`/`web_socket` 멤버 표는 등록만 한다 | Accepted

- reference의 `[NativeType]` 이름은 `http_client`와 `web_socket`이고 `Global.Init`는 그
  표를 `originalClasses`에만 등록한다. `originalClasses`는 attribute lookup이 참고하는
  표이지 `Global.GetAttr`가 직접 보는 표가 아니다.
- 객체의 타입 이름은 `http`와 `socket`이므로 멤버 접근은 `'http' object has no attribute
  'get'` / `'socket' object has no attribute 'send'`로 실패하고 `inspect.attr`는 `[]`다.
  `http_client()`와 `web_socket()`는 `undefined variable`이다(`[NativeType]`은 전역
  생성자를 만들지 않는다).
- 그래서 16개 http 멤버와 4개 ws 멤버는 reference와 같은 상태로 등록하되 Un 코드에서
  도달할 수 없다.

## D-059 | `random`의 값은 parity target이 아니다 | Accepted

- reference는 `System.Random`를 쓰고, 그 값 순서는 .NET 버전의 내부 알고리즘에 의존한다.
  값 순서는 언어의 의미와 무관하다.
- 그래서 xorshift 생성기를 쓴다. target은 재현성이다: 같은 `seed`는 같은 값을 낸다.
  공유 생성기(모듈 함수)와 객체별 생성기(`random()` 객체)의 구별, `seed`가 생성기를
  교체하는 성질, `Clone()`이 생성기 cell을 공유하는 성질은 재현한다.

## D-060 | http/ws의 request/frame I/O는 parity 대상이 아니다 | Accepted

- D-058 때문에 멤버가 도달할 수 없어서 HTTP request와 WebSocket frame의 입출력은 Un
  코드에서 관측할 수 없다. `test/**.un`의 어떤 파일도 `http`/`ws`를 `use`하지 않고
  `PlaygroundRunner.DefaultAllowed`에도 없다.
- 그래서 검증은 probe suite로만 한다(`p10\ws1..ws6.un` + `p10\http1..http7.un`,
  **13 PASS / 0 DIFF**). probe는 URI 검증표, 연결 실패 문구, 객체의 타입/display/
  비교/`bool`, `inspect.attr`를 reference와 byte-identical로 비교하고, frame 송수신은
  Node `ws` package의 echo server(port 19085)에 대한 실제 handshake와 echo로 확인한다.

## D-061 | `[Native(Async = true)]`는 binder가 무시하고, `f43637f`의 dead-code 정리는 Rust에 영향이 없다 | Accepted

- reference의 binder는 `Async`를 읽지 않는다(실제 비동기 호출 경로가 없고, `f43637f`는
  `Native.Async` 필드를 제거했다). 그래서 Rust의 `_async` 멤버는 blocking 멤버와 같은
  동작이다.
- `f43637f`가 지운 것(`UnType`의 `TGeneric`/`UGeneric`/`Infinity`/`Skip`/`Break`,
  `ArgInfo.Keyword`, `Native.Async`, `Source.FullName`, `IFileSystem.Enumerate*`,
  `IO.ResetStdout`/`ResetStdin`, `Scope`의 `SetLocalOrDeclare`/`ContainsKeyInTop`/
  `Snapshot`/`Restore`)은 Rust에 없다. reference의 대입 규칙은 `SetLocal` 하나로
  합쳐졌고(`localOnly || IsFunctionScope`), Rust는 `set_local`이 그 규칙을 쓴다.
- `Global.AllowProcessExit`와 `Fs.GetBlockedPath`는 playground 전용이다. Rust runtime은
  playground API가 없고 `world::set_allowed`가 모듈 허용을 담당한다.

## D-062 | `atomic`의 값은 한 cell에 담는다 | Accepted

- reference는 `Interlocked` 연산(`Interlocked.Read`/`Exchange`/`Add`/
  `CompareExchange`)을 쓴다. Rust runtime는 객체 모델을 스레드로 넘기지 못하므로(D-012)
  counter와 flag는 `Rc<RefCell<i64>>`/`Rc<RefCell<bool>>` 한 cell에 값을 담고, 연산은 같은
  값을 만든다. `atomic.int(5).add(3).get()`는 `8`이고, `exchange`는 이전 값을 돌려주며
  `compare_and_swap`은 일치할 때만 swap한다.
- `Clone()`은 counter/flag/pool/lock에서 오류(`'atomic_int' cannot be cloned` 등)이고
  `Copy()`는 네 개 모두 `this`다. `Entry()`는 `this`이고 `Exit()`는 override가 없어서 base
  문구(`unsupported operand type(s) for using exit: 'atomic_int'`)를 답한다.
- 인자 이름은 reference의 `[ArgInfo]` 이름(`initial`/`amount`/`expected`/`newValue`)을
  따른다. 오류 문구는 reference가 hard-code한 `'value'`/`'new_value'`를 그대로 쓴다.

## D-063 | `go`는 `go` 시점의 scope chain 복사본을 가진 deferred future다 | Accepted

- reference의 `EvalGo`는 `Task.Run(() => { var goScope = new Scope(context.Scope); ... })`
  이다. task body가 `new Scope(context.Scope)`를 **task가 시작할 때** 열므로, task는 `go`
  시점의 scope chain을 본다. `Future.Wait()`는 `Task.Result`를 블록하고 `AggregateException`
  을 flatten해서 inner 예외를 rethrow한다.
- Rust는 단일 스레드(D-012)에서 work를 `wait` 시점에 실행한다. `go` 시점의 live chain을
  공유하면 loop block이 사라진 뒤 값이 resolve되지 않으므로, `MAKE_FUTURE`가 **chain의
  복사본**을 future에 담는다(각 level의 symbol table과 slot 값을 복사하고 value cell은
  공유한다). `AWAIT`는 복사본의 child scope에서 proto를 실행하고 값을 memoize한다.
- 실측: `fn sq(n) -> go (n * n)` 후 `wait f`는 `25`, `for i in [1,2,3]` 안에서
  `futs.add(go (i * 2))`의 future는 각 iteration 값을 유지해서 `out =  [2, 4, 6]`이 된다.
  `go n * n`은 `(go n) * n`이고 future에 `*` override가 없어
  `unsupported operand type(s) for *: 'future' and 'int'`로 abort한다(`go`는 unary이며
  binary보다 강하게 bind한다).
- `test/negative/go_loop_var.un`은 맞출 수 없다: reference는 racy해서 6회 실행 중 4회가
  exit `-1`, 2회가 exit `1`이고, manifest가 기대하는 `undefined variable 'i'`는 task가 loop
  scope를 버린 뒤에 시작하는 순서에서만 나온다. Rust는 `sorted =  [1, 4, 9]`로 exit 0이다.

## D-064 | .NET compatible regex engine; char-based text instead of UTF-16 | Accepted

- reference�� `System.Text.RegularExpressions`�� backtracking ������ ����. �� ������
  .NET �˰�����(`RegexParser` �� `RegexWriter` �� `RegexInterpreter`)�� ����� �̽��̴�:
  ���� �뷮 �迭(track/stack/crawl)�� peek-after-pop �Ծ�, `Branchcount` loop arm�� mark��
  track�� �����ϰ� straight path�� `[mark, count, -codepos]`�� �����ϴ� ��Ģ,
  `Backtrack()`�� negative marker = `BacktrackingSecond`(flag�� OR�� �ƴ϶� **��ü**),
  `Matches()` enumeration(�� ��ġ�� �� ���� bump, len���� ����), `Regex.Split`�� ĸó
  splice, `ScanDollar`(`$` expansion: ``, `..`�� ��ü �ڸ��� �Һ� + ����
  ������ literal emission), `Escape`(`Regex.Escape`�� ���� metachar set)���� �����ߴ�.
  node kind/opcode ��ȣ�� .NET�� �����ϴ�.
- ���� �� �߰ߡ������� ���� ����(���� byte-identical �����):
  `scan_group_open`�� Rust `break`�� loop ��ü�� Ż���� option groups�� expression
  conditional(`(?(x)b)`)�� �׻� `Unrecognized grouping construct.`�� �Ǵ� ���� ����,
  ī�װ��� group result�� �ʱⰪ(`false` ��� `true`�� ������ ��� ���ڰ� `\p{..}`��
  ��Ī), `NOT_SPACE_CONST`�� double negation(`\S`�� ���鿡 ��Ī), Ŭ���� ���� shorthand
  (`[\D]`/`[\S]`/`[\W]`/`[\p{...}]`/`\-`)�� `ch`(=`\`)�� ���� �׻� ��������
  ������ ����, `[\b]`�� `_pos` ���� rewinding, `Branchcount` track frame�� mark�� ����
  ��, `backtrack()`�� `back |= BACKTRACKING_SECOND`(384)�� � arm���� ���� �ʾ� capture��
  �������� ���ϰ� `goto(31)` OOB panic�� ����� ����, `is_true_quantifier`�� ���ڶ�
  OOB read(`a{2`���� panic).
- ����: probes 25�� **25 PASS / 0 DIFF**(IO.write�� >0x7F�� `?`�� �ڸ��� �ͱ��� ����),
  differential suite 37 PASS / 0 DIFF, `cargo test --release` 199 passed, cargo build 0/0.
- engine�� BMP+astral Unicode category table�� .NET numbering���� ������ ����.
- **UTF-16 ��� Rust `char` ���**�̴�: .NET�� pattern�� input�� UTF-16 code unit����
  �����Ƿ� astral ���ڰ� surrogate 2���� ��������, Rust ������ char �ϳ��� ����. �׷���
  BMP literal/class�� ���������� astral ���ڿ� ���� �Ϻ� pattern�� ���ڰ� 2��ó��
  ��Ī�Ǵ� .NET�� �ٸ��� ������ �� �ִ�(astral literal�� category table�θ� ��Ī ����).
  ���� parity harness(probe/differential)���� astral pattern�� ���� ������.
- case-insensitivity�� .NET�� culture-specific case equivalence table�� �ƴ϶� Rust��
  Unicode lowercase/uppercase/titlecase�� �ܼ�ȭ�ߴ�(Turkish I, Cyrillic culture Ư��
  ���̽��� �ٸ�). POSIX `[:name:]`�� .NETó�� literal ���� ������ �⿩���� �ʴ´�(probe��
  Ȯ��): `[a[:alpha:]]`�� literal `a`, `[`, `:`, `a`, `l`, `p`...�� �ش�.
- `Escape`�� metachar set�� `\t \n \f \r ' ' # $ ( ) * + . ? [ \ ^ { |`�̸� ���� ���ڴ�
  `\t`/`\n`/`\r`/`\f`�� espace�Ѵ�(`Regex.Escape` ground truth).
