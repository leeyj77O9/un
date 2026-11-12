# Un

간결한 문법, 명시적 비동기 실행, 패턴 매칭을 제공하는 현대적인 스크립팅 언어.

Un은 Rust로 작성된 바이트코드 컴파일러와 스택 기반 VM(`unvm`)으로 구현되어 있다. 처리 파이프라인은 다음과 같다.

`Source -> Lexer -> Parser -> Desugarer -> Optimizer -> Bytecode Compiler -> Stack VM`

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.99-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust" />
  <img src="https://img.shields.io/badge/license-MIT-22C55E?style=flat-square" alt="MIT License" />
</p>

## 소개

Un은 읽기 쉽고 간결한 프로그램을 작성하기 위한 동적 타입 스크립팅 언어이다. Python 스타일의 들여쓰기 기반 블록, 명시적인 동시성 기본 요소, 구조적 패턴 매칭, 작은 표준 라이브러리를 결합한다.

Un은 Rust로 작성되었다: 소스 파일을 렉싱/파싱한 뒤 AST로 역탈당(desugar), 최적화(상수 폴딩, 최대 64패스)하고, 바이트코드로 컴파일해 스택 기반 VM에서 실행한다. 이전에 C#/.NET 8 트리 워킹 인터프리터로 구현되어 있었으나 Rust 구현이 C# 구현의 의미론을 byte-identical로 재현함을 differential 검증으로 확인하고 대체했다(문서화된 차이 4개는 `test/KnownDivergent.txt` 참고).

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
- `use`, 별칭, 선택적 import를 갖는 모듈 시스템
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

- 선택적 타입 어노테이션: `x: int`, `a: list[int]`, `dict[str, int]`, `int | str`
- 어노테이션은 현재 가독성과 점진적 검사를 위한 용도이며, 엄격한 런타임 검사는 강화 중이다

### 동시성 (안정)

- 백그라운드에서 표현식을 실행하고 `Future`를 반환하는 `go`
- `Future`를 대기하는 `wait`
- `map`과 `submit`을 제공하는 `flow.spawn(n)` 워커 풀

### 계획됨

- 더 강한 정적 타입 검사
- LSP 및 편집기 통합
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

- Rust toolchain (`stable-x86_64-pc-windows-gnu`, Rust 1.99+). Linux/macOS에서는 MSVC/`gcc` 대신 GNU 링크를 사용한다.

### 빌드

```bash
rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml
```

Windows 산출물은 `rust\target\release\unvm.exe`이다.

### 실행

```bash
.\rust\target\release\unvm.exe run path/to/file.un
```

### 첫 프로그램

```un
use io

fn main()
    io.write("Hello, Un!")

main()
```

`hello.un`으로 저장한 뒤 실행한다.

```bash
.\rust\target\release\unvm.exe run hello.un
```

### 테스트와 벤치마크

빌드와 스크립트 실행:

```bash
rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml
.\rust\target\release\unvm.exe run test/test_suite.un
```

전체 테스트는 runner로 실행한다. runner는 `test/MANIFEST.txt`의 `group|path` 행을
실행 파일로 실행하고 exit code와 stdout의 검사 수를 세고,
`test/negative/manifest.txt`의 `path|exit|substring` 행으로 stderr와 exit code를
확인하는 스크립트를 실행한다.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test.ps1
```

`test/KnownFailures.txt`에 있는 파일은 runner가 KNOWN FAIL로 기록하고,
Positive 집계의 통과·실패 수에 포함하지 않는다. `test/KnownDivergent.txt`에 있는 파일은
구현 간 동작이 의도적으로 다른 문서화된 차이로 기록되어 Positive/Negative 집계의 실패 수에
포함하지 않는다. 현재 기준은 Positive 39 파일 / 1363 passed / 0 failed / 0 crashed,
Negative 42 checks 중 38 매칭 + 4 documented divergences, Known 1 파일이다.

벤치마크는 `bench/`의 스크립트를 같은 절차로 반복 측정하고, 결과를
`bench/results/<timestamp>/results.csv`와 `results.md`에 저장한다.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group all
```

Un과 Node.js를 같은 입력·알고리즘·측정 절차로 비교하는 규칙은
`docs/BENCHMARK_GUIDE.md`에 있고, 실측 값과 그 한계는 `docs/BENCHMARK_LOG.md`에 있다.
테스트 구조와 규칙은 `docs/TEST_GUIDE.md`에 있다.

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

