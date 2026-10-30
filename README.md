# Un — Modern Scripting Language

<p align="center">
  <img src="https://img.shields.io/badge/.NET-8.0-512BD4?style=for-the-badge&logo=dotnet" />
  <img src="https://img.shields.io/badge/license-MIT-22C55E?style=for-the-badge" />
  <img src="https://img.shields.io/badge/status-beta-orange?style=for-the-badge" />
</p>

<p align="center">
  <b>간결하고 표현력 있는 문법 + 명시적 비동기 + 패턴 매칭을 갖춘 스크립팅 언어</b><br/>
  C# .NET 8로 구현된 인터프리터 · Lexer → Parser → Desugarer → Optimizer → Evaluator
</p>

<p align="center">
  <a href="https://un-playground.vercel.app/"><b>▶ Playground에서 바로 실행하기</b></a> ·
  <a href="#빠른-시작">Quick Start</a> ·
  <a href="#언어-가이드">Language Guide</a> ·
  <a href="#표준-라이브러리">Stdlib</a>
</p>

---

## 목차

- [왜 Un인가](#왜-un인가)
- [주요 기능](#주요-기능)
- [설치](#설치)
- [빠른 시작](#빠른-시작)
- [CLI 사용법](#cli-사용법)
- [언어 가이드](#언어-가이드)
- [표준 라이브러리](#표준-라이브러리)
- [예제](#예제)
- [아키텍처](#아키텍처)
- [테스트](#테스트)
- [기여하기](#기여하기)
- [로드맵](#로드맵)
- [License](#license)

---

## 왜 Un인가

- **읽기 쉬운 문법**: Python처럼 들여쓰기 기반 블록, 최소한의 키워드로 표현
- **명시적 비동기**: `go`로 백그라운드 실행, `wait`로 대기 — 숨은 await 없음
- **안전한 자원 관리**: `using`/`defer`로 파일·락 자동 해제
- **실용적 타입**: `x: int`, `a: list[int]`, `dict[str, int]`, `int | str` 제네릭/유니온 지원
- **배터리 포함**: `fs`/`http`/`re`/`math`/`random`/`iter`/`os`/`time` 등 표준 모듈 내장

---

## 주요 기능

| 영역 | 기능 | 예시 |
|---|---|---|
| **기초** | 변수·상수, 타입 어노테이션, 구조 분해 | `x, y = (1, 2)` / `a: list[int] = [1,2,3]` |
| **함수** | 기본/키워드 인자, 가변인자, 람다, 클로저 | `fn add(a, b=0) -> a+b` / `fn(a,b) -> a*b` |
| **제어** | `if/elif/else`, `for/while`, `break/skip`, `match` | `match x { 0: "zero", _: "other" }` |
| **클래스** | 프로퍼티·메서드, 상속, `Stru`/`Enu` | `class Point(x,y)` / `class Child: Parent` |
| **비동기** | `go`/`wait`, `flow.spawn` 풀, `Future` | `a = go slow(5)` / `wait a` |
| **모듈** | `use`, 별칭, 선택 import | `use io` / `use re as regex` / `use fs {read, write}` |
| **자원** | `using`, `defer` | `using file = open("log.txt")` |
| **문자열** | 보간, Tagged Template | `` `Hello, {name}!` `` / `` html`<div>{x}</div>` `` |
| **컬렉션** | `list`/`dict`/`set`/`tuple`/`json` + 40+ 메서드 | `[1,2,3].map(fn)`, `dict.keys()` |
| **표준** | `fs/http/re/math/random/iter/os/time/ws` | `re.test("\\d+", s)` |

---

## 설치

**요구사항**: .NET SDK 8.0+

```bash
git clone https://github.com/your-org/un.git
cd un
dotnet build -c Release
```

**실행**
```bash
# 스크립트 실행
dotnet run -c Release -- run test/test_all.un

# 또는 빌드 후 직접
./bin/Release/net8.0/Un run path/to/file.un
```

**Playground** (설치 없이 브라우저에서 실행)

> **[Un Playground](https://un-playground.vercel.app/)**

---

## 빠른 시작

### 변수와 타입

```un
x = 10
name = "Un"
value: int = 20

a: list[int] = [1, 2, 3]
b: dict[str, int] = {"x": 10}
c: int | str = 42
d: list[list[int]] = [[1,2],[3,4]]
```

### 함수

```un
fn add(a, b) -> a + b
fn greet(name = "world") -> `hello {name}`

add(3, 4)        # 7
greet()          # hello world
greet("Un")      # hello Un

# 람다
mul = fn(a, b) -> a * b
mul(6, 7)        # 42
```

### 제어 흐름

```un
if x > 5
    write("big")
elif x == 5
    write("equal")
else
    write("small")

for v in [1,2,3]
    write(v)

n = 0
while n < 5
    write(n)
    n += 1

match x {
    0: "zero",
    1: "one",
    _: "other",
}
```

### 비동기

```un
use time

fn slow(x)
    time.sleep(1000)
    -> x * 2

a = go slow(5)
write(wait a)  # 10

# 워커 풀
use flow
using pool = flow.spawn(4)
pool.map(fn(x) -> x*2, 1, 2, 3)
```

### 클래스

```un
class Point(x, y)
    fn len()
        -> self.x + self.y

p = Point(10, 20)
write(p.x)      # 10
write(p.len())  # 30

# 상속
class Child: Point
    fn greet()
        write(`Hi {self.x}`)
```

### 문자열

```un
name = "Un"
write(`Hello, {name}!`)          # Hello, Un!
write("hello".to_upper())         # HELLO
write("a,b,c".split(","))         # ['a','b','c']
html`<div>{name}</div>`           # Tagged Template
```

---

## CLI 사용법

```bash
un run <file.un>   # 파일 실행
un help            # 도움말
```

`src/main.un`은 `DEBUG` 빌드시 자동 실행되는 엔트리포인트입니다.

---

## 언어 가이드

### 모듈 시스템

```un
use io
use fs as filesystem
use re {test, find_all}
use net.http as http

res = wait http.get("https://example.com")
io.write(res)
```

### 구조 분해 & 패턴

```un
a, b = (1, 2)
x, y = point
a, (b, c) = (1, (2, 3))

# match 패턴
match value {
    0: "zero",
    (x, 0): `x={x}`,
    _: "other",
}
```

### 자원 관리

```un
using file = io.open("log.txt", "w")
file.write("hello")
# 블록 종료시 자동 close

# defer (함수 종료시 실행)
fn work()
    defer write("done")
    write("start")
```

### 타입 어노테이션

```un
x: int = 10
y: str = "hi"
a: list[int] = [1,2]
b: dict[str, int] = {}
c: int | none = none
```

> 타입은 현재 문서화/가독성 목적이며 런타임 엄격 검사는 점진적으로 강화 중입니다.

### 연산자

```
산술: + - * / // % **
비트: & | ^ << >> ~
비교: == != < <= > >= is in
논리: and or xor not
기타: . ?. [] [:] *spread **kwspread
```

---

## 표준 라이브러리

| 모듈 | 주요 API |
|---|---|
| `io` | `write`, `read`, `open`, `clear` |
| `fs` | `read/write/append`, `exists/file/dir`, `list/files/dirs/walk`, `copy/move/delete`, `mkdir/rmdir`, `join/parent/name/ext` |
| `re` | `test/match/search`, `find_all/groups`, `replace/replace_all/split/escape` (1초 타임아웃 캐시) |
| `math` | `abs/sqrt/pow/log/sin/cos/tan`, `gcd/lcm/clamp`, `pi/e/tau`, `is_nan/is_infinite` |
| `iter` | `range/counter/reverse/repeat`, `zip/enumerate`, `map/filter/take/skip/chain/flatten`, `sum/max/min/sorted` |
| `random` | `int/float/bool/choice/shuffle`, `sample/choices/weighted`, `seed/uuid` |
| `os` | `env/environ/exec`(10초 타임아웃), `pid/hostname/args`, `sep/pathsep` |
| `time` | `sleep/now/stopwatch` |
| `http/ws` | `http.connect().get/post/put/delete` (+ `*_async`), `ws.connect().send/receive` |
| `inspect` | `attr/hasattr/getattr/setattr` |

컬렉션 메서드: `list.add/extend/sort/reverse/hpush/hpop/lower_bound`, `dict.get/keys/values`, `set.add/union/intersect`, `str.split/join/trim/center/replace`

---

## 예제

**Floyd-Warshall** (`test/test_floyd.un`)

```un
n = 4
INF = 99999999
graph = [[0,5,INF,10],[INF,0,3,INF],[INF,INF,0,1],[INF,INF,INF,0]]
for k in range(0,n)
    for i in range(0,n)
        for j in range(0,n)
            if graph[i][j] > graph[i][k] + graph[k][j]
                graph[i][j] = graph[i][k] + graph[k][j]
```

**Heap** (`test/test_heap.un`)

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

더 많은 예제: `test/test_all.un`, `test/test_class.un`, `test/test_for_loop.un`

---

## 아키텍처

```
소스(.un)
   │
   ▼
 Lexer  ── 들여쓰기/토큰화 (TokenType 80+)
   │
   ▼
 Parser ── Node(AST) 생성 (NodeKind 40+)
   │
   ▼
Desugarer ── += → +, if/elif → IfCase 정규화
   │
   ▼
Optimizer ── 상수 폴딩, 대수 최적화, fixed-point 64패스
   │
   ▼
Evaluator ── Context/Scope/Frame 기반 실행, Go/Wait Future
   │
   ▼
Runtime ── Obj/Val/Ref/Enu/Stru/TObj + Type(UnType/Union/Collection)
   │
   ▼
Stdlib ── NativeModule/NativeType 리플렉션 바인딩
```

- **Runner** (`src/Interpreter/Runner/Runner.cs`): `Load → Parse → Desugar → Optimize → Eval`, `Source` 줄/컬럼 추적
- **Scope** (`Scope.cs`): 슬롯 기반 심볼 테이블, 부모 체인
- **Global** (`Core/Global.cs`): `Builtin` + `Native` 모듈 레지스트리, `import`/`include` 해석

---

## 테스트

```bash
# 전체 테스트 (수동 write 기반)
dotnet run -c Release -- run test/test_all.un
dotnet run -c Release -- run test/test_floyd.un
dotnet run -c Release -- run test/test_heap.un

# 검증용 (test/에 저장됨)
dotnet run -c Release -- run test/test_verify_generic.un
dotnet run -c Release -- run test/test_verify_typed_var.un
```

---

## 기여하기

1. Fork & clone
2. `dotnet build -c Release` 로 빌드 확인 (0 경고 0 오류)
3. 브랜치 생성 `feat/my-feature`
4. `test/`에 `.un` 테스트 추가 (검증 파일은 `test/`에 저장)
5. PR 제출 — 커밋은 직접 수행해주세요 (AI는 git에 접근하지 않습니다)

코딩 컨벤션: `ImplicitUsings`/`Nullable` 활성화, `Native`/`BuiltinType` 어트리뷰트 기반 등록 유지

---

## 로드맵

- [ ] 정적 타입 검사 강화
- [ ] LSP / VSCode 확장
- [ ] 패키지 매니저

---

## License

MIT License — 자유롭게 사용·수정·배포할 수 있습니다.

<p align="center">
  Made with C# & .NET 8 · <a href="https://un-playground.vercel.app/">Playground</a>
</p>
