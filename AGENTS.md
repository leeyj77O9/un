# AGENTS.md

이 문서는 이 저장소에서 작업하는 AI 코딩 에이전트를 위한 프로젝트 가이드다.
사람이 읽는 문서는 `README.md`(영어)와 `docs/README.ko.md`(한국어)를 참고한다.

## 프로젝트 개요

- **Un**: 들여쓰기 기반 문법, 패턴 매칭, `go`/`wait` 비동기 실행을 제공하는 동적 타입 스크립팅 언어.
- **구현**: Rust 바이트코드 컴파일러 + 레지스터 기반 VM (`unvm`, 라이브러리 이름 `unvm`).
- **파이프라인**: `Source -> Lexer -> Parser -> Optimizer (desugar+fold) -> Bytecode Compiler -> Register VM`
- **규모**: Rust 113개 파일(target 제외; `rust/src` 90 + `rust/nrt/` 22 + `rust/playground/` 1) / 64,378줄(`rust/src` 57,383 + `rust/nrt/` 6,995), `test/` 아래 `.un` 스크립트 175개. (2026-11-23 재측정)
- **JIT 동작**: `jit_bc.rs`는 `.unbc`를 직접 번역하는 Cranelift JIT다. `vm::plan_for`가 프로그램을 읽을 때 Int 부분집합(i64 파라미터·지역변수, `iter.range` 루프, 정수 산술과 비트연산 `&` `|` `^`, `FusedIntMulAdd`)과 IntDict/IntVec/class intrinsic와 math Float intrinsic에 속하는 proto를 native로 컴파일하고 `jit_cache`에 등록한다. 결과는 hidden pointer로 (value, tag)를 기록하는데 tag=0 Int, tag=1 Float(IEEE-754 비트), tag=2는 런타임 bail(엔진 오류가 아니라 의미론 보존을 위한 인터프리터 fallback)이다. 나머지는 전부 인터프리터로 fallback. 0인자 사용자 함수도 JIT 대상이다(컴파일러 합성 proto는 제외). **클래스 메서드는 JIT 대상이다**: receiver는 hidden pointer 형태의 raw `Rc<ObjData>` 포인터로 전달되고, 클래스 메서드는 receiver를 local slot 0에 바인딩한다. `self.<Int필드>`의 직접 read/write는 layout 인덱스와 `[이름 == 필드이름, payload == Int]`를 **호출 시점에 name-checked**하는 intrinsic으로 수행하고, 검증 실패 시 bail → 인터프리터로 fallback한다. `UV_JIT=0`으로 런타임에 opt-out. `UV_JIT=2`는 strict canary로, JIT 후보가 하나라도 fallback stub이 되면 exit code 2로 즉시 죽는다(`.github/workflows/build.yml`의 strict canary 3스텝이 `test/jit/strict.un`(Int), `strict_float.un`(Float), `strict_{collections,class,ops}.un`(컬렉션·클래스·연산)으로 검증).
- **기준 문서**: `docs/project/REFACTOR_SUMMARY.md`(전체 요약), `docs/project/PROJECT_AUDIT.md`(감사 결과), `docs/project/REFACTOR_LOG.md`(변경 이력), `docs/spec/ARCHITECTURE.md`(구조), `docs/project/TECHNICAL_DEBT.md`(부채 목록)가 현재 상태의 기준 문서다.
- **이전 역사**: 원래 C#/.NET 8 트리 워킹 인터프리터로 구현되어 있었다. Rust 포트가
  differential 검증으로 C# 구현과 byte-identical임을 확인한 뒤 대체했고, C# 소스는
  `git` 이력에만 남아 있다.

## Rust 포트 (`rust/`)

원래 C#으로 작성됐다가 Rust로 대체된 경위와 설계 판단은 `docs/project/DECISIONS.md`와
git 이력에 남아 있다. 현재 디렉터리를 참고:

