# Un

간결한 문법, 명시적 비동기 실행, 패턴 매칭을 제공하는 현대적인 스크립팅 언어.

Un은 Rust로 작성된 바이트코드 컴파일러와 레지스터 기반 VM(`unvm`)으로 구현되어 있다. 핫 패스 Int 및 Float 승격 누산기 함수는 Cranelift JIT으로 native로 컴파일된다. 처리 파이프라인은 다음과 같다.

`Source -> Lexer -> Parser -> Optimizer (desugar+fold, single pass) -> Bytecode Compiler -> Register VM (optionally JIT)`

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.100-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust" />
  <img src="https://img.shields.io/badge/license-MIT-22C55E?style=flat-square" alt="MIT License" />
</p>

## 소개

Un은 읽기 쉽고 간결한 프로그램을 작성하기 위한 동적 타입 스크립팅 언어이다. Python 스타일의 들여쓰기 기반 블록, 명시적인 동시성 기본 요소, 구조적 패턴 매칭, 작은 표준 라이브러리를 결합한다.

Un은 Rust로 작성되었다: 소스 파일을 렉싱/파싱한 뒤 최적화(상수 폴딩, 단일 bottom-up 패스에서 desugar 포함)하고, 바이트코드로 컴파일해 레지스터 기반 VM에서 실행한다. 이전에 C#/.NET 8 트리 워킹 인터프리터로 구현되어 있었으나 Rust 구현이 C# 구현의 의미론을 byte-identical로 재현함을 differential 검증으로 확인하고 대체했다.

설계 목표:

- 문법을 작고 읽기 쉽게 유지한다.
- `go`와 `wait`를 통해 비동기 실행을 명시적으로 만든다.
- 실용적인 컬렉션과 입출력 모듈을 기본 제공한다.
- 정적 검사기를 요구하지 않는 점진적 타입 어노테이션을 지원한다.

## 기능

### 언어 핵심 (안정)

- 변수와 구조 분해 할당
- 기본값, 키워드, 가변 인자를 갖는 함수
- `@decorator` 함수 장식
- 람다와 클로저
- `if` / `elif` / `else`, `for`, `while`, `break`, `skip`
- 패턴 매칭을 지원하는 `match`
- 클래스, 상속, 다중 상속, `enum`
- 클래스 Property: `get`/`set` 접근자와 `field`, `value`
- 범위 연산자 `a..b` (반개구간, 지연 평가)
- `use`, 별칭, 선택적 import, 명시적 상대 경로(`use .foo` / `use ..pkg`)를 갖는 모듈 시스템
- `using`과 `defer`를 통한 자원 관리
- 백틱 문자열 보간 `` `..{expr}..` ``
- `list`, `dict`, `set`, `tuple` 컬렉션
- 슬라이싱과 인덱싱
- 후위 `?`를 통한 null/에러 전파
- C 스타일 삼항 표현식 `cond ? a : b`
- 복합 대입 `+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `**=`, `&=`, `|=`, `^=`, `<<=`, `>>=`, `??=`
- 식 형태의 대입 `:=` (대입한 값을 반환)
- `??` null 병합
- `?.`를 통한 안전한 속성/메서드 접근
- `try` / `catch` / `finally`

### 타입 시스템 (실험적)

- 선택적 타입 어노테이션: `x: int`, `a: list[int]`, `dict[str, int]`, `dict[int, int]`, `int | str`
  파라미터와 반환타입 (`fn add(a: int, b: int) -> int`)
- **함수 타입**: `f: func(int, str) -> int`로 호출 가능한 값을 표기한다. 고차 함수에서
  `f: any`를 대체하는 정식 문법이며 `un build` fully typed mode에서도 동작한다
  (capture 없는 wrapper closure를 NRT에 등록).
- **타입 패턴**: `match`에서 `int`, `float`, `str`, 클래스명, 타입 별칭(`Id` 등)을
  그대로 쓰면 `is`와 동일한 **런타임 타입 검사**가 된다. 별칭은 `Or(Type…)`으로 펼쳐지고,
  같은 이름의 지역 변수가 있으면 그것이 우선한다. `un run`/`un native`/plain `un build`
  모두 지원.
- **제네릭**: `fn identity[T](x: T) -> T`, `class Box[T]` — 호출부에서 타입을 추론하며
  명시적 타입 인자 `identity[int](42)`도 쓸 수 있다. `[T: int | float]` 같은 제약을 지정할 수 있다
- 어노테이션은 현재 가독성과 점진적 검사를 위한 용도이며, 엄격한 런타임 검사는 강화 중이다

### 동시성 (안정)

- 백그라운드에서 표현식을 실행하고 `Future`를 반환하는 `go`
- `Future`를 대기하는 `wait`
- map과 submit을 제공하는 `flow.spawn(n)` 풀 핸들 (Un은 협업적 싱글스레드 런타임이라 OS 스레드를 만들지 않고, pool.map은 인자마다 결과를 만들지만 호출자 스레드에서 인라인으로 실행됨)

### 계획됨

- 더 강한 정적 타입 검사
- 패키지 매니저

## 문법 미리보기

변수와 함수:

```un
use io

fn add(a, b)
    -> a + b

fn greet(name = "world")
    -> `hello {name}`

io.write(add(3, 4))   # 7
io.write(greet())      # hello world
mul = fn(a, b) -> a * b
io.write(mul(6, 7))   # 42
```

제어 흐름과 패턴 매칭:

```un
use io

x = 5
if x > 5
    io.write("big")
elif x == 5
    io.write("equal")
else
    io.write("small")

res = match x {
    0 -> "zero",
    1 -> "one",
    _ -> "other",
}
io.write(res)
```

클래스:

```un
use io

class Point(x, y)
    fn len()
        -> self.x + self.y

p = Point(10, 20)
io.write(p.x)      # 10
io.write(p.len())  # 30
```

클래스 본문의 멤버에는 타입 어노테이션과(선택적으로) 기본값을 함께 쓸 수 있다.
어노테이션은 "의도 선언"일 뿐이며, 저장값은 런타임에 들어온 그대로다:

```un
use io

class TypedDefaults
    count: int = 0
    name: str = "anon"

