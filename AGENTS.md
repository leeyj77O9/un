# AGENTS.md

이 문서는 이 저장소에서 작업하는 AI 코딩 에이전트를 위한 프로젝트 가이드다.
사람이 읽는 문서는 `README.md`(한국어)와 `docs/README.md`(영어)를 참고한다.

## 프로젝트 개요

- **Un**: 들여쓰기 기반 문법, 패턴 매칭, `go`/`wait` 비동기 실행을 제공하는 동적 타입 스크립팅 언어.
- **구현**: Rust 바이트코드 컴파일러 + 스택 기반 VM (`unvm`, 라이브러리 이름 `unvm`).
- **파이프라인**: `Source -> Lexer -> Parser -> Desugarer -> Optimizer -> Bytecode Compiler -> Stack VM`
- **규모**: Rust 약 88개 파일 / 약 15,000줄, `test/` 아래 `.un` 스크립트 89개.
- **이전 역사**: 원래 C#/.NET 8 트리 워킹 인터프리터로 구현되어 있었다. Rust 포트가
  differential 검증으로 C# 구현과 byte-identical임을 확인한 뒤 대체했고, C# 소스는
  `git` 이력에만 남아 있다.

## Rust 포트 (`rust/`)

원래 C#으로 작성됐다가 Rust로 대체된 경위와 설계 판단은 `docs/DECISIONS.md`와
git 이력에 남아 있다. 현재 디렉터리를 참고:

```text
C:\dev\un/
├─ rust/
│  ├─ Cargo.toml            # unvm 라이브러리 + unvm CLI + unvm-playground workspace
│  ├─ src/
│  │  ├─ main.rs             # CLI 진입점 (run/compile/tokens/ast/desugar/opt/dis/help)
│  │  ├─ lib.rs
│  │  ├─ bytecode/           # Op(explicit #[repr(u8)]), Instruction, Program, Proto, unbc
│  │  ├─ lexer.rs / parser.rs / desugar.rs / optimize.rs / compiler.rs / vm.rs / module.rs
│  │  ├─ runtime/            # obj, collections, iters, function, world, scope, date, ...
│  │  ├─ native/             # io, fs, re, math, iter, ... (`native::init`에서 명시적 등록)
│  │  └─ playground/         # WASM 진입점 (PlaygroundRunner.RunCode 대응)
│  └─ target/               # 빌드 산출물 (gitignore)
└─ main.un                 # 사용자의 스크래치; 추적하지 않음
```

- **`main.un`은 사용자의 스크래치 파일**이다. 테스트/제품 진입점이 아니며, 에이전트는
  이 파일을 수정하거나 스테이징하지 않는다.
- **`Op` enum은 explicit `#[repr(u8)]`로 고정**되어 있다. opcode 값을 바꾸면 `.unbc`가 깨진다.
- **외부 crate 의존성은 없다**(표준 라이브러리만 사용). 새 의존성은 정말 필요할 때만 추가한다.

## 빠른 명령

```bash
# 빌드 (항상 release, GNU toolchain 필수)
rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml

# 스크립트 실행
.\rust\target\release\unvm.exe run test\test_suite.un

# WASM 산출물 (un-playground용)
rustup run stable-x86_64-pc-windows-gnu cargo build --release --target wasm32-unknown-unknown --manifest-path rust/Cargo.toml -p unvm-playground

# 전체 테스트 runner ( Positive + Negative + KnownFailures + KnownDivergent 집계 )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test.ps1

# 벤치마크 runner ( -Group un|node|all, -Rebuild, -NoSave )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group all
```

- Windows에서 `unvm.exe`는 `rust\target\release\`에 빌드된다. PATH에 추가해서 쓸 수 있다.
- `stable-x86_64-pc-windows-gnu` toolchain을 쓴다(MSVC linker가 없는 개발 머신).
- `wasm32-unknown-unknown` 타겟도 같은 toolchain으로 빌드된다(link가 필요 없다).
- 현재 `cargo build --release`는 오류 0개, 경고 0개로 끝난다. **새 경고를 추가하지 말 것.**

## 디렉터리 구조

```text
C:\dev\un
├─ AGENTS.md                 # 이 문서 (에이전트용, 커밋 대상)
├─ README.md / docs\README.md # 한국어/영어 문서 쌍
├─ LICENSE
├─ rust/                     # Rust 구현 전체
├─ src/                      # 삭제됨(C# 포트; git 이력에만 보존)
├─ test/                     # .un 테스트/픽스처
│  ├─ unit/                  (unit/lexer, unit/parser, unit/runtime, unit/native)
│  ├─ integration/           (algorithms, concurrency, modules, recursion)
│  ├─ regression/            (defects.un — 문서화된 동작 기준)
│  ├─ negative/              (exit code + stderr 확인 + manifest.txt)
│  ├─ fixtures/              (circular/, rel_test/, dirmod/, dirall/)
│  ├─ MANIFEST.txt           # group|path 행
│  ├─ KnownFailures.txt      # runner가 KNOWN FAIL로 기록하는 파일
│  ├─ KnownDivergent.txt     # 문서화된 구현 간 차이(실패로 세지 않음)
│  └─ assert.un              # assert 모듈 (assert_eq / assert_true)
├─ scripts/                  (test.ps1, bench.ps1)
├─ bench/                    (측정 스크립트, bench/node/, bench/results/)
└─ docs/                     (프로덕션 문서)
```

## 아키텍처 메모

- `Runner.RunCore` 대응: 파싱 → desugar → optimize → compile → evaluate를 순서대로 수행한다.
  `Runner.Load`는 `Global.PATH` 기준으로 파일을 읽고 CRLF를 LF로 정규화한다.
- `Scope`는 부모 체인을 갖는 슬롯 기반 심볼 테이블이다. `Scope`의 slot table에서
  `Set`은 declaring scope에 write하고(`for` body의 `sum += v` 축적),
  `SetLocal`/`SetLocalOrDeclare`는 local 전용, `Declare`는 덮지 않는다.
- `World`는 프로세스 전역 정적 상태다(`path`, `global_scope`, `natives`, `stdout`, `allowed`).
  테스트나 플레이그라운드에서 상태를 바꾸면 반드시 복원한다.
- 모듈 import는 `use` 키워드로 선언하고, 현재 파일 기준 상대 경로를 먼저 찾고,
  없으면 Global.PATH 기준으로 찾는다. 디렉터리 모듈은 `mod.un` 또는 디렉터리 내 모든
  `.un` 파일을 로드한다.
- `PlaygroundRunner`의 Rust 대응은 `rust/playground/`다. `MemoryFileSystem` 대신
  `World`의 전역 상태로 sandbox stdout을 측정하고, `finally`에서 전역 상태를 복원한다.

## 코드 컨벤션

- Rust 2021 edition. 모듈은 `//!` 설명과 함께 파일 상단에 문서를 두고,
  `?` 연산자로 오류를 전파하며, `match`/`if let` 패턴 매칭을 적극적으로 쓴다.
- 외부 crate 의존성은 없다(표준 라이브러리만).
- `Op` enum은 explicit `#[repr(u8)]`로 고정되어 있다. opcode 값을 바꾸면 `.unbc`가 깨진다.
- 네이티브 모듈은 `rust/src/native/`에서 `AnModule` 어트리뷰트 없이 `native::init`이 명시적 등록한다.
- `README.md`와 `docs/README.md`는 같은 내용의 한/영 쌍이다. 사용자 기능을 바꾸면
  두 문서를 함께 갱신한다.

## 테스트와 검증

- 통합 테스트 러너(`.csproj`)는 없다. runner는 PowerShell 스크립트이고, 빌드 산출물로
  `.un` 스크립트를 실행한다: `scripts/test.ps1`(test), `scripts/bench.ps1`(bench).
- `scripts/test.ps1`은 `test/MANIFEST.txt`의 `group|path` 행을 실행하고 exit code와
  stdout의 `N passed, M failed`를 집계한다. 정상 기준은 **Positive 39 files /
  1363 passed / 0 failed / 0 crashed**, Negative 38 checks 매칭 + 4 documented divergences,
  Known 1 file이다. runner는 `[FAIL]`이 증가하면 exit 1로 끝난다.
- `test/negative/manifest.txt`는 `path|expected exit|expected substring` 행으로
  stderr와 exit code를 검증한다(파싱 오류, 미처리 예외, stack overflow 포함).
- `test/KnownFailures.txt`의 파일은 runner가 KNOWN FAIL로 기록하고 Positive 집계에
  포함하지 않는다. 현재 1개 파일(`test/regression/defects.un`, 6 assertions)이
  문서화된 동작과 실제 동작이 다른 결함을 기록한다.
- `test/KnownDivergent.txt`의 파일은 전부 문서화된 차이이다. 정상적으로 실행되지 않지만
  의도된 동작 차이이므로 실패 집계에 포함하지 않는다.
- `test/test_suite.un`은 가장 넓은 스모크 테스트이며 `test/assert.un`을 모듈로 사용한다.
  정상 기준 출력은 `109 passed, 0 failed`이고, 실패 시 `[FAIL]` 라인이 나온다.
- 상대 import/순환 import/디렉터리 모듈 검증용 픽스처는 `test/fixtures/circular/`,
  `test/fixtures/rel_test/`, `test/fixtures/dirmod/`, `test/fixtures/dirall/`에 있다.
- 벤치마크 스크립트는 `bench/`에 있고, Node.js 상대 스크립트는 `bench/node/`에 있다.
  측정은 웜업 + 반복 + 통계(최솟값·중위값·평균·최댓값·표준편차·연산 수)를 사용한다.
  결과와 측정 한계는 `docs/BENCHMARK_GUIDE.md`를 참고한다.
- 변경 후 최소한 다음을 수행한다.
  1. `rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml`
     — 오류 0, 새 경고 0.
  2. 변경과 관련된 `test/**.un` 실행.
  3. `scripts/test.ps1` 실행 후 `[FAIL]`과 crashed가 늘지 않았는지 확인.

## 알려진 함정

- `.unbc` 파일을 디버깅할 때는 `.gitignore`에 있는 `rust/target/` 아래의 산출물을 보면 된다.
- `World`는 프로세스 전역 정적 상태다. 테스트/플레이그라운드에서 상태를 바꾸면 반드시 복원한다.
- `native::init` 호출 전에는 전역 함수(`panic` 등)가 등록되지 않는다.
- 파일 경로는 `Global.PATH` 기준으로 해석된다. CLI는 현재 작업 디렉터리로 초기화한다.
- 이 저장소는 외부 crate 의존성이 없다. 새 의존성은 정말 필요할 때만 추가한다.

## Git / 기여 규칙

- 기본 브랜치: `main`. 기능 브랜치는 `feat/my-feature` 형식을 사용한다.
- 커밋은 기여자가 직접 수행한다. **에이전트는 사용자가 명시적으로 요청하지 않는 한
  커밋/푸시하지 않는다.**
- `bin/`, `obj/`와 로컬 에이전트 상태 디렉터리는 `.gitignore`에 의해 제외된다.
- 생성된 빌드 산출물이나 도구 캐시를 저장소에 추가하지 않는다.