```text
C:\dev\un/
├─ rust/
│  ├─ Cargo.toml            # unvm 라이브러리 + unvm CLI + unvm-playground workspace
│  ├─ src/
│  │  ├─ main.rs             # CLI 진입점 (run/compile/check/native/obj/build/lsp/help)
│  │  ├─ lib.rs
│  │  ├─ bytecode/           # Op(explicit #[repr(u8)]), Instruction, Program, Proto, unbc
│  │  ├─ lexer.rs / parser.rs / optimize.rs / compiler.rs / module.rs
│  │  ├─ vm/                 # 단일 실행 모듈: register VM (mod.rs) + stack→register 번역(transfer.rs)
│  │  ├─ jit_cache.rs        # JIT 컴파일된 함수 전역 캐시/등록
│  │  ├─ jit_bc.rs         # .unbc가 1:1 대응되는 Cranelift JIT (Int 부분집합 + IntVec/IntDict/class intrinsic + math Float intrinsic, Float poly-tag 반환)
│  │  ├─ runtime/            # obj, collections, iters, function, world/(모듈 분할), scope, date, ...
│  │  ├─ native/             # io, fs, re, math, iter, ... (`native::init`에서 명시적 등록)
│  │  └─ playground/         # WASM 진입점 (PlaygroundRunner.RunCode 대응)
│  └─ target/               # 빌드 산출물 (gitignore)
└─ main.un                 # 사용자의 스크래치 (git에 추적 중; 에이전트는 수정하지 않는다)
```

- **`main.un`은 사용자의 스크래치 파일**이다. 테스트/제품 진입점이 아니며, 에이전트는
  이 파일을 수정하거나 스테이징하지 않는다. (참고: 이 파일은 현재 `git`에 추적되어 있다.)
- **`Op` enum은 explicit `#[repr(u8)]`로 고정**되어 있다. opcode 값을 바꾸면 `.unbc`가 깨진다.
- **외부 crate 의존성**: 기본적으로는 표준 라이브러리이지만, `vendor/` 아래 패치된
  `region 3.0.2`, `cranelift-jit 0.137.1`, `wasmtime-internal-jit-icache-coherence 50.0.1`이
  있다(Windows/GNU에서 `dlltool.exe` 부재로 `windows-sys 0.52.0`을 빌드할 수 없기 위해
  raw kernel32 FFI로 교체). 새 외부 crate는 정말 필요할 때만 추가하고 `vendor/`에 패치할
  수 있으면 raw FFI로 해결한다.