t = TypedDefaults()
io.write(t.count)   # 0
io.write(t.name)    # anon
t.count = 42        # int면 무엇이든 통과한다; 슬롯은 동적 타입이다
```

기본값이 없는 어노테이션 멤버(`class Foo\n    x: int`)는 `none`으로 초기화된다.

클래스의 일반 멤버는 기본 Getter/Setter를 갖고, `get`/`set` 접근자로 재정의할 수
있다. 접근자 안에서 `field`는 그 멤버의 저장 값, `value`는 setter에 들어온 값이다.

```un
use io

class Counter
    count = 0
    doubled = 0
    get doubled -> field * 2
    set doubled = value / 2

c = Counter()
io.write(c.doubled)   # 0
c.doubled = 10        # field = 5
io.write(c.doubled)   # 10
io.write(c.count)     # 0
```

상속은 클래스 이름 뒤에 부모를 적는다. 다중 상속은 선언 순서대로 모든 부모를
상속하고, 이름이 겹치면 첫 번째 부모가 이긴다.

```un
use io

class BaseA
    a = 1
class BaseB
    b = 2
class C: BaseA, BaseB
    c = 3

io.write(C().a, C().b, C().c)        # 1 2 3
io.write(C() is BaseA, C() is BaseB)  # true true
```

`enum`은 회원이 정수 값으로 자동 넘겨지는 타입이다.

```un
use io

enum Color
    RED
    GREEN
    BLUE

io.write(Color.RED, Color.GREEN, Color.BLUE)   # 0 1 2
```

필드를 가진 variant도 선언할 수 있다. 러스트의 enum과 비슷하지만 들여쓰기 기반이고
`이름: 타입` 형태로 한 개의 페이로드를 선언한다. 타입 파라미터(`[T, U]`)도 쓸 수 있다.

```un
enum Test[T, U]
    a: int
    b: T
    c: tuple[int, U]

x = Test.a(42)
io.write(x)             # Test.a(42)
io.write(x.value)       # 42
```

- `Test.a(42)`처럼 enum 이름 아래 variant를 호출해 생성하고, `.value`로 값을 읽는다.
- enum 이름·variant·페이로드가 모두 같으면 `==`가 참이며 dict 키로도 쓸 수 있다.
- `match x { Test.a(v) -> v, Test.b(v) -> v, _ -> 0 }`로 페이로드를 분해할 수 있다.
- 필드 타입은 `un check`에서 정적으로 검사하고, `un run`에서는 타입을 지운다.
- 데이터 variant가 있는 enum은 Compile Mode의 VM 경로로 실행된다(no-VM `un build`는 미지원).

범위 연산자 `..`는 반개구간이고 값을 즉시 만들지 않는다. 우선순위는 덧셈과 비교
사이에 있어 `1 + 2..10`은 `(1 + 2)..10`이다. 경계는 정수 또는 정수값을 가진
float만 허용한다.

```un
use io

for i in 1..5
    io.write(i)              # 1 2 3 4

for 0..5
    io.write("tick")         # 요소를 바인딩하지 않는 반복

io.write(list(0..3))         # [0, 1, 2]
io.write(len(0..100))        # 100
io.write(list(1 + 2..6))     # [3, 4, 5]
io.write(list((1..10).take(3)))  # [1, 2, 3]
```

`for` 뒤에 `in` 대상 없이 이터러블만 쓰면 요소를 바인딩하지 않고 순회한다.
`for _ in ...`처럼 `_`로 버리는 것과 같은 동작을 컬렉션만으로 쓸 수 있다.

```un
wild = 0
for 0..3
    wild += 1                # 3번 반복
```

비동기 실행:

```un
use io
use time

fn slow(x)
    time.sleep(100)
    -> x * 2

