# ARCHITECTURE.md — Rust Un VM 설계

> **Historical design doc.** Written while the Rust VM was being ported alongside
> the C# reference. The design was kept (modulo documented divergences), and the
> C# reference has since been removed. Current state is documented in
> `docs/README.md` and `docs/rust-migration/PROGRESS.md`.

Rust 프로젝트 `rust/`는 C# Reference Implementation(`src/`, 이후 제거됨)의 동작을 재현하는
**바이트코드 컴파일러 + 스택 기반 VM**이다. C# 코드는 수정하지 않고 기준선(reference oracle)으로만 쓴다.

## 1. 파이프라인

```text
.un source
  -> source::Source        (CRLF/CR -> LF 정규화, IgnoredNewLines)
  -> lexer::lex            (Indent/Dedent/NewLine 포함 토큰 스트림)
  -> parser::parse         (Node 트리)
  -> desugar::desugar      (복합 대입 확장, if/elif -> IfCase/ElIf 정규화)
  -> optimize::optimize    (상수 폴딩, fixed point <= 64 pass)
  -> compiler::compile     (Proto + .unbc 모듈)
  -> vm::run               (스택 VM 실행)
```

- **Desugarer/Optimizer는 AST 단계에 남는다.** 컴파일러는 desugared+optimized AST만 소비하고
  desugar/optimization을 재구현하지 않는다(재구현하면 C#와 동작이 갈린다).
- 런타임에 필요한 동적 컴파일(`eval`류, dynamic import)은 같은 컴파일러를 재사용한다.

## 2. 크레이트/모듈 레이아웃

```text
rust/
├─ Cargo.toml               (single crate `unvm`, bin `unvm`)
└─ src/
   ├─ main.rs               CLI: tokens / ast / run / compile / dis
   ├─ lib.rs
   ├─ source.rs             Source, line/column/line-text, span
   ├─ token.rs              TokenType, Token, Display(Reference enum name)
   ├─ lexer.rs              Phase 1
   ├─ ast.rs                NodeKind, Node, Const
   ├─ parser.rs             Phase 2
   ├─ desugar.rs            Phase 3
   ├─ optimize.rs           Phase 4
   ├─ compiler/
   │  ├─ mod.rs             compile(module) -> Program
   │  ├─ proto.rs           Proto, Instr, Opcode, Constant, SourceMap
   │  ├─ scope.rs           compile-time lexical scope, slot/upvalue 할당
   │  └─ emit.rs            instruction emission, jump patching
   ├─ vm/
   │  ├─ mod.rs             VM loop
   │  ├─ frame.rs           Frame, Context
   │  ├─ exec.rs            opcode dispatch
   │  └─ trace.rs           call frames, trace 출력
   ├─ runtime/
   │  ├─ obj.rs             Obj 계층(enum + match dispatch)
   │  ├─ scope.rs           런타임 Scope (symbols/slots/parent)
   │  ├─ primitive.rs       Int, Float, Str, Bool, Date
   │  ├─ collections.rs     List, Dict, Set, Tup, Json, Spreads
   │  ├─ iters.rs           Iter, Range, Counter, Repeat, Reverse
   │  ├─ function.rs        Fn, LFn, NFn, Arg, Future, Property, FieldRef, Template
   │  ├─ types.rs           UnType/BaseType/CollectionType/UnionType, TypeTable(intern)
   │  ├─ objects.rs         Obj(plain), Stru, TObj, Enu
   │  ├─ flow.rs            ReturnFlow/BreakFlow/SkipFlow/PropagateFlow에 해당하는 VM 제어
   │  ├─ error.rs           LangError, Panic, Err 객체, exit code
   │  ├─ module.rs          Global: module registry, import, cache, circular 검사
   │  └─ io.rs              IFileSystem(Physical/Memory), stdout 버퍼
   ├─ native/
   │  └─ mod.rs + modules   `io`, `fs`, `re`, `math`, `iter`, ...
   └─ bytecode/
      ├─ writer.rs           .unbc 직렬화
      └─ reader.rs           .unbc 복호화/검증
```

Phase 순서가 문서화되어 있고(`ROADMAP.md`), 각 Phase는 테스트 통과 후 다음 Phase로 간다.

## 3. 런타임 값 모델

C#는 `Obj` 클래스 계층 + virtual dispatch다. Rust에서는 **`Obj`를 enum + `Rc<RefCell<>>` 셀**로 표현한다.

```rust
pub type Cell = Rc<RefCell<Obj>>;

pub enum Obj {
    None, Null,
    Int(i64), Float(f64), Str(String), Bool(bool), Date(i64),
    List(Rc<RefCell<List>>), Dict(Rc<RefCell<Dict>>), Set(Rc<RefCell<Set>>),
    Tup(Rc<RefCell<Tup>>), Json(...), Spreads(...),
    Iter(...), Property(Rc<RefCell<Property>>), FieldRef(Rc<RefCell<FieldRef>>),
    LFn(Rc<RefCell<LClosure>>), NFn(Rc<RefCell<NFn>>),
    Obj(Rc<RefCell<UserObj>>),     // plain object + Members + Type + Super/Parents
    TObj(TypeId), Enu(...), Future(...), Err(...),
}
```

- 값 타입(Int/Float/Str/Bool)은 `Obj`의 직접 변형. C#의 `Int` intern(-5..256)과
  `Str` intern pool(max 10000 entry / 1000 char)은 **identity(`is`)가 관측**되므로
  intern 테이블을 런타임에 유지한다.
- 가변 컨테이너(List/Dict/Set/Tup/user object/Property/Fn)는 `Rc<RefCell<_>>`.
  `Copy()`는 alias(같은 `Rc` 반환), `Clone()`은 deep/shallow로 C# semantics를 재현한다.
- `self`/`super`/parent chain은 `Weak<RefCell<_>>` 역참조로 cycle을 피한다.
  다만 **semantic 기준은 C#의 `Obj.Self = receiver` 할당(공유)이므로**,
  Rust에서도 self는 " receiver의 `Rc`를 참조"로 두고, reference cycle은
  `Super`/`Parents`를 `Weak`로 저장해서 해소한다. (Phase 6에서 검증)

## 4. 매직 메소드 dispatch

C#의 `TryMethod("__add__", args)` + super fallback + fallback Err 구조를 그대로 재현한다.

```rust
fn binary(&mut self, op: Op, left: &Obj, right: &Obj) -> Obj {
    if let Some(m) = lookup_dunder(left, op.dunder()) { return call(m, [right]); }
    if valid_super(left) { return binary(op, &super_of(left), right); }
    Err(format!("unsupported operand type(s) for {}: '{left_type}' and '{right_type}'", op.sym()))
}
```

`Eq`, `Gt`, `LtOrEq`, `GtOrEq`, `NEq`, `Xor`, `Not`는 C#의 fallthrough 특례(6.1절)를
별도 함수로 구현한다(단순 dunder 패턴이 아니다).

속성 조회/설정 순서(7.2절)는 `get_attr`/`set_attr`에서 순서를 정확히 재현해야 한다.
`Found:` 라벨에서 `Fn`이면 `self`/`super`를 심는 시점도 같다.

## 5. 스코프/클로저 모델

### 5.1 런타임 Scope

```rust
pub struct Scope {
    pub symbols: HashMap<String, usize>,
    pub slots: Vec<Obj>,
    pub parent: Option<Rc<RefCell<Scope>>>,
    pub is_function_scope: bool,
}
```

`get/set/set_local/set_local_or_declare/declare/remove/contains_key/contains_key_in_top`은
`Scope.cs`의 라우팅을 1:1로 옮긴다.

- `set(key, v)`: 자기 symbols → slot 갱신. **parent chain에 key가 있으면** parent로 전파.
  없으면 자기 scope에 신규 slot.
- `set_local_or_declare`: 자기 scope에만.
- 대입 라우팅(`BindName`): FieldRef면 `.Value` 갱신, `localOnly || scope.is_function_scope`이면
  `set_local_or_declare`, 아니면 `set`.

### 5.2 Context

C#의 `Context`는 call 단위로 생성되고 `Frames`는 클로저를 통해 공유된다.

```rust
pub struct Context {
    pub scope: Rc<RefCell<Scope>>,
    pub source: Rc<Source>,
    pub frames: Rc<RefCell<Vec<FrameInfo>>>,   // 공유: trace에 사용
    pub defers: Vec<Obj>,      // per-context LIFO
    pub usings: Vec<Obj>,      // per-context LIFO
    pub call_depth: usize,
}
```

`LClosure`는 `Context` 전체(`Rc<RefCell<Context>>`)를 캡처한다. C#가 `Closure = context`로
Context를 잡는 것과 동일하게, **nested 함수는 enclosing Context의 스코프 객체를 참조로 공유**한다.
이것은 클로저의 변수 변경이 enclosing에 즉시 보이는 semantics의 원천이다.

### 5.3 compile-time slot 할당

Lexical scope chain은 컴파일 타임에 알려진 구조다. 모든 선언(`Set`이 자기 scope에 slot을 만든다)은
정적이며, `for`의 `localOnly`도 compile-time 규칙이다.

- 각 compile scope는 `(names: name -> slot)`를 가지고, slot index는 런타임 Scope의 slot index와 대응한다.
- enclosing scope의 slot 접근은 **upvalue가 아니라 scope chain**으로 런타임에 해석한다
  (C#가 parent chain lookup이므로). 성능상 hot path은 **cache된 (scope depth, slot) pair**를
  instruction operand로 두고, VM이 scope chain을 따라 `depth`번 parent로 이동한다.
  런타임에 scope chain이 변형되는 경우(match/catch의 scope 전환)는 depth cache가 깨질 수 있으므로
  **depth cache는 같은 function body 안에서만 유효**하고, 함수 경계를 넘으면 chain lookup으로 복귀한다.
  → 이 최적화 안전성 검증은 Phase 11 differential test에서 확인한다. 안전하지 않으면 plain chain lookup.
- 식별자 해석 순서: scope chain → global registry → class registry(TObj) → `undefined variable '{name}'`.

## 6. VM

- 연산자 스택 `Vec<Obj>`, call frame `Vec<Frame>`.
- `Frame { proto: Rc<Proto>, ip: usize, scope: Rc<RefCell<Scope>>, context: Rc<RefCell<Context>> }`
- call 시 `OP_CALL`은 `FrameInfo(name = call node 원문, source, start, length)`를 frames에 push하고
  `finally` semantics는 VM이 frame exit 시 pop한다(defers/usings flush 후).
- 제어 흐름(`return`/`break`/`skip`)은 C#의 예외에 대응하는 **명시적 VM 이벤트**로 표현한다:
  `Flow::Return(Obj)`, `Flow::Break`, `Flow::Skip`, `Flow::Propagate(Obj)`.
  `try`는 VM 내 `catch` 영역 테이블로 구현하고, `PropagateFlow`는 `catch (Error)`에 잡히지 않는다.
- 재귀 한도 `MAXRECURSIONDEPTH = 1000` → `maximum recursion depth exceeded` (`Err`).
- call depth 초과로 native 스택이 고갈되는 케이스(`depth_limit_crash.un`, `property_recursion.un`)의
  exit code 재현은 DECISIONS.md 참조.

## 7. 컴파일러

- 각 `Proto`는 `code: Vec<Instr>`, `constants: Vec<Constant>`, `locals: Vec<String>`,
  `upvalue/scope metadata`, `source_map: Vec<(start,length)>`(instruction별),
  `functions: Vec<Proto>`를 가진다.
- 함수는 `OP_PROTO` 테이블에Proto로 저장되고, `OP_MAKE_FUNC`이 실행 시점의 `Context`를 캡처해
  `LClosure`를 만든다(annotations은 desugar된 `@` call 시퀀스로 컴파일; reverse 순서 유지).
- classes/enums는 선언 시 `Global.SetClass`에 등록하는 런타임 효과가 있으므로
  컴파일 결과에 `OP_MAKE_CLASS`/`OP_REGISTER_CLASS` 같은 명령을 남긴다.
- `use`/`import`는 런타임 effect이며 module registry와 cache, circular 검사를 VM이 수행한다.

## 8. 네이티브 바인딩

C#는 reflection으로 `[NativeModule]`/`[Native]`를 찾아 `NFn`을 만든다. Rust에서는 명시적 등록 테이블로 대체한다.

```rust
pub struct NativeDef {
    pub module: &'static str,
    pub name: &'static str,
    pub args: Vec<Arg>,
    pub example: &'static str,
    pub description: &'static str,
    pub f: fn(&mut Vm, &Tup) -> Obj,
}
```

- `Global.Init`에 해당하는 `registry::init()`이 모든 `NativeDef`를 등록하고,
  `originalClasses[type][name]` 맵(네이티브 타입의 멤버)도 같은 테이블에서 만든다.
  `Global.TryGetOriginalValue(Type.Name, name)` 경로가 7.2절 조회 순서에 필요하다.
- 인자 lookup은 C#와 같게: `[ArgInfo.Name]` 우선, 없으면 파라미터 이름, `Err`면 positional fallback,
  `[Self]`는 `args["self"]`.
- sandbox 허용 목록(`PlaygroundRunner.DefaultAllowed`)은 `Global.IsAllowed`와 같은 panic 메시지를 낸다.

## 9. .unbc serialization

`bytecode/`는 `Program`을 `.unbc`로 쓰고 읽는다. 레이아웃과 필드 인코딩은 `BYTECODE.md`에 정의한다.
Phase 17에서 `unvm compile file.un -o file.unbc`와 `unvm run file.unbc`가 동작한다.
VM은 `.unbc` 로드 시 source mapping을 유지해야 trace/오류 위치가 유지된다.

## 10. differential testing

- harness: `scripts/rust_test.ps1` + `scripts/diff_test.ps1`
  1) `scripts/test.ps1`과 같은 MANIFEST 목록을 Rust VM으로 실행하고 exit code/stdout을 C#와 비교.
  2) negative manifest는 exit code + stderr substring 비교.
  3) AST parity는 `unvm ast` dump가 C#에 없으므로 **parse 성공/오류 문자열 일치**로 검증(Phase 2 완료).
- 기준선: Positive 33 files / 1265 passed / 0 failed / 0 crashed, Negative 42 matched, Known 1.
- `KnownFailures.txt`의 6개는 C# 동작 기준과 Rust 동작이 **같은** 것을 기대한다(Reference 기준 통과).

## 11. 비-JIT 원칙

초기 단계에 JIT는 없다. `Proto`의 bytecode는 interpreter loop로 실행한다.
JIT을 추가하더라도 **동작 기준은 VM semantics**로 고정하고, JIT는 performance layer로만 붙인다.