- **JIT 동작**: jit_bc.rs는 .unbc를 직접 번역하는 Cranelift JIT다. m::plan_for가 프로그램을 읽을 때 eligibility에 속하는 proto를 native로 컴파일하고 jit_cache에 (program_ptr, proto_index) 키로 등록한다. m::call_proto가 이름 대신 이 키로 조회하고, 결과는 hidden pointer로 (value, tag)가 기록된다: tag=0 Int, tag=1 Float(IEEE-754 비트), tag=2 런타임 bail(엔진 오류가 아니라 의미론 보존용 인터프리터 fallback). 지원 범위: 0인자 함수 포함 ESSENTIAL/OPTIONAL i64 파라미터(최대 8개), i64 지역변수, iter.range(a, b) 루프, 정수 산술(//, %는 제수 상수 정수)과 비트연산(&, |, ^), 비교 연산, FusedIntMulAdd 누산기, Int consts, IntVec/IntDict/class intrinsic, math Float intrinsic(sqrt/floor/sin 등). 나머지는 전부 인터프리터로 fallback. UV_JIT=0으로 런타임에 opt-out. UV_JIT=2는 strict canary로, JIT 후보가 하나라도 fallback stub이 되면 process exit(2)로 즉시 죽는다(CI의 strict canary 3스텝이 test/jit/strict.un(Int), strict_float.un(Float), strict_{collections,class,ops}.un(컬렉션·클래스·연산)으로 검증). WASM 타겟에서는 비활성화된다.


## 빠른 명령

```bash
# 빌드 (항상 release, GNU toolchain 필수)
rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml

# 스크립트 실행
.\rust\target\release\un.exe run test\test_suite.un

# WASM 산출물 (un-playground용)
rustup run stable-x86_64-pc-windows-gnu cargo build --release --target wasm32-unknown-unknown --manifest-path rust/Cargo.toml -p unvm-playground

# Rust 테스트 (nrt 4 + lib 246 + LSP 4 + playground 8 = 262 passed)
rustup run stable-x86_64-pc-windows-gnu cargo test --release --workspace --manifest-path rust/Cargo.toml

# 전체 테스트 runner ( Positive + Negative + KnownFailures + KnownDivergent 집계 )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test.ps1

# JIT on/off differential runner ( 모든 manifest 파일을 UV_JIT=0 과 기본 JIT으로 두 번 실행 )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\differential.ps1

# .unbc 왕복 runner ( Positive + negative 중 컴파일 가능 파일을 run vs compile+run 으로 바이트 비교 )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\unbc-roundtrip.ps1 -IncludeNegative

# Compile Mode cross-validation ( test/compile/*.un 을 un run vs un native 로 실행해 바이트 비교 )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\compile-check.ps1

# Compile Mode 벤치마크 ( un run vs UV_JIT=0 vs un native fib(24) 측정 )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\compile-bench.ps1

# 오브젝트 파일 데모 ( .unbc → .obj → rustc link → 실행, 출력 "fib = 55" + un build 단일 명령 )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\obj-demo.ps1
# un build --embed <file.un> → 임의 프로그램도 단일 .exe로 (libunvm.a 정적 링크)

# no-VM exe 동치 검증 ( plain un build ↔ un run stdout/stderr/exit byte 비교 )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\exe-diff.ps1

# 크로스 레인 edge fuzz ( VM/JIT/Compile/Python/Node )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\edge-fuzz.ps1

# 벤치마크 runner ( -Group all|un|unvm|unjit|node|python|unnative|unbuild, -Rebuild, -NoSave, -Report, -Compare <csv> )
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group all
# 6레인(Un VM/Un JIT/Compile Mode/standalone exe/Python/Node)을 측정한다. -Report는 BENCHMARK.md도 갱신한다.
# HTML 리포트: powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench-report.ps1
```

- Windows에서 `un.exe`는 `rust\target\release\`에 빌드된다. PATH에 추가해서 쓸 수 있다.
- `.github/workflows/build.yml`은 push/PR마다 13스텝을 실행한다: checkout, GNU toolchain 설치, `cargo build --release`, `scripts/test.ps1`, `cargo test --release`, `scripts/compile-check.ps1`, `scripts/exe-diff.ps1`, `scripts/edge-fuzz.ps1`, `scripts/differential.ps1`(JIT on/off 바이트 비교), `scripts/unbc-roundtrip.ps1 -IncludeNegative`, `JIT strict canary (UV_JIT=2, test/jit/strict.un)`, `JIT Float strict canary (UV_JIT=2, strict_float.un)`, `JIT strict canary — collections / class / ops (strict_{collections,class,ops}.un)` (`rust-toolchain.toml`이 툴체인을 고정한다). 2026-11-17에 두 번 green (`build #1` 4m 0s, `build #2` 3m 28s)였고, 2026-11-23에 Compile Mode/exe-diff/edge-fuzz 스텝이 추가됐다.
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
│  ├─ compile/               # Compile Mode 테스트 (typed_pass.un, build/, errors/ — un native/check로 실행)
│  ├─ jit/                   # strict.un / strict_float.un (UV_JIT=2 canary)
│  └─ assert.un              # assert 모듈 (assert_eq / assert_true)
├─ scripts/                  (test.ps1, differential.ps1, unbc-roundtrip.ps1, bench.ps1,
│                            bench-report.ps1, compile-check.ps1, compile-bench.ps1,
│                            obj-demo.ps1, exe-diff.ps1, edge-fuzz.ps1)
├─ bench/                    (측정 스크립트, bench/{node,python}/, bench/results/ — results/는 gitignore,
│                            bench/obj_fib_driver.rs — `un obj` 데모 드라이버)
├─ vendor/                   (패치된 외부 crate: region, cranelift-jit, icache-coherence — vendor/README.md 참고)
├─ docs/                     # 문서 전체
│  ├─ README.ko.md           # 국문 user doc (README.md ↔ docs/README.ko.md 쌍)
│  ├─ spec/                  # 명세
│  │  ├─ ARCHITECTURE.md     # 구조                  ┐
│  │  └─ BYTECODE.md         # .unbc 포맷            ┘ 현재 상태의 기준 문서
│  ├─ guides/                # 사용/운영 가이드
│  │  ├─ CLI.md              # `un` CLI 설명서 (명령, 종료 코드, 오류/트레이스)
│  │  ├─ CLI.ko.md           # CLI 설명서 한국어 버전
│  │  ├─ TEST_GUIDE.md       # 테스트 가이드
│  │  └─ BENCHMARK_GUIDE.md  # 벤치마크
│  ├─ project/               # 기준 문서 + 이력
│  │  ├─ REFACTOR_SUMMARY.md # 전체 작업 요약 (감사 → pass 10)
│  │  ├─ PROJECT_AUDIT.md    # 감사 결과
│  │  ├─ REFACTOR_LOG.md     # 변경 이력
│  │  ├─ TECHNICAL_DEBT.md   # 부채 목록
│  │  └─ DECISIONS.md        # D-001…D-092
│  ├─ compile/               # Compile Mode 문서 (COMPILE_DESIGN/PROGRESS/FINDINGS/TESTS/BENCHMARK/OBJ_MILESTONES)
│  └─ audit/                 # 감사 보고서(AUDIT_REVIEW.md) + evidence/ 로그 + 재현 스크립트
```

## 아키텍처 메모

- Compile Mode: `un check`/`un native` 두 진입점이 있다 (`main.rs`). 파이프라인은
  `parse -> optimize -> typeck -> tir -> natcode`이며, core 함수(int/float/bool)만
  Cranelift로 컴파일하고 FFI wrapper를 통해 VM의 `try_call_jit` 경로에 등록한다
  (`get_tir_entry`, `UV_JIT` 우회). 자세한 내용은 `docs/compile/COMPILE_DESIGN.md`.
- `un obj <file.un|file.unbc> [-o out.obj]`: `.unbc`/소스를 Cranelift로 번역해 재배치
  가능한 오브젝트(.obj)를 출력한다 (`objify.rs`, `natcode.rs`의 `obj_compile`).
  core 부분집합 + 소스 경로의 `if`/`while`/`for` 루프를 지원한다. 소스 경로는
  추가로 `Str`(리터럴/concat/`str == str`), `list[int]`/`list[float]`(리터럴,
  `for ... in`, `xs[i]` read/write, 리스트 메서드 `add`/`insert`/`reverse`/`sort`/`pop`/`contains`/`index_of`)와
  `dict[str, int]`/`dict[str, float]`/`dict[int, int]`(리터럴, `d["k"]` read/write),
  `len(x)`까지 지원하며 제네릭 elem kind로 `list[str]`, `dict[int, str]`,
  `dict[str, ClassName]`도 컴파일된다(중첩 리스트의 element는 불투명 핸들).
  `unrt_*` C ABI로 런타임 연산을 호출한다(plain build에서는 이 ABI를
  `libnrt.a`가 구현하고, `--embed`에서는 VM이 구현한다). 등록된 list/dict 멤버
  시그니처는 정적 체크에도 반영된다.
  class/예외(`try`/`catch`/`finally` + `catch`할 수 없는 int `%` 0 포함) 역시
  네이티브로 컴파일된다. 클래스는 TIR 메타데이터로 네이티브 등록하고
  `<classes_init>` 바이트코드를 쓰지 않는다.
  필드는 annotated 되어 있어야 하고, 순수 getter property(타입 `any`)는 plain
  native에서 거부된다(`--embed`). `un native`(Vm 모드)는 클래스를 인터프리터로
  fallback시킨다.
  `.unbc` 경로는 Int 코어만 지원한다.
  `un build <file.un> [-o out.exe]`는 소스 → .obj → 링크 → 실행형까지 단일 명령으로
  수행한다 (rustup self-contained mingw gcc 경유 rustc). **plain exe는 VM을 전혀
  포함하지 않는다**: `rust/nrt/`(`libnrt.a`)의 독립 네이티브 런타임을 링크하고
  Int/Float/Bool/Str, list/dict, 클래스(필드·메서드·상속), try/catch, 네이티브 모듈
  (io/math/iter/os/sys/fs/codes/re/time/types)을 모두 기계어로 실행한다.
  `random`은 시간 기반 시드라 `un run`과 byte-identical할 수 없어 plain build가
  거부하고 `--embed`를 안내한다.
  클래스는 TIR 메타데이터로 네이티브 등록하고 `<classes_init>` 바이트코드를 쓰지
  않는다. 약 5 MB exe이며, `un run`과 stdout/stderr/exit code가 바이트 단위로 같다.
  기본값 인자, lambda(캡처 + closure 객체로 escape/리스트 저장 포함), 단순 사용자
  모듈 import도 no-VM exe에서 동작한다: 기본값은 TIR 호출부에 채워지고, lambda는
  NRT closure 객체(tag 8)로 생성돼 간접 호출되며, 사용자 모듈은 빌드 시 텍스트로
  인라인된다. traceback도 NRT가 생성해 VM과 byte-identical이다(소스/프레임/에러
  위치를 NRT에 등록).
  단순 모듈 전역 상태(NRT 전역 슬롯)도 지원한다. 동적 타입(`Ty::Any`/Union,
  타입 없는 파라미터)과 lambda 값의 `any` 전달·반환은 plain build가 거부하고
  `un build --embed`를 안내한다.
  `tir::nrt_supported`가 경계를 판정하고, `tir::core_rejection_reason`이
  어노테이션 힌트를 출력한다. 검증은 `scripts\obj-demo.ps1`, `scripts\exe-diff.ps1`
  (no-VM 마커 + exe 크기 + 클래스/모듈 출력 비교), 진행 상황은
  `docs/compile/OBJ_MILESTONES.md`.
- **언어 기능(2026-11-23)**: 타입 별칭 `type Name = Ty`, 패턴 가드·or-pattern·중첩 구조 분해·`case x is T` narrowing, bytes 리터럴 `b"..."`과 `bytes` 모듈(12함수)·`fs.read_bytes`/`write_bytes`를 지원한다. `bytes`는 런타임에서 임시 `List<Int>` 표현을 쓰고(후속 `Payload::Bytes` 교체 지점은 `native/bytes.rs` 어댑터 3함수), plain build는 `nrt_supported`가 거부해 `--embed`를 안내한다. `un check`는 union narrowing과 exhaustiveness를 타입 수준에서 정밀화했다.
- **제네릭 제약**: 타입 파라미터마다 제약을 하나 둘 수 있다(`fn add[T: int | float](a: T, b: T) -> T`). 사용 가능한 이름은 `int`, `float`, `int | float`, `bool`, `str`, `list`, `dict`와 클래스 이름이다. 산술 연산 `+ - * / // **`은 제약이 `int`, `float` 또는 `int | float`일 때만 허용되고, 제약 없는 `T + T`는 컴파일 오류다. `%`는 위 세 가지 제약에서, 비트연산은 `int` 제약에서 지원한다. 숫자 union(`int | float`) 산술은 결과도 union으로 추론되고, 고정 폭 타입(`i8` 등)은 상수 전파 기반 범위 검사를 수행한다. 제약은 호출부 인자에도 적용된다(예: `int | float`에 `str` 전달 시 거부).
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
- **외부 crate 의존성**: 대부분의 코드는 표준 라이브러리만 쓰지만, `rust/Cargo.toml`은 crates.io에서 `cranelift-codegen/-frontend/-jit/-module/-native 0.137.1`을 가져오고, `vendor/` 아래 패치된
  `region 3.0.2`, `cranelift-jit 0.137.1`, `wasmtime-internal-jit-icache-coherence 50.0.1`이
  있다(Windows/GNU에서 `dlltool.exe` 부재로 `windows-sys 0.52.0`을 빌드할 수 없기 위해
  raw kernel32 FFI로 교체). 새 외부 crate는 정말 필요할 때만 추가하고 `vendor/`에 패치할
  수 있으면 raw FFI로 해결한다.
- `Op` enum은 explicit `#[repr(u8)]`로 고정되어 있다. opcode 값을 바꾸면 `.unbc`가 깨진다.
- 네이티브 모듈은 `rust/src/native/`에서 `AnModule` 어트리뷰트 없이 `native::init`이 명시적 등록한다.
- `README.md`(영어)와 `docs/README.ko.md`(한국어)는 같은 내용의 한/영 쌍이다. 사용자 기능을 바꾸면
  두 문서를 함께 갱신한다.

## 테스트와 검증

- 통합 테스트 러너(`.csproj`)는 없다. runner는 PowerShell 스크립트이고, 빌드 산출물로
  `.un` 스크립트를 실행한다: `scripts/test.ps1`(test), `scripts/bench.ps1`(bench, 6레인),
  `scripts/differential.ps1`(JIT on/off 비교), `scripts/unbc-roundtrip.ps1`(.unbc 왕복),
  `scripts/compile-check.ps1`(Compile Mode 교차 검증), `scripts/exe-diff.ps1`(no-VM exe 동치),
  `scripts/edge-fuzz.ps1`(크로스 레인 fuzz), `scripts/bench-report.ps1`(HTML 리포트/-Gate).
- `scripts/unbc-roundtrip.ps1`은 Positive manifest 전체와 negative 중 컴파일 가능한
  파일을 `un run` vs `un compile` + `un run <file>.unbc`로 실행해 stdout/stderr/
  exit code를 바이트 비교한다. `-IncludeNegative`를 쓰면 negative manifest도 포함한다.
- `scripts/test.ps1`은 `test/MANIFEST.txt`의 `group|path` 행을 실행하고 exit code와
  stdout의 `N passed, M failed`를 집계한다. 정상 기준은 **Positive 50 files /
  1606 passed / 0 failed / 0 crashed**, Negative 51 checks 매칭 + 0 documented divergences,
  Known 0이다. runner는 `[FAIL]`이 증가하면 exit 1로 끝난다. (2026-11-23 재측정)
  그 외 게이트: `cargo test --release --workspace` 262 passed, differential 101 파일 / 0 차이,
  `.unbc` 왕복 Positive 50/50 + Negative 42 identical(9 skip), compile-check 36/0, exe-diff 42/0.
- `test/negative/manifest.txt`는 `path|expected exit|expected substring` 행으로
  stderr와 exit code를 검증한다(파싱 오류, 미처리 예외, stack overflow 포함).
- `test/KnownFailures.txt`의 파일은 runner가 KNOWN FAIL로 기록하고 Positive 집계에
  포함하지 않는다. 현재 등록된 파일은 없다(2026-11-16에 `test/regression/defects.un`의
  6개 항목을 표준 semantics로 고치고 `regression` 그룹으로 옮겼다).
- `test/KnownDivergent.txt`의 파일은 전부 문서화된 차이이다. 정상적으로 실행되지 않지만
  의도된 동작 차이이므로 실패 집계에 포함하지 않는다.
- `test/test_suite.un`은 가장 넓은 스모크 테스트이며 `test/assert.un`을 모듈로 사용한다.
  정상 기준 출력은 `109 passed, 0 failed`이고, 실패 시 `[FAIL]` 라인이 나온다.
- 상대 import/순환 import/디렉터리 모듈 검증용 픽스처는 `test/fixtures/circular/`,
  `test/fixtures/rel_test/`, `test/fixtures/dirmod/`, `test/fixtures/dirall/`에 있다.
- 벤치마크 스크립트는 `bench/`에 있고, 언어별 대응은 `bench/node/`(Node.js)와 `bench/python/`(Python)에 있다.
  측정은 웜업 + 반복 + 통계(최솟값·중위값·평균·최댓값·표준편차·연산 수)를 사용한다.
  `-Group all`은 6레인(Un VM/Un JIT/Compile Mode/standalone exe/Python/Node)을 측정하고,
  `scripts/bench-report.ps1`이 `bench/report.html`을 생성하며 `-Gate`로 회귀를 검사한다.
  `BENCHMARK.md`는 측정 시점 커밋을 명시하며(최근 2026-11-24 00:47, `64e4b7e`,
  Un JIT/Python geomean 3.882x), 결과와 측정 한계는 `docs/guides/BENCHMARK_GUIDE.md`를 참고한다.
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
- `.gitignore`에는 `bin/`, `obj/`가 없다(현재 `bin/`·`obj/` 디렉터리는 존재하지 않지만, 그런 경로가 생겨도 자동으로 제외되지 않는다). 로컬 에이전트 상태 디렉터리는 제외된다.
- 생성된 빌드 산출물이나 도구 캐시를 저장소에 추가하지 않는다.
