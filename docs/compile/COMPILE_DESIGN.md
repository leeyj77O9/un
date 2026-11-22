# COMPILE_DESIGN.md — Un Compile Mode 전체 설계

> 기준: 2026-11-23, Phase 1 (아키텍처 분석) 완료 후 작성.
> 이 문서는 설계의 기준 문서이며, 구현이 진행되면서 실제 코드와 어긋나는 부분이
> 생기면 코드를 기준으로 갱신한다. 함께 보면 좋은 문서: `COMPILE_PROGRESS.md`,
> `COMPILE_FINDINGS.md`, `COMPILE_TESTS.md`, `COMPILE_BENCHMARK.md`.

## 1. 목표와 범위

Un은 하나의 언어를 두 가지 실행 모드로 제공한다.

| | Interpreter Mode (기존) | Compile Mode (신규) |
|---|---|---|
| CLI | `un run <file.un>` | `un native <file.un>` (+ `un check`) |
| 타입 검사 | 없음 (동적, gradual annotations는 문서화용) | **정적 타입 검사 강제** |
| 실행 | 레지스터 VM (+ Cranelift JIT, Int 부분집합) | 레지스터 VM (동일) + TIR 네이티브 코드 |
| 호환성 | 기존 그대로 | 인터프리터와 동일 언어 명세, 더 엄격한 검사 |

핵심 원칙:

1. **기존 인터프리터는 단 한 줄도 의미를 바꾸지 않는다.** 컴파일 모드는 추가이다.
2. **두 모드는 같은 AST, 같은 Optimizer를 공유한다.** 차이는 분석/코드젠 단계부터이다.
3. **정적으로 확정된 코드만 네이티브로 내린다.** 확정이 안 되는 코드는 인터프리터로
   내려간다(런타임 bail이 아니라 *컴파일 시점*에 결정) — 동적 디스패치를 정적
   디스패치로 가장하지 않는다.
4. **동적 타입 경계를 명시적으로 관리한다.** `any` 타입과 인터프리터 fallback이
   그 경계이다. 경계에서의 boxing/unboxing 비용은 측정 대상이다.

## 2. CLI 설계

기존 CLI(`run`, `compile`, `tokens`, `ast`, `desugar`, `opt`, `dis`, `dis-reg`,
`help`)를 분석한 결과, `un compile`은 이미 **`.unbc` 바이트코드 산출**을 의미한다
(`main.rs:318-348`). 기존 규칙을 우선해 그 의미를 유지하고, 컴파일 모드에는
새 동사를 추가한다.

| 명령 | 동작 |
|---|---|
| `un check <file.un>` | 파싱→optimize→**정적 타입 검사**만 수행. 성공 시 exit 0, 타입 오류 시 오류를 출력하고 exit 1. |
| `un native <file.un>` | 정적 타입 검사 후 실행. 정적으로 확정된 core 함수는 TIR→Cranelift 네이티브 코드로 등록되어 VM이 호출하고, 나머지는 기존 인터프리터(+ `UV_JIT`에 따른 기존 JIT)가 실행. |
| `un compile <file.un>` | **변경 없음** (.unbc 산출) |
| `un run <file.un>` | **변경 없음** (인터프리터) |

`un help`에 새 명령을 추가한다.

## 3. 파이프라인

```
Source ──► Lexer ──► Parser ──► Optimizer (기존 그대로)
                                   │
                                   ▼
                        typeck (신규)  ──► 타입 오류 시 여기서 종료 (un check)
                                   │  TypedProgram (시그니처 테이블 + 지역 타입 맵)
                                   ▼
                            tir (신규)   ──► TIR 검증기
                                   │
              ┌────────────────────┴────────────────────┐
              ▼ core 함수 (정적으로 확정)                 ▼ 나머지 함수
       natcode (신규, Cranelift)                  Register VM 인터프리터
              │                                    (+ 기존 jit_bc JIT)
              └──────────── VM 이 통합 ────────────┘
```

핵심 결정:

- **Typed IR (TIR)은 AST의 타입 확정 버전**이다. 별도의 SSA/기본블록 IR은 만들지
  않는다 — 기존 Un은 구조화된 제어 흐름(if/while/for/match)을 가지므로, TIR도
  구조화된 tree IR로 유지하고 검증기로 타입 일관성을 확인한 뒤 Cranelift로 내린다.
- **네이티브 ABI는 기존 JIT와 동일하다**: hidden result pointer + `param_tags` +
  positional i64 args. TIR 진입점은 `jit_cache`에 `(program.id, proto_index)`로
  등록되어 VM 경로(`try_call_jit`)를 그대로 탄다. **VM에는 숨겨진 호출 규약
  분기만 추가하고 실행 의미는 바꾸지 않는다.**
- **`param_tags`의 상위 비트를 확장해 인자 종류를 전달**한다
  (bit p=IntVec 핸들, 기존 그대로 / bit 32+p=Float 비트, bit 40+p=Bool).
  TIR 진입점은 이 비트들을 읽어 f64/bool로 해석하고, 기대한 타입과 다르면
  TAG_BAIL을 써서 즉시 반환한다. VM은 bail을 보고 인터프리터로 넘긴다.
  즉 **런타임에라도 타입이 틀릴 수 있는 경우는 항상 인터프리터로 안전하게
  빠진다.** 인터프리터 의미가 항상 이긴다.
- TIR 함수가 같은 모듈의 다른 TIR 함수를 호출할 때는 FFI를 거치지 않고
  Cranelift 내부 call을 직접 생성한다 (정적 타입 직접 호출 최적화).

### Register VM과 JIT의 역할 (기존 그대로 유지)

- VM: 슬롯 기반 로컬, 레지스터는 임시값, `Op` dispatch.
- jit_bc: unbc를 읽어 Int 부분집합을 Cranelift로 번역. 의미 보존을 위해
  bail(TAG_BAIL) → 인터프리터.
- Compile Mode는 여기에 **세 번째 소비자(TIR native backend)**를 추가할 뿐이다.

## 4. 정적 타입 시스템 (`typeck` 모듈)

### 4.1 타입 표현 (`Ty`)

```rust
enum Ty {
    Int, Float, Bool, Str, None_,
    List(Box<Ty>), Dict(Box<Ty>, Box<Ty>), Set(Box<Ty>), Tuple(Vec<Ty>),
    Union(Vec<Ty>),     // 정규화: 정렬+중복 제거, any를 포함하면 any로 축약
    Any,                // 동적 경계 (명시적 dynamic 타입)
    Class(Rc<str>),     // 클래스 선언 이름
    Func { params: Vec<Ty>, ret: Ty },   // 1급 함수 값 타입
    Never?              // (도입하지 않음. 불일치 오류에서 Any로 흡수)
    Unknown,            // 추론 실패 자리 (오류 복구용, 사용자에게 노출되지 않음)
}
```

기존 런타임 `Types`(runtime/types.rs)는 문자열 기반 Union 표현이다. **타입
시스템을 중복 구현하지 않기 위해**, 런타임 `Types`는 그대로 두고(오류 경로와
`is`/`in` 판정에 이미 사용 중), compile mode의 `Ty`는 정적 전용 표현으로 둔다.
두 표현 사이의 변환은 코드젠/런타임 경계가 아닌 분석 내부에서만 일어난다.

### 4.2 어노테이션 문법 (기존 재사용)

파서는 이미 다음을 파싱해 둔다 (검증 완료):

- 매개변수 타입: `fn add(a: int, b: int)` → `Parameter` → `Typed` → `Identifier(Const::Str("int"))`
- 컬렉션: `list[int]`, `dict[str, int]` → Identifier with Const::Str("list[int]")
- union: `int | str` → Identifier with Const::Str("int | str")
- 반환 타입: `fn f() -> int` → Function children에 Identifier 추가 (Const::Str("int"))

즉 타입 어노테이션은 **문자열**로 보관되므로, typeck는 이 문자열을 재파싱하여
`Ty`로 변환한다 (`parse_type_text`). 별도의 AST 타입 노드를 만들지 않는다.

지원 문법 (v1):

```text
int | float | bool | str | none | any
list[T] | dict[K, V] | set[T] | tuple[A, B, ...] | tuple[T, ...] (homogeneous)
T | U | ... (any arity)
```

### 4.3 함수 시그니처 정책 (기본 정책: 명시적 어노테이션 필수)

compile mode에서 **모든 함수는 매개변수 타입과 반환 타입을 명시**해야 한다.

```un
fn add(a: int, b: int) -> int
    -> a + b
```

다음은 컴파일 모드에서 오류:

```un
fn add(a, b)        # 오류: parameter 'a' requires a type annotation
    -> a + b        # 오류: function 'add' requires a return type annotation
```

근거와 완화:

- 반환 타입 **추론은 기술적으로 가능**하지만(함수 본문을 先 수집 후 후순위로
  추론), 초기 버전에서는 **명시를 기본 정책**으로 한다. 순환 참조/상호 재귀에서
  추론 순서가 결과를 바꾸는 경로를 없애고, 시그니처를 계약으로 고정하기 위함이다.
  단, 반환 타입 어노테이션이 `none`인 함수(`-> none`)는 반환 값이 없어도 된다.
- 어노테이션이 전혀 없는 함수(기존 Un 스타일)는 compile mode에서 **오류**이다.
  단, 이 규칙은 "모듈 단위로" 적용되며, **인터프리터 모드의 동작은 바꾸지
  않는다.** 기존 테스트는 전부 어노테이션이 없으므로 `un run`은 전부 통과하고,
  `un native`/`un check`에서는 거부되는 프로그램이 존재한다 — 이것이 명시적
  문서화 대상 차이이다 (README.md / docs/README.ko.md에 기록).
- 예외: `fn` 키워드 없이 만들어지는 람다(`fn(a, b) -> a + b`, 즉 Lambda 노드)는
  **동적 값**으로 취급한다 (`Any`). 호출 검사는 하지 않는다. 인터프리터와 동일한
  호출 규칙을 공유한다.
- **네이티브 함수(`io.write` 등)는 레지스터된 시그니처 테이블**을 가진다.
  v1은 사용 빈도가 높은 builtin/math/iter/time/fs에 대해 시그니처를 내장하고,
  나머지 네이티브는 `(Any, ...) -> Any`로 가정한다 (동적 경계).

### 4.4 지역 변수 추론

- 첫 할당(`x = expr`)에서 `expr`의 타입으로 변수 타입을 확정한다.
- **재할당 정책: 타입은 최초 확정 후 불변.** 같은 타입(또는 union의 자기 부분
  집합)으로의 재할당만 허용하고, 다른 타입으로의 재할당은 오류로 처리한다.
  (`x = 10` 이후 `x = "hello"` → 오류. `x: int | str = 10` 이후 `x = "hello"` → 허용)
- 선언 없이 사용한 변수, 없으면 Scope 밖 참조는 오류.
- `x += 1` 등의 복합 대입은 `x = x + 1`로 해석하여 동일하게 검사.

### 4.5 이항/단항 연산 규칙

| 연산 | 결과 |
|---|---|
| `int + int` `-` `*` `//` `%` | `int` |
| `float op float`, `int op float`, `float op int` | `float` (int→float 승격) |
| `str + str` | `str` (그 외 str 연산은 오류) |
| `bool and/or` (및 `not`) | `bool`; `and`/`or`의 피연산자는 bool |
| 비교(`< <= > >= == !=`) | 같은 타입(또는 int/float 혼합)에서 `bool`; int/float 혼합 비교 허용 |
| `==`/`!=` 다른 타입 | 오류 (static mismatch). 단 `any`가 끼면 `bool` (동적 경계) |
| `int & int`, `|`, `^`, `<<`, `>>` | `int` |

정적 타입이 맞지 않는 연산(`int + str`)은 compile mode에서 **오류**이다.
인터프리터에서는 런타임 오류/문자열 연결 등 기존 의미를 유지한다.

### 4.6 Union Type과 동적 경계

```un
T = int | str              # 타입 별칭 (top-level 대입이 타입 표현식으로 파싱되면 별칭)
fn process(value: T) -> T
    -> value
```

- 별칭: top-level statement가 `Identifier = <타입 표현식>` 형태이고 우변이
  타입 문법으로만 구성되면(known type identifiers, `|`, `list[..]` 등) 별칭으로 등록한다.
  런타임 변수로는 등록하지 않는다(타입 이름은 런타임 식별자가 아님).
