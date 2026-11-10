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