a = go slow(5)
io.write(wait a)   # 10
```

## 시작하기

### 요구사항

- Rust toolchain (`stable-x86_64-pc-windows-gnu`, Rust 1.100+; `rust-toolchain.toml`이 고정). Linux/macOS에서는 MSVC/`gcc` 대신 GNU 링크를 사용한다.

### 빌드

```bash
rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml
```

Windows 산출물은 `rust\target\release\un.exe`이다.

### 실행

```bash
.\rust\target\release\un.exe run path/to/file.un
```

### 컴파일 모드 (실험적)

Interpreter Mode(`un run`)와 Compile Mode는 같은 언어를 공유하는 두 실행 모드이다.
Compile Mode는 프로그램을 미리 타입 검사한 뒤 core 부분집합의 함수들을
Cranelift 네이티브 코드로 컴파일한다. Core 밖의 함수는 동일한 의미론으로
인터프리터가 실행한다.

```bash
.\rust\target\release\un.exe check path/to/file.un   # 정적 검사만
.\rust\target\release\un.exe native path/to/file.un  # 검사 + 네이티브 실행
```

- `un check`는 프로그램이 타입 검사를 통과하면 exit 0, 아니면 exit 1이며,
  최대 50개의 진단을 줄/캐럿 마커와 함께 출력한다.
- Compile Mode는 더 엄격하다: **모든 함수의 매개변수와 반환 타입에 어노테이션이
  필요하다** (`fn add(a: int, b: int) -> int`). 어노테이션이 없는 인터프리터
  프로그램은 컴파일 모드에서 의도적으로 거부된다.
- `un native`는 `un run`의 상위 집합이다. core에 들어가는 함수(int/float/bool
  매개변수·지역변수·반환, 직접 호출)는 레지스터 VM의 호출 경로를 통해 네이티브로
  컴파일되고, 나머지 함수는 투명하게 인터프리터로 fallback한다. bail이 발생해도
  잘못된 답이 나오지 않는다 — VM이 해당 호출을 인터프리터로 재실행한다.
- `un run`과 `un compile`은 변경 없었다. 컴파일 모드는 `docs/compile/COMPILE_DESIGN.md`에
  설명되어 있으며,
  `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\compile-check.ps1`로
  교차 검증할 수 있다.
- `un obj <file.un|file.unbc> [-o out.obj] [--entry name]`는 `.unbc` 바이트코드 또는
  `.un` 소스를 재배치 가능한 네이티브 오브젝트 파일(COFF .obj)로 번역한다. 소스
  경로는 `if`/`while`/`for` 루프까지 네이티브로 지원한다. core 부분집합만
  지원하며 `scripts\obj-demo.ps1`이 fib 예제를 링크해 실행까지 검증한다
  (`docs/compile/OBJ_MILESTONES.md`).
- `un build <file.un> [-o out.exe]`는 위 과정을 명령 하나로 수행해 독립 실행형
  `.exe`를 만든다 (rustup self-contained mingw gcc로 링크). **plain exe는 `fn main`만 실행한다**
  — 모듈 최상위 문장은 실행되지 않고 종료 코드는 `main()`의 반환값이다. 이에 반해
  `un run` / `un native` / `--embed`는 최상위 문장을 실행하고 `main()`은 파일이
  직접 호출해야 실행된다.
- `un build --embed <file.un> [-o out.exe]`는 대신 프로그램의 `.unbc` 이미지를
  작은 드라이버에 구워 `libunvm.a`(전체 VM 런타임, 정적으로 링크)와 함께 링크한다.
  이 경로는 **모든 Un 프로그램**(class, collection, IO, 문자열 보간 등 Int core
  부분집합에 속하지 않는 것 포함)에서 완전하게 동작하며, 소스는 물론 공유 런타임
  없이도 단일 실행형이다.
- plain `un build`(`--embed` 없이)는 **VM을 전혀 포함하지 않는다**. 대신 `libnrt.a`
  (독립 네이티브 런타임)를 링크하며 exe는 약 5 MB다. 클래스(필드·메서드·`__init__`·상속),
  예외, 문자열/리스트/딕트, 네이티브 모듈(io/math/iter/os/sys/fs/codes/re/time/types,
  그리고 `bytes`·`http`/`ws`), 기본값 인자, 람다(캡처·closure·리스트 저장), 단순 사용자
  모듈 import/전역, VM과 byte-identical한 traceback을 지원한다. 여기에 `b"..."` 리터럴과
  12개 함수의 `bytes` 모듈, tuple/set 값, 슬라이스 읽기·할당, FString·삼항 표현식,
  빈 컬렉션 리터럴, `list.clear()`/`list.resize()`까지 네이티브로 지원한다.
  제네릭 컬렉션 타입(`list[str]`, `list[bool]`, `dict[int, str]`,
  `dict[str, ClassName]` 등)은 elem kind를 TIR에 명시해 plain build에서도 네이티브로
  컴파일된다. 중첩 리스트의 element는 불투명 핸들로 취급되어 내부 인덱싱·연산은
  lowering하지 않는다.
- `un build`는 plain과 `--embed` 양쪽 모두 기본적으로 **fully typed mode**다.
  명시적 `any`/Union 어노테이션과 별칭, variadic·이름 없는 파라미터, 타입 없는 lambda
  파라미터는 진단과 함께 거부된다 — callable 파라미터는 `func(..) -> ..`를 쓸 것.
  `--allow-dynamic`을 붙이면 이전의 동적 동작으로 되돌린다. `un run`은 동적 타입을
  유지하고 `un check` / `un native` / `un obj` / LSP / 플레이그라운드는 변하지 않는다.
  시간 기반 `random`과 단순 대입을 넘어서는 모듈 최상위 부수 효과는 여전히
  `un build --embed`가 필요하다.
- `un lsp`는 stdin/stdout으로 LSP 언어 서버를 실행한다. 기본은 lexer/parser
  오류만 보고하며, Compile Mode 타입 진단은 `initializationOptions.compileMode`
  또는 `un/compileMode` 알림으로 opt-in한다. Rust를 쓰지 않는 클라이언트도
  그대로 쓸 수 있으며, 저장소 밖 `C:\dev\un-vscode`에는 이를 사용하는
  VS Code 확장(`un-language`) 소스가 있다 — 구문 강조 + 실시간 진단, 호버,
  F12 정의로 이동, 계층 아웃라인(클래스 > 메서드/필드), Shift+F12 참조 찾기,
  F2 이름 변경, 접기, 자동완성(`.` 멤버추천 — 사용자 모듈 멤버 포함 — 시그니처
  도움말, 스니펫 포함), AST 기반 semantic tokens를 제공한다. 사용자 모듈은
  파일을 넘나드는 호버·정의로 이동과 상대 import(`use .foo` / `use ..pkg`) 해석도
  지원한다. 에디터 안에서 `Un: Run File` 버튼(에디터 제목 바/우클릭/상태 바)으로
  현재 파일을 바로 `un run`으로 실행할 수 있다.

### 첫 프로그램

```un
use io

fn main()
    io.write("Hello, Un!")

main()
```

`hello.un`으로 저장한 뒤 실행한다.

```bash
.\rust\target\release\un.exe run hello.un
```

### 테스트와 벤치마크

빌드와 스크립트 실행:

```bash
rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml
.\rust\target\release\un.exe run test/test_suite.un
```

전체 테스트는 runner로 실행한다. runner는 `test/MANIFEST.txt`의 `group|path` 행을
실행 파일로 실행하고 exit code와 stdout의 검사 수를 세고,
`test/negative/manifest.txt`의 `path|exit|substring` 행으로 stderr와 exit code를
확인하는 스크립트를 실행한다.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test.ps1
```

JIT 포함/미포함이 같은 동작을 하는지도 같은 절차로 검증한다. runner는
`test/MANIFEST.txt`와 `test/negative/manifest.txt`의 모든 파일을 `UV_JIT=0`과
기본 JIT으로 두 번 실행해 stdout과 exit code를 비교한다.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\differential.ps1
```

`UV_JIT=2`는 strict canary다. JIT 후보가 하나라도 fallback stub이 되면 exit 2로 즉시
종료해 회귀를 잡는다. Int용 `test/jit/strict.un`과 Float math intrinsic용
`test/jit/strict_float.un`을 CI가 검증한다. `.unbc` 왕복 검증은 `un run`과
`un compile` + `un run <file>.unbc`의 stdout/stderr/exit code를 바이트 비교한다.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\unbc-roundtrip.ps1 -IncludeNegative
```

Compile Mode 교차 검증, no-VM exe 동치 검증, 크로스 레인 edge fuzz도 같은 방식으로
실행한다(`scripts\compile-check.ps1`, `scripts\exe-diff.ps1`, `scripts\edge-fuzz.ps1`).
CI(`.github/workflows/build.yml`)는 push/PR마다 release 빌드와 위 검증 스텝을 포함해
총 12스텝을 실행한다.