`flow.spawn(n)`은 워커 풀이다. `pool.map`은 인자마다 결과를 만들고, `pool.submit`은
Future를 돌려준다. 공유 상태를 두고 경쟁이 일어나지 않게 하려면 `atomic.int`/`atomic.bool`
(예: `counter.inc()`)로 원자 연산을 할 수 있다.

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
| `atomic` | `atomic.int(초기값?)`/`atomic.bool(초기값?)`로 `atomic_int`/`atomic_bool` 생성. `get`/`set`/`add`(int만)/`inc`/`dec`(int만)/`exchange`/`compare_and_swap`, `Interlocked` 기반 |
| `io` | `write`, `read`, `open`, `clear` |
| `fs` | `read`/`write`/`append`/`touch`, `exists`/`file`/`dir`, `list`/`files`/`dirs`/`walk`, `copy`/`move`/`rename`/`delete`, `mkdir`/`rmdir`, `size`/`created`/`modified`/`accessed`, `join`/`parent`/`name`/`stem`/`ext`, `absolute`/`relative`/`normalize`, `cwd`/`chdir`, `temp`/`home`/`exe`/`app` |
| `re` | `test`/`match`/`search`, `find_all`/`groups`, `replace`/`replace_all`/`split`/`escape` |
| `math` | `round`/`abs`/`ceil`/`floor`/`trunc`/`sign`, `sqrt`/`cbrt`/`pow`/`exp`/`log`/`log2`/`log10`, `sin`/`cos`/`tan`/`asin`/`acos`/`atan`/`atan2`/`hypot`, `gcd`/`lcm`/`clamp`, `is_nan`/`is_infinite`/`is_finite`, `pi`/`e`/`tau` |
| `iter` | `array`/`range`/`counter`/`reverse`/`repeat`, `zip`/`enumerate`, `map`/`filter`/`take`/`skip`/`chain`/`flatten`, `sum`/`max`/`min`/`sorted`, `all`/`any`/`count` |
| `random` | `int`/`float`/`bool`/`bytes`/`chance`, `choice`/`choices`/`sample`/`shuffle`/`weighted`, `range`/`str`/`value`/`next`/`seed`/`uuid` |
| `os` | `env`/`setenv`/`unsetenv`/`environ`/`expand_vars`, `pid`/`name`/`arch`/`version`/`cpuCount`/`is64bit`/`hostname`/`username`, `args`/`exit`/`exec`, `sep`/`pathsep`/`linesep`/`tickCount` |
| `sys` | `env`/`environ`/`expand_vars`, `pid`/`name`/`arch`/`version`, `getcalldepth`/`setcalldepth`/`getrecursionlimit`/`setrecursionlimit` |
| `time` | `sleep`(int=ms, float=초)/`now`/`utc_now`/`epoch`/`epoch_ms`/`from_epoch`/`from_epoch_ms`/`monotonic`/`monotonic_ms`/`stopwatch` |
| `flow` | `spawn`(pool)과 `lock` |
| `http`/`ws` | `http.connect().get`/`post`/`put`/`patch`/`delete`/`head`/`options`와 `_async` 변형, `ws.connect().send`/`receive`/`close`/`is_open` |
| `inspect` | `attr`/`global`/`getattr`/`hasattr`/`setattr`/`delattr`/`meta` |

컬렉션 메서드로는 `list.add`/`insert`/`extend`/`remove`/`remove_at`/`index_of`/`contains`/`order`/`sort`/`reverse`/`binary_search`/`hpush`/`hpop`/`pop`/`map`/`resize`, `dict.add`/`get`/`remove`/`keys`/`values`/`contains_key`/`contains_value`/`clear`, `set.add`/`union`/`intersect`/`difference`/`contains`/`remove`/`clone`/`clear`, 그리고 `split`/`join`/`trim`/`center`/`left`/`right`/`replace`/`find`/`sub_str`/`to_upper`/`to_lower`/`starts_with`/`ends_with` 같은 문자열 메서드가 있다.

모듈 없이 전역에 있는 내장 함수는 `len`/`type`/`hash`/`exit`/`throw`/`throw_if`/`throw_unless`/`chr`/`ord`/`bin`/`oct`/`hex`/`repr`/`ascii`이고, 생성자로는 `int`/`float`/`str`/`bool`/`list`/`set`/`tuple`/`date`가 있다. `math.pi`, `math.e`, `math.tau`, `os.sep`, `os.pathsep`, `os.linesep`, `os.cpuCount`, `os.is64bit` 같은 값은 함수로 노출되므로 호출해서 쓴다.

`iter`의 메서드 체이닝(`(0..10).map(fn(x) -> x * 2).take(3)`)은 지연 평가되어 소비할 때만 실행되고, 전역 `iter.map`/`iter.filter`/`iter.take`은 인자를 즉시 평가해 `list`를 반환한다. `iter.zip`는 두 소스를 같은 속도로 소비하므로 무한 `iter.counter`와 유한 범위를 함께 쓸 수 있다.

`skip`은 예약어이므로 `iter.skip(...)`나 `.skip(...)`는 파싱 오류로 끝나 이름으로 호출할 수 없다. 앞 부분을 잘라내려면 `take`나 슬라이싱을 쓴다.

## 아키텍처

바이트코드 컴파일러와 스택 기반 VM 위에서 동작한다.

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
Desugarer      복합 대입 확장, if/elif 정규화
  |
  v
Optimizer      상수 폴딩, 대수적 단순화, 최대 64패스
  |
  v
Bytecode Compiler   Opcode/Proto/Program, Instruction(flags, operand) 인코딩
  |
  v
Stack VM        Context / Scope / Frame 기반 실행, go/wait를 위한 Future
  |
  v
Runtime         Obj / Val / Ref / Enu / Stru / TObj / Property와 타입 시스템
  |
  v
