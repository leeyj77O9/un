# AGENTS.md

이 문서는 이 저장소에서 작업하는 AI 코딩 에이전트를 위한 프로젝트 가이드다.
사람이 읽는 문서는 `README.md`(한국어)와 `docs/README.md`(영어)를 참고한다.

## 프로젝트 개요

- **Un**: 들여쓰기 기반 문법, 패턴 매칭, `go`/`wait` 비동기 실행을 제공하는 동적 타입 스크립팅 언어.
- **구현**: C# / .NET 8 트리 워킹 인터프리터. 바이트코드나 별도 VM은 없다.
- **파이프라인**: `Lexer -> Parser -> Desugarer -> Optimizer -> Evaluator`
- **규모**: C# 약 83개 파일 / 11,750줄, `test/` 아래 `.un` 스크립트 28개.

## 빠른 명령

```bash
# 빌드 (항상 Release 사용)
dotnet build -c Release

# 스크립트 실행
dotnet run -c Release -- run test/test_suite.un

# 빌드 산출물로 직접 실행 (Windows)
.\bin\Release\net8.0\Un.exe run test\test_suite.un

# 전체 테스트 runner ( Positive + Negative + KnownFailures 집계 )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test.ps1

# 벤치마크 runner ( -Group un|node|all, -Rebuild, -NoSave )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group all
```

- 이 개발 머신에는 .NET 10 런타임만 설치되어 있다. `net8.0` 앱 실행이 실패하면
  `DOTNET_ROLL_FORWARD=Major`를 설정한다(PowerShell: `$env:DOTNET_ROLL_FORWARD='Major'`).
- 현재 `dotnet build -c Release`는 오류 0개, 경고 0개로 끝난다. **새 경고를 추가하지 말 것.**

## 디렉터리 구조

```text
C:\dev\un
├─ AGENTS.md                 # 이 문서 (에이전트용, 커밋 대상)
├─ Un.sln / Un.csproj        # .NET 8, ImplicitUsings/Nullable 활성화
├─ README.md                 # 한국어 문서
├─ docs/README.md            # 영어 문서 (README.md와 동기화 유지)
├─ src/
│  ├─ Program.cs             # CLI 진입점 (run/help)
│  ├─ PlaygroundRunner.cs    # 메모리 FS 기반 샌드박스 실행기
│  ├─ main.un                # 개인용 스크래치 스크립트 (제품 진입점 아님)
│  ├─ Core/
│  │  ├─ Global.cs           # 전역 스코프, 모듈 로딩/캐시, 네이티브 바인딩
│  │  ├─ IO/IFileSystem.cs   # PhysicalFileSystem / MemoryFileSystem
│  │  ├─ Native/             # 표준 라이브러리 모듈 (io, fs, re, math, iter, ...)
│  │  └─ Object/             # 런타임 객체 모델
│  │     ├─ Base/            # Obj, Val, Ref, Enu, Stru, TObj, Template
│  │     ├─ Collections/     # List, Dict, Set, Tup, Json, Spread
│  │     ├─ Flow/            # Lock, Pool
│  │     ├─ Function/        # Fn, LFn, NFn, Arg, Future
│  │     ├─ IO/              # Stream, HttpClient, WebSocket
│  │     ├─ Iters/           # Iter, Range, Counter, Repeat, Reverse
│  │     ├─ Primitive/       # Int, Float, Bool, Str, Date
│  │     ├─ Type/            # BaseType, CollectionType, UnionType, UnType
│  │     └─ Util/            # Random, Stopwatch
│  └─ Interpreter/
│     ├─ Lexer/              # 토크나이저 (들여쓰기 인식)
│     ├─ Parser/             # AST 생성 (Node, NodeKind)
│     ├─ Desugarer/          # 복합 대입 확장, if/elif 정규화
│     ├─ Optimizer/          # 상수 폴딩/대수 단순화 (최대 64패스)
│     ├─ Evaluator/          # AST 평가 (Evaluator, Flows)
│     ├─ Runner/             # Runner, Context, Scope, Frame, Source
│     ├─ Reflection/         # Native/Builtin 어트리뷰트와 바인딩
│     ├─ Error/              # Error(위치 포함), Panic(전역)
│     └─ Util/               # ISpan 등
 ├─ scripts/                  # test.ps1, bench.ps1 (runner)
 ├─ bench/                    # 측정 스크립트, bench/node/, bench/results/
 └─ test/                     # .un 테스트/픽스처
    ├─ unit/                  # unit/lexer, unit/parser, unit/runtime, unit/native
    ├─ integration/           # algorithms, concurrency, modules, recursion
    ├─ regression/            # defects.un (문서화된 동작 기준)
    ├─ negative/              # exit code + stderr 확인 + manifest.txt
    ├─ fixtures/              # circular/, rel_test/, dirmod/, dirall/, circular_main.un
    ├─ MANIFEST.txt           # group|path 행
    ├─ KnownFailures.txt      # runner가 KNOWN FAIL로 기록하는 파일
    └─ assert.un              # assert 모듈 (assert_eq / assert_true)
```

## 아키텍처 메모

- `Runner.RunCore`가 파싱 → desugar → optimize → evaluate를 순서대로 수행한다.
  `Runner.Load`는 `Global.PATH` 기준으로 파일을 읽고 CRLF를 LF로 정규화한다.
- `Scope`는 부모 체인을 갖는 슬롯 기반 심볼 테이블이다.
- 대입 스코프 규칙: 함수 본문(`Scope.IsFunctionScope`)의 대입은 새 로컬 변수를 만들어
  외부 바인딩을 가린다. `for`/`match`/`catch` 같은 블록 스코프의 대입은 기존 부모
  바인딩을 갱신한다. 덕분에 루프 안의 누적 변수(`sum += v`)가 정상 동작한다.
- `Context`는 스코프, 소스, 콜 프레임, `defer`/`using` 스택을 가진다.
- `Global`은 정적 전역 상태다. `Init`으로 내장/네이티브 모듈을 등록하고,
  네이티브 모듈은 어셈블리 리플렉션으로 자동 발견된다.
- 모듈 import는 현재 파일 기준 상대 경로를 먼저 찾고, 없으면 `Global.PATH` 기준으로
  찾는다. 디렉터리 모듈은 `mod.un` 또는 디렉터리 내 모든 `.un` 파일을 로드한다.
- `PlaygroundRunner`는 `MemoryFileSystem`, 메모리 stdout, 허용 모듈 목록을 사용하고
  `finally`에서 전역 상태를 복원한다. 이 코드를 수정할 때는 복원 누락에 주의한다.

## 코드 컨벤션

- C# 12 스타일을 따른다: file-scoped namespace, primary constructor, 컬렉션 식(`[]`),
  target-typed `new(...)`, 패턴 매칭.
- 네임스페이스는 폴더 구조를 따른다: `Un`, `Un.Object`, `Un.Native`, `Un.Reflection`.
- 런타임 값은 `Obj` 계층(`Val<T>`, `Ref<T>`, 컬렉션, `Fn`/`NFn`)으로 표현한다.
- 사용자에게 보여 줄 오류는 가능하면 `new Err("...")` 값으로 반환한다.
  복구 불가능한 인터프리터/환경 오류는 `Panic`, 위치 정보가 필요한 언어 오류는
  `Error(message, node, context.Source)`를 사용한다.
- 기존 파일의 스타일과 명명을 우선한다. 불필요한 리팩터링이나 패키지 추가는 하지 않는다.
- `README.md`와 `docs/README.md`는 같은 내용의 한/영 쌍이다. 사용자 기능을 바꾸면
  두 문서를 함께 갱신한다.

### 네이티브 모듈 추가

`src/Core/Native/`에 정적 클래스를 만들고 어트리뷰트로 등록한다. `Global`이 리플렉션으로
자동 발견하므로 별도 등록 코드는 필요 없다.

```csharp
[NativeModule("mymod")]
public static class MyMod
{
    [Native(
        Name = "add",
        Description = "Adds two integers.",
        Example = "mymod.add(1, 2)",
        ReturnType = "int",
        ArgumentTypes = new[] { "int", "int" }
    )]
    public static Obj Add(
        [ArgInfo(Positional = true)] Obj a,
        [ArgInfo(Positional = true)] Obj b)
    {
        if (!a.As<Int>(out var x) || !b.As<Int>(out var y))
            return new Err("expected int arguments");

        return Int.From(x.Value + y.Value);
    }
}
```

- 메서드는 `Obj`를 반환하고, 인자는 `Obj`로 받는다. `self`가 필요하면 `[Self]`를 쓴다.
- 인자 메타데이터는 `[ArgInfo(Name=..., Essential=..., Optional=..., Positional=..., Keyword=...)]`.
- 전역 내장 함수는 `[BuiltinModule("name")]`, 내장 타입은 `[BuiltinType("name")]`,
  네이티브 타입은 `[NativeType("name")]`으로 등록한다.
- 샌드박스(`PlaygroundRunner.DefaultAllowed`)에서 새 모듈을 허용할지는 별도로 판단한다.

### 언어 기능 추가 체크리스트

새 문법/연산을 추가할 때는 보통 아래를 모두 건드린다.

1. `Lexer/TokenType.cs`, `Lexer/Token.cs`, `Lexer/Lexer.cs` — 토큰/렉싱.
2. `Parser/NodeKind.cs`, `Parser/Node.cs`, `Parser/Parser.cs` — AST 생성.
3. `Desugarer/Desugarer.cs` — 자식 순회 및 확장 규칙.
4. `Optimizer/Optimizer.cs` — 상수 폴딩/단순화가 가능한 경우.
5. `Evaluator/Evaluator.cs` (또는 `Flows.cs`) — 실제 평가.
6. `test/`의 알맞은 그룹(`unit/…`, `integration/`, `regression/`, `negative/`)에 `.un`
   스크립트를 추가하고, `test/MANIFEST.txt`에 `group|path` 행을 등록한다. exit code와
   stderr를 확인하는 스크립트는 `test/negative/manifest.txt`에
   `path|expected exit|expected substring` 행으로 등록한다.
7. `README.md` + `docs/README.md`의 기능/문법 표 갱신.

## 테스트와 검증

- 통합 테스트 러너(`.csproj`)는 없다. runner는 PowerShell 스크립트이고, 빌드 산출물로
  `.un` 스크립트를 실행한다: `scripts/test.ps1`(test), `scripts/bench.ps1`(bench).
- `scripts/test.ps1`은 `test/MANIFEST.txt`의 `group|path` 행을 실행하고 exit code와
  stdout의 `N passed, M failed`를 집계한다. 정상 기준은 **Positive 31 files /
  1150 passed / 0 failed / 0 crashed**, Negative 36 checks 매칭, Known 1 file이다.
  runner는 `[FAIL]`이 증가하면 exit 1로 끝난다.
- `test/negative/manifest.txt`는 `path|expected exit|expected substring` 행으로
  stderr와 exit code를 검증한다(파싱 오류, 미처리 예외, stack overflow 포함).
- `test/KnownFailures.txt`의 파일은 runner가 KNOWN FAIL로 기록하고 Positive 집계에
  포함하지 않는다. 현재 1개 파일(`test/regression/defects.un`, 10 assertions)이
  문서화된 동작과 실제 동작이 다른 결함을 기록한다.
- `test/test_suite.un`은 가장 넓은 스모크 테스트이며 `test/assert.un`을 모듈로 사용한다.
  정상 기준 출력은 `104 passed, 0 failed`이고, 실패 시 `[FAIL]` 라인이 나온다.
- 상대 import/순환 import/디렉터리 모듈 검증용 픽스처는 `test/fixtures/circular/`,
  `test/fixtures/rel_test/`, `test/fixtures/dirmod/`, `test/fixtures/dirall/`에 있다.
- 벤치마크 스크립트는 `bench/`에 있고, Node.js 상대 스크립트는 `bench/node/`에 있다.
  측정은 웜업 + 반복 + 통계(최솟값·중위값·평균·최댓값·표준편차·연산 수)를 사용한다.
  결과와 측정 한계는 `docs/BENCHMARK_GUIDE.md`, `docs/BENCHMARK_LOG.md`를 참고한다.
- 변경 후 최소한 다음을 수행한다.
  1. `dotnet build -c Release` — 오류 0, 새 경고 0.
  2. 변경과 관련된 `test/**.un` 실행.
  3. `scripts/test.ps1` 실행 후 `[FAIL]`과 crashed가 늘지 않았는지 확인.

## 알려진 함정

- `src/Program.cs`의 `#if DEBUG` 블록은 `C:/Users/leeyj/Desktop/dev/un/` 절대 경로와
  `src/main.un`을 하드코딩한다. CLI 동작은 `#if RELEASE`에 있으므로 **항상 `-c Release`로
  실행**한다. Debug 실행이 필요하면 이 블록을 환경에 맞게 수정해야 한다.
- `src/main.un`은 개인 스크래치 파일이며 현재 문법/변수 참조가 유효하지 않을 수 있다.
  테스트나 제품 진입점으로 사용하지 않는다.
- `Global`은 프로세스 전역 정적 상태다. 테스트/플레이그라운드에서 상태를 바꾸면 반드시 복원한다.
- `Global.Init` 호출 전에는 전역 함수(`panic` 등)가 등록되지 않는다.
- 파일 경로는 `Global.PATH` 기준으로 해석된다. CLI는 현재 작업 디렉터리로 초기화한다.
- 이 저장소는 외부 NuGet 패키지에 의존하지 않는다. 새 의존성은 정말 필요할 때만 추가한다.

## Git / 기여 규칙

- 기본 브랜치: `main`. 기능 브랜치는 `feat/my-feature` 형식을 사용한다.
- 커밋은 기여자가 직접 수행한다. **에이전트는 사용자가 명시적으로 요청하지 않는 한
  커밋/푸시하지 않는다.**
- `bin/`, `obj/`와 로컬 에이전트 상태 디렉터리는 `.gitignore`에 의해 제외된다.
  공유 지침은 이 `AGENTS.md`에만 두고 커밋한다.
- 생성된 빌드 산출물이나 도구 캐시를 저장소에 추가하지 않는다.