`test/KnownFailures.txt`에 있는 파일은 runner가 KNOWN FAIL로 기록하고,
Positive 집계의 통과·실패 수에 포함하지 않는다. `test/KnownDivergent.txt`에 있는 파일은
구현 간 동작이 의도적으로 다른 문서화된 차이로 기록되어 Positive/Negative 집계의 실패 수에
포함하지 않는다. 현재 기준은 Positive 51 파일 / 1638 passed / 0 failed / 0 crashed,
Negative 51 checks 매칭 + 0 documented divergences, Known 0 파일이다. (2026-11-23 재측정)
Rust 테스트는 `cargo test --release --workspace` 314 passed(nrt 16 + lib 263 + LSP 24 +
playground 8)다. JIT differential 102 파일 / 0 차이, `.unbc` 왕복 Positive 51/51 ·
Negative 42 identical(9 compile-rejected skip), Compile 교차 검증 36/0, exe 동치 검증 42/0이다.

벤치마크는 6레인(Un VM, Un JIT, Compile Mode, standalone exe, CPython, Node.js)을 같은
입력·알고리즘·측정 절차로 반복 측정하고, 결과를
`bench/results/<timestamp>/results.csv`, `results.json`, `results.md`에 저장한다.
`scripts\bench-report.ps1`은 결과를 `bench/report.html`로 변환하고 `-Gate`로 회귀를 검사한다.
현재 보고서의 Un JIT/Python 기하평균은 **4.434x**이며, 새 워크로드
`dict_str5000`(문자열 키 dict 삽입·조회), `bytes_ops`(bytes 모듈 왕복),
`slice_ops`(리스트 슬라이스 읽기·할당)이 포함되어 있다.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group all
```

언어 간 같은 입력·알고리즘·측정 절차로 비교하는 규칙은
`docs/guides/BENCHMARK_GUIDE.md`에 있다. 측정은 `scripts/bench.ps1`로 직접 수행한다.
테스트 구조와 규칙은 `docs/guides/TEST_GUIDE.md`에 있다. `un` CLI 설명서는 `docs/guides/CLI.md`(영어)와 `docs/guides/CLI.ko.md`(한국어)에 있다.

## 언어 개요

### 변수와 구조 분해

```un
x = 10
name = "Un"
value: int = 20
a, b = (1, 2)
```

### 함수

```un
fn add(a, b)
    -> a + b

fn greet(name = "world")
    -> `hello {name}`

mul = fn(a, b) -> a * b
```

함수 본문은 들여쓰기된 블록을 사용한다. 람다는 한 줄에 `->`를 사용한다. 함수 앞에
`@decorator`를 적으면 그 장식이 함수를 감싼다.

```un
use io

fn tag(target)
    fn wrapped(x)
        -> target(x)
    -> wrapped

@tag
fn tagged(x)
    -> x + 1

io.write(tagged(1))   # 2
```

### 제어 흐름

```un
use io

x = 5
if x > 5
    io.write("big")
elif x == 5
    io.write("equal")
else
    io.write("small")

for v in [1, 2, 3]
    io.write(v)

n = 0
while n < 5
    io.write(n)
    n += 1
```

루프 안에서 `break`와 `skip`을 사용할 수 있다. 루프 본문 안의 `->`는 루프를
마치고 그 함수를 반환한다.

### 패턴 매칭

```un
use io

value = (3, 0)
res = match value {
    0 -> "zero",
    (x, 0) -> `x={x}`,
    _ -> "other",
}
io.write(res)    # x=3
```

### 컬렉션과 슬라이싱

```un
use io

l = [1, 2, 3, 4, 5]
io.write(l[1:3])                  # [2, 3]
io.write(l[::2])                  # [1, 3, 5]
io.write(l[::-1])                 # [5, 4, 3, 2, 1]
io.write(l[-2:])                  # [4, 5]
io.write(2 in l)                  # true
io.write(set(1, 2) == set(1, 2))  # true
io.write({"a": 1} == {"a": 1})    # true
```

슬라이스는 `list`, `tuple`, `str`에 같은 규칙으로 적용된다. 시작과 끝 인덱스는
음수이면 뒤에서부터 세고, 생략하면 스텝의 부호에 따라 정해진다(양수 스텝이면
처음부터 끝까지, 음수 스텝이면 마지막부터 처음까지). 끝 인덱스는 포함하지
않으며, 스텝은 0이 될 수 없다.

`==`는 `list`, `tuple`, `dict`, `set`을 원소 단위로 비교한다. `dict`는 키와
값이 모두 같아야 하고, `set`은 순서와 무관하게 원소가 같아야 한다.

### 타입

```un
use io

x: int = 10
y: str = "hi"
a: list[int] = [1, 2]
b: dict[str, int] = {}
c: int | str = 1
io.write(x, y, a, b, c)   # 10 hi [1, 2] {} 1
```

`none`은 예약어이므로 어노테이션의 타입 이름으로 쓰지 못한다. `x: none`이나
`int | none`은 파싱 오류로 끝난다.

### 클래스와 Property

```un
use io

class Person
    name = "anon"            # 일반 멤버는 기본 Getter/Setter
    older = 1
    get older -> field + 1   # 단일 행 getter
    set older = value - 1    # 단일 행 setter
    get display
        -> `name={self.name}`

p = Person()
p.name = "bob"
io.write(p.older)    # 2
p.older = 10         # field = 9
io.write(p.older)    # 10
io.write(p.display)  # name=bob
```

접근자는 여러 행 본문을 쓸 수 있고, 접근자 안에서 `field`는 그 프로퍼티의 저장
값을, `value`는 setter에 들어온 값을 가리킨다. getter는 `self`로 다른 멤버를
읽고 쓸 수 있다. 프로퍼티의 기본값은 인스턴스마다 복제되므로 `items = []` 같은
mutable 기본값은 인스턴스 사이에 공유되지 않는다.

`get`만 선언하면 대입은 저장 값에 직접 쓰고, `set`만 선언하면 읽기는 저장 값을
읽는다. 생성자 헤더(`class Person(name)`)의 초기화는 setter를 거치지 않는다.
자식 클래스가 접근자를 재정의하면 부모의 기본값을 물려받는다. 같은 이름의
메서드가 있거나 같은 프로퍼티에 getter를 두 번 선언하면 오류다.

getter가 `self.같은 이름`으로 자기 프로퍼티를 읽으면 getter가 자기 자신을 다시
부르므로 스택 오버플로로 끝난다. getter는 저장 값을 `field`로 읽어야 한다.
`test/negative/property_recursion.un`이 이 경계를 종료 코드로 기록한다.

### 범위

```un
use io

