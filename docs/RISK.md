# RISK.md

> **Legacy document.** This file describes the pre-replacement C# implementation
> (commit `fe05894`). The C# codebase has since been replaced by the Rust port
> (`rust/`), which reproduces the runtime object model and the same fault surface.
> Kept for historical reference; the current implementation is documented in
> `docs/README.md` and `docs/rust-migration/`.

`main` 브랜치(커밋 fe05894) 기준 코드 리뷰에서 발견된 리스크 목록과 해결 기록.
각각 수정 후 Release 빌드 + `test/test_suite.un`(109 passed) + `scripts/test.ps1` 기준으로 검증했다.

상태 범례: `대기` (미수정) / `해결` (수정 + 검증 완료) / `수용` (수정하지 않고 근거와 함께 수용)

---

## 높음

### R-1. `EvalTry`: catch 핸들러 내부 오류가 원 오류로 마스킹됨

- 위치: `src/Interpreter/Evaluator/Evaluator.cs` `EvalTry`
- 증상: try 블록에서 오류 E1이 발생해 catch로 진입한 뒤, catch 블록에서 다시 오류 E2가 나면 `pendingError`가 여전히 E1을 가리킨 채 finally의 `throw pendingError`가 **E2를 지워 버리고 E1을 다시 던진다**. 핸들러 내부의 진짜 원인이 사용자에게 보이지 않는다.
- 해결: 내부 catch의 `catch (Error)`에서 `pendingError = null`로 초기화하고 `throw`한다. finally는 `pendingError == null`이면 rethrow하지 않으므로 catch 블록의 오류 E2가 그대로 전파된다.
- 상태: 해결

### R-2. `EvalGo`: 공유 `Context`(Scope/Frames)를 스레드 간 공유

- 위치: `src/Interpreter/Evaluator/Evaluator.cs` `EvalGo`, `LFn.Call`, `NFn.Call`, `Context`
- 증상: `go`로 시작된 태스크가 부모 `Context` 인스턴스를 그대로 사용한다. `Scope`(Dictionary/List), `Frames`(List)는 락 없는 가변 컬렉션이라 두 스레드가 동시에 쓰면 Dictionary 내부 상태 손상·인덱스 꼬임·예외가 발생할 수 있다. 함수 호출마다 `Closure.Frames`를 재사용하는 것도 동일한 문제다.
- 해결:
  1. `LFn.Call`/`NFn.Call`의 새 `Context` 생성 시 `Closure.Frames` 대신 **복사본 리스트**(`new List<Frame>(Closure.Frames)`)를 넘긴다. push/pop이 호출 트리 안에서 균형을 이루므로 스택 규율은 유지되고, 스레드 간 리스트 공유는 사라진다.
  2. `Context`에 `Gate`(락 객체)를 추가하고, 재귀 깊이 체크/증가/감소를 `lock (Closure.Gate)` 아래로 모아 원자화한다.
  3. `EvalGo`가 시작할 goroutine에 **부모를 상속한 자식 Scope**(읽기는 부모 체인, 쓰기는 슬롯 고정 인덱스 갱신)와 새 `Frames`/`Defers`/`Usings`를 갖는 `Context`를 만들어 넘긴다.
- 상태: 해결

### R-3. `PlaygroundRunner`: 전역 상태 복원 누락

- 위치: `src/PlaygroundRunner.cs`, `src/Core/Global.cs`, `src/Interpreter/Runner/Scope.cs`
- 증상: 플레이그라운드가 `FileSystem`/`PATH`/allowed/stdout/stdin만 복원하고, 사용자 코드가 변경한 `Global.classes`(덮어쓰기 포함), `originalClasses`, `originalBaseTypes`, 전역 `Scope`, `_moduleCache`, `_loading`은 복원하지 않는다. 같은 프로세스에서 반복 실행하면 사용자 정의 클래스/전역 변수/모듈 캐시가 새어 다음 실행을 오염시킨다. builtin 클래스를 동명 클래스로 덮으면 이후 전체 실행이 깨진다.
- 해결:
  1. `Scope`에 `Snapshot()`/`Restore()` 추가(심볼→값 딕셔너리 복사, 슬롯 재구성).
  2. `Global`에 `CaptureState()`/`RestoreState()` 추가: 전역 스코프, classes 스코프, `originalClasses`(내부 맵까지 복사), `originalBaseTypes`, `_moduleCache`, `_loading`, `AllowProcessExit`를 스냅샷/복원.
  3. `PlaygroundRunner`가 실행 전 `CaptureState()`를 저장하고 `finally`에서 `RestoreState()` 호출.
- 상태: 해결

### R-4. `builtin.exit()` → `Environment.Exit()`: 플레이그라운드/호스트 프로세스 강제 종료

- 위치: `src/Core/Native/Builtin.cs`, `src/PlaygroundRunner.cs`
- 증상: `builtin`이 기본 허용 모듈에 포함되고 `exit`이 `Environment.Exit(code)`를 호출한다. 샌드박스에서 `exit(1)` 한 줄이면 인터프리터 프로세스 전체가 죽고, 타임아웃 처리도 무의미해진다.
- 해결: `Global.AllowProcessExit`(기본 `true`)를 추가한다. `PlaygroundRunner`가 실행 동안 `false`로 설정(성공/실패와 무관하게 `finally`에서 복원). `exit()`는 이 값이 `false`이면 `Panic("exit() is not allowed in this environment")`를 던진다. CLI 동작(종료 코드)은 그대로 유지된다.
- 상태: 해결

### R-5. `Fs` 네이티브가 `Global.FileSystem` 추상화를 무시하고 `System.IO`를 직접 호출

- 위치: `src/Core/Native/Fs.cs` (37건), `src/Core/Global.cs` `Import`, `src/Core/IO/IFileSystem.cs`
- 증상: 인터프리터는 I/O를 `IFileSystem`으로 추상화해 메모리 FS(플레이그라운드)를 지원하지만, `fs` 모듈은 `System.IO.File`/`Directory`를 직접 호출한다. 메모리 FS 환경에서 `fs`가 허용되면 외부 디스크를 읽고 쓸 수 있는 샌드박스 탈출이 되고, `Import`도 `File.Exists`를 직접 호출해 인메모리 모듈 해석과 어긋난다.
- 해결:
  1. `Import`의 경로 존재 판정을 `FileSystem` 추상화(`FileExists`/`DirectoryExists`)로 교체.
  2. `Fs` 네이티브의 모든 공개 함수에 가드를 두어, `Global.FileSystem`이 `PhysicalFileSystem`이 아니면 `Err("fs module is not available in this environment")`를 반환. 플레이그라운드 기본 허용 목록에 `fs`가 없는 현 정책은 유지하고, 커스텀 allowedModules로 `fs`를 명시해도 물리 디스크에 접근하지 못하게 한다.
- 상태: 해결

### R-6. async 네이티브 메서드가 `Task<Obj>` 캐스트에서 깨짐

- 위치: `src/Core/Global.cs` `CreateMethod`, `src/Core/Object/IO/HttpClient.cs`
- 증상: `HttpClient`의 async `[Native]` 메서드 7개는 `Task<Obj>`를 반환하는데, `CreateMethod`는 `(Obj)method.Invoke(...)`로 무조건 캐스트한다. `Task<Obj>`는 `Obj`가 아니므로 호출 시 `InvalidCastException`이 위치 정보 없이 발생한다. `NativeAttribute.Async` 속성은 어디서도 읽히지 않는 죽은 코드이기도 하다.
- 해결:
  1. `CreateMethod`에서 메서드의 `ReturnType`이 `Task<Obj>`이면 `await`(동기 차단) 후 `Obj`를 반환하도록 바인딩을 수정.
  2. 죽은 코드인 `NativeAttribute.Async` 속성과 `Async = true` 설정을 제거.
- 상태: 해결

### R-7. `EvalMatch`가 매치 스코프를 복원하지 않음

- 위치: `src/Interpreter/Evaluator/Evaluator.cs` `EvalMatch`
- 증상: `match` 평가가 `context.Scope = scope`(자식 스코프)로 교체한 뒤 **복원하지 않는다**. 패턴에서 바인딩된 이름이 match 이후 코드 전체에 누출되고 Scope 체인도 계속 자란다. `EvalFor`/`EvalTry`는 finally로 복원하는 것과 대조된다.
- 해결: `EvalMatch`에서 `prevScope`를 저장하고 `try { ... return Eval(body); } finally { context.Scope = prevScope; }` 패턴으로 복원한다.
- 참고: 패턴의 타입 검사를 하지 않는 `Typed` 케이스는 "점진적 타입 어노테이션(런타임 미검사)"이라는 언어 설계와 일치하는 것이므로 동작은 유지한다.
- 상태: 해결

---