- Union 값을 Union 매개변수에 전달: 허용. Union 값에 대해 구체 타입 매개변수에
  전달: 부분집합 검사(members가 전부 target에 속해야 함) 통과 시 허용.
- Union 값의 멤버 접근(`value.x`): 모든 멤버가 그 멤버를 가지면 허용 아니면 오류.
  union member access는 `match` dispatch로만 해소 가능한 것으로 하고,
  match 내 좁히기(narrowing)는 지원한다. **exhaustive checking도 구현됨** —
  bool/none는 해당 arm(또는 wildcard), 그 외 타입은 `_` wildcard arm을 필수로
  검사한다 (`COMPILE_FINDINGS.md` F-014, `test/compile/matches.un`).
- `any`: 모든 타입이 any로 변환 가능(implicit boxing), any에서 구체 타입으로의
  변환은 **명시적 변환 함수**(`int(x)`, `float(x)`, `str(x)`, `bool(x)`)로만 가능.
  그 외 any→구체 타입 대입은 오류. 이것이 동적 경계의 boxing/unboxing 규칙이다.
  (단, TIR 진입점의 TAG_BAIL 안전망은 모든 경우를 인터프리터로 되돌리므로
  프로그램 의미는 절대 잘못되지 않는다.)

### 4.7 클래스

- `class Point(x: int, y: int)` — header 매개변수는 annotated 필드로 등록.
- `class Foo` 본문의 `x: int = 3` — annotated default를 필드 타입으로 등록.
  `x: int` (bare) — 필드 타입은 int, 초기값 none(기존 의미 유지).
- 메서드: `fn len() -> int` — `self`는 암묵적 첫 인자, 타입 `Class("Point")`.
  나머지 매개변수/반환 타입 어노테이션 필수 (함수와 동일 규칙).
- 필드 접근(`p.x`): 클래스 선언에서 타입을 조회. 알 수 없으면 `any`.
  `self.x`는 slot 0(클래스 메서드 ABI)에 `Class(name)` 바인딩.
- 상속(`class Child: Base`): 필드 조회는 상속 체인을 순회
  (compile-time single pass, MRO는 Un의 실제 순서를 미러 — 런타임과 동일한
  parent 순서). §5의 core 제외 목록은 Vm 모드(`un native`)의 in-process TIR
  대상이다. **plain `un build`에서는 클래스/상속이 `libnrt.a`로 네이티브
  실행된다** (§10, `OBJ_MILESTONES.md`). 타입 검사는 수행한다.

### 4.8 스코프 / 선언 / 참조

- 함수 스코프: 매개변수 + 블록 내 할당. 블록은 스코프를 만들지만(컴파일러와
  동일한 slot 의미론 반영), 타입 환경은 lexical chain으로 관리.
- 사용 전 할당: 오류 (`x = 1` 이전에 `x` 참조 → "use before assignment").
  `if/else` 양쪽에서 정의되는 변수는 합집합 지점에서 join (both 정의 → 타입
  meet; one쪽만 → 오류? 아니면 else 경로에서 Unknown → 이후 사용 오류).
  v1: **양쪽 모두 없으면 오류, 한쪽만 있으면 오류**(보수적), 단
  `if/elif/else`가 모두 값을 정의하면 union으로 join. while 루프는 0회
  실행 가능하므로 루프 본문에서만 정의된 변수는 루프 밖에서 사용 불가.

### 4.9 검사 범위 (v1)

검사하는 것:
함수 선언/호출(개수·타입), 반환 타입, 지역 변수 불변, 연산, 조건식 bool 강제,
클래스 멤버/메서드, 모듈 별칭 사용, 정의되지 않은 식별자, 잘못된 반환.

검사하지 않는 것(v1, `any`로 흡수, 문서화):

- 첫 번째 클래스가 아닌 다중 상속의 정밀 필드 타입 — 체인 순서대로 첫 발견 사용
- Union narrowing 범위 제한 (`match` exhaustiveness는 구현됨 — §4.6, F-014)
- 리스트/딕셔너리 **리터럴 요소 타입 추론** — `[1, 2, 3]` → `list[int]`로 추론
  (동질이면). 혼합 → `list[any]`.