for i in 1..5              # 1 2 3 4 (`..`는 반개구간)
    io.write(i)

io.write(list(0..3))       # [0, 1, 2]
io.write(len(0..100))      # 100
io.write(3 in 1..5)        # true
io.write(str(1..4))        # 1..4
io.write(list((1..10).map(fn(x) -> x * 2)))   # [2, 4, 6, 8, 10, 12, 14, 16, 18]
```

`..`가 만드는 범위는 `iter`의 지연 평가 계열과 같고, `list()`처럼 소비하는
연산이 있을 때만 값이 만들어진다. `a..b`는 `a.__range__(b)`를 호출하므로
클래스가 `__range__` 매직 메서드로 범위 생성을 정의할 수 있다. 우선순위는
덧셈과 비교 사이에 있고, 시작이 끝보다 크면 빈 범위가 된다. 경계는 정수 또는
정수값을 가진 float만 허용한다.

### 비동기 실행

```un
use io
use time

fn slow(x)
    time.sleep(100)
    -> x * 2

a = go slow(5)
io.write(wait a)    # 10
```

`flow.spawn(n)`은 map과 submit을 제공하는 풀 핸들을 만든다. 다만 Un은 협업적 싱글스레드
런타임이라 OS 스레드를 만들지 않는다. `pool.map`은 인자마다 결과를 만들고 `pool.submit`은
`Future`를 돌려주지만, 실제 실행은 호출자 스레드에서 인라인으로 이루어진다. 공유 상태를 두고
경쟁이 일어나지 않게 하려면 `atomic.int`/`atomic.bool`(예: `counter.inc()`)로 원자 연산을 할 수 있다.

```un
use atomic

counter = atomic.int(0)
counter.inc()              # 1
counter.add(10)            # 11
counter.compare_and_swap(11, 100)   # true
```

```un
use io
use flow
use iter

using pool = flow.spawn(2)
r = pool.map(fn(x) -> x * x, 1, 2, 3, 4)
io.write(list(iter.sorted(r)))   # [1, 4, 9, 16]
s = pool.submit(fn(x) -> x * 3, (7))
io.write(wait s)                 # 21
```

### 모듈

```un
use io
use fs as filesystem
use re {test, find_all}
use re as re

io.write(re.test("\\d+", "abc123"))
```

상대 import는 현재 파일이 있는 디렉터리를 기준으로 해석한다: 같은 디렉터리의
모듈은 `use .foo`, 부모 디렉터리 아래의 모듈은 `use ..pkg`로 가져온다. LSP와
plain `un build`가 두 형태를 모두 인식한다.

### 자원 관리

```un
use io

using file = io.open("log.txt", "w")
file.write("hello")

fn work()
    defer io.write("done")
    io.write("start")

work()    # start done
```

## 표준 라이브러리

| 모듈 | 주요 API |
|---|---|
| `atomic` | `atomic.int(initial?)`/`atomic.bool(initial?)` — `atomic_int`/`atomic_bool` 값 생성. `get`/`set`/`add`(int 전용)/`inc`/`dec`(int 전용)/`exchange`/`compare_and_swap` 제공. 싱글스레드 협업 런타임이라 `RefCell` 기반이며, `add`/`inc`/`dec`는 Un Int와 같은 i64 wrapping 동작 |
| `io` | `write`, `read`, `open`, `clear` |
| `fs` | `read`/`write`/`append`/`touch`, `exists`/`file`/`dir`, `list`/`files`/`dirs`/`walk`, `copy`/`move`/`rename`/`delete`, `mkdir`/`rmdir`, `size`/`created`/`modified`/`accessed`, `join`/`parent`/`name`/`stem`/`ext`, `absolute`/`relative`/`normalize`, `cwd`/`chdir`, `temp`/`home`/`exe`/`app` |
| `re` | `test`/`match`/`search`, `find_all`/`groups`, `replace`/`replace_all`/`split`/`escape` |
| `math` | `round`/`abs`/`ceil`/`floor`/`trunc`/`sign`, `sqrt`/`cbrt`/`pow`/`exp`/`log`/`log2`/`log10`, `sin`/`cos`/`tan`/`asin`/`acos`/`atan`/`atan2`/`hypot`, `gcd`/`lcm`/`clamp`, `is_nan`/`is_infinite`/`is_finite`, `pi`/`e`/`tau` |
| `codes` | `bin`/`oct`/`hex` (정수 2/8/16진), `ascii` (첫 UTF-16 코드 유닛), `chr`/`ord` (코드포인트 ↔ 문자) |
| `bytes` | `from_str`/`from_list`/`from_hex`, `to_str`/`to_list`/`to_hex`, `len`/`get`/`slice`/`concat`/`eq`/`is_bytes` — `b"..."` 리터럴과 `fs.read_bytes`/`fs.write_bytes`가 만드는 바이트 문자열; plain `un build`에서 네이티브로 컴파일됨 |
| `iter` | `array`/`range`/`counter`/`reverse`/`repeat`, `zip`/`enumerate`, `map`/`filter`/`take`/`skip`/`chain`/`flatten`, `sum`/`max`/`min`/`sorted`, `all`/`any`/`count` |
| `random` | `int`/`float`/`bool`/`bytes`/`chance`, `choice`/`choices`/`sample`/`shuffle`/`weighted`, `range`/`str`/`value`/`next`/`seed`/`uuid` |
| `os` | `env`/`setenv`/`unsetenv`/`environ`/`expand_vars`, `pid`/`name`/`arch`/`version`/`cpuCount`/`is64bit`/`hostname`/`username`, `args`/`exit`/`exec`, `sep`/`pathsep`/`linesep`/`tickCount` |
| `sys` | `env`/`environ`/`expand_vars`, `pid`/`name`/`arch`/`version`, `getcalldepth`/`setcalldepth`/`getrecursionlimit`/`setrecursionlimit` |
| `time` | `sleep`(int=ms, float=초)/`now`/`utc_now`/`epoch`/`epoch_ms`/`from_epoch`/`from_epoch_ms`/`monotonic`/`monotonic_ms`/`stopwatch` (`ms`·`ns` read-only 프로퍼티) |
| `flow` | `spawn`(pool 핸들, 협업적 싱글스레드 — 호출자 스레드에서 인라인 실행)과 `lock` |
| `http`/`ws` | `http.connect().get`/`post`/`put`/`patch`/`delete`/`head`/`options`, `ws.connect().send`/`receive`/`close`/`is_open` |
| `inspect` | `attr`/`global`/`getattr`/`hasattr`/`setattr`/`delattr`/`meta` |
| `types` | `u8`/`u16`/`u32`/`u64`, `i8`/`i16`/`i32`/`i64`, `f32`/`f64` — 고정 폭 정수/실수 변환 생성자. 범위 검사는 변환 시점에만 하고 벗어나면 잡을 수 없는 예외를 발생(출력은 `error: <메시지>`). 이후 연산은 일반 `int`/`float`과 동일(`u64`는 Un int가 i64라 `i64::MAX`까지만 담김). `use types` 필요 — import가 없는 모듈에서는 고정 폭 이름(`u32` 등 포함)이 알 수 없는 타입 어노테이션이 됨 |

컬렉션 메서드로는 `list.add`/`insert`/`extend`/`remove`/`remove_at`/`index_of`/`contains`/`order`/`sort`/`reverse`/`binary_search`/`hpush`/`hpop`/`pop`/`map`/`clear`/`resize`, `dict.add`/`get`/`remove`/`keys`/`values`/`contains_key`/`contains_value`/`clear`, `set.add`/`union`/`intersect`/`difference`/`contains`/`remove`/`clone`/`clear`, 그리고 `split`/`join`/`trim`/`center`/`left`/`right`/`replace`/`find`/`sub_str`/`to_upper`/`to_lower`/`starts_with`/`ends_with` 같은 문자열 메서드가 있다.

모듈 없이 전역에 있는 내장 함수는 `len`/`type`/`hash`/`exit`/`throw`/`throw_if`/`throw_unless`/`repr`이고, 생성자로는 `int`/`float`/`str`/`bool`/`list`/`set`/`tuple`/`date`가 있다. 기수/문자 코드 변환은 `use codes`로 가져온다 (`codes.hex()`/`codes.bin()`/`codes.oct()`/`codes.ascii()`/`codes.chr()`/`codes.ord()`). `math.pi`, `math.e`, `math.tau`, `os.sep`, `os.pathsep`, `os.linesep`, `os.cpuCount`, `os.is64bit` 같은 값은 함수로 노출되므로 호출해서 쓴다.

`iter`의 메서드 체이닝(`(0..10).map(fn(x) -> x * 2).take(3)`)은 지연 평가되어 소비할 때만 실행되고, 전역 `iter.map`/`iter.filter`/`iter.take`은 인자를 즉시 평가해 `list`를 반환한다. `iter.zip`는 두 소스를 같은 속도로 소비하므로 무한 `iter.counter`와 유한 범위를 함께 쓸 수 있다.

`skip`은 예약어이므로 `iter.skip(...)`나 `.skip(...)`는 파싱 오류로 끝나 이름으로 호출할 수 없다. 앞 부분을 잘라내려면 `take`나 슬라이싱을 쓴다.

### 의미 주의사항

- **메모리(D-093)**: Un에는 GC가 없고 `Weak` 참조도 쓰지 않는다. 객체는 참조
  카운팅으로 해제되고, 의도적인 순환(bound method, closure 캡처)은 유계로 두고
  `world::reset()` / 프로세스 종료 시 회수한다. JIT 네이티브 코드는 네 가지 실행
  경계 — CLI 정상 종료, CLI 오류/패닉, 플레이그라운드 매 실행,
  `unvm_run_embedded()` 종료 — 에서 명시 해제하며, `clear()` 이후에는 다시
  진입할 수 없다. 장기 실행 embed 호스트는 프로세스를 재시작하는 전제로 둔다.
- `time.stopwatch`: `.elapsed`는 단조 경과시간을 담은 `date`를 반환한다(타임스탬프가 아니다). `start()`는 재개가 아니라 타이머 재시작이다.
- `hash(...)`: `NaN`에 대한 해시 키는 딕셔너리에서 찾을 수 없다 — IEEE semantics / .NET과 같은 동작이며 버그가 아니다. `NaN`이 키에 들어가면 조회는 항상 `key not found`가 된다.
- `2 ** 3 ** 2`는 `2 ** (3 ** 2)`(거듭제곱은 오른쪽 결합, Python/JS와 동일)로 평가된다. `2 ** 3`은 `8`이다.
- 문자열 이스케이프: 알 수 없는 이스케이프는 백슬래시를 그대로 남긴다. `"\q"`는 `"\q"`이고 `"\x"`는 `"\x"`이다. `\n`, `\r`, `\t`, `\\`, `\"`, `\'`만 해석된다.
- 한 줄에 두 개의 문장을 쓸 수 없다. `x := 1 y := 2`는 `expected a newline after the previous statement` 파싱 에러다.
- `b?.v = e`(선택적 체이닝 좌변 대입)는 `b`가 none이면 `'none' object has no attribute` 에러를 던진다. Un은 대입을 조용히 건너뛰지 않는다.
- `http.get/post/put/patch/head/options/delete`: 301/302/303/307/308 리다이렉트를 최대 8회 따라간다. 301/302/303은 메서드를 GET으로 바꾸고 본문을 버린다(.NET 동작); 307/308은 메서드와 본문을 유지한다. Location 헤더가 없으면 리다이렉트 응답 자체가 최종 응답이다. `get`은 404식 예외 계약을 유지하고, 나머지 verb는 상태와 본문을 그대로 반환한다.
- `time`의 `date(str)` 생성자는 연/월/일(윤년 포함), 시/분/초/밀리초를 모두 엄격하게 검증한다. `date("2024-02-30")`은 에러다.
- `use a.*`는 `use a { * }`의 축약형이다(공개 멤버 전부를 와일드카드 import).
- 닫히지 않은 에러가 프로그램 밖으로 전파되면 Un식 트레이스를 출력한다: `un: error trace` 헤더 아래 에러가 지나온 각 프레임을 `-> <함수> <파일>, line [N]`와 소스 라인으로 나열하고, 마지막에 에러 위치(라인/스팬/캐럿)와 `error: <메시지>`를 찍는다. 반복 프레임은 `... (repeated N more times)`로 접히고, 추적이 10개 프레임을 넘으면 compact 모드로 소스 라인 없이 라인 정보만 출력한다. 프레임이 1개뿐인 얕은 에러는 트레이스를 생략하고 캐럿 블록만 출력한다. import한 모듈에서 발생한 에러도 모듈 기준 파일/라인으로 표시되며, `use` 라인도 프레임으로 기록된다.

## 아키텍처

바이트코드 컴파일러와 레지스터 기반 VM 위에서 동작하며, 핫 패스 Int 및 Float 승격 누산기 함수는 Cranelift JIT으로 native로 컴파일된다.

```
Source (.un)
  |
  v
Lexer          들여쓰기를 인식하는 토크나이저
  |
  v
Parser         AST 구성 (Node / NodeKind)
  |
  v
Optimizer      desugar(복합 대입, if/elif) + 상수 폴딩, 대수적 단순화, 단일
               bottom-up 패스
  |
  v
Bytecode Compiler   Opcode/Proto/Program, Instruction(flags, operand) 인코딩
  |
  v
Register VM (interpreter)  Context / Scope / Frame 기반 실행, go/wait를 위한 Future
  |                           + 핫 패스 Int 및 Float 승격 누산기 함수는 Cranelift JIT으로 native 코드로 변환
  v
Runtime         Obj / Ref / Enu / Stru / TObj / Property와 타입 시스템
  |
  v
Stdlib          네이티브 모듈 (`native::init` 명시적 등록, 리플렉션 미사용)
```

주요 구성 요소:

- `rust/src/compiler.rs`는 역탈당·최적화된 AST를 바이트코드 Proto로 컴파일한다.
- `rust/src/vm/`은 `Proto`를 레지스터 기계로 실행한다(스택 기계 바이트코드를 `transfer.rs`가 레지스터 계획으로 번역한다). `Op` enum은 explicit `#[repr(u8)]`로 고정되어 있고, `Proto`는 `code`/`consts`/`names`/`protos`/`source_map`을 owns한다.
- `rust/src/runtime/scope.rs`는 부모 체인을 갖는 슬롯 기반 심볼 테이블을 제공한다. `Scope`의 slot table에서 `Set`은 declaring scope에 write하고(`for` body의 `sum += v` 축적), `SetLocal`/`SetLocalOrDeclare`는 local 전용, `Declare`는 덮지 않는다.
- `rust/src/runtime/world/`는 전역 상태(`path`, `global_scope`, `natives`, `stdout`, `allowed`)를 owns하고 모듈 캐시를 관리한다(책임별로 `world/` 하위 모듈로 분할됨).
- `rust/src/native/`는 표준 라이브러리 모듈(`io`, `fs`, `re`, `math`, `iter`, ...)이다. 네이티브 모듈은 `Global`과 같은 방식으로 어셈블리 없이 리플렉션으로 자동 발견되지 않고, `native::init`이 명시적으로 등록한다.
- `rust/playground/`는 `unvm`을 웹 플레이그라운드용으로 감싼 WASM 진입점(`PlaygroundRunner.RunCode`의 Rust 대응)이다.

## 프로젝트 구조

```
Un/
|-- README.md                  (영문 사용자 문서; 국문: docs/README.ko.md)
|-- LICENSE / AGENTS.md / BENCHMARK.md
|-- rust/
|   |-- Cargo.toml / rust-toolchain.toml
|   |-- src/
|   |   |-- main.rs          (CLI: run/compile/check/native/obj/build/lsp/help +
|   |   |                    tokens/ast/desugar/opt/dis/dis-reg 진단 서브커맨드)
|   |   |-- lib.rs
|   |   |-- bytecode/        (opcodes, instruction, Program, Proto, unbc container)
|   |   |-- lexer.rs / parser.rs / optimize.rs / compiler.rs / vm/ / module.rs
|   |   |-- jit_bc.rs / jit_cache.rs / tir.rs / objify.rs / natcode.rs
|   |   |-- runtime/         (obj, collections, iters, function, world, scope, date, ...)
|   |   `-- native/          (io, math, iter, os, sys, fs, atomic, codes, flow,
|                            types, time, random, re, http, ws, inspect)
|   |-- playground/          (WASM 진입점, unvm-playground 재빌드 대상)
|   |-- nrt/                 (libnrt.a — VM 없는 plain un build용 네이티브 런타임)
|   `-- target/              (생성된 빌드 산출물, gitignore)
|-- main.un                 (사용자의 스크래치; git에 추적 중)
|-- test/                   (.un 테스트 스크립트와 픽스처)
|   |-- unit/ integration/ regression/ compile/ jit/
|   |-- negative/        (exit code와 stderr를 확인하는 스크립트와 manifest.txt)
|   |-- fixtures/        (circular/, rel_test/, dirmod/, dirall/)
|   |-- MANIFEST.txt     (group|path 행)
|   |-- KnownFailures.txt / KnownDivergent.txt
|   `-- assert.un        (assert 실행 검사)
|-- scripts/             (test.ps1, bench.ps1, bench-report.ps1, compile-check.ps1,
|                        compile-bench.ps1, differential.ps1, unbc-roundtrip.ps1,
|                        obj-demo.ps1, exe-diff.ps1, edge-fuzz.ps1)
|-- bench/               (벤치마크 스크립트, bench/{node,python}/, bench/results/ — gitignore)
|-- docs/                (guides/, spec/, project/, compile/, audit/, README.md)
`-- .github/workflows/build.yml  (GNU toolchain 빌드 + 12개 검증 스텝)
```

## 예제

Floyd-Warshall (`test/integration/algorithms.un`):

```un
n = 4
INF = 99999999
graph = [[0,5,INF,10],[INF,0,3,INF],[INF,INF,0,1],[INF,INF,INF,0]]
for k in 0..n
    for i in 0..n
        for j in 0..n
            if graph[i][j] > graph[i][k] + graph[k][j]
                graph[i][j] = graph[i][k] + graph[k][j]
```

Property (`test/unit/runtime/property.un`):

```un
use io
class Temperature
    celsius = 0.0
    get fahrenheit -> self.celsius * 9 / 5 + 32
    set fahrenheit
        self.celsius = (value - 32) * 5 / 9

t = Temperature()
t.fahrenheit = 212
io.write(t.celsius)
io.write(t.fahrenheit)
```

범위와 지연 평가 (`test/unit/runtime/range.un`), 바인딩 생략 for (`test/unit/runtime/for_no_bind.un`):

```un
use io
total = 0
for n in 1..10
    total += n
io.write(total)
io.write((0..10).map(fn(x) -> x * 2).take(3))
```

Heap (`test/integration/algorithms.un`):

```un
class Heap
    data = []
    fn push(x)
        self.data.add(x)
        self.up(len(self.data)-1)
    fn pop()
        top = self.data[0]
        self.data[0] = self.data[len(self.data)-1]
        self.data.pop()
        self.down(0)
        -> top
```

더 많은 예제는 `test/test_suite.un`, `test/integration/algorithms.un`,
`test/unit/runtime/classes.un`, `bench/`에서 확인할 수 있다. 예제를 실행하는 절차는
`docs/guides/TEST_GUIDE.md`와 `docs/guides/BENCHMARK_GUIDE.md`에 있다.

## 로드맵

구현 완료:

- 바이트코드 컴파일러와 레지스터 기반 VM (lexer, parser, desugarer, optimizer, compiler, vm)
- 핫 패스 Int 부분집합(loops, 산술, 비트연산, iter.range, FusedIntMulAdd, IntVec/IntDict intrinsic, Int 필드 위주 클래스 메서드, math Float intrinsic)을 native 코드로 컴파일하는 Cranelift JIT (UV_JIT=0으로 비활성화, UV_JIT=2로 strict canary). Int-Int 전용 `d[k] = v` / `d[k]` IntDict intrinsic과 Bool 반환도 포함.
- 제네릭 함수/클래스(`fn f[T]`, `class Box[T]`)와 호출부 추론, 명시적 타입 인자(`f[int](42)`), 제약(`[T: int | float]`)
- 타입 별칭(`type Id = int | str`), 패턴 가드·or-pattern·중첩 구조 분해, `match`/`un check`의 `is` 기반 타입 좁히기
- 바이트 리터럴(`b"..."`), 12개 함수의 `bytes` 모듈, `fs.read_bytes`/`fs.write_bytes` 바이너리 파일 I/O — plain `un build`에서 네이티브 지원
- 페이로드를 갖는 enum variant(`Test.a(42)`)와 `match` 분해
- Compile Mode(`un check`/`un native`)와 VM 없는 독립 실행형 `un build`/`un obj` (libnrt.a)
- LSP 언어 서버(`un lsp`)와 VS Code 확장 `un-language`
- `un build`(plain·`--embed`)의 기본 fully typed mode와 escape hatch `--allow-dynamic` (`un run`은 동적 유지)
- no-VM AOT 커버리지 확대: `bytes` 모듈·바이트 리터럴, `http`/`ws`, tuple/set, 슬라이스 읽기·할당, FString·삼항, 빈 리터럴, `list.clear()`/`list.resize()`
- 명시적 상대 import(`use .foo` / `use ..pkg`)와 LSP 사용자 모듈 멤버 자동완성·크로스 파일 호버/정의로 이동
- 메모리 계약 문서화(D-093): 참조 카운팅, GC/Weak 없음, 유계 순환, 실행 경계 JIT 정리
- 런타임 객체 모델과 타입 시스템
- 표준 라이브러리 모듈
- 비동기 실행과 워커 풀
- 클래스, 다중 상속, `enum`, 필드 목록을 받는 클래스(`class Point(x, y)`)
- 클래스 Property 접근자(`get`/`set`, `field`, `value`)
- 범위 연산자 `a..b`와 지연 평가 이터레이터 체이닝
- 패턴 매칭과 구조 분해
- 모듈과 선택적 import
- 슬라이싱, 삼항 표현식, null/에러 전파
- `try`/`catch`/`finally`, `using`, `defer`

진행 중:

- 점진적 타입 어노테이션 검증

계획됨:

- 더 강한 정적 타입 검사
- 패키지 매니저
- 컴파일 언어 잔여 항목(`docs/project/COMPILER_LANGUAGE_ROADMAP.md`): class/variant pattern AOT와 named field/spread, `func(..)` 반환 타입, 상속/`__is__` 타입 패턴, `go`/`wait` 진짜 병렬(RFC), 빌드 캐시

## 기여하기

1. 저장소를 포크하고 클론한다.
2. `rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml`로 빌드하고 경고와 오류가 0개인지 확인한다.
3. `feat/my-feature` 형식의 브랜치를 만든다.
4. `test/`의 알맞은 그룹에 `.un` 테스트를 추가하고 `test/MANIFEST.txt`에 `group|path`
   행을 등록한다. exit code와 stderr를 확인하는 테스트는
   `test/negative/manifest.txt`에 `path|expected exit|expected substring` 행으로 등록한다.
5. 풀 리퀘스트를 연다. 커밋은 기여자가 직접 수행한다. 자동화 도구는 git에 푸시하지 않는다.

코딩 컨벤션: Rust 2021 edition을 쓴다. 모듈은 `//!` 설명과 함께 파일 상단에 문서를 두고, `?` 연산자로 오류를 전파하며, 패턴 매칭을 적극적으로 쓴다. 외부 crate 의존성은 기본적으로 없고 표준 라이브러리만 쓴다. 예외로 `vendor/` 아래 패치된 `region 3.0.2`, `cranelift-jit 0.137.1`, `wasmtime-internal-jit-icache-coherence 50.0.1`이 Cranelift JIT에 쓰인다 — GNU toolchain에 `dlltool.exe`가 없어 `windows-sys 0.52.0`을 빌드할 수 없기 때문에 raw kernel32 FFI로 패치한 사본이다.

## 라이선스

MIT License. Copyright (c) 2024 Usejun.

이 소프트웨어와 관련 문서 파일의 사본을 얻은 모든 사람에게, 사용, 복사, 수정, 병합, 출판, 배포, 서브라이선스, 판매 권한을 포함하여 소프트웨어를 제한 없이 다룰 수 있는 권한이 무료로 부여된다.