### R-25. `http`/`ws` 네이티브 모듈이 어떤 메서드도 호출할 수 없었던 결함

- 위치: `src/Core/Object/IO/HttpClient.cs`, `src/Core/Object/IO/WebSocket.cs`
- 증상: `[NativeType(Name = "http_client")]`/`[NativeType(Name = "web_socket")]`로 리플렉션 등록되는 동안, 인스턴스는 `UnType.Create("http")`/`UnType.Create("socket")`로 만들어졌다. 런타임에서 `client.get(...)` 등을 호출하면 타입 이름이 등록 키와 달라 네이티브 멤버를 찾지 못하고 `'http' object has no attribute 'get'`로 실패한다. 즉 `http`/`ws` 모듈은 전혀 동작하지 않았다.
- 해결: `UnType.Create(...)` 인자를 등록 키와 동일하게 `"http_client"`/`"web_socket"`으로 정정했다. `http.connect().get("http://127.0.0.1:PORT/")`로 로컬 서버 200응답, 404 예외(catch 가능)를 검증했고, ws는 `connect`가 e2e 인자 검증을 통과함을 확인했다.
- 상태: 해결

### R-26. 플레이그라운드 샌드박스가 호스트 정보/내부 상태를 노출

- 위치: `src/PlaygroundRunner.cs` `DefaultAllowed`
- 증상: 기본 허용 목록에 `sys`(`env`/`environ`/`hostname`/`username`/`args` — 호스트 환경 변수와 명령행 인자 노출)와 `inspect`(`getattr`/`setattr` — 내장 객체의 메타/멤버 임의 읽기·쓰기, `global()` — 전역 스코프 전체 덤프)가 포함되어 있었다. 웹 샌드박스에서 시크릿(환경 변수)이 새거나 내장 동작을 monkey-patch할 수 있다.
- 해결: 기본 허용에서 `sys`, `inspect`를 제거했다(의도적 선택: 샌드박스는 호스트 자원을 노출하지 않는다). CLI(신뢰된 환경)에서는 모두 그대로 사용 가능. 플레이그라운드에서 `use sys`/`use inspect`가 차단됨과 `math`가 정상 동작함을 별도 콘솔 앱으로 검증했다.
- 상태: 해결

---

## 중간

### R-8. `Pool`: 백그라운드 아닌 워커 스레드 + `map` 결과 순서 비결정

- 위치: `src/Core/Object/Flow/Pool.cs`
- 증상: 워커 스레드가 `IsBackground = false`라 `pool.close()`를 호출하지 않으면 프로세스가 종료되지 않는다. `map`은 완료 순서대로 `result.Add`하므로 결과 리스트 순서가 비결정적이고 입력 순서와 무관하다.
- 해결: 워커 스레드를 `IsBackground = true`로 설정. `map`은 `Obj[]` 사전 할당 + 인덱스 지정으로 입력 순서를 보존한다. 아울러 `Pool.Clone()`의 오타 메시지("'lock' cannot be cloned")를 수정.
- 상태: 해결

### R-9. `Future.Wait`: `AggregateException`으로 포장되어 원 오류가 숨겨짐

- 위치: `src/Core/Object/Function/Future.cs`, `src/Interpreter/Evaluator/Evaluator.cs` `EvalWait`
- 증상: `future.Wait()`는 `Task.Result`를 직접 반환해, 태스크 내부 예외가 `AggregateException`으로 포장되어 전파된다. Un 코드의 `try`/`catch`가 `Error`를 잡을 수 없고, CLI는 원시 스택만 출력한다.
- 해결: `Future.Wait()`에서 `AggregateException`을 벗겨 원 예외를 다시 던진다. `EvalWait`는 `Unwrap`으로 `Err`를 위치 정보가 있는 `Error`로 승격시킨다.
- 상태: 해결

### R-10. `Str` intern 풀: 가득 차면 `Clear()` + 카운트 리셋

- 위치: `src/Core/Object/Primitive/Str.cs`
- 증상: 풀이 10,000엔트리에 도달하면 `pool.Clear()` 후 `poolSize = 0`으로 리셋하는데, 이 사이 다른 스레드가 추가한 엔트리는 지워지면서 카운터는 0이 되어 풀 상태가 어긋나고 불필요한 churn이 발생한다.
- 해결: `pool.Clear()` 대신 풀에 넣지 않고 `new Str(value)`를 직접 반환한다(용량 상한 도달 시 인턴만 중단). 카운트 불일치 자체가 사라진다. 오타 상수 `MAX_POOLED_STRING_LEGNTH`도 `MAX_POOLED_STRING_LENGTH`로 정정한다.
- 상태: 해결

### R-11. `Obj.Repr`: `__repr__`이 `Str`가 아닌 값을 반환하면 `InvalidCastException`

- 위치: `src/Core/Object/Base/Obj.cs`
- 증상: `(Str)value` 캐스트라 네이티브/사용자 코드가 `__repr__`에서 다른 타입을 반환하면 위치 없는 캐스트 예외가 터진다.
- 해결: `value is Str str ? str : Str.From(Type.Name)` 형태로 `as` 기반 폴백을 적용한다.
- 상태: 해결

### R-12. `Obj.Clone`: `Annotations`/`Parents` 참조 공유

- 위치: `src/Core/Object/Base/Obj.cs`
- 증상: 클론 시 `Members`만 복제하고 `Annotations`(Dictionary)와 `Parents`(List)는 원본 참조를 그대로 넘긴다. 클론의 주석/부모를 변경하면 원본까지 변이된다(별칭 때문에).
- 해결: `Annotations = new(Annotations)`, `Parents = Parents is null ? null : [.. Parents]`로 복사한다.
- 상태: 해결

### R-13. 네이티브 바인딩: 호출마다 `MethodInfo.GetParameters()` 반복 + Invoke 박싱

- 위치: `src/Core/Global.cs` `CreateMethod`
- 증상: 네이티브 호출마다 `method.GetParameters()`로 파라미터 리플렉션 정보를 새로 만들고 `method.Invoke`의 박싱 경로를 탄다. 호출 횟수가 많은 벤치마크에서 비용이 크다.
- 해결: `ParameterInfo[]`를 모듈 등록 시점에 한 번만 계산해 클로저에 캡처한다. `Invoke` 경로 자체는 의미 변화 위험이 있어 유지하되, 호출당 리플렉션 조회 비용은 제거한다.
- 상태: 해결

### R-14. `LFn`/`NFn`: `CallDepth` 공유 + 증가 순서 비대칭 + 비원자적 갱신

- 위치: `src/Core/Object/Function/LFn.cs`, `NFn.cs`, `Fn.cs`, `src/Interpreter/Runner/Context.cs`
- 증상: 모듈 import 시 `Map.New()` → `Clone()`이 같은 `Closure`를 공유해 재귀 깊이 카운터가 모듈 인스턴스 간에 섞인다. LFn은 체크 후 증가, NFn은 증가 후 체크로 비대칭이며, `CallDepth++` 자체도 원자적이지 않다.
- 해결:
  1. `Context`에 `Fresh()` 추가: Scope/Source/Frames는 공유하되 `CallDepth = 0`인 새 `Context`.
  2. `LFn.Clone`/`NFn.Clone`이 `Closure.Fresh()`를 쓰도록 변경.
  3. 깊이 체크/증가/감소를 `lock (Closure.Gate)` 안에서 수행하고 두 클래스 모두 check-then-increment로 통일.
- 상태: 해결

### R-15. `Global.Builtin()`: 어셈블리 타입 목록 3~4회 중복 스캔

- 위치: `src/Core/Global.cs` `Builtin()`
- 증상: `Assembly.GetExecutingAssembly().GetTypes()`를 primitives/nativeTypes/전체 순회 등으로 여러 번 호출한다(전부 O(전체 타입 수)).
- 해결: `var allTypes = Assembly...GetTypes();`로 한 번만 가져와 재사용한다.
- 상태: 해결

### R-16. `Optimizer`: `If` 자식이 두 번 최적화됨

- 위치: `src/Interpreter/Optimizer/Optimizer.cs` `OptimizeIf`
- 증상: `OptimizePass`가 이미 `OptimizeChildren`으로 모든 자식을 최적화하는데, `OptimizeIf`가 `OptimizePass`를 다시 호출해 IfCase/Else 본문을 이중으로 순회한다. 의미 오류는 아니지만 패스당 비용이 늘어난다.
- 해결: `OptimizePass`가 이미 최적화한 자식 노드를 그대로 사용하고 `OptimizeIf` 내부의 재호출을 제거한다.
- 상태: 해결

---

## 낮음

### R-17. 오타 식별자: `Error.Lenght`, `MAX_POOLED_STRING_LEGNTH`

- 위치: `src/Interpreter/Error/Error.cs`, `src/Core/Object/Primitive/Str.cs`, `src/Interpreter/Parser/Parser.cs`
- 증상: 공개 속성 `Lenght`가 "Length"를 오타로 노출하고, 내부 상수도 마찬가지다.
- 해결: 각각 `Length`/`MAX_POOLED_STRING_LENGTH`로 개명하고 참조(`Parser.cs:1044`, `Error.cs` ToString)를 갱신한다.
- 상태: 해결

### R-18. `NativeAttribute.Async` 죽은 코드

- 위치: `src/Interpreter/Reflection/Native.cs`, `src/Core/Object/IO/HttpClient.cs`
- 증상: 속성은 선언되고 설정만 되며 아무 곳도 읽지 않는다.
- 해결: R-6에서 반환 타입 기반 감지로 `Async`의 존재 이유 자체가 사라지므로 속성과 설정을 삭제한다.
- 상태: 해결

### R-19. `AGENTS.md`의 규모 수치 드리프트

- 위치: `AGENTS.md`
- 증상: "C# 약 83개 파일 / 11,750줄, `.un` 스크립트 28개"로 적혀 있으나 실제는 85개 파일 / 약 14,900줄 / 83개 스크립트다.
- 해결: 실측값으로 갱신한다.
- 상태: 해결

### R-20. `Fn.Bind` 이중 경로(빠른 경로/일반 경로)의 판정 일관성

- 위치: `src/Core/Object/Function/Fn.cs` `TryBindSimplePositional` vs `Bind`
- 검토 결과: 두 경로 모두 동일한 규칙(위치 우선 채움 → 키워드 → 기본값, `self`/`super` 제외, `*args`/`**kwargs` 없으면 빠른 경로)을 따르며, 판정 분기 후 반드시 한쪽만 실행된다. 차이는 없음을 대조 확인했으나 유지보수 시 한쪽만 수정하면 조용히 어긋날 수 있다.
- 해결: 두 경로의 불변조건을 주석으로 명시하고, `Bind`를 기준으로 빠른 경로가 "보수적 사전 검사"임을 문서화한다. 할당을 없애는 빠른 경로의 이점(핫 패스)을 유지한다. 동작 변경 없음.
- 상태: 수용

### R-21. `GetText`: Identifier/Call마다 `Substring` 할당

- 위치: `src/Interpreter/Evaluator/Evaluator.cs` `GetText`, `src/Interpreter/Parser/Node.cs`
- 증상: 함수 호출마다 `GetText(node)`가 `Source.Code.Substring`을 새로 할당한다(식별자에는 `Value`가 없음).
- 해결: `Node`에 `CachedText`(지연 계산 캐시)를 두고 `GetText`가 캐시를 사용하게 한다. 문자열 작성은 최대 1회다.
- 상태: 해결

### R-22. `_moduleCache` 무제한 성장 + 무효화 API 없음

- 위치: `src/Core/Global.cs`
- 증상: 모듈 캐시가 프로세스 수명 동안 무한정 쌓이고 비울 수 없다. 장기 실행 호스트(플레이그라운드 서버)에서 오래된 모듈이 잔존한다.
- 해결: R-3의 스냅샷/복원으로 플레이그라운드 반복 실행 간 누수는 제거한다. 추가로 `Global.ClearModuleCache()` 공개 API를 제공해 호스트가 명시적으로 비울 수 있게 한다.
- 상태: 해결

### R-23. `Runner.RunCore`: `PropagateFlow`를 새 `Error`로 만들며 원인(inner) 미보존

- 위치: `src/Interpreter/Runner/Runner.cs`
- 증상: `?` 연산자 전파를 오류로 변환할 때 `pf`가 원인인데 `inner`에 연결하지 않아 스택/원인이 유실된다.
- 해결: `new Error(..., inner: pf)`로 원인을 연결한다.
- 상태: 해결

### R-24. `Pool.Clone()` 오타 메시지

- 위치: `src/Core/Object/Flow/Pool.cs`
- 증상: 메시지가 `"'lock' cannot be cloned"`로 Lock과 혼동됨.
- 해결: `"'pool' cannot be cloned"`로 수정. (R-8에 포함)
- 상태: 해결 (R-8에서 함께 수정)

---

## 검토했으나 결함이 아닌 항목

- `Runner.RunCore`의 Flow→Error 변환 — 예외를 제어 흐름으로 쓰는 설계이며 inner 예외 문제는 R-23.
- `Source.GetLine`/`GetColumn`의 O(n) 스캔 — 에러 렌더링 경로에서만 호출(프레임당 1회), 실사용 비용이 낮아 캐싱 대신 유지.
- `MAXRECURSIONDEPTH`가 public mutable — Un 스크립트의 `sys.set_max_recursion_depth`를 위한 의도된 설계. `ulong` 단일 값 읽기/쓰기는 실질적으로 원자적이므로 유지.
- `match`의 `Typed` 패턴(`x: int`)이 타입 검사를 하지 않는 것 — "점진적 타입 어노테이션은 런타임에 검사하지 않는다"는 언어 설계와 일치.
- `Scope.Remove`가 슬롯을 남기는 것 — 인덱스 안정성을 위한 것으로 메모리 외 문제 없음.
- `os` 모듈의 `env`/`setenv`/`exec`, `http`/`ws` 모듈 — 플레이그라운드 기본 허용 목록에 없고 `IsAllowed`로 차단됨. CLI(신뢰된 스크립트)에서는 의도된 기능.
- `Why did LFn not increment after check` — LFn의 check-then-increment은 NFn과 통일시키되(R-14), 최대 깊이 시 증가하지 않는 동작은 의도된 것.

---

## 검증 결과 (최신, macOS + .NET 10 롤포워드, `dotnet build -c Release` 경고 0/오류 0)

- `test/test_suite.un`: **109 passed, 0 failed**.
- Positive 매니페스트 39개 파일: **1356 passed / 7 failed / 0 crashed** (mac). 실패 7건은 Windows 전용 기대값이며 원래 플랫폼 종속.
- 이후 회차에서 `test/assert.un`의 `assert_err_eq`를 f-string 비교로 정비하고, `http`/`ws` 모듈을 type-name 불일치(R-25) 수정으로 동작시켰다. 현재 기준치: Positive 39 files / 1363 passed (Windows) / 1356 passed (mac).
- `http` e2e 스모크(별도 콘솔 앱 + 로컬 서버): 200 응답 수신, 404 catch 가능 확인.
- 플레이그라운드 검증(별도 콘솔 앱 + Release DLL): `use sys`/`use inspect`가 차단되고 `math`는 정상 동작.
- Negative 42건: **38 matched / 4 mismatched** (전부 플랫폼 종속, 기존과 동일).
- KnownFailures(`regression/defects.un`): 6 assertions 실패 유지(변경 없음).
- 플레이그라운드 격리 스모크(별도 콘솔 앱 + Release DLL):
  - 1회차 실행에서 정의한 클래스가 2회차 실행에서 "not found"로 차단됨(전역 클래스 복원).
  - `global_marker` 같은 전역 변수가 실행 간 누출되지 않음.
  - `exit(1)` 호출 시 `panic : exit() is not allowed in this environment`만 발생하고 **호스트 프로세스는 종료되지 않음**.
  - `import fs`는 `panic : module 'fs' is not allowed in this environment`로 차단되고, `math`는 정상 동작.
- 동시성 스모크: 6개의 `go fib(20)`를 동시에 실행해 `wait`로 수집 — 40590, 프레임/재귀 깊이 이상 없음.
- `match` 스코프 누수 확인: 패턴에서 바인딩한 이름이 match 이후 코드에 노출되지 않음 (`catch`에서 확인).
- `try`의 catch 핸들러 내부에서 던진 오류가 원 오류로 덮이지 않고 `error: handler error`로 전파됨을 확인.
- `TargetInvocationException` 언래핑: 네이티브 함수가 `Panic`을 던질 때 원시 래핑 예외 대신 `panic : ...` 메시지가 그대로 출력됨을 확인.

## 남은 과제 / 향후 권장

- 네이티브 바인딩을 `MethodInfo.Invoke` 대신 컴파일된 델리게이트로 바꾸면 호출당 박싱 비용을 없앨 수 있다(시그니처 동적 바인딩 탓에 변경 범위가 커서 이번에는 파라미터 캐싱(R-13)까지만 적용).
- `EvalGo`가 공유 슬롯(부모 스코프)에 쓰는 경우의 마지막-쓰기-승리 경합은 막지 않는다. 정확한 직렬화가 필요하면 Un 레벨의 lock(Pool/Lock 타입)을 사용해야 한다.
- 플랫폼 의존 negative 테스트 4건은 CI에서 OS별로 분기하거나 macOS용 기대값을 갖는 별도 항목으로 분리할 것.