- 튜플 리터럴 `(1, "a")` → `tuple[int, str]`
- 제너릭: 초기 v1 설계에는 문법이 없었으나 **현재는 구현됨** —
  `fn identity[T](x: T) -> T`, `class Box[T]`, 호출부 추론·명시적 `identity[int](42)`·
  bound `[T: numeric]`를 지원하고 TIR이 monomorphization한다 (`tir.rs`, §10).

## 5. TIR (Typed IR)

### 5.1 구조

```rust
struct TypedProgram {
    fns: Vec<TypedFn>,            // 모듈 순회 순서
    ...
}
struct TypedFn {
    name: String,
    ret: Ty,
    params: Vec<(String, Ty)>,    // self 포함 (클래스 메서드)
    locals: Vec<Ty>,              // 최종 slot → Ty
    body: Vec<TypedStmt>,
    native_core: bool,            // natcode 대상 여부
}
enum TypedStmt { ... }
enum TypedExpr { Int(i64), Float(f64), Bool(bool), Str(Rc<str>), None_, Local(u16),
    Bin(BinOp, Box<TypedExpr>, Box<TypedExpr>), Cmp(...), Unary(...),
    Call { callee: Callee, args: Vec<TypedExpr> }, ... }
```

검증기(`tir::validate`)는 TIR을 순회하며 모든 피연산자 타입이 선언 slot/expr
타입과 일치함을 확인하고, `native_core` 함수는 core 제약(아래)을 재확인한다.
검증 실패는 내부 버그이므로 panic이 아니라 오류로 보고한다.

### 5.2 Native core 제약 (v1에서 네이티브로 내리는 함수)

하나라도 어긋나면 그 함수는 **컴파일 타임에** core가 아니게 되어 인터프리터로
실행된다 (bail-at-compile-time, not at runtime):

- 매개변수/반환: `int`, `float`, `bool` 만 (none/class/str/list/union/any 불가)
- 지역 변수: `int`, `float`, `bool` 만
- 연산: 정수/실수 산술·비교·비트연산, bool 논리연산, 단항 `-`/`not`
- 문: 대입, if/elif/else, while, `for i in iter.range(a, b[, step])` (a,b,step은 int),
  `break`, `skip`(=continue), return
- 호출: 같은 모듈의 core 함수만 (직접 호출로 lowering; 재귀 포함)
- 클래스 메서드, match, try, go/wait, defer, list/dict/str 연산, 네이티브 호출,
  FString 등은 core 제외

Core가 아닌 함수는 인터프리터가 실행한다. 기존 `jit_bc`가 그 함수를 네이티브로
컴파일할 수 있으면 그렇게 되고(기존 동작), 아니면 인터프리터 bytecode 실행이다.

이 core 제약은 Vm 모드(`un native`)의 in-process TIR 네이티브 대상에 관한 것이다.
**plain `un build`는 이 범위 밖의 클래스/상속·예외·Str·list/dict·closure·기본값
인자·단순 모듈까지 `libnrt.a`로 네이티브 실행한다** (§10, `OBJ_MILESTONES.md`).

### 5.3 호출 규약

기존 JIT와 동일한 hidden-ptr ABI + `param_tags` 상위 비트 확장:

```text
out: *mut i64  // 16바이트 (value, tag)
arm0: param_tags: i64  // bit p=0/1: arg p = Int(0)/IntVec handle(1) (기존)
                       // bit 32+p: arg p = Float bits
                       // bit 40+p: arg p = Bool (0/1)
args...: i64 (Int value / Float bits / Bool / Rc 포인터)
```

반환 tag: `0=Int, 1=Float(bits), 3=Bool, 4=None, 2=Bail` (2는 진입 가드 불일치
시 설정되어 인터프리터 재실행). 기존 TAG 값과 동일.

core 함수는 진입 시 **가드를 한 번만** 둔다: `param_tags`의 Float/Bool 비트와
기대 타입을 대조하고, Int 인자에 대해선 태그가 Int이므로 무조건 수용. 불일치 시
Bail → VM이 인터프리터로 넘긴다. 가드 비용은 인자당 1~2개 비교.

TIR 내부 호출은 완전한 정적 타입 직접 호출이므로 태그 검사 없음.

## 6. Lowering과 코드 생성 (`natcode`)

- TIR → Cranelift IR (cranelift-codegen 0.137.1, 기존과 동일 버전).
- int → i64, float → f64, bool → i8(또는 i64 — Cranelift 레지스터 핸들 차원에서
  i8로 저장, 반환 시 0/1로 확장).
- 변수는 `Variable` + `Variable::declare_var`/SSA 스택(cranelift-frontend 활용,
  jit_bc.rs가 사용하는 것과 동일 패턴).
- if/while/for-range → 구조화된 블록 + conditional br/br. break/skip은
  루프 헤더/탈출 블록으로 br.
- 상수 폴딩/전파: 기존 Optimizer가 AST 단계에서 이미 fold한다. TIR 단계에서는
  **semantic check 이후 죽은 코드 분기**를 한 번 더 제거(상수 조건 if의 dead
  branch 제거)하고, 나머지 최적화(공통 식 제거, 루프 최적화, escape analysis,
  allocation elimination, 경계 검사 제거)는 구체 IR 구조가 확정되는 대로
  COMPILE_PROGRESS.md에 단계적으로 반영한다. v1에서 구현하는 최적화:
  1. 상수 folding/전파 — 기존 optimizer 재사용
  2. 죽은 분기 제거 — typeck의 상수 조건 정보 활용
  3. 정적 디스패치 — core 내부 직접 호출 (태그 검사 제거) ← **본 설계의 핵심**
  4. 레지스터 할당 — Cranelift allocator에 위임
  나머지(CSE, 루프 최적화, 이스케이프 분석 등)는 IR이 충분히 안정된 뒤.

## 7. 메모리와 런타임

- core 네이티브 함수는 **unboxed i64/f64/bool로만 동작**한다 — Object 할당 0.
- TIR 진입 가드가 태그를 확인하므로, 동적 값이 core로 새는 일은 없다.
- `none`/`any`/class/list 등은 core가 아니므로 기존 Rc/RefCell Object 런타임이
  그대로 담당. **Object 시스템은 제거하지 않고, core 경로에서만 우회**한다.
- 두 세계의 왕복은 진입/이탈 가드(값 태그 변환, 새 Obj 할당)에서만 일어난다.
- GC는 기존 전략(Rc + 풀 + 인터프리터)을 그대로 둔다. 소유권 기반 관리 도입은
  범위 외.

## 8. 오류 메시지와 테스트 방향

- 모든 타입 오류는 `LangError`(source line + caret + "error: message")로 출력,
  `un check`는 exit 1.
- 다중 오류 수집: 최대 50개까지 수집 후 출력 (fail-fast 아님).
- 테스트:
  - `test/compile/` — 정상 실행 스크립트 (assert 기반, `un native`로 실행,
    같은 파일을 `un run`으로도 실행해 stdout 비교 — cross-validation)
  - `test/compile/errors/` — 반드시 `un check` 오류인 스크립트 + manifest
    (expected exit 1 + stderr substring) — negative 테스트와 같은 패턴
  - `scripts/compile-check.ps1` — 위 둘 + cross-check를 실행하는 runner
  - bench: `bench/compile/*.un` (typed 버전) + `bench/rust/` (Rust 레퍼런스,
    `rustc -O`) 추가, `scripts/bench.ps1`에 `compile` 그룹 추가

## 9. 끝까지 하지 않는 것 (명시적 보류)

- Standalone native executable (`.exe` 산출): **구현됨** — `un build`가 소스 →
  `.obj` → 링크를 수행하고, plain build는 `rust/nrt/`의 `libnrt.a`(독립 런타임)를
  링크해 VM 없이 약 5 MB exe를 만든다. 클래스/상속·예외·Str·list/dict·제너릭 타입
  컬렉션·closure·기본값 인자·단순 모듈·byte-identical traceback을 지원한다.
  동적 `Any`/union, 시간 시드 `random`, 모듈 최상위 부수 효과는 `un build --embed`가
  필요하다 (`OBJ_MILESTONES.md`).
- union exhaustive checking의 정밀화(구체 분해·`is` narrowing 기반), union member
  access 일반화 (제너릭 monomorphization은 구현됨 — §10 참조)
- GC 전략 변경, Object 제거, 소유권 도입
- 다중 오류 정밀화(예: 첫 오류만 보고) — 하지 않음, 수집형
- Optimizer의 고급 최적화(상기 4단계 이후)

---

## 10. 구현 상태 업데이트 (2026-11-23)

이 문서의 §2〜§9는 원래 설계이며, 현재 코드와의 차이와 함께 다음을 명시한다.

### 구현된 것 (2026-11-23 기준)

- **`un check <file.un>`** — §2의 `un check` 그대로 구현됨. 정적 분석만 수행,
  exit 0/1, 최대 50개 진단, caret+line 형식.
- **`un native <file.un>`** — §2의 `un native` 그대로 구현됨. 정적 분석 후
  core 부분집합 proto를 TIR로 내리고 Cranelift 네이티브 코드를 등록한 뒤
  VM을 실행한다. `un help`에 두 명령이 추가되었다. `un run`/`un compile`은
  변경 없음.
- **단일 진입점**: compile mode는 `Proto.fn_span`으로 TIR 함수와 proto를
  1:1 매칭한다. `.unbc` 직렬화에는 영향 없음.
- **core 부분집합**: int/float/bool (타입) + none return. 산술(+,-,*,//,%),
  비교, 논리, `for i in a..b`와 `iter.range(a,b)` 루프, `while`, `if/elif/else`,
  직접 호출(동일 모듈 core 함수). Cranelift 네이티브 직결.
- **FFI ABI**: hidden pointer + param_tags + i64 args. Float는 bit 32+p에 IEEE
  비트, Bool은 bit 40+p에 0/1. VM은 `JitEntry { arg_kinds }`로 마샬링하고
  TIR entries는 `UV_JIT`와 무관하게 조회된다 (`get_tir_entry`).
- **bail 경로**: `//` div-zero, `INT_MIN // -1`, 네이티브 재귀 깊이 초과 등
  불가능한 조건은 `TAG_BAIL`로 보고되어 VM이 해당 호출만 interpreter로
  재실행한다. 출력/종료 코드는 `un run`과 바이트 동일.
- **제너릭**: `fn identity[T](x: T) -> T`, `class Box[T]` 문법, 호출부 추론·명시적
  `identity[int](42)`·bound `[T: numeric]`를 지원하고 TIR이 monomorphization한다
  (`tir.rs`). 데이터를 담는 enum variant(`enum Test[T,U]`의 `Test.a(42)`, `.value`,
  match 분해)도 VM/Compile Mode 경로에서 동작한다(plain no-VM build는 여전히 거부).
- **Standalone exe**: `un build` + `libnrt.a`로 VM 없는 네이티브 exe 산출 (§9).

### 미구현/제외 (범위)

- 바이트코드 JIT(`jit_bc`) 대상에서는 classes, closures, collections, union,
  `any`, 네이티브 호출, float math intrinsics, match가 여전히 인터프리터 폴백이다.
  다만 **plain `un build` 경로**에서는 classes/상속, exceptions, Str, list/dict,
  제너릭 타입 컬렉션, closure, 기본값 인자, 단순 모듈, byte-identical traceback이
  `libnrt.a`로 네이티브 실행된다 (`OBJ_MILESTONES.md`).
- Compile Mode 벤치마크는 `scripts/bench.ps1 -Group all`의 6개 레인(Un VM / Un JIT /
  Compile Mode / standalone exe / Python / Node)에 편입되었고,
  `scripts/bench-report.ps1`이 `bench/report.html`을 생성한다.
  `scripts/compile-bench.ps1`은 상세 비교용으로 남아 있다. 전체 결과는 루트
  `BENCHMARK.md` 참조.
- `test/compile/errors/` negative manifest — 구현됨 (`manifest.txt` 기반,
  `compile-check.ps1`가 33 passed 검증; §8).

### 배운 것 (요약)

- cranelift-frontend의 SSA construction은 `seal_block` 시점의 predecessor
  목록으로 SSA를 만든다. back-edge가 있는 블록은 반드시 back-edge 방출 후에
  seal해야 한다 (`COMPILE_FINDINGS.md` F-020).
- seal되지 않거나 terminator가 없는 블록은 finalize에서 삭제되어 reference가
  끊어진다 (F-021).
- Pass-1 선언 수집이 함수 본문의 지역 변수를 글로벌로 오염시켜 모든 core
  eligibility를 떨어뜨렸다 (F-017).