Stdlib          NativeModule / NativeType 리플렉션 바인딩
```

주요 구성 요소:

- `rust/src/compiler.rs`는 역탈당·최적화된 AST를 바이트코드 Proto로 컴파일한다.
- `rust/src/vm.rs`는 Proto를 스택 기계로 실행한다. `Op` enum은 explicit `#[repr(u8)]`로 고정되어 있고, `Proto`는 `code`/`consts`/`names`/`protos`/`source_map`을 owns한다.
- `rust/src/runtime/scope.rs`는 부모 체인을 갖는 슬롯 기반 심볼 테이블을 제공한다. `Scope`의 slot table에서 `Set`은 declaring scope에 write하고(`for` body의 `sum += v` 축적), `SetLocal`/`SetLocalOrDeclare`는 local 전용, `Declare`는 덮지 않는다.
- `rust/src/runtime/world.rs`는 전역 상태(`path`, `global_scope`, `natives`, `stdout`, `allowed`)를 owns하고 모듈 캐시를 관리한다.
- `rust/src/native/`는 표준 라이브러리 모듈(`io`, `fs`, `re`, `math`, `iter`, ...)이다. 네이티브 모듈은 `Global`과 같은 방식으로 어셈블리 없이 리플렉션으로 자동 발견되지 않고, `native::init`이 명시적으로 등록한다.
- `rust/playground/`는 `unvm`을 웹 플레이그라운드용으로 감싼 WASM 진입점(`PlaygroundRunner.RunCode`의 Rust 대응)이다.

## 프로젝트 구조

```
Un/
|-- README.md
|-- LICENSE
|-- rust/
|   |-- Cargo.toml
|   |-- src/
|   |   |-- main.rs          (CLI 진입점: run/compile/tokens/ast/desugar/opt/dis/help)
|   |   |-- lib.rs
|   |   |-- bytecode/        (opcodes, instruction, Program, Proto, unbc container)
|   |   |-- lexer.rs / parser.rs / desugar.rs / optimize.rs / compiler.rs / vm.rs / module.rs
|   |   |-- runtime/         (obj, collections, iters, function, world, scope, date, ...)
|   |   |-- native/          (Builtin, Fs, Http, Inspect, IO, Iter, Math, Os, Random, Re, Sys, Time, Ws)
|   |-- playground/          (WASM 진입점, un-playground 재빌드 대상)
|   `-- target/              (생성된 빌드 산출물, gitignore)
|-- main.un                 (사용자의 스크래치; 추적하지 않음)
|-- test/                  (.un 테스트 스크립트와 픽스처)
|   |-- unit/            (unit/lexer, unit/parser, unit/runtime, unit/native)
|   |-- integration/
|   |-- regression/
|   |-- negative/        (exit code와 stderr를 확인하는 스크립트와 manifest.txt)
|   |-- fixtures/        (circular/, rel_test/, dirmod/, dirall/)
|   |-- MANIFEST.txt     (group|path 행)
|   |-- KnownFailures.txt
|   |-- KnownDivergent.txt   (문서화된 구현 간 차이)
|   `-- assert.un        (assert 실행 검사)
|-- scripts/             (test.ps1, bench.ps1)
|-- bench/               (벤치마크 스크립트, bench/node/, bench/results/)
`-- docs/                (프로덕션 문서; rust-migration/에 Rust 포트 역사 보존)
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
`docs/TEST_GUIDE.md`와 `docs/BENCHMARK_GUIDE.md`에 있다.

## 로드맵

구현 완료:

- 바이트코드 컴파일러와 스택 기반 VM (lexer, parser, desugarer, optimizer, compiler, vm)
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
- LSP와 VS Code 확장
- 패키지 매니저

## 기여하기

1. 저장소를 포크하고 클론한다.
2. `rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml`로 빌드하고 경고와 오류가 0개인지 확인한다.
3. `feat/my-feature` 형식의 브랜치를 만든다.
4. `test/`의 알맞은 그룹에 `.un` 테스트를 추가하고 `test/MANIFEST.txt`에 `group|path`
   행을 등록한다. exit code와 stderr를 확인하는 테스트는
   `test/negative/manifest.txt`에 `path|expected exit|expected substring` 행으로 등록한다.
5. 풀 리퀘스트를 연다. 커밋은 기여자가 직접 수행한다. 자동화 도구는 git에 푸시하지 않는다.

코딩 컨벤션: Rust 2021 edition을 쓴다. 모듈은 `//!` 설명과 함께 파일 상단에 문서를 두고, `?` 연산자로 오류를 전파하며, 패턴 매칭을 적극적으로 쓴다. 외부 crate 의존성은 없고 표준 라이브러리만 쓴다.

## 라이선스

MIT License. Copyright (c) 2024 Usejun.

이 소프트웨어와 관련 문서 파일의 사본을 얻은 모든 사람에게, 사용, 복사, 수정, 병합, 출판, 배포, 서브라이선스, 판매 권한을 포함하여 소프트웨어를 제한 없이 다룰 수 있는 권한이 무료로 부여된다.