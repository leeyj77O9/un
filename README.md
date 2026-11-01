# Un

간결한 문법, 명시적 비동기 실행, 패턴 매칭을 제공하는 현대적인 스크립팅 언어.

Un은 C#과 .NET 8 위에서 동작하는 트리 워킹 인터프리터로 구현되어 있다. 처리 파이프라인은 다음과 같다.

`Lexer -> Parser -> Desugarer -> Optimizer -> Evaluator`

<p align="center">
  <img src="https://img.shields.io/badge/.NET-8.0-512BD4?style=flat-square&logo=dotnet&logoColor=white" alt=".NET 8" />
  <img src="https://img.shields.io/badge/license-MIT-22C55E?style=flat-square" alt="MIT License" />
</p>

## 소개

Un은 읽기 쉽고 간결한 프로그램을 작성하기 위한 동적 타입 스크립팅 언어이다. Python 스타일의 들여쓰기 기반 블록, 명시적인 동시성 기본 요소, 구조적 패턴 매칭, 작은 표준 라이브러리를 결합한다.

언어는 C#으로 작성된 인터프리터 하나로 구현되어 있다. 별도의 컴파일 단계나 바이트코드 가상 머신은 없다. 소스 파일은 AST로 파싱된 뒤 역탈당(desugar), 최적화, 직접 평가된다.

설계 목표:

- 문법을 작고 읽기 쉽게 유지한다.
- `go`와 `wait`를 통해 비동기 실행을 명시적으로 만든다.
- 실용적인 컬렉션과 입출력 모듈을 기본 제공한다.
- 정적 검사기를 요구하지 않는 점진적 타입 어노테이션을 지원한다.

## 기능

### 언어 핵심 (안정)

- 변수와 구조 분해 할당
- 기본값, 키워드, 가변 인자를 갖는 함수
- 람다와 클로저
- `if` / `elif` / `else`, `for`, `while`, `break`, `skip`
- 패턴 매칭을 지원하는 `match`
- 클래스, 상속, `Stru`, `Enu`
- `use`, 별칭, 선택적 import를 갖는 모듈 시스템
- `using`과 `defer`를 통한 자원 관리
- 문자열 보간과 태그드 템플릿
- `list`, `dict`, `set`, `tuple`, `json` 컬렉션
- 슬라이싱과 인덱싱
- 후위 `?`를 통한 null/에러 전파
- C 스타일 삼항 표현식 `cond ? a : b`
- `try` / `catch` / `finally`

### 타입 시스템 (실험적)

- 선택적 타입 어노테이션: `x: int`, `a: list[int]`, `dict[str, int]`, `int | str`
- 어노테이션은 현재 가독성과 점진적 검사를 위한 용도이며, 엄격한 런타임 검사는 강화 중이다

### 동시성 (안정)

- 백그라운드에서 표현식을 실행하고 `Future`를 반환하는 `go`
- `Future`를 대기하는 `wait`
- `map`을 제공하는 `flow.spawn(n)` 워커 풀

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

- .NET SDK 8.0 이상

### 빌드

```bash
dotnet build -c Release
```

### 실행

```bash
dotnet run -c Release -- run path/to/file.un

# 또는 빌드된 실행 파일 사용
./bin/Release/net8.0/Un run path/to/file.un
```

Windows에서는 실행 파일이 `bin\Release\net8.0\Un.exe`이다.

### 첫 프로그램

```un
use io

fn main()
    io.write("Hello, Un!")

main()
```

`hello.un`으로 저장한 뒤 실행한다.

```bash
dotnet run -c Release -- run hello.un
```

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

함수 본문은 들여쓰기된 블록을 사용한다. 람다는 한 줄에 `->`를 사용한다.

### 제어 흐름

```un
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

루프 안에서 `break`와 `skip`을 사용할 수 있다.

### 패턴 매칭

```un
res = match value {
    0 -> "zero",
    (x, 0) -> `x={x}`,
    _ -> "other",
}
```

### 타입

```un
x: int = 10
y: str = "hi"
a: list[int] = [1, 2]
b: dict[str, int] = {}
c: int | none = none
```

### 비동기 실행

```un
use time

fn slow(x)
    time.sleep(100)
    -> x * 2

a = go slow(5)
io.write(wait a)
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
using file = io.open("log.txt", "w")
file.write("hello")

fn work()
    defer io.write("done")
    io.write("start")
```

## 표준 라이브러리

| 모듈 | 주요 API |
|---|---|
| `io` | `write`, `read`, `open`, `clear` |
| `fs` | `read`/`write`/`append`, `exists`/`file`/`dir`, `list`/`files`/`dirs`/`walk`, `copy`/`move`/`delete`, `mkdir`/`rmdir`, `join`/`parent`/`name`/`ext` |
| `re` | `test`/`match`/`search`, `find_all`/`groups`, `replace`/`replace_all`/`split`/`escape` |
| `math` | `abs`/`sqrt`/`pow`/`log`/`sin`/`cos`/`tan`, `gcd`/`lcm`/`clamp`, `pi`/`e`/`tau` |
| `iter` | `range`/`counter`/`reverse`/`repeat`, `zip`/`enumerate`, `map`/`filter`/`take`/`skip`/`chain`/`flatten`, `sum`/`max`/`min`/`sorted` |
| `random` | `int`/`float`/`bool`/`choice`/`shuffle`, `sample`/`choices`/`weighted`, `seed`/`uuid` |
| `os` | `env`/`environ`/`exec`, `pid`/`hostname`/`args`, `sep`/`pathsep` |
| `time` | `sleep`/`now`/`stopwatch` |
| `http`/`ws` | `http.connect().get`/`post`/`put`/`delete`, `ws.connect().send`/`receive` |
| `inspect` | `attr`/`hasattr`/`getattr`/`setattr` |

컬렉션 메서드로는 `list.add`/`extend`/`sort`/`reverse`/`hpush`/`hpop`, `dict.get`/`keys`/`values`, `set.add`/`union`/`intersect`, 그리고 `split`/`join`/`trim`/`center`/`replace` 같은 문자열 메서드가 있다.

## 아키텍처

인터프리터는 AST 위에서 단일 파이프라인으로 동작한다.

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
Evaluator      Context / Scope / Frame 기반 실행, go/wait를 위한 Future
  |
  v
Runtime        Obj / Val / Ref / Enu / Stru / TObj와 타입 시스템
  |
  v
Stdlib         NativeModule / NativeType 리플렉션 바인딩
```

주요 구성 요소:

- `src/Interpreter/Runner/Runner.cs`는 소스 파일을 읽고 파싱, 역탈당, 최적화, 평가한다. `Source`는 줄과 열 정보를 추적한다.
- `src/Interpreter/Runner/Scope.cs`는 부모 체인을 갖는 슬롯 기반 심볼 테이블을 제공한다.
- `src/Core/Global.cs`는 내장 및 네이티브 모듈을 등록하고 `import` / `include`를 해석한다.
- `src/Interpreter/Reflection/`는 어트리뷰트를 통해 네이티브 모듈과 타입을 바인딩한다.

중간 바이트코드나 별도 가상 머신은 없다.

## 프로젝트 구조

```
Un/
|-- Un.sln
|-- Un.csproj
|-- README.md
|-- LICENSE
|-- src/
|   |-- Program.cs
|   |-- PlaygroundRunner.cs
|   |-- main.un
|   |-- Core/
|   |   |-- Global.cs
|   |   |-- IO/
|   |   |-- Native/        (Builtin, Fs, Http, Inspect, IO, Iter, Math, Os, Random, Re, Sys, Time, Ws)
|   |   `-- Object/       (런타임 객체 모델, 컬렉션, 함수, 흐름, 입출력, 이터레이터, 기본 타입, 타입, 유틸)
|   `-- Interpreter/
|       |-- Lexer/
|       |-- Parser/
|       |-- Desugarer/
|       |-- Optimizer/
|       |-- Evaluator/
|       |-- Runner/
|       |-- Reflection/
|       |-- Error/
|       `-- Util/
|-- test/                  (.un 스크립트와 픽스처)
`-- bin/, obj/             (생성된 빌드 산출물)
```

## 예제

Floyd-Warshall (`test/test_floyd.un`):

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

Heap (`test/test_heap.un`):

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

더 많은 예제는 `test/test_all.un`, `test/test_class.un`, `test/test_new_features.un`에서 확인할 수 있다.

## 로드맵

구현 완료:

- 인터프리터 파이프라인 (lexer, parser, desugarer, optimizer, evaluator)
- 런타임 객체 모델과 타입 시스템
- 표준 라이브러리 모듈
- 비동기 실행과 워커 풀
- 클래스, 상속, enum, struct
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
2. `dotnet build -c Release`로 빌드하고 경고와 오류가 0개인지 확인한다.
3. `feat/my-feature` 형식의 브랜치를 만든다.
4. `test/` 아래에 `.un` 테스트를 추가한다.
5. 풀 리퀘스트를 연다. 커밋은 기여자가 직접 수행한다. 자동화 도구는 git에 푸시하지 않는다.

코딩 컨벤션: `ImplicitUsings`와 `Nullable`이 활성화되어 있다. 네이티브 및 내장 멤버는 어트리뷰트를 통해 등록한다.

## 라이선스

MIT License. Copyright (c) 2024 Usejun.

이 소프트웨어와 관련 문서 파일의 사본을 얻은 모든 사람에게, 사용, 복사, 수정, 병합, 출판, 배포, 서브라이선스, 판매 권한을 포함하여 소프트웨어를 제한 없이 다룰 수 있는 권한이 무료로 부여된다.