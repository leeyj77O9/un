# PROJECT_CLEANUP_REPORT.md

> **Legacy document.** This report describes the C# codebase as of commit `4941988`
> (2026-11-11). The C# implementation has since been replaced by the Rust port
> (`rust/`), and this file is kept for historical context. The current implementation
> is documented in `docs/README.md` and `docs/rust-migration/`.

작성일: 2026-11-11 · 기준 커밋: `4941988` · 브랜치: `main` (미커밋 변경 없음, 사용자 작업물 보존)

## 1. 프로젝트 현황

- **구조**: C# / .NET 8 트리 워킹 인터프리터. 파이프라인 `Lexer → Parser → Desugarer → Optimizer → Evaluator`.
- 규모: C# 88개 파일 / 약 15,217줄(작업 전 ~15,400줄), `test/` 아래 `.un` 85개.
- **주요 문제점(발견)**
  - 네이티브 모듈 `random`(`src/Core/Native/Random.cs`)이 `random` 타입(`src/Core/Object/Util/Random.cs`)과 동일한 구현을 두 벌로 갖고 있었음.
  - 인자의 클래스 경로가 public API 2중화(호출자 없는 "통계" 수집 경로, 실제 사용 없는 overload 등)로 노출되어 있었음.
  - `set/tuple` 경로 중복 구현, 인터페이스의 사용되지 않는 멤버, 회귀되지 않은 stale 메타데이터(`IO\w42w4eky.udl~`) 등.
  - 정리 철학 문서(`AGENTS.md`)의 ArgInfo 메타데이터 목록이 실제 속성(`Keyword`)과 어긋남(해당 속성은 어디서도 읽히지 않음).
- **개선 방향**: 삭제 후보는 반드시 grep으로 실제 사용 여부를 확인하고, 반사 호출 경로(네이티브/내장 모듈)는 제외한 뒤 수정한다. 파일/디렉터리는 삭제하지 않고 멤버/중복을 정리한다.

## 2. 제거된 항목

### 2.1 파일·참조 제거

| 대상 | 사유 |
|---|---|
| `Un.csproj`의 `<None Remove="IO\w42w4eky.udl~" />` | 존재하지 않는 파일 참조(과거 Office Interop 잔재) |
| `Un.csproj`의 `<Folder Include="test\" />` | 빌드에 영향 없고 stale한 IDE용 메타데이터 |

### 2.2 함수·메서드 제거 (모두 실제 참조 0건 확인 후 삭제)

| 위치 | 항목 |
|---|---|
| `Global.cs` | `SetGlobalVariable`, `Import(string[], string?, IReadOnlyList<…>?)` 오버로드(소스 없는 변형), `GetClass(BaseType)`, `TryGetClass(BaseType, out Obj)` **public** 오버로드(→ private 내부 구현으로 교체) |
| `Runner.cs` | `ParentContext` 프로퍼티, `Run(Node)` 오버로드, `Reset()`, `LastOptimization` 프로퍼티, `Load(Context, Context)` 오버로드, 생성자의 `parentContext` 매개변수 |
| `Optimizer.cs` | `OptimizeWithStats`, `OptimizationResult` 레코드 구조체(어디서도 읽히지 않는 통계 수집 경로) |
| `Scope.cs` | `ContainsKeyInTop`, `Declare`, `SetLocalOrDeclare`(`SetLocal`과 완전 동일 구현, 호출부 1건을 `SetLocal`로 이전) |
| `Source.cs` | `FullName` |
| `Tup.cs` | `IsSingle()` |
| `IO.cs` | `ResetStdout()`, `ResetStdin()` |
| `UnType.cs` | `TGeneric`, `UGeneric`, `Infinity`, `Skip`, `Break` 등록(어떤 이름 조회에서도 참조되지 않음) |
| `ArgInfo.cs` | `Keyword` 속성(바인딩 코드가 읽지 않음; AGENTS.md의 ArgInfo 목록도 갱신) |
| `Reflection/Native.cs` | 사용되지 않는 `using Un.Object.Type;` |
| `IFileSystem.cs` | `EnumerateFileSystemEntries`/`EnumerateFiles`/`EnumerateDirectories` 인터페이스 멤버 및 Physical/Memory 구현 2세트 |
| `Global.cs`의 `Import` 소스 없는 변형도 제거되어 `Import`는 `(path, currentSource, …)` 단일 형태가 되었다. |

### 2.3 정리된 중복 로직

- `Dict`의 `ToList`/`ToTuple`/`Iter`/`Spread`에 반복되던 `Keys.Zip(Values) → Tup` 투영을 내부 `private IEnumerable<Tup> Pairs()`로 통합.
- `Runner`의 공개 표면을 `Run()`/`Load(string, Scope)`로 축소: 기존 `Run(Node)`(테스트 호출자 없음), `Reset()`, `Load(Context, Context)`(우회 호출자 없음) 삭제.
- `Optimizer`를 "고정점 도달 여부"만 반환하는 형태로 단순화(`OptimizeWithStats` → `Optimize`).
- `Global.GetClass(TObj)`/`TryGetClass(BaseType)`을 `GetClassName(BaseType)` 기반 단일 경로로 재작성.

### 2.4 삭제하지 않고 보존한 것

- `Global.ClearModuleCache()`, `IO.BrowserPromptHandler`, `NeedInputException`, `PlaygroundRunner` — 모두 외부 호스팅(웹 플레이그라운드)을 위한 공개 API/계약으로 판단하여 보존.
- `Global.MAXRECURSIONDEPTH`, `Global.AllowProcessExit` — Un 스크립트의 `sys.set_max_recursion_depth` 등에서 의도적으로 노출한 public mutable 상태.
- `Future.Future` / `Pool`의 cold/hot Task 구분 — `Pool`은 cold semantics에 의존.

## 3. 통합된 항목

| 항목 | 통합 전 | 통합 후 |
|---|---|---|
| `random` 모듈/타입 | `Native/Random.cs`(~355줄, 전 함수 중복 구현)와 `Object/Util/Random.cs`(동일 로직)의 두 벌 존재 | 모듈 함수는 모두 `Object.Util.Random`의 정적 메서드로 위임(`Next/Int/Float/Bool/Bytes/Shuffle/Choice/Choices/Sample/Chance/Seed/String/Weighted/Range/Value`). ~250줄 제거, 동작 불변 |
| `Dict` 키/값 쌍 투영 | 동일 식을 `ToList`/`ToTuple`/`Iter`에 3회 복사 | `Pairs()` 1곳 |
| `Scope` 로컬 선언 | `SetLocal`과 `SetLocalOrDeclare` 동일 구현 2개 | `SetLocal`만 유지 |
| `Global` 클래스 해석 | `GetClass`/`TryGetClass`에 BaseType 오버로드 2개 중복 | `GetClassName()` 기반 단일 구현 + 내부 `internal` `TryGetClass(BaseType)` |

- uuid(`random.uuid()`)는 모듈 경로(`Guid.NewGuid()`)와 클래스 경로(시드된 PRNG)의 **기존 동작을 모두 보존**했다(통합하지 않음). 반면 `Str` 인터닝 풀·샌드박스 복원 동작은 변경 없음.

## 4. 구조 개선

- **API 표면**: `Runner` 공개 API가 3종(`Run`/`Load`/접근 가능 프로퍼티)으로 축소. `OptimizationResult`public record 삭제.
- **의존성**: `IFileSystem`에서 사용되지 않는 `Enumerate*` 멤버 제거로 `Fs`/`Global.Load`가 더 좁은 계약에 의존한다. `Fs`의 직접 `System.IO` 접근은 이번 범위에 포함하지 않고 그대로 두되 샌드박스는 `GetBlockedPath`로 차단 중.
- **모듈 간 책임**: `random`의 공유 상태는 `Object.Util.Random shared` 하나로 일원화. 모듈은 얇은 위임 계층.
- **미사용 추상화 제거**: `ArgInfo.Keyword`(아무도 읽지 않던 메타데이터) 제거, `OptimizationResult`(관찰되지 않는 통계 계층) 제거.
- 디렉터리 구조와 파일 배치는 변경하지 않았다(불필요한 이동 없음).

## 5. 성능 및 메모리

- **측정 가능한 개선은 없다.** 이번 작업은 대부분 죽은 코드 제거와 중복 통합이다.
- 중복 제거로 `random` 모듈 호출 체인이 위임 한 단계를 더 거치나(정적 호출 1회), 측정상 무의미하다.
- `Dict.Pairs()`로 투영식을 단일화했으나 생성되는 객체 수는 동일하다.
- 측정하지 못한 항목: 벤치마크(`bench/`)는 실행환경 편차가 커 이번 정리의 효과를 숫자로 제시할 수 없다. 기존 벤치 스크립트는 모두 그대로 동작한다.
- 메모리: 무의미한 공개 멤버/레지스트리(`UnType.TGeneric/UGeneric/Infinity/Skip/Break`)를 제거해 어셈블리 로드 시 상수 등록 5건이 줄었다.

## 6. 코드 품질 개선

- **중복 제거**: `random` 모듈/타입 전 기능(약 250줄), `Dict` 키쌍 투영 ×3, `Scope` 로컬 선언, `Global` BaseType 클래스 해석 2개 오버로드.
- **복잡성 감소**: `Runner`(26+ 순 삭감), `Optimizer` 단순화, `Scope` 축소.
- **일관성**: 네이티브 인자 메타데이터 문서(`AGENTS.md`)와 실제 속성 일치; `ArgInfo`는 `Name/Essential/Optional/Positional`만 존재.
- **유지보수성**: public API 표면이 줄어 리플렉션 발견 경로 외의 실수 여지가 감소. `.csproj`의 stale MSBuild 엔트리 제거로 빌드 노이즈 감소.

## 7. 검증 결과

- `dotnet build -c Release` — 오류 0, 경고 0 (증분/클린 빌드 모두).
- `test/test_suite.un` — **109 passed / 0 failed**.
- Positive 매니페스트 전체 실행 — **1323 passed / 0 crashed**. 실패 7건은 `os.un`(4)/`sys.un`(1)/`fs.un`(2)의 Windows 전용 기대값으로, macOS 플랫폼 종속이며 정리 전 빌드에서도 동일.
- Negative 42건 — **38 matched / 4 mismatched**(모두 기존과 동일한 플랫폼 종속; `use_member`/`use_missing`(절대경로), `depth_limit_crash`/`property_recursion`(Windows 스택 오버플로 종료코드/메시지)).
- KnownFailures(`regression/defects.un`) — 6 assertions 실패 그대로 유지.
- 보안 경로(샌드박스): 스냅샷 복원·`exit` 차단·`fs` 가드·`Timeout` 경로는 수정하지 않았고 동작 불변.
- 오류/경고 처리: 작업 중 build breakage 2건(타입 해석 오버로드 삭제에 따른 CS1503, Random 모듈의 네임스페이스 유실 CS0246)은 각각 `GetClassName`/내부 `TryGetClass(BaseType)`로, `using` 보강으로 즉시 수정.

## 8. 보류된 작업

| 항목 | 사유 |
|---|---|
| `NeedInputException` | 브라우저 호스트 계약 예정 상태로 보임. 사용처가 이 저장소에 없고 외부 호스팅 API로만 존재 — 보존 |
| `IO.BrowserPromptHandler` | 위와 동일 |
| `Global.ClearModuleCache()` | `docs/RISK.md`에 명시된 호스트용 공개 API. 제거 시 호환성 깨짐 |
| `os`의 `env`/`setenv`/`exec`, `http`/`ws` 모듈 | 플레이그라운드 기본 차단으로 안전하며, CLI에서는 의도된 기능. 변경하지 않음 |
| `_moduleCache` 무제한 성장 | 샌드박스는 스냅샷으로 복원되지만 장기 CLI 호스트의 캐시 정책은 별도 설계 필요 — 보존 |
| `Str` 인터닝 풀 상한 도달 시 동작 | 이미 수용된 설계(용량 상한 시 인터닝 중단). 변경 불필요 |
| 플랫폼 종속 negative 테스트 4건 | CI를 OS별로 나누거나 기대값을 분리해야 하는 별도 과제. 이번 범위 아님 |
| `Runner`/`Context` 공유 상태 완전 격리 | 공유 `Scope` 슬롯 쓰기 경합은 언어 설계(공유 메모리) 문제 — 이번 정리 범위 아님 |

## 9. 추가 개선 권장 사항

- 네이티브 바인딩을 컴파일된 델리게이트로 바꾸면 호출당 박싱을 없앨 수 있다(R-13 후속).
- `Fs` 네이티브를 `IFileSystem` 추상화로 전부 포팅하고 메모리 FS에서 `fs`를 허용하는 샌드박스를 지원할 수 있다(별도 설계 필요).
- `http`/`ws` 모듈은 네트워크 통합 테스트가 없다. 시간 여유가 있을 때 로컬 수신 서버 기반 테스트를 추가할 것.
- `test/negative`의 OS 종속 기대값은 `scripts/test.ps1`에서 플랫폼별로 분기하거나 파일을 분리할 것.
- `Parser.cs`(~1,600줄)와 `Evaluator.cs`(~1,300줄)는 단일 클래스 밀집 상태다. Statement/Expression/Class 파서 분리는 별도 작업으로 할 것.
- `x64` 전용 네이티브 `Pool`의 완료 순서가 비결정적일 수 있고, `future.Wait()`의 예외 래핑은 정리되었지만 `wait` 타임아웃 개념은 없다.
- 벤치마크(`bench/`) 결과를 CI에 붙이고 회귀를 감지하는 측정 지표를 도입할 것.

## 10. 최종 변경 요약

| 지표 | 값 |
|---|---|
| 수정된 파일 | 17개 (실제 diff: **+53 / −315줄**) |
| 삭제된 파일 | 0 (파일 배치는 유지) |
| 삭제된 코드 항목 | 36개 (메서드/오버로드 17개, 인터페이스 멤버 + 두 구현 9개, `UnType` 등록 5개, `OptimizationResult`, `ArgInfo.Keyword`, stale using 1개, `Un.csproj` 2개) |
| 통합된 기능 | `random` 모듈(약 250줄) → 타입으로 위임, `Dict` 쌍 투영 3중복 → 1개, `Scope` 로컬 선언 2개 → 1개, `Global` BaseType 클래스 해석 2중복 → 1개 |
| 제거된 추상화 | `OptimizationResult`(관찰되지 않는 통계 계층), `ArgInfo.Keyword` |
| 빌드 | 오류 0 / 경고 0 |
| 테스트 | 109/109 (suite), Positive 1323, 0 crashed, Negative 동판 |
| 보존한 공개 API | `ClearModuleCache`, `PlaygroundRunner.RunCode`, `IO.BrowserPromptHandler`, `MAXRECURSIONDEPTH`, `AllowProcessExit`, `os`/`http`/`ws`(기본 차단), `random.uuid()` 두 경로(module=Guid.NewGuid / class=시드된 PRNG) |

총평: 기능과 동작을 완전히 보존한 채 중복 구현과 실제 사용처가 없는 API를 제거하고, 의존성을 좁혔다. 동작 불변은 동일 플랫폼 테스트 스위트 실행으로 확인했다.
