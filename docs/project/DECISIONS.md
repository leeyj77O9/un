# DECISIONS.md — 결정 로그

형식: `D-NNN | 제목 | 상태 | 결정 | 영향`
- 상태: **Accepted** = 채택되어 코드에 반영, **Proposed** = 채택되었으나 코드 반영 전, **Pending** = 검증 필요
- 이 문서는 append-only 결정 기록이다. 개별 항목의 검증 숫자(1415/1493/1525/205/213 등)는
  **결정 시점 스냅샷**이며 현재 기준값이 아니다. 현재 기준은
  `docs/project/REFACTOR_SUMMARY.md`의 "Current baseline"을 참조한다.
  상태가 바뀐 결정은 원문을 지우지 않고 `Superseded by D-NNN` 각주만 덧붙인다.

## D-001 | C# 코드는 수정하지 않는다 | Accepted

Reference Implementation(`src/`, `test/`, `.csproj`)은 읽기 전용 기준선(oracle)이다.
Rust 코드와 문서는 `rust/`, `docs/`에만 추가한다. 커밋/푸시는 사용자 승인 후.

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
`a[1:2:3]`는 영향 없다. `a[..2]`는 reference에서 `expected an expression; unexpected token DotDot here`로 실패한다.

## D-007 | 문자열/타입 이름/off-by-one은 reference 문자열을 verbatim 재현한다 | Accepted

모든 오류 메시지(`'{T}' object has no attribute '{n}'`, `undefined variable '{n}'`,
`no match case`, `tuple unpack mismatch`, …)와 네이티브 오타(`invaild`, `invalid argumnets: index`)는
그대로 재현한다. 테스트가 이 문자열에 의존한다.

## D-008 | `.NET` 예외 이름을 재현해야 하는 케이스 | Superseded by D-092

`test/negative/mod_zero.un`은 exit 1 + `System.DivideByZeroException`을 기대하고,
`replace_missing.un`은 `System.NullReferenceException`, `replace_empty.un`은
`System.ArgumentException`을 기대한다. Rust 네이티브/연산 구현은 이 문자열을
**동일한 위치에서** 낸다(header는 `System.X` 형태로 ToString 형태를 유지).

> 2026-11-20: D-092에서 `.NET`식 `System.*` 명칭 전체를 제거하고("전부 `error:`" 결정,
> 이름은 untagged Un 오류로 통일) supersede됐다. 예외 문자열 계약은 D-092의 기준으로 대체.

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

## D-012 | `go`/`wait`는 single-threaded cooperative scheduler | Accepted (see D-054/D-063)

C#는 `go`에서 **같은 `Context`**를 새 `Evaluator`에 넣어 다른 스레드에서 실행한다.
`Scope`/`Obj`에 락이 없어 경쟁 조건이 가능하고, `test/negative/go_loop_var.un`(exit 1,
`undefined variable 'i'`; 파일은 2026-11-16에 삭제되어 현재는 `test/unit/runtime/async.un`의
go-in-a-loop assertions로 검증한다)를 루프 변수 가시성 semantics로 고정했다.
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

`docs/spec/BYTECODE.md` §13. 재구축 없이 field append 가능. magic `UNB1`. load 시 validation 실패는
침묵하지 않는다.

## D-015 | 초기 단계에 JIT 없음 | Accepted

`Proto` bytecode는 interpreter loop로 실행한다. JIT은 performance layer로만 붙이고
동작 기준은 VM semantics에 고정한다.

## D-016 | C# 코드 컨벤션 준수는 Rust에 적용하지 않는다 | Accepted

Rust는 Rust 관례(`rustfmt`, snake_case, module-per-concern)를 따른다. 단 **필드명은
reference에 맞춤**(`start`, `length`, `typ`)해서 error/AST 매핑이 추적 가능하게 한다.

## D-017 | Rust CLI는 reference CLI와 `run` 출력만 정합한다 | Accepted

`un tokens`/`un ast`/`un desugar`/`un opt`/`un dis`는 Rust 전용 진단 명령이고
`un run <file>`의 stdout/exit/stderr가 정합 대상이다. reference에 `ast` dump가 없으므로
Phase 2 정합은 parse 성공/오류 문자열 일치 + 17개 probe로 확인했다(치환 전 분석 문서 기준).

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

## D-027 | `Tup` 생성자의 name erase와 `Slice` 음수 step clamping을 그대로 재현한다 | Partially superseded by D-067

> D-067에서 `Slice` 음수 step 규칙을 표준 semantics로 바꿨다. 아래 설명은
> 변경 전 동작의 기록이다. `Tup` 생성자의 name erase 규칙은 그대로다.

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
실측된 멤버 집합은 치환 전 진행 기록(Phase 7b, 삭제됨)에 verbatim으로 기록돼 있었고, 요약하면
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

## D-035 | `List.Slice`의 step != 1은 capacity-0 list를 만들고 bounds check는 남는다 | Superseded by D-067

> D-067에서 step != 1 슬라이스를 표준 semantics로 바꿨다. 아래 설명은 변경 전
> 동작의 기록이다.

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

## D-058 | `http_client`/`web_socket` 멤버 표는 등록만 한다 | Superseded by D-066

- reference의 `[NativeType]` 이름은 `http_client`와 `web_socket`이고 `Global.Init`는 그
  표를 `originalClasses`에만 등록한다. `originalClasses`는 attribute lookup이 참고하는
  표이지 `Global.GetAttr`가 직접 보는 표가 아니다.
- 객체의 타입 이름은 `http`와 `socket`이므로 멤버 접근은 `'http' object has no attribute
  'get'` / `'socket' object has no attribute 'send'`로 실패하고 `inspect.attr`는 `[]`다.
- 그래서 16개 http 멤버와 4개 ws 멤버는 reference와 같은 상태로 등록하되 Un 코드에서
  도달할 수 없었다(**단, 2026-11 `0ab82df`에서 reference가 수정되어 D-066으로 대체됨**).

## D-059 | `random`의 값은 parity target이 아니다 | Accepted

- reference는 `System.Random`를 쓰고, 그 값 순서는 .NET 버전의 내부 알고리즘에 의존한다.
  값 순서는 언어의 의미와 무관하다.
- 그래서 xorshift 생성기를 쓴다. target은 재현성이다: 같은 `seed`는 같은 값을 낸다.
  공유 생성기(모듈 함수)와 객체별 생성기(`random()` 객체)의 구별, `seed`가 생성기를
  교체하는 성질, `Clone()`이 생성기 cell을 공유하는 성질은 재현한다.

## D-060 | http/ws의 request/frame I/O는 parity 대상이 아니다 | Partially superseded by D-066

- ~~D-058 때문에 멤버가 도달할 수 없어서 HTTP request와 WebSocket frame의 입출력은 Un
  코드에서 관측할 수 없다~~ → **`0ab82df` 이후 멤버가 도달 가능해졌다(D-066).**
  `test/unit/native/http.un`의 검증 경로(인자 타입·연결 거부 포트)와 `ws.connect`의
  검증 경로가 Rust 포트에서도 byte-identical로 동작한다(http.un 16/16, differential
  40/40 positive, exit 0).
- 다만 frame/request의 실제 round trip은 probe suite로만 검증한다(`p10\ws1..ws6.un` +
  `http1..http7.un`, **13 PASS / 0 DIFF**). frame 송수신은 Node `ws` package의
  echo server(port 19085)에 대한 실제 handshake와 echo로 확인한다.

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

- reference는 `System.Text.RegularExpressions`의 backtracking 엔진을 쓴다. 이 엔진은
  .NET 알고리즘(`RegexParser` → `RegexWriter` → `RegexInterpreter`)의 충실한 이식이다:
  고정 용량 배열(track/stack/crawl)과 peek-after-pop 규약, `Branchcount` loop arm이 mark를
  track에 저장하고 straight path는 `[mark, count, -codepos]`로 저장하는 규칙,
  `Backtrack()`의 negative marker = `BacktrackingSecond`(flag를 OR이 아니라 **교체**),
  `Matches()` enumeration(빈 매치는 한 글자 bump, len에서 멈춤), `Regex.Split`의 캡처
  splice, `ScanDollar`(`$` expansion: ``, `..`는 전체 자릿수 소비 + 슬롯
  없으면 literal emission), `Escape`(`Regex.Escape`와 같은 metachar set)까지 재현했다.
  node kind/opcode 번호는 .NET과 동일하다.
- 구현 중 발견·수정한 엔진 결함(전부 byte-identical 재검증):
  `scan_group_open`의 Rust `break`가 loop 전체를 탈출해 option groups와 expression
  conditional(`(?(x)b)`)이 항상 `Unrecognized grouping construct.`가 되던 구조 오류,
  카테고리 group result의 초기값(`false` 대신 `true`로 시작해 모든 문자가 `\p{..}`에
  매칭), `NOT_SPACE_CONST`의 double negation(`\S`가 공백에 매칭), 클래스 내부 shorthand
  (`[\D]`/`[\S]`/`[\W]`/`[\p{...}]`/`\-`)가 `ch`(=`\`)를 비교해 항상 부정절로
  빠지던 오류, `[\b]`의 `_pos` 이중 rewinding, `Branchcount` track frame에 mark가 빠진
  것, `backtrack()`의 `back |= BACKTRACKING_SECOND`(384)가 어떤 arm에도 맞지 않아 capture를
  복원하지 못하고 `goto(31)` OOB panic을 만들던 문제, `is_true_quantifier`의 끝자락
  OOB read(`a{2`에서 panic).
- 검증: probes 25종 **25 PASS / 0 DIFF**(IO.write가 >0x7F를 `?`로 자르는 것까지 포함),
  differential suite 37 PASS / 0 DIFF, `cargo test --release` 205 passed, cargo build 0/0.
- engine은 BMP+astral Unicode category table을 .NET numbering으로 병합해 쓴다.
- **UTF-16 대신 Rust `char` 기반**이다: .NET은 pattern과 input을 UTF-16 code unit으로
  읽으므로 astral 문자가 surrogate 2개로 보이지만, Rust 엔진은 char 하나로 본다. 그래서
  BMP literal/class는 동일하지만 astral 문자에 대한 일부 pattern은 문자가 2개처럼
  매칭되는 .NET과 다르게 동작할 수 있다(astral literal은 category table로만 매칭 가능).
  현재 parity harness(probe/differential)에는 astral pattern이 없어 미측정.
- case-insensitivity는 .NET의 culture-specific case equivalence table이 아니라 Rust의
  Unicode lowercase/uppercase/titlecase로 단순화했다(Turkish I, Cyrillic culture 특수
  케이스는 다름). POSIX `[:name:]`는 .NET처럼 literal 문자 집합을 기여하지 않는다(probe로
  확인): `[a[:alpha:]]`는 literal `a`, `[`, `:`, `a`, `l`, `p`...를 준다.
- `Escape`의 metachar set은 `\t \n \f \r ' ' # $ ( ) * + . ? [ \ ^ { |`이며 제어 문자는
  `\t`/`\n`/`\r`/`\f`로 espace한다(`Regex.Escape` ground truth).
## D-065 | KeyMap은 bucket-chained hash table이다(order 유지) | Accepted

- 기존 `KeyMap`은 `Vec<(hash, key, value)>`를 매번 linear scan했다(bucket이 없었다). dict5000 벤치마크가 reference 대비 ~10x 느렸다.
- `entries: Vec<Entry>`(insertion-ordered store, `Entry { hash, key, value, used }`) + power-of-two `buckets`(head index) + `next`(chain link)로 바꿨다. 제거는 tombstone으로 표시하고 free list를 `next`에 threading하여 재사용한다 → live entry의 순서가 바뀌지 않고, `remove`도 순서를 보존한다.
- duplicate key는 find에서 가장 이른 index를 반환(첫 삽입이 get에 응답하는 기존 동작 유지). bucket chain은 head-prepend이고 find는 chain을 끝까지 본다(chain 길이는 평균 1).
- `Set`도 같은 `KeyMap`으로 backed 했다(value = `none`). member 함수들은 `try_insert`/`contains_key`/`get_key`/`remove`를 쓴다.
- 검증: cargo test 199/199, differential 37 PASS / 0 DIFF, negcmp 38/4(기존 차이), wscmp 13/0, fullcmp 37/0. dict5000 17.9ms → ~9.3ms(ref ~6.1ms, loop-only는 양쪽 parity). reference보다 느린 나머지는 interpreter dispatch overhead 수준이다.

## D-066 | http/ws 인스턴스 타입 이름을 registration key와 맞춘다 | Accepted

- reference가 `0ab82df`(2026-11, leeyj77O9)로 `http`→`http_client`, `socket`→`web_socket`으로
  인스턴스 `UnType`을 바꿔 멤버 표가 도달 가능해졌다. Rust 포트도 동일하게
  `obj_new("http_client")` / `obj_new("web_socket")`으로 인스턴스를 만들고, payload
  `type_name` 매핑을 `http_client` / `web_socket`으로 바꿨다.
- 이제 `client.get(1)` → `error: expected url is string` 같은 인자 검증 경로와
  `client.set_header` / `clear_headers`가 Un 코드에서 동작하며 `test/unit/native/http.un`이
  differential byte-identical이다(16/16 assertions).
- error 메시지와 `type()` display의 타입 이름이 `http_client` / `web_socket`으로 바뀌었다
  (reference와 동일).
- 검증: `scripts/differential.ps1` **108 checked / 0 unexpected DIFF / 4 known-divergent**,
  `cargo test --release` 205 passed, `dotnet build -c Release` 오류 0 / 경고 0.

## D-067 | 슬라이스/집합 비교/멤버십/`is`는 표준 semantics를 따른다 | Accepted

- 레퍼런스(C#) 구현의 quirks는 "문서에 있는 동작"과 어긋났고, C# 소스와
  differential 하네스는 이미 삭제되어 검증 수단이 없다. 따라서 **표준
  semantics를 ground truth**로 삼아 구현을 고쳤다. `test/regression/defects.un`의
  6개 항목이 이 변경의 대상이었다.
- **슬라이스**: `list`/`tuple`/`str`이 같은 규칙을 쓴다. 음수 bound는 뒤에서부터
  세고, 생략된 bound는 step 부호에 따라 정해진다(양수 step이면 처음→끝, 음수
  step이면 끝→처음), end는 exclusive, step은 0이 될 수 없다.
  `l[2:0:-1]` → `[3, 2]`, `l[::-1]` → 역순, `l[10:20]` → `[]`, `l[-2:]` → 뒤 두 개.
  이전 구현(D-027/D-035)은 step != 1에서 빈 list/무역화 tuple을 만들었고,
  bound를 `i as i32 as i64`로 잘라 32비트 범위를 넘으면 잘못된 위치가 됐다.
- **`upper_bound`**: "value보다 큰 첫 원소"의 인덱스, 즉 삽입 위치. 마지막 원소와
  같으면 `len`이 될 수 있다(`[1,2,4,5].upper_bound(5) == 4`). 이전 구현은
  `len - 1`에서 시작해 `len`을 반환하지 못했다.
- **`set`/`dict`의 `==`**: 원소/항목 단위 비교. `dict`는 키와 값이 모두 같아야
  하고 `set`은 순서와 무관하다. 이전 구현은 `unsupported operand type(s) for ==`
  에러를 냈다.
- **`in`**: list/tuple은 원소 단위로 검사하며 스칼라를 받는다. 이전 구현은
  list/tuple 인스턴스만 받아 스칼라에 대해 에러를 냈다.
- **`is`**: 우변이 타입 객체면 "좌변이 그 타입의 인스턴스인가"로 판정한다.
  이전 `is_obj`는 좌우를 뒤집어 `type(none) is none`이 false였다. `type(none)`
  의 표시는 `<type: none>` 그대로다.
- 검증: `scripts/test.ps1` Positive **40 files / 1383 passed / 0 failed / 0
  crashed**, Negative 38 matched / 4 documented divergences, Known 0, exit 0.
  `cargo test --release` 202 passed. `test/regression/defects.un`은 `known`
  그룹에서 `regression` 그룹(Positive)으로 옮겨 회귀를 계속 검증한다. (이후
  네이티브·빌드/문서 감사 후 현재 기준은 **Positive 40 files / 1391 passed,
  Negative 39 matched / 0 documented divergences**다 — D-068 참고.)

## D-068 | 네이티브 레이어의 침묵 데이터 손상을 수정한다(표준 기준) | Accepted

- 2026-11-16 네이티브/빌드 감사에서 reference 동작과 무관하게 **침묵하는
  데이터 손상 경로**를 확인하고 고쳤다. 동작 기준은 표준/문서(D-067와 같은
  원칙)다.
- **`lower_bound`**: `upper_bound`(D-067)와 같은 구조로 수정 — 삽입 위치를
  반환하고, 빈 리스트는 0, 끝을 넘으면 `len`. 이전 구현은 `len - 1`에서
  시작해 `len`을 반환할 수 없었고 빈 리스트에 −1을 반환했다.
- **eager `extend_insert`**: 이터러블 insert가 원소 순서를 역순으로 삽입하던
  것을 순서대로 삽입하도록 고치고, 잘못된 `index` 인자는 무음이 아니라
  `list_insert`와 같은 에러를 반환한다.
- **`-0.0` float 해시**: `key_eq`는 float를 `==`로 비교하므로 `-0.0 == 0.0`
  이고, 따라서 `hash_code`도 두 값을 같게 해시해야 한다. 이전 구현은
  `to_bits()`라 `set([0.0]).contains(-0.0) == false`였다.
- **lazy `.map()`/`.filter()`의 에러 삼킴**: 람다 에러를 map은 리스트 값으로
  넣고 filter는 원소를 조용히 버렸다. 이제 `Step::Panic`으로 전파되어 반복이
  abort된다.
- **`fs.*` 시간 문자열**: `fs.modified()`/`created()`/`accessed()`가 날짜를
  ~86배 미래로 인쇄하던 계산 오류를 `runtime::date::parts` 기반으로 수정했다.
- **`fs.rmdir` 기본값**: `recursive` 기본값이 `true`여서 `fs.rmdir(dir)`이
  `rm -rf`였다. 기본값은 `false`이고, 재귀 삭제는 `recursive = true`를
  명시해야 한다.
- **`fs.move`/`fs.rename`**: destination이 있으면 source를 먼저 copy한 뒤에만
  source를 지운다(destination 파일의 데이터 손실 창을 없앴다).
- **`fs.walk_dir` symlink 순환**: canonical path 추적 + 깊이 제한(64).
- **WS 프레임 마커를 RFC 6455로**: 100/101 → 126/127, 1 MiB 페이로드 상한,
  `ping`(opcode 9)은 `pong`(opcode 10)으로 응답한다.
- **무-경계 할당 상한**: `iter.array` 총 원소 ≤ 10,000,000과 음수 차원 거부,
  `random.range` ≤ 1,000,000, `str.center/left/right` 패딩 ≤ 10,000.
- **기본 RNG를 entropy로 시드**: 모듈 레벨 `random.*`가 항상 seed 0이었다.
  재현성은 `random.seed(…)`로 유지된다. `below()`의 modulo bias를 rejection
  sampling으로 제거하고 `between()`의 i64 오버플로우를 i128로 해결했다.
- **`time.sleep`**: `as i32` truncation을 제거하고 상한 clamp, 음수는 0으로.
  계약(int=ms/float=seconds)은 그대로다.
- **`date.add_years`**: 윤년 2월 29일 + 1년은 3월 1일이 아니라 2월 28일로
  clamp한다(`add_months`와 같은 규칙).
- 검증: `scripts/test.ps1` Positive **40 files / 1391 passed / 0 failed / 0
  crashed**, Negative 39 matched / 0 documented divergences, Known 0, exit 0.
  (이후 negative 셋 수리로 현재 기준도 같다 — see D-067의 주석과
  docs/project/REFACTOR_LOG.md.)

## D-069 | `chr`의 surrogate range는 inner .NET 문자열만 재현한다 | Accepted

> 이름 중복으로 2026-11-16에 D-033(두 번째 사용분)에서 재번호된 항목. 본문은 그대로다.

`chr(55296)`은 reference에서 `System.Reflection.TargetInvocationException`으로 감싸진
`System.ArgumentOutOfRangeException`다. inner message는
`A valid UTF32 value is between 0x000000 and 0x10ffff, inclusive, and should not include
surrogate codepoint values (0x00d800 ~ 0x00dfff). (Parameter 'utf32')`이고 wrapper와
stack trace는 재현하지 않는다(D-024). Rust는 `Payload::Panic { name, message }`로 같은
inner 문자열만 저장하고 CLI는 `System.ArgumentOutOfRangeException: {message}`로 출력한다.
`chr`의 range check(`0..1114111`)는 `Err("chr: code point out of range (0..1114111)")`로
먼저 잡히고, surrogate는 그 뒤의 host throw다.

## D-071 | 클래스 메서드 JIT: receiver는 hidden pointer, field 읽기/쓰기는 name-checked intrinsic | Accepted

- 2026-11-16 Phase 8의 출발점. 클래스 메서드 프로토는 이제 `precheck_fn`을
  통과하고 native로 컴파일된다. 이전에는 `precheck_fn`이 모든 클래스 메서드를
  거부해 class1000은 인터프리터로만 실행됐다.
- **receiver 표현**: 클래스 메서드의 첫 hidden ABI 인자는 `Rc<ObjData>`의 raw
  포인터(`Rc::as_ptr`)이며, 컴파일러의 `slot_alloc("self")`로 local slot 0이
  `self`다. entry 블록에서 `incoming[1]`(receiver)을 `vars[arity]`(local slot 0)에
  고정한다.
- **local slot 0에 로드하는 레지스터**: 레지스터 번호는 컴파일러가 임의로
  배정하므로 `ins.b == 0` 추측은 폐기했다. `LoadLocal(0)`이 쓰는 것과 똑같이
  `self_regs` 비트맵에 기록되고, 다른 레지스터 instruction이 그 번호에 쓰면
  소거한다(보수적; bail → interpreter로, 어느 쪽도 정답을 잃지 않는다).
- **field intrinsic**: `self.<Int필드>`의 read/write는
  `un_instance_int_ok_name(self, field_index, name_ptr, name_len)`가
  `[index == layout ix ∧ bytes(name) == 필드이름 ∧ payload == Int]`를 call
  시점에 비교해 검증하고 mismatch면 bail(tag=2)한다. layout(컴파일 타임 상수)과
  실측 member table이 어긋나면 interpreter가 실행한다 — 문서화된 layout 순서와
  무관하게 name check가 보장한다. Float/Bool/None 필드, `none`을 반환하는
  생성자, 리스트를 만드는 메서드는 여전히 인터프리터로 fallback한다.
- **Float compare 지원**: `Lt/Le/Gt/Ge/Eq/Ne`는 Int와 Float 피연산자를 모두
  지원한다(Int는 `fcvt_from_sint`로 Float로 승격 후 `fcmp`). NaN 의미론은
  Rust와 동일하다. 반환은 Bool(tag 3)이다.
- **JIT 모듈 수명**: emit된 JITModule은 `jit_cache::JIT_MODULES`에 별도 보관된다.
  일반 실행에서는 이전처럼 leak(프로세스 수명과 동일)하지만, `clear()`가
  native code를 실제로 해제하므로 caller는 그 뒤에 어떤 JIT 함수도 진입해선
  안 된다(벤치/테스트 teardown 전용). 이전에는 존재만 하고 native code는
  항상 leak됐다.
- 검증: `scripts/test.ps1` Positive **40 files / 1395 passed** (classes.un +4),
  `tests/unit/runtime/classes.un`의 새 어설션 4개는 `inc`, `__len__`,
  `__getitem__`, `__setitem__` 클래스의 native 경로를 통과한다.
  `scripts/differential.ps1` **79 files / 0 differences**, `cargo test` 202 passed,
  clean build 0 warnings.
- **BuildDict 팩토리 형태 지원**: `d = {1: 10, 2: 20}`의 inline literal도
  IntDict로 native로 구축된다(기존에는 빈 `{ }` 형식만 지원). 각 entry를
  `un_dict_int_empty` → `un_dict_int_set(id, key, value)` 시퀀스로 translates.
  Float/Bool/None entry는 bail → 인터프리터로 fallback.
- **lazy compile**: `plan_for`가 프로그램 로드 시 무조건 native 코드를
  생성하던 경로를 제거하고, 첫 `try_call_jit` 호출 시에만 컴파일한다.
  ineligible 프로토의 STUB 등록도 같은 지연을 따른다. 로드-time 비용은
  eligible 프로토가 있는 프로그램에서만 지불된다(bench_class처럼 Int-heavy
  프로그램에서만 측정 가능한 차이).

## D-070 | 네이티브 레이어는 공유 closure + world stdout buffer를 쓴다 | Accepted

> 이름 중복으로 2026-11-16에 D-034(두 번째 사용분)에서 재번호된 항목. 본문은 그대로다.

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

## D-072 | `Program.source_path`는 stem이 아니라 전체 경로를 저장한다 | Accepted

- `.unbc` 왕복(compile → write → read → run)을 검증하는 과정에서 발견한
  정합성 결함이다. compiler는 `Program.source_path`에 `source.name()`(파일 stem,
  예 `rel_test`)만 저장했고, `run_bytecode`는 임베디드 source 텍스트를
  `Source::new(source_path, …)`로 복원한다. 때문에 `.unbc` 재실행에서는
  `use` 모듈 해석의 "임포터 디렉토리 기준" 규칙이 stem의 빈 parent에 적용되어
  cwd 상대 경로로 어긋났다:
  - `test/fixtures/rel_test/main.un` 은 direct run에서 통과(exit 0)하지만
    `.unbc`에서는 `use utils`가 cwd(=repo root) 기준으로 해석되어
    `panic : module 'C:\dev\un\utils' not found`로 죽는다.
  - `circular_main.un`의 "circular import detected" 진단이 direct(절대 경로)와
    `.unbc`(cwd 상대 경로)에서 서로 다른 문자열을 출력한다.
- **수정**: compiler가 `source.path.clone()`(전달받은 전체 경로를 그대로)을
  저장한다. compile-timed cwd와 re-run cwd가 같으면 direct run과 바이트 단위로
  동일하게 해석되고, cwd가 달라도 왜 그런지가 "임베디드 path"로 설명 가능하다.
  에러 디스플레이는 `Source::name()`(stem)을 쓰므로 메시지 모양은 그대로다.
  `.unbc` 포맷과 opcode ABI는 바뀌지 않았다.
- **검증 하니스**: `scripts/unbc-roundtrip.ps1`(신규). Positive manifest 전체 +
  negative manifest 중 컴파일 가능한 파일을 `un run` vs
  `un compile` 후 `un run <file>.unbc`로 실행해 stdout/stderr/종료 코드를
  바이트 비교한다. 파싱 단계에서 거부되는 negative 파일(6개)은 바이트코드를
  만들 수 없으므로 skip으로 집계한다. 수정 후 Positive **40/40 identical**,
  negative compilable **33/33 identical**(수정 전 0/40→2건 불일치: 위 두 건이
  잡혔다). CI(`.github/workflows/build.yml`)에도 같은 step을 추가했다.
  `test.ps1`/`cargo test` 기준은 그대로 green(아래 docs/project/REFACTOR_LOG.md 참고).
- **교훈**: 음성(negative) 픽스처 셋은 "모듈 해석의 명세"였는데, 지금까지는
  source 경로만 통해서만 실행됐다. `.unbc` 경로도 동일 명세로 검증해야
  컴파일 아티팩트가 1급 실행 경로로 유지된다.


## D-073 | JIT의 조용한 사망과 레이아웃 진입 블록 시그니처 파라미터 | Accepted

- Pass 15(`aad0f42`)에서 도입한 "prologue를 별도 블록으로 분리하고 body block으로 점프" 구조는
  Cranelift layout의 **entry block(position 0)이 시그니처 파라미터를 갖지 않는** IR을 만들었다.
  `append_block_params_for_function_params`는 첫 블록에만 파라미터를 붙여야 하는데, 시그니처
  파라미터는 `entry`(첫 번째로 만든 블록)에 붙이고 직후에는 `prologue`를 먼저 switch해서
  layout position 0은 파라미터 없는 `prologue`가 됐다. `module.define_function`의 verifier
  (`typecheck_entry_block_params`: "entry block parameters (0) must match function signature (2)")가
  모든 proto를 거부했다.
- 거부는 STUB 치환을 통해 **인터프리터 fallback**으로 조용히 삼켜졌다. `UV_JIT=0` vs 기본 JIT
  differential은 완전히 clean, `scripts/test.ps1`(1406/41), `unbc-roundtrip` 40/40, `cargo test`
  205는 모두 green이었다. 즉 Pass 16/17의 모든 JIT 수치는 사실상 인터프리터 수치였다(예:
  `int_ops` 3,226 ms). 컴파일이 실패할 때 proto별 OK/STUB 로그는 `UV_JIT_DEBUG=1`로만 보이며
  어느 게이트도 이를 감시하지 않는다.
- **수정**: prologue 블록에 시그니처 파라미터를 붙여 그 자체를 layout entry로 만들고, 본체는
  파라미터 없는 `entry`로 점프. `has_user_call`(brif→cont/bail) 경로도 동일한 순서로 유지된다.
- **부가 변경**: ① `jit_bc.rs`의 `Band`/`Bor`/`Bxor`(&, |, ^)을 Int-only i64로 지원(IntDict/bool/none/float 피연산자는 인터프리터로 bail), 정수 `i & 0x5a5a` 패턴이 native로 실행되도록 했다. ② `define_function` 실패를 `UV_JIT_DEBUG` 로그로 남기도록 했다.
- **측정**: `int_ops(1_000_000)` — UV_JIT=0 ≈ 31 s, 수정 전(=전부 fallback) ≈ 3.2 s, 수정 후
  **≈ 0.08 s**(약 400배). 벤치 전체 geomean `Un JIT / Python`이 0.224x → **1.99x**로 갱신.
- **교훈**: fallback은 의미론을 보존하지만, "**fallback이 전면적으로 켜져 있는지도 알 수 없다**"는
  문제가 있었다. `UV_JIT_DEBUG=1`로 proto별 OK/STUB를 보고, 벤치의 VM/JIT 비교로 두 값이 일치하면
  JIT이 사망했다는 신호다. 남은 관측성 부채는 TD-51.
- **검증**: differential 81 files 0 differences, test.ps1 1406/41, cargo test 205(release+debug),
  unbc-roundtrip 40/40 + 34/34, `scripts/bench.ps1 -Group all -Report` exit 0.


## D-074 | `split` 기본 구분자 `' '`, `map`은 타입=변환기·인자 수에 따른 호출 | Accepted

- **문제**: `io.read().split(' ').map(int)` 관용구가 Un에서 동작하지 않았다. 세 겹의 원인이
  있었다: ① `split`에 구분자가 **필수 인자**(`arg("sep", true, ...)`)라 `split()` 문법이 불가,
  ② `list.map`/`iter.map`/global `iter.map`/`lazy .map`이 콜러블을 `function::is_func`로
  확인하고 `function::fn_call`로 호출했는데, `int`는 TObj라 `fn_cell`이 없어
  `unsupported operand type(s) for (): 'type'`, ③ `list.map`은 인자 수와 무관하게
  `(item, index)` 2개를 넘겨 1-arg lambda가 `too many positional arguments`를 냈다.
- **결정 (사용자 요청)**:
  1. `str.split()`의 `sep`는 생략 가능하며 기본값은 단일 공백 `' '`이다.
  2. `map` 계열 네이티브(`list.map`, `iter_map`(lazy method), `iter.map`(global),
     `iters::StageDef::Map`)는 콜러블로 **Func와 TObj 둘 다** 받는다(`function::is_callable`).
     타입 객체는 각 요소에 대해 그 타입으로 변환한다: `["3", "1", "2"].map(int)` → `[3, 1, 2]`,
     `io.read().split().map(int)` 가능.
  3. 콜러블의 호출은 `function::arg_count(f) >= 2`이면 `(value, index)`, 아니면 `(value)`를
     넘긴다. 타입 변환기는 항상 1-arg.
  4. 호출은 모두 `obj::call_args`로 디스패치(Func → `fn_call`, TObj → `tobj_call`의
     프리미티브 캐스팅/클래스 생성자 경로와 동일)한다. 호출 실패는 기존처럼 `is_err`로 전파.
- **호환성**: 기존 `list.map(fn(x, i) -> ...)` 2-arg lambda는 그대로 동작한다. 1-arg lambda에
  대한 기존 에러 기대(`"map requires a 2-arg lambda"`, `test/unit/native/list.un`)는 이
  요청으로 의미론이 바뀌므로 그대로 두고 테스트를 **양방향 검증으로 교체**했다(약화 아님).
  `filter`/`any`/`all`은 Func-only 계약을 유지한다.
- **검증**: Positive **1414 passed**(증가 +8 assertions), Negative **41**, differential
  **81 files / 0 differences**, cargo test **205**(release + debug), unbc-roundtrip
  40/40 + 34/34, `test_suite.un` 109.


## D-075 | `str.trim()` 기본은 Unicode 공백 제거 | Accepted

- D-074 계열의 "필수 인자지만 기본값이 자연스러운" 문제의 또 하나. `trim(chars)`는 Python
  `str.strip()`과 이름이 다른 Un의 `trim`이지만, `chars` 생략 시 **Unicode 양끝 공백**을
  제거한다(`char::is_whitespace()`, Python `strip()`과 동일한 White_Space 집합). 기존
  `trim("x")`처럼 chars를 주면 그 문자 집합을 양끝에서 드물게 제거하는 동작은 그대로다.
- **호환성**: 기존 기대 `assert_err(fn() -> "abc".trim(), "trim requires chars")`는
  사용자 편의 요청으로 교체되었다(약화 아님). Positive 1414 → **1415**.
- **조사에서 확인된 것 (수정 없음)**:
  - `int(" 42")`/`float(" 1.5 ")`/`int("42\n")` — 이미 공백을 허용.
  - `dict.get(k)` / `(k, default)` — 이미 두 형태 지원, 미존재 키는 `none`.
  - `list.pop()` — 인자 생략 시 마지막 요소를 pop하고 음수 인덱스도 지원.
  - `s.index_of(x)`(문자열/리스트), `d.keys()`/`.values()`, `l.sort()`, `l.reverse()`,
    `len`, `type(x)` 등 — 기존 호환 동작 양호.
  - 전역 builtin(`sorted`/`sum`/`max`/`min`)은 모듈 설계(`use iter`/`use math`)상 비워두는
    것이 Un의 선택이며, Python 관용구라서 추가하지 않는다(범위 밖).
- **재검증**: Positive **1415 passed**, Negative 41, differential 81/0, cargo test
  205(debug+release), unbc-roundtrip 40/40+34/34, suite 109.


## D-076 | 전역 빌트인 6개를 `codes` 모듈로 이동 | Accepted

- **배경/문제**: 전역 네임스페이스는 `len`/`type`/… + 타입 객체로 구성되어 있었는데,
  `bin`/`oct`/`hex`/`ascii`/`chr`/`ord`는 실제 사용 빈도가 매우 낮았다(테스트·벤치·문서
  전수에서 bin 2, oct 2, hex 2, ascii 2, chr 5, ord 4). 전역이 크면 자동완성/충돌/신규
  API 선택이 나빠진다. `int`/`str`/… 와 같은 타입 객체는 생성자 필수라 전역에 남겼고,
  `true`/`false`/`none`/`null` 도 언어 상수이므로 전역 그대로다.
- **결정 (사용자 요청)**: `bin`/`oct`/`hex`/`ascii`/`chr`/`ord`를 새 모듈 `codes`
  (`use codes`)로 이동한다. 동작·에러 메시지·경계값(문자 코드 0..0x10FFFF
  범위, `-0b...`, UTF-16 단위 동작)는 전혀 바꾸지 않는다. 하드 브레이크: 전역
  이름은 완전히 제거한다(`WORLD`의 전역에서 삭제; `world::try_get_global(name)`로
  `None` 확인하는 테스트가 이를 고정한다).
- **전역에 남는 것**: `len`/`exit`/`type`/`hash`/`throw`/`throw_if`/`throw_unless`/`repr`
  + 타입 객체 + `true`/`false`/`none`/`null` 상수.
- **사용처 갱신**: `test/unit/runtime/builtins.un`(`codes.hex()` 등), negative 픽스처
  3개(`chr_space`/`ord_space`/`multi_space_arg`: `codes.chr()`/`codes.ord()`).
- **검증**: Positive **1415 passed**(동일), Negative 41, differential 81/0, cargo test
  205(release+debug), unbc-roundtrip 40/40+34/34, suite 109.


## D-077 | JIT depth guard: atomic에서 cached static으로 | Accepted

- **문제**: `fib(24)` (150k 재귀 호출)에서 Un JIT이 Python과 비슷한 3.39ms에 머물렀다.
  VM/JIT/Python 모두 같은 하네스(7 reps, warmup 2, median)로 측정한다.
- **근본 원인**: Cranelift-native recursion은 동일 함수를 자기 자신으로 직접 호출(emit_user_call)하지만,
  모든 진입경로가 `un_jit_depth_enter`(`atomic fetch_add` + `WORLD.with(borrow)`로 max였던
  `max_recursion_depth()` 호출)를 수행했다. 단일 스레드 + 협력형 환경인데 힙 셀 TLS + 원자 RMW를
  매 호출마다 수행했다.
- **해결**: 단일 전역 원자 카운터(Relaxed)로 변경하고, `max_recursion_depth`를 `Relaxed` 로드 가능한
  `AtomicUsize` 캐시로 바꾸었다. 핫패스가 `WORLD`를 borrow하지 않고, 역시 RMW는 필요한 만큼만 수행한다.
  `sys.setrecursionlimit`이 호출되면 `world::set_max_recursion_depth`가 캐시를 갱신한다.
- **결과**: fib24 3.39ms → **1.32ms** (Python 3.13ms의 **0.4x**, Python 대비 2.4배 빠름).
  정확성/게이트는 동일: Positive 1415, Negative 41, differential 81/0,
  cargo test 205, unbc-roundtrip 40/40+34/34.


## D-078 | math Float intrinsic: f64 FFI lowering, domain violations bail | Accepted

- **문제**: 정규화 패스에서 벤치마크 모양 intrinsic을 제거한 뒤, `math.*` 호출이 포함된
  proto는 load 시점에 candidacy가 떨어져 전부 interpreter로 실행됐다(math10000 JIT ≈ 22ms,
  sin20000 JIT ≈ 27ms). semantics 위반은 아니지만 수치 루프를 쓰는 실제 프로그램까지
  같은 JIT 손실을 입었다.
- **결정**: `math` 모듈의 19개 함수(sin/cos/tan/asin/acos/atan/sqrt/cbrt/exp/log/log2/log10/
  floor/ceil/trunc/sign/pow/hypot/abs)를 일반 Float intrinsic으로 낮춘다. 인식은 이름
  텍스트(`LoadName "math"` + `GetAttr <fn>` + `Call`) — `iter.range` peephole와 `len`
  마커의 확장 방식. FFI는 total, side-effect-free Rust `f64` shim(`native/math.rs`의
  `un_math_*`). 반환 태그는 interpreter 그대로다(floor/ceil/trunc의 Int identity, trunc의
  Float→포화 Int, sign 항상 Int, pow는 Float 포함일 때만 FFI — **both-Int는 bail**: nameless
  레지스터에는 Int/Float 구분을 담을 수 없고 poly-tag는 promoted local 전용).
- **정확성 계약**: sqrt(x<0) / log(x<=0) 등 정의역 위반은 JIT이 `fcmp` 가드로 tag=2
  whole-frame bail → pristine 인자로 interpreter가 재실행 → 정확한 Un 오류를 그대로 발생.
  오류 메시지·NaN 비트·pow Int 의미론이 모두 동일. `program_has_intrinsic_rebind`를
  확장해 `math` store는 무조건 프로그램 전체 JIT veto, math 함수와 같은 이름의 member
  `SetAttr`은 **프로그램이 math global을 읽을 때만** veto(수학을 안 쓰는 `self.sin = 3`류는
  영향 없음).
- **검증**: Positive **1493 passed**(47 files), Negative **41 matched**, differential
  **88 files 0 differences**, unbc-roundtrip **47/47+34/34 identical**, cargo test
  --release **205 passed**, `UV_JIT=2` canary OK, bench/math.un JIT 0.053ms(3/3 assert),
  bench/floatmath.un JIT 0.54ms(2/2 assert). `test/unit/native/math.un` 47 passed(오류
  경로 포함 양 lane 동일).
- **Trade-off**: `use math` + member rebind(`math.sin = spy`, `m = math; m.sin = spy`
  포함)은 정확성 우선으로 프로그램 전체 JIT veto. `self.pow = 3`처럼 math와 무관한 instance
  속성을 math 사용 프로그램에서 재정의하면 JIT가 꺼진다(interpreter가 정확). 런타임 probe
  정밀화는 plan 타임에 program global scope에 접근이 필요해 별도 엔진 설계.
- **상세**: 설계 문서는 2026-11-19에 레거시로 분류되어 삭제됨 (git 이력에 보존).


## D-079 | JIT: unary `Op::Neg` lowering (ineg/fneg); TD-53 closed | Accepted

- **결정**: `Op::Neg`을 JIT에서 `ineg`(Int — two's-complement wrapping, interpreter
  `wrapping_neg`과 동일) / `fneg`(Float)으로 lower. Bool/none/arena handle
  (IntVec/IntDict/class marker)의 피연산은 bail → interpreter가 정확한
  "unsupported operand type(s) for -" 오류를 그대로 발생시킨다.
- **배경**: `strict_float.un` bring-up에서 실측으로 발견(`math.sign(-0.5)`가 Neg 미지원으로
  candidate reject → `1/6 candidate(s) failed JIT codegen`). 부채 TD-53으로 등록 후 해결.
- **검증**: Positive 1493, differential 88/0, unbc-roundtrip 47/47, cargo test 205,
  strict_float canary 양 lane + UV_JIT=2 green, i64 경계값(9223372036854775807) JIT/인터프리터
  동일. `strict_float.un`은 `-0.5`/`-7`을 써서 Neg arm을 상시 CI 검증한다.
- **잔여**: `Op::Pos`/`Op::Bnot`은 여전히 미지원(같은 클래스 — 필요할 때 D-0xx로).


## D-080 | `go` fork-time scope snapshot 유지 (TD-09 closed by design)

- **문제**: `scope::snapshot`이 `go` future 생성 시 상위 스코프 체인을 통째로 deep clone한다. 측정상 200,000 forks ≈ 1,058 ms(포크당 ≈ 5 µs)이고, snapshot 자체는 ~1–3 µs로 interpreter dispatch 아래 파묻혀 포크 수가 늘어날 때까지는 비측정역에 위치한다.
- **결정**: 현재의 fork-time snapshot(SKIP하여 capture-by-value) 의미론을 유지한다. 대안은 전부 채택하지 않는다:
  - fork-on-write — Scope 쓰기 경로 전체를 분기로 갈라야 한다. 의미론은 유지되지만 alias 판정 구현 난도가 크고, 호출 사이트마다 검사를 넣어야 한다. 측정 불가는 의미론적 리스크가 ROI보다 크다.
  - slots만 얕게 복사하고 부모는 공유 — 캡처-by-value가 깨진다(부모 쓰기가 future로 누설) — reference Un과의 의미론 동일성을 깨는 계약 변경.
  - 컴파일타임 capture-set 분석 — 이론상 가장 크지만 `go`는 interpreter에서 보통 한두 개의 값만 캡처하는 spawn 형태라 200k fork 기준 절약분은 < 5 ms. 구현 난도 대비 이득 불가.
- **근거**: reference Un과의 의미론 동일성이 최우선 계약, 측정된 snapshot 비용이 pool/interpreter dispatch 밑에 파묻혀 측정이 안정적으로 분리되지 않는다.
- **후속**: 필요 시 컴파일타임 capture-set 분석은 구현 가능하나 현 스케일에 맞지 않아 보류.
- **검증**: 20,000 forks ≈ 99 ms / 200,000 forks ≈ 1,058 ms — 현재 구현에서 병목이 아님. TD-09 closed by evidence.

## D-081 | use a.*를 use a { * }의 축약형으로 쓰기 | Accepted

- **배경**: `use a.*`는 파서가 wildcard import로 인식했으나 코드 경로는 지원하지 않았고, 런타임에 `module '<절대경로>\a\*'를 찾을 수 없음` panic이 발생했다.
- **결정**: `use a.*`를 `use a { * }`와 동일하게 wildcard import로 컴파일한다. `parse_use_path`가 생성하는 `Unary(Wildcard, [path])`를 code_use가 `imports = [("*", "*")]`로 매핑한다.
- **전**: 알 수 없는 모듈 panic. **후**: `use re { * }`와 같은 와일드카드 import.

## D-082 | JIT: hidden `param_tags` ABI + 프로토 엔트리 가드 (list-handle 인자) | Accepted

- **배경**: JIT ABI의 Int 인자로 `IntVec` arena handle이 섞여 들어오는 경우(원시 Int 인자가 ID_BASE 이상이면 FFI 경계에서 거부됐지만, List 인자는 materialize되어 통과) 다음 결함이 있었다.
  1. `xs + 1`처럼 List를 산술에 쓰면 interpreter는 TypeError로 거부하지만 JIT는 raw handle + 1을 Int로 반환했다(silent miscompile).
  2. `-> xs` identity 반환은 Return probe blanket가 handle을 INTLIST로 잘못 tag했다(heterogeneous probe 남용).
  3. `xs.add(inner)`에서 inner를 handle로 넘겼는데 `intlist_regs` 마커가 ArgPos를 지나며 flush되어 plain Int처럼 취급됐다.
  4. `int_dict_seen`가 list 마커를 포함해 IntVec row 삽입까지 거부했다.
- **결정**:
  - JIT entry는 hidden `param_tags: i64`를 받는다. 비트 p는 argument p의 IntVec arena handle 여부.
  - `try_call_jit`이 list 인자의 bit를 set한다. nested JIT call은 staged arg reg 마커로 bitmap을 구성한다.
  - 프로토 진입 시 `unsafe_param_mask(code, param_mask_by_d)`를 한 번 검사한다. 산술/비교/ToStr 등 Int-consuming op에 흐를 수 있는 파라미터 위치면 태그가 있으면 전체 프레임을 interpreter로 bail (int 인자면 0 비용).
  - Return probe는 UNION 또는 param_idx인 경우에만 수행. 다른 레지스터는 태그 없음.
  - `int_dict_seen`에서 IntVec은 분리해 `int_list_foreign_seen`(dict/instance/method/iter)로 IntVec intrinsic에서 거부 범위를 좁혔다.
  - ArgBegin/ArgPos/ArgKw를 JIT marker flush에서 제외(순수 인자 프레이밍).
  - `un_list_int_push_scalar`/`un_list_int_set_scalar` entry point 추가 — 평범한 Int는 KIND_OUTER row로 승격되지 않음.
- **검증**: fib(24) JIT ~34-53 ms vs UV_JIT=0 ~221 ms. union_calls.un 포함 differential 91/0. strict.un/strict_float.un canary UV_JIT=2 green. functions.un identity list + union_calls list 인자 가드 신규 회귀 테스트 추가.
- **남음**: nested JIT-only 경로에서 handshake 태그가 아닌 mark로 우회되는 caller가 있으면 동일 클래스의 케이스가 남는다.

## D-083 | 다중 `catch`는 컴파일 에러로 거부 | Accepted

- **배경**: 파서는 `catch`를 무한 허용하지만 컴파일러는 첫 번째 catch만 등록하고 나머지를 무시했다(사용자는 모르고 넘어감).
- **결정**: 두 번째 `catch`는 `error: multiple 'catch' clauses are not supported; use a single catch and discriminate inside it` 컴파일 에러로 거부.
- **검증**: `test/negative/multi_catch.un` regression (manifest에 `multi_catch.un|1|multiple 'catch' clauses` 추가). Positive 1525 / Negative 44 matched.

## D-084 | `**`는 오른쪽 결합 | Accepted

- **기준**: `2 ** 3 ** 2`는 `2 ** (3 ** 2)` = 512. Python/JS/수학 표기와 같은 규칙.
- **이전**: 왼쪽 결합이라 `(2 ** 3) ** 2` = 64 (테스트 문구는 "right associative"로 바이트 파라미터가 틀렸음을 2026-11-18 감사에서 지적받음).
- **변경점**: `parse_binary`가 DoubleAsterisk의 오른쪽 피연산자를 `precedence - 1` 수준으로 재귀.
- **테스트**: `test/unit/lexer/literals.un`, `test/unit/parser/precedence.un`, `test/unit/runtime/operators.un`의 기대값 64 → 512.
- **대조 검증**: 테스트 + 벤치마크(bench) 모두 통과. 바이트코드 diff 러너 91/0.
- **사용자 결정**: 2026-11-19 "오른쪽 결합으로 변경".

## D-085 | 알 수 없는 이스케이프는 백슬래시를 그대로 유지 | Accepted

- **이전**: `"a\qb"` → `"aqb"` (백슬래시 삭제). `"\x41"` → `"x41"`.
- **변경**: `Some(c)` 미매칭은 `out.push('\\'); out.push(c)`를 추가. 끝의 외로운 백슬래시도 유지.
- **결과**: `\q` → `\q`, `\x` → `\x`. Python의 unknown escape 규칙과 동일.
- **사용자 결정**: 2026-11-19 "백슬래시 유지 (Python 방식)".

## D-086 | 한 줄에 여러 문장 금지 | Accepted

- **이전**: `x := 1 y := 2` 같은 같은 줄 연속 문장이 조용히 허용됐다(토큰 유형이 문장 시작이면 무조건 허용).
- **변경**: 블록/간격 경계 기준으로, 이전 유의미 토큰과 같은 줄에 새 문장이 시작하면
  `expected a newline after the previous statement (got \`Identifier\`)` 파싱 에러.
  같은 줄이 아닌 경우(Dedent 뒤의 첫 문장)는 여전히 허용.
- **테스트**: `test/negative/same_line.un` (manifest에 `same_line.un|1|expected a newline after the previous statement` 추가).
- **사용자 결정**: 2026-11-19 "엄격하게 거부". Positive 1525 / Negative 45 matched.

## D-087 | HTTP 리다이렉트(301/302/303/307/308) 자동 추적 (최대 8회) | Accepted

- **이전**: 어떤 3xx도 따라가지 않고 Location 무시; `get`만 404류에서 예외.
- **변경**:
  - 301/302/303 → GET 으로 메서드 바꾸고 본문 버림 (.NET HttpClient 규칙).
  - 307/308 → 메서드/본문 유지.
  - Location은 절대/상대 모두 지원(RFC 7231 계단식 path resolution), `..`/`.` 세그먼트 정리 포함.
  - `//host/path` scheme-relative, `/path`, `?query`, `#fragment` 모두 처리.
  - https Location은 거부(클라이언트는 http만 지원).
  - Location 헤더가 없으면 리다이렉트 응답을 최종 응답으로 반환.
  - 8홉 초과 → `http redirect limit exceeded` 에러.
- **검증**: `net::redirect_tests` 8개 Rust 통합 테스트(실제 127.0.0.1 TCP 서버 기반: 302→301 체인,
  POST→GET 전환, HEAD 유지, PUT+본문 307 보존, 상대 Location 해석, 절대 Location, 무한루프 제한).
- **이전과의 차이**: `get`의 404식 예외 계약은 유지(사용자 결정: "verb 오류 모델은 개별 유지"가 아닌 "리다이렉트만 추가").
- **사용자 결정**: 2026-11-19 "리다이렉트 추적 추가".

## D-088 | `b?.v = e`는 런타임 에러 유지 | Accepted

- **이전**: `'none' object has no attribute 'v'` 에러 (선택적 체이닝을 대입 좌변에 쓸 때).
- **결정**: 현행 유지. 조용히 skip하는 것은 사용자의 눈 가려짐(데이터 손실이 보이지 않음)으로 금지;
  컴파일 에러는 현 테스트/파이프라인 호환성상 후순위. 문서화만 한다.
- **사용자 결정**: 2026-11-19 "현행 유지 + 문서화".

## D-089 | `date(str)`의 성분 엄격 검증(회귀 확인) | Accepted (확인)

- **2026-11-18 감사 A17**: `date::parse`에 hour/minute/second 범위 검사가 없던 결함 → 해당 패스에서 수정됨.
- **이번 감사에서 검증**: `date("2024-02-30")`, `date("2024-13-01")`, `date("2024-01-01 25:00:00")`,
  `date("2024-02-29")`, `date("2023-02-29")`, `date("2024-01-01 23:60:00")`, `date("e")` — 모두 올바르게 판정.
- **사용자 결정**: "엄격하게 거부" — 현행과 일치. 코드 변경 없음.

## D-090 | 클래스 멤버의 타입 어노테이션은 선언이다 | Accepted

- **이전**: 클래스 본문 `x: float` / `x: float = 5`처럼 쓰면 멤버가 `x: float`라는
  *이상한 이름*으로 등록되거나(`x: float = 5`), 아예 등록되지 않았다(`x: float`).
  `class Point(x, y)`의 클래스 헤더 파라미터는 `Identifier`/`Typed`만 허용해서
  기본값이 섞인 파라미터는 `internal: invalid initializer`로 죽었다.
- **결정**:
  - 클래스 본문 멤버의 어노테이션은 이름 결정 시 벗겨진다. `x: float = 5` → 멤버는
    `x`, 저장값은 `5`. `x: float` (기본값 없음) → 멤버는 `x`, 저장값은 `none`.
  - 클래스 헤더 파라미터에 어노테이션·기본값이 있어도 필드 이름은 맨 바깥
    식별자다. `class Pair(x: float = 0.0)` → 필드는 `x`로 등록된다
    (다만 struct-init 의미론은 기존 그대로: 정확히 `len(fields)`개의 위치 인자를
    넘겨야 하고, 아니면 `invalid initialize`다 — `class Point(x, y)`와 같은 규칙).
  - 저장값에는 어떤 타입이든 들어간다. 어노테이션은 "내가 이 슬롯에 무엇을 넣을 것이다"
    선언일 뿐이며, 동적 타입 체크는 없다.
- **검증**: `test/unit/runtime/classes.un`에 타입 어노테이션 케이스 추가(73개 통과).
  기대 출력 message column은 상수.
- **거절된 대안**:
  - 슬롯별 동적 타입 강제: Un의 동적 타입 철학(어노테이션은 힌트)에 어긋난다.
  - `Point()`에서 기본값 사용: struct-init 의미론(위치 인자 정확히 일치)과의
    역류가 있을 수 있어 보류. 향후 `class Point(x=0.0, y=0.0)`의 기본값 배선을
    지원하려면 별도의 결정이 필요하다.
- **사용자 결정** (2026-11-19 "클래스 내부의 멤버 변수에 타입 어노테이션이 안되는데 가능하게 해줘").

## D-091 | 미처리 오류는 Un식 트레이스(`un: error trace`)를 출력한다 | Accepted

- **문제**: 미처리 에러가 최상위에 도달할 때 함수 경계들의 호출 위치가 출력되지 않았다.
  에러가 콜 프레임을 넘어가는 순간 `pop_frame`으로 프레임이 버려져서 `report()`에는
  프레임이 0개였고(`trace:` 블록은 사실상 dead code), 출력은 raise 지점의 한 줄짜리
  헤더뿐이었다. 모듈에서 발생한 에러의 span은 main 파일 좌표로 해석되어 엉뚱한 줄이
  찍히는 잠재 버그도 있었다.
- **결정**:
  - 에러가 콜 경계를 넘을 때마다 공유 trace 버퍼에 `TraceEntry`
    (call-site의 `Rc<Source>` + span + **콜러** 함수명)를 푸시한다. 출력은
    `un: error trace` 헤더 아래 outermost-first로
    `-> <함수> <파일>, line [N]`와 소스 라인을 표시하고,
    마지막에 기존 LangError 블록(파일/라인/캐럿 + `error: <msg>`)을 그대로 이어 쓴다.
  - `catch`는 trace를 클리어한다(잡힌 에러의 잔재가 이후의 무관한 에러에 섞이지 않게).
    `finally`를 통과하는 에러는 계속 누적된다.
  - `?`로 전파되는 Err(`Outcome::Propagate`) 경로도 기록한다.
    `Break`/`Skip`/`Return`은 에러가 아니므로 기록하지 않는다.
  - `LangError`에 `source: Option<Rc<Source>>`를 추가하고, VM이 에러를 만들 때 현재
    ctx의 소스를 첨부한다. `display()`는 첨부된 소스를 우선 사용한다 — import 모듈의
    에러가 모듈 기준 파일/라인으로 표시된다(기존 잠재 버그 수정).
  - 루트 프로토는 `<module>`로 등록된다. 프레임의 함수명은 caller(현재 프로토)의
    이름을 사용한다 — 각 프레임을 그 프레임을 실행 중인 함수로 명명한다.
- **형식**: 최초에는 Python식(`Traceback (most recent call last):` + `File "..."` 행)으로
  구현했으나, 요청자 판단("너무 파이썬 같다")으로 Un식으로 다시 디자인했다. 요청자 선택 —
  형식 A(`un:` 접두 + `->` 체인), 프레임 순서 바깥→안쪽, 프레임에 소스 라인 표시.
  프레임 위치 표기(`<파일>, line [N]`)는 기존 에러 캐럿 블록과 동일 규칙을 쓴다. 예시:

  ```
  un: error trace
      -> <module>  <temp_fmt2>, line [12]
           main()
      -> main      <temp_fmt2>, line [10]
           outer()
      -> outer     <temp_fmt2>, line [7]
           inner()

  <temp_fmt2>, line [4], column [8]
      -> 1 / 0
         ^^^^^
  error: int division by zero
  ```
- **알려진 한계**: Python의 "During handling of the above exception" 식 중첩 예외 체인에
  해당하는 기능은 없다. catch 안에서 새 에러를 throw하면 새 트레이스로 시작한다(의도된 단순화).
  중첩 예외 체인은 아니지만, 미처리 에러는 CLI / `.unbc` / **웹 플레이그라운드**
  모두 동일한 `un: error trace` 출력을 낸다(플레이그라운드는 `parse_error` 경로만
  예외 — VM-native 오류 지점이 없으므로 트레이스 없이 진단 메시지만 나온다).
- **검증**: Negative 48(`test/negative/traceback.un`, `traceback_toplevel.un`,
  `traceback_import` 핀), differential **95** files, compilable Negative **39/39**,
  cargo test 213. 벤치 영향 없음(에러 경로에서만 호출).
- **보강 (같은 날 후속, "트레이스백 안되는거 같은데?")**:
  - **최상위/모듈 import-time 에러도 트레이스백을 찍는다.** 호출 경계를 0번 건넌
    에러(예: 모듈 최상위의 `1 / 0`, import 실패)는 LangError의 span에서 `<module>`
    프레임을 합성한다. (이전에는 trace가 비어 "Traceback" 헤더 자체가 빠졌다.)
  - **import 경계도 프레임으로 기록**한다(`Op::Use`의 실패 경로). 그리고 모듈 실행이
    임포터의 공유 trace를 물려받도록 `import_file → load → load_inner → load_file`로
    `Option<Traces>`를 전달 — 모듈 내부의 호출 경계도 같은 트레이스에 쌓인다.
    모듈의 `frames` 스택 자체는 기존대로 신선하게 유지(depth 불변). 모듈의 Parse
    에러는 `LangError::with_source(모듈 소스)`를 붙여 모듈 기준으로 표시된다.
  - **반복 프레임 접기**: 연속으로 동일한 (source, span, name) 프레임은 한 줄로 접고
    `... (repeated N more times)`만 출력한다. `maximum recursion depth` 에러 출력이
    2007줄 → 10줄로 줄었다.
  - **형식 재설계 ("출력 예시가 너무 파이썬 같으니깐 un스럽게 새롭게 만들자")**:
    Python식 출력을 `un: error trace` 헤더 + `-> <함수> <파일>, line [N]` 체인
    (이름 컬럼 정렬, 소스 라인 동반)으로 교체. 프레임 순서는 바깥→안쪽 유지,
    프레임의 위치 표기는 에러 캐럿 블록의 `<파일>, line [N]` 규칙과 통일.
    세 negative 핀의 expected substring을 `Traceback (most recent call last):`에서
    `un: error trace`로 갱신.
  - **웹 플레이그라운드도 동일 출력** ("웹 플레이그라운드에서는 왜 안돼?"):
    `report()`는 CLI 전용이고 플레이그라운드는 `PlaygroundResult.error` 문자열을
    직접 만들기 때문에 트레이스가 빠져 있었다. 트레이스 렌더링을 `unvm`의
    `runtime::context::trace_string`으로 뽑아 CLI(`report()`)와
    플레이그라운드(`render_error`)가 같은 렌더러를 공유한다. 각 Outcome 팔에서
    `context::take_trace`로 버퍼를 드레인해 에러 텍스트 앞에 붙인다.
    `catch` 경로는 보상 없이 그대로 — 잡힌 에러는 트레이스가 클리어되므로
    플레이그라운드도 `error: null` + exit 0 유지. 모듈 경계 체인도 동일하게 동작.
    WASM smoke(`rust/playground/smoke.mjs`) 64/64 OK, 미처 panic 전용 케이스는
    `panic_display` + (있을시) 트레이스 병합.
  - **익명 함수 프레임 이름 ("플레이그라운드에서 만든 함수는 왜 <function name>의
    형태로 안나와? 그냥 함수 이름만 덜렁 나오네")**: 익명 `fn ... ->` 표현식의
    프로토 이름은 컴파일러가 `lambda`로 고정한다. 프레임 명명 규칙에
    `"" → "<module>"`에 더해 `"lambda" → "<lambda>"`를 추가 — 이름 있는 함수는
    이름만, 모듈 스코프는 `<module>`, 익명 함수는 `<lambda>`로 표시한다.
    raise 지점 프레임은 trace에 넣지 않고 caret 블록으로 표시하는 현행을 유지하고
    (요청자 선택: "raise 지점 프레임은 현행 유지"), 익명 함수만 `<lambda>`로 감싼다.
  - **프레임이 1개면 트레이스를 생략한다** ("추가로 트레이스가 1개 이면 안나오게
    바꿔줘"): 얕은 에러는 caret 블록 한 줄이 트레이스보다 읽기 쉽다 —
    `trace_string()`은 행이 1개뿐이면(`rows.len() <= 1`) `un:` 블록을 아예
    출력하지 않는다. 예: `fn f() -> 1 / 0` + `f()` → caret + `error: int division by zero`뿐.
    `traceback_toplevel.un` 핀의 substring을 `un: error trace` →
    `int division by zero`로 갱신(48 유지).
  - **트레이스가 너무 길어지면 소스 라인을 생략한다** ("추가로 트레이스가 너무 길어지면
    코드 부분을 지우고 그냥 그 라인정보만 주게 만들어줘"): 접힌 뒤 프레임 행이
    10개(`COMPACT_FRAME_THRESHOLD`)를 넘으면 compact 모드 — 각 행은
    `-> <fn> <file>, line [N]`만 출력하고 소스 라인은 버린다. 치환 마커
    (`... (repeated N more times)`)는 그대로. 교차 재귀(a ↔ b) 1000단 에러 출력이
    소스 라인 포함 약 2000줄 → compact 약 1006줄.
  - 추가 핀: `test/negative/traceback_toplevel.un`,
    `test/fixtures/traceback_import/{main.un,mod.un}`. Negative 48, differential
    **95** files, roundtrip compilable Negative **39/39**.
- **요청자 발언** (2026-11-19 "에러 나올때 에러를 전파해서 이너 예외도 에러 출력할때 나오게 해줘",
  같은 날 "출력 예시가 너무 파이썬 같으니깐 un스럽게 새롭게 만들자" — 형식 A(`un:` 접두 +
  `->` 체인), 바깥→안쪽 순서, 소스 라인 표시 선택).

## D-092 | 내장 System.*Exception 명칭 제거 — catch 불가 Panic도 `error:` 로 통일 | Accepted

- **배경**: D-008("예외 이름은 .NET 이름 재현")은 C# 포팅 검증기 시절의 잔재였다.
  Rust 포팅 후 Un의 사용자-노출 오류 형식은 이미 `error: <msg>`(LangError header "error")
  으로 통일되어 있었고, `int % 0` 같은 catch 불가한 Panic만 `System.DivideByZeroException`
  같은 .NET 식 이름을 쓰는 불일치가 있었다. 2026-11-20 "이 형식말고 un 형식으로 바꿔줘"
  지시로 통일 작업을 수행했다.
- **결정**:
  - 내장 런타임이 만드는 모든 Panic의 `name`을 `"error"`로 통일한다
    (`divide_by_zero()` → `panic("error", "Attempted to divide by zero.")`).
  - 출력은 기존 catchable 오류와 동일하게 `error: <msg>` 한 줄로 통일된다. catch 불가는
    계속 `try`로 잡히지 않고 최상위에서 종료한다(D-091 트레이스 포함).
  - Panic 특수 분기(`name == "panic"` → `\npanic : {msg}`)와 내부용 `need_input` 계통은 유지.
  - `test/negative/manifest.txt`의 6개 substring이 System.*에서 메시지 자체로 바뀌었다.
    Rust 단위 테스트의 expected 문자열도 동일하게 바뀌었다.
  - D-008 / S4-10(PROJECT_AUDIT.md) / TD-48(TECHNICAL_DEBT.md)의 System.* 계약은 supersede된다.
    catchable 여부·exit 1·메시지 내용은 변경 없고, 겉 이름만 바뀐다.
- **검증**: `cargo build --release` 0 errors/0 warnings, `cargo test --release --lib` green,
  `scripts\test.ps1` Positive·Negative 기준 유지, differential 기준 유지.
- **주의**: `.NET` parity를 계약으로 삼던 부분(mod_zero / replace_missing / replace_empty /
  dict_add_duplicate / types_overflow*)은 `error:` 기준으로 재작성됐다. 이후 내장 오류를
  새로 추가할 때도 `System.*` 접두사는 쓰지 않는다.

## D-093 | Un의 메모리 모델: RC + 실행 경계 정리, GC/Weak 없음 | Accepted

- **배경**: MEM-01/MEMPLAN-01 분석(2026-11-24). Un에는 GC가 없고 `rust/src/runtime` 전역에
  `Weak`이 0개다. VM/JIT/embed는 `Rc<RefCell<...>>` 참조 카운팅만으로 객체를 해제하며,
  no-VM plain exe는 `Box::into_raw`로 할당하고 해제 코드 없이 프로세스 종료 시 OS가 회수한다.
  순환 참조는 수집되지 않는다.
- **결정**: 다음을 **언어/런타임 계약으로 확정**한다.
  1. **GC는 도입하지 않는다.** 외부 crate 없이 marking collector를 만드는 비용과
     byte-identical 게이트 위험이 이득보다 크다.
  2. **`self_`와 closure/capture의 강한 참조는 의도다.** `let f = obj.m; obj = null; f()`
     가 동작해야 하고, lambda escape/capture-by-value가 계약(D-080)이다. 따라서
     `ObjData.self_`→`Weak`, `FnData.closure`→`Weak`, `Scope.slots`→`Weak` 전환은
     **하지 않는다**. TD-22/TD-09의 순환은 "유계 누수"로 분류하고 매 실행의
     `world::reset()`(native::init)와 프로세스 종료 시점 회수로 충분하다고 본다.
  3. **JIT 코드 메모리는 실행 경계에서 명시 해제한다.** vendored Cranelift의 Drop이
     `mem::forget`하므로 `jit_cache::clear_program`/`clear()`가 유일한 해제 경로다.
     호출 지점은 CLI 정상(`main.rs`), CLI 에러(`report()`), playground 매 실행,
     **embed `unvm_run_embedded()` 종료 시**(`unrt.rs`) 4곳으로 확정(2026-11-24 보완).
     `clear()` 이후 JIT 코드를 다시 진입하는 것은 계약 위반이다.
  4. **`PLAN_CACHE`는 상한 64개 FIFO로 유지**(TD-03). 실행 중 재클레이밍 정책은 두지 않는다.
     모듈 unload/hot-reload는 **미설계 상태로 명시**하고, 필요해지면 opt-in API로 별도 결정(D-신규)한다.
  5. **NRT는 의도적 누수 모델을 유지한다.** `NObj`와 내부 버퍼는 `Box::into_raw`,
     작은 정수 0..16383은 정적 배열. bump arena는 측정 후 필요할 때만 NRT-06이 별도 제안한다.
     현재는 allocator pressure가 유의미하지 않아 도입하지 않는다.
  6. **메모리 증가 추적은 프로세스 단위로만 한다.** 장기 실행 embed/REPL/서버는
     위 계약상 누적이 예상되므로, 필요한 경우 호스트가 프로세스를 재시작하는 전제를 문서화한다.
- **영향**: 공개 API/C ABI/`Op`/`unrt_*` 심볼 변경 없음. TD-03/TD-05/TD-22의 상태를
  "해결"이 아니라 "계약 확정(유계 누수 포함)"으로 재기재. ARCHITECTURE의 Resource lifecycle
  절에 실행 경계 4곳과 clear 후 계약을 명시.
- **근거**: `MEM-01`(경로별 메모리 모델), `MEMPLAN-01`(Weak 전환 시 byte-identical 위반
  및 JIT raw receiver UAF 위험), `test/unit/runtime/classes.un`의 bound-method 수명,
  `test/unit/runtime/async.un`의 lambda escape, `vendor/cranelift-jit/src/memory/system.rs`.
- **검증**: 전체 게이트(Positive 51/1638, differential 102/0, roundtrip 51/51+42,
  compile-check 37/0, exe-diff 63/0, strict canary 5종)와 embed 반복 실행 회귀.
## D-094 | 모듈 unload는 opt-in으로 허용하되 객체 메모리는 유계 누수 유지 | Accepted

- **배경**: D-093 §4는 "모듈 unload/hot-reload는 미설계, 필요하면 별도 결정"이라고 명시했다.
  BIG-01 분석(2026-11-24)에서 `world::reset()`은 이미 WORLD 전체를 교체하지만 JIT 엔트리와
  class registry는 모듈 단위로 정리할 수 없는 것이 확인됐다.
- **결정**: 다음 계약으로 opt-in unload API를 허용한다.
  1. **기본 경로는 변경 없다.** CLI 1회 실행 / embed / playground는 지금과 동일하게
     `native::init`의 `world::reset()`과 프로세스 종료에 의존한다.
  2. **unload의 의미는 "심볼·레지스트리·JIT 정리"로 한정**한다. 캐시 엔트리, caller scope의
     alias, module이 등록한 class/layout, 해당 program의 JIT/TIR 엔트리와 PLAN_CACHE 항목을
     제거한다. `TD-22`/`TD-09`의 의도적 순환 때문에 **closure/scope가 소유한 객체 메모리는
     회수하지 못하며 유계 누수로 남는다** — D-093 §2와 충돌하지 않는다.
  3. **모듈→program_id 추적을 신설**한다(`module.rs`의 load 결과와 `bytecode::Program.id`를
     캐시 값에 함께 저장). `jit_cache::clear_program(id)` 호출 후에는 해당 JIT 엔트리를
     재진입하지 않는 것이 호출자 계약이다(D-093 §3).
  4. **reload는 top-level side effect를 다시 실행한다.** 이는 opt-in 호출자의 책임이며,
     `un run`과 byte-identical을 게이트로 요구하지 않는다(호출자가 의도적으로 상태를 초기화).
  5. **공개 API로 노출하려면 별도 승인**이 필요하다. `unrt_*` C ABI로 내보내는 경우
     NRT-06 + CMP-05 + ARC-00 승인을 다시 받는다.
- **영향 파일(구현 시)**: `runtime/world/modules.rs`, `runtime/world/mod.rs`, `module.rs`,
  `jit_cache.rs`(JIT-04), `runtime/world/classes.rs`(RT-03). 게이트는 미호출 시 무변경.
- **근거**: BIG-01 §3, D-093 §4, TD-03, `ARCHITECTURE.md` Resource lifecycle.
- **검증**: opt-in API 미호출 시 전체 게이트 무변경. 호출 시 전용 fixture로
  심볼 제거·JIT 정리·side effect 재실행을 확인한다.
## D-095 | callable 값 타입은 `func(..) -> ..` 문법으로 표기 | Accepted

- **배경**: Un에는 함수 객체 자체를 적는 타입 어노테이션이 없어 고차 함수는 `f: any`로
  선언해야 했다. `Ty::Func { params, ret, .. }`는 typeck 내부에만 존재하고 소스
  어노테이션으로 만드는 경로가 없었다(`FN-01` 조사).
- **결정**: `func(int, str) -> int` 형태의 어노테이션 문법을 추가한다.
  1. `func`는 일반 식별자이므로 lexer/token/AST 변경 없이 `parser.rs::parse_type`만으로
     소비한다. 생성 AST는 기존 타입 표현과 동일한 `Identifier + Const::Str(원본 텍스트)`.
  2. `typeck.rs`의 텍스트 파서(`parse_type_atom`/`try_atom_text`)가 `Ty::Func`를 만들고,
     헬퍼 3개(`split_union`/`find_top_bracket`/`split_top_level`)가 괄호 깊이를 추적한다.
     `display()`는 기존 `fn(...) -> ...`를 유지한다(진단·mangle 일관성).
  3. `tir.rs`는 `Ty::Func`를 `TirTy::Lambda(u32::MAX)` opaque 슬롯으로 낮추고,
     함수 이름을 값으로 참조하면 zero-capture wrapper `__lambda_N`을 합성한다.
     `main.rs`의 `__lambda_` prefix 스캔이 NRT에 자동 등록하므로 ABI 변경이 없다.
  4. `un native`(Vm 모드)의 `fn_uses_dynamic`이 opaque `Lambda(u32::MAX)`를 감지해
     인터프리터로 fallback한다 — 기존 `any` 경로와 동일한 계약.
- **효과**: `un build` fully typed mode에서 callable 파라미터를 가진 고차 함수를
  작성할 수 있다. 이는 `any` 탈출구를 대체하는 정식 문법이며, `any` 자체는 여전히
  fully typed mode에서 거부된다.
- **영향 파일**: `parser.rs`, `typeck.rs`, `tir.rs`, `README.md` ↔ `docs/README.ko.md`,
  `docs/guides/COMPILED_LANGUAGE.md`. 신규 fixture `test/compile/func_type.un`,
  `test/compile/build/higher_order.un`.
- **근거**: FN-01, FNTYPE-01 설계, Work-ID `W-2026-11-24-FNSYNTAX` / `FNTYPECK` /
  `FNTIR` / `FNVALUE`.
- **검증**: `un run` / `un native` / plain `un build` stdout byte-identical
  (`42|42|42`); workspace 308 passed; compile-check 38/0; exe-diff 67/0.
### D-095 후속 (2026-11-24)

- **클래스 필드**: `class Holder / op: func(int) -> int` 형태의 필드 선언·대입·호출
  (`h.op = twice`, `h.op(21)`)을 plain build에서 지원한다. `class_member_ty` /
  `ty_to_tir_simple`가 `Ty::Func`를 `Lambda(u32::MAX)`로 매핑하고, `lower_call`의
  Property 경로에서 메서드 대신 `CallObj(MemberGet)`를 emit한다. VM과 byte-identical.
- **반환 타입은 계속 거부**: `fn maker() -> func(int) -> int`는 plain build가 거부한다.
  VM이 `-> twice`를 int로 반환하는 기존 버그(`any` 반환에서도 동일)가 있어, native/exe만
  func을 반환하면 byte-identical 계약이 깨진다. `update` 시점에 VM 수정이 선행돼야 한다.
## D-096 | tuple/list 구조분해는 plain build에서 지원, class/variant는 보류 | Accepted

- **배경**: match AOT는 literal/bind/wildcard → guard → or-pattern까지 구현했으나 중첩
  구조분해(`case (a, b)`, `case Point(x, y)`)는 VM fallback이었다(W-2026-11-24-NESTEDPAT).
- **결정**:
  1. **tuple/list 구조분해를 지원**한다. `TirPattern::Tuple(Vec<TirPattern>)`을 추가하고
     VM의 `compile_pattern`/`match_pattern`과 동일하게 "정확한 길이 일치 + 좌→우 검사 +
     실패 시 binding 미기록"을 구현한다. 중첩 tuple/list, guard, or-pattern(winner commit)도
     그대로 동작한다.
  2. 새 `unrt_match_seq_len(u64) -> i64` / `unrt_match_seq_get(u64, i64) -> u64` 심볼을
     NRT에 추가한다(ARC-00 승인). 실패는 sentinel(-1/0)로 보고해 non-match가 에러로
     번역되지 않게 한다. 기존 `emit_cmp(Eq)`, `fail_blk`, `no match case` 경로는 재사용.
  3. **class/variant pattern은 VM fallback 유지** — 새 type dispatch ABI가 별도 범위.
     named field/spread도 보류.
- **검증**: `un run` / `un native` / plain `un build` byte-identical. 신규 fixture
  `test/compile/tuple_pattern.un`, `test/compile/build/tuple_pattern.un`.

### D-094 후속 (2026-11-24, W-2026-11-24-UNLOADCLEAN)
- opt-in unload가 `jit_cache::clear_program` 외에 `vm::clear_plan_cache(program_id)`로
  PLAN_CACHE 항목도 제거하고, `module::load`의 before/after diff로 추적한 모듈 등록
  class만 `unregister_class`로 레지스트리에서 제거한다. native 클래스는 보존.
- D-093 §2의 객체 메모리 유계 누수 계약은 그대로 — 제거 대상은 레지스트리 엔트리뿐이다.
## D-097 | match의 타입 이름 패턴은 런타임 타입 검사로 동작 | Accepted

- **배경**: Un의 match는 값 패턴만 지원했고, `int ->`나 `Id ->` 같은 식별자 패턴은
  `Pattern::Bind`로 컴파일되어 **항상 매치**했다(사용자 혼란 + `un check`도 통과).
  저장소 전체에 이 문법을 의도적으로 쓴 사례는 없어 byte-identical 위험은 낮았다.
- **결정**:
  1. `compile_pattern`이 타입명(원시 타입·등록된 클래스·타입 별칭)을 `Pattern::Type`으로
     컴파일한다. 로컬 변수가 같은 이름이면 섀도잉 규칙에 따라 기존 `Bind` 유지.
  2. 타입 별칭은 컴파일 시 union 멤버로 펼쳐 `Pattern::Or(Type…)`이 된다
     (`Id = int | float` → int 또는 float 매치). 별칭은 런타임 값이 없으므로
     `x is Id` 가드로 우회할 수 없다는 점이 결정 근거였다.
  3. `.unbc`에 `PATTERN_TYPE = 8`을 가산 추가 — `VERSION`은 1 유지, 구 파일 호환.
     VM은 `world::class_tobj` + 기존 `is_obj`로 검사(상속·`__is__` 포함).
  4. Compile Mode(AOT)는 `TirPattern::Type/Or`과 신규 `unrt_type_is` 심볼로 동일
     검사를 생성한다. NRT 구현은 태그 기반 정확 비교라 클래스 상속·사용자 `__is__`
     의미는 재현하지 못한다 — 해당 프로그램은 `un build --embed`로 안내.
  5. `case v is T`(`NodeKind::Typed`)는 현행 유지 — 정적 narrowing 전용, 런타임无검사.
- **영향 파일**: `compiler.rs`, `bytecode/*`, `vm/mod.rs`, `typeck.rs`, `tir.rs`,
  `natcode.rs`, `unrt.rs`, `nrt/src/{obj,class}.rs`, `main.rs`, 한영 README,
  `docs/spec/BYTECODE.md`, `docs/guides/COMPILED_LANGUAGE.md`.
- **검증**: `int`→100 / `float`→-100.1 / 별칭 int·float 양쪽 / tuple 중첩 `(int,float)`→1,
  `(float,float)`→3. `un run` ≡ `un native` ≡ plain `un build` byte-identical.
## D-098 | bytes 값을 전용 Payload::Bytes 표현으로 전환 | Accepted

- **배경**: `native/bytes.rs:5-33`은 `bytes` 값을 `Payload::List` + `Int(0..=255)`로 흉내내는 임시 표현을 쓰고 있다. 어댑터 세 함수(`bytes_obj`/`as_bytes`/`is_bytes`)만 교체하면 나머지 native 코드는 그대로 새 표현을 쓰도록 설계돼 있다(32개 호출 지점).
- **결정**:
  1. `ObjData`/`Payload`에 `Bytes(Vec<u8>)` variant를 추가한다. VM과 NRT 양쪽에 동일하게 구현한다(`rust/src/runtime/obj.rs`, `rust/nrt/src/obj.rs`).
  2. `native/bytes.rs`의 어댑터 세 함수만 `Payload::Bytes`를 우선 사용하도록 교체한다. 편의 입력(Str / 0..=255 Int 리스트)은 유지한다.
  3. `fs.read_bytes`/`write_bytes`가 어댑터를 거치므로 자동으로 새 표현을 사용한다.
  4. **tag/JIT 계약**: `Payload::Bytes`는 `Payload::List`와 동일한 NObj tag를 유지하거나 새 tag를 명시적으로 할당해야 한다 — JIT tag 표는 `jit_cache.rs`가 단일 소스.
  5. byte-identical 게이트 필수: `bytes` 모듈 12함수 결과, `type(b)`/`repr(b)`, `fs.read_bytes` → `io.write` 등 4레인 비교.
- **영향 파일**: `runtime/obj.rs`(RT-03), `nrt/src/obj.rs`(NRT-06), `native/bytes.rs`(NAT-07), JIT tag 판정(JIT-04), 게이트(QA-09).
- **근거**: AGENTS.md 후속 지점, `native/bytes.rs:5-33` 계약 주석.
- **검증**: `scripts/test.ps1`, `differential.ps1`, `exe-diff.ps1`, `unbc-roundtrip.ps1 -IncludeNegative`, strict canary 5종.
### D-097 후속 정정 (2026-11-25, PATTYPE/NRTINHERIT 실측)
- VM의 타입 패턴(`Pattern::Type`)은 `class_tobj(name)` → `is_obj` → `Types::accepts`의 **이름 매칭**만 수행한다. `obj_is`의 `__is__` 디스패치는 non-TObj RHS에서만 도달하며 타입 패턴에서는 호출되지 않는다(`vm/mod.rs:1714-1720, 2087-2097`, `test/unit/runtime/classes.un:110-116`).
- 따라서 위 §4의 "`__is__` 포함" 기술은 실제와 달랐다. plain 미지원 분기에서 NRT가 `__is__`를 호출해 VM과 불일치(및 typed `__is__`에서 0xC0000005 crash)가 발생 → NRT의 `user_is` 디스패치 제거로 정렬.
- 상속客户关系도 VM `Types::accepts`가 부모를 포함하지 않아 `c is Base`가 false. `TirProgram.class_parents`는 수집하되 driver에 배선하지 않음(배선 시 plain만 true가 되어 byte-identical 위반).