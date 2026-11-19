# CLI 설명서 (`un`)

`un`은 Un 언어의 명령행 인터페이스입니다 — Rust로 작성된 바이트코드 컴파일러 +
레지스터 기반 VM(`unvm`, 라이브러리/툴체인 이름)입니다. 이 문서는 `un` 바이너리만
다루고, 언어 자체는 `README.md` / `docs/README.md`, 바이트코드 포맷은
`docs/spec/BYTECODE.md`를 참고하세요.

> **명칭 이력**: CLI 바이너리는 2026-11-19에 `unvm`에서 `un`으로 개명되었습니다.
> 라이브러리 crate, WASM 플레이그라운드(`unvm-playground`), 테스트 픽스처, 그리고
> 일부 옛 문서는 여전히 `unvm` 이름을 씁니다.
>
> English version: `docs/guides/CLI.md`

## 빌드

```powershell
rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml
```

Windows(GNU 툴체인)에서는 `rust\target\release\un.exe`가 생성됩니다. `un`을
어디서든 실행하고 싶으면 이 디렉터리를 `PATH`에 추가하세요.

## 사용법

    un <command> [args]

| 명령 | 용도 |
|---|---|
| `un run <file.un>` | 스크립트를 컴파일하고 실행 |
| `un run <file.unbc>` | 미리 컴파일된 바이트코드 실행 (예: `un run hello.un.unbc`) |
| `un compile <file.un>` | 스크립트를 `<file.un>.unbc`로 컴파일 |
| `un check <file.un>` | **실험적**: 정적 타입 분석만 (compile mode) |
| `un native <file.un>` | **실험적**: 정적 분석 + native 실행 (compile mode) |
| `un obj <file.un|file.unbc> [-o out.obj] [--entry name]` | **실험적**: 소스/바이트코드를 재배치 가능한 오브젝트 파일(Windows/GNU는 COFF .obj)로 번역; core Int 부분집합만 지원 |
| `un build <file.un> [-o out.exe] [--embed]` | **실험적**: 소스를 재배치 가능한 오브젝트로 컴파일해 독립 실행형으로 링크 (rustup self-contained mingw gcc 경유); `--embed`는 .unbc와 VM 런타임을 실행형에 구워 임의의 Un 프로그램을 단일 exe로 |
| `un dis <file.un>` | 라인 번호와 함께 스택 바이트코드 덤프 |
| `un dis-reg <file.un>` | 바이트코드의 register-plan 형태 덤프 |
| `un tokens <file.un>` | 토큰 스트림 출력 |
| `un ast <file.un>` | AST 출력 |
| `un desugar <file.un>` | desugar된 최적화 AST 출력 |
| `un opt <file.un>` | 옵티마이저 패스 수와 최적화된 AST 출력 |
| `un help` | 사용법 요약 |

파일 인자에 확장자가 없으면 `.un`이 자동으로 붙습니다. 경로는 현재 작업 디렉터리
기준으로 해석합니다. import되는 모듈은 importing 파일 기준 상대 경로를 먼저 찾고,
없으면 `Global.PATH` 기준으로 찾습니다. 디렉터리 모듈은 `mod.un` 또는 디렉터리
내 모든 `.un` 파일을 로드합니다.

## 종료 코드

| 코드 | 의미 |
|---|---|
| 0 | 성공 |
| 1 | 미처리 오류 (파싱/컴파일 오류, 런타임 오류, 스택 오버플로) |
| 2 | 사용법 오류 (인자 오류, 예: 파일명 누락) |

## `un run <file.un>`

파일을 파싱·최적화·컴파일하고 실행합니다. 출력은 stdout으로 나가며, 오류도
stdout으로 출력됩니다(인터프리터가 죽기 전에 flush됩니다).

    $ cat hello.un
    use io

    fn main()
        io.write("hello, un")

    main()

    $ un run hello.un
    hello, un

### 오류와 트레이스

미처리 오류는 raise 지점의 위치, 오류가 지나온 프레임들의 트레이스(선택), 메시지를
출력한 뒤 종료 코드 1로 끝납니다.

```text
fn inner()
    -> 1 / 0

fn outer()
    inner()

outer()
```

    $ un run example.un
    un: error trace
        -> <module>  <example>, line [9]
             outer()
        -> outer     <example>, line [7]
             inner()

    <example>, line [4], column [8]
        -> 1 / 0
           ^^^^^
    error: int division by zero

규칙:

- 프레임은 outermost-first로 나열되며 각 프레임 아래에 소스 라인이 표시됩니다.
  프레임의 위치 표기(`<file>, line [N]`)는 에러 캐럿 블록과 동일한 규칙을 씁니다.
  모듈 스코프는 `<module>`, 익명 `fn ... ->`는 `<lambda>`로 표시됩니다.
- 프레임이 1개뿐이면 출력하지 않습니다 — caret 블록이 이미 raise 지점을 보여주므로.
- 반복되는 프레임은 `... (repeated N more times)`로 접힙니다.
- 트레이스가 10개 프레임을 넘으면 compact 모드로 전환합니다: 라인 정보만 출력하고
  소스 라인은 생략합니다(예: 교차 재귀).
- `catch`는 트레이스를 클리어합니다 — 잡힌 오류 그 자체는 아무것도 출력하지 않습니다.
- 모듈: import된 모듈 내부에서 발생한 오류는 모듈 기준 파일/라인으로 표시되며,
  import를 유발한 `use` 라인이 프레임으로 기록됩니다.

자주 나오는 메시지들(오류 사례별 exit code/stderr 계약은 `test/negative/manifest.txt`
참고): `error: int division by zero`, `error: key '<key>' not found in dictionary`,
`circular import detected: '<path>'`, `error: maximum recursion depth exceeded`,
`error: undefined variable '<name>'`.

## `un run <file.unbc>` / `un compile`

`un compile <file.un>`은 소스 옆에 `<file.un>.unbc`를 작성합니다(전체 파일명에
`.unbc`를 덧붙이므로 `hello.un`은 `hello.un.unbc`로 컴파일되고,
`un run hello.un.unbc`로 실행합니다). 소스 자체도 임베드되어 오류 표시에 쓰입니다.
`.unbc`는 자급 바이트코드 컨테이너입니다(opcode 스트림, proto, 상수 풀, 이름 테이블,
source map, 소스 자체). 로딩 시 경계 검증을 수행 — 잘린 파일, 범위를 벗어난 span,
끝 쓰레기 바이트는 VM이 시작되기 전에 거부됩니다.

`un run file.un`과 `un run file.unbc`는 바이트 동일한 출력을 만듭니다. 이는 CI에서
검증됩니다(`scripts/unbc-roundtrip.ps1`). 바이트코드 포맷은 `docs/spec/BYTECODE.md`에
문서화되어 있으며, `Op` 값은 고정(`#[repr(u8)]`)이므로 어떤 빌드에서 만든 `.unbc`도
이후 빌드에서 계속 로드할 수 있습니다.

## 진단 명령

아래 명령들은 파이프라인(`Source -> Lexer -> Parser -> Optimizer -> Compiler -> VM`)을
들여다보는 창입니다. 전부 파일 인자 1개를 받습니다.

### `un tokens`

토큰당 한 줄씩 출력: `Type start..end<옵션 "span 텍스트">`. 출력의 빈 줄은 빈
span에 대응합니다(예: `Dedent 27..27`).

    $ un tokens add.un
    Func 0..2 "fn"
    Identifier 3..6 "add"
    LParen 6..7 "("
    Identifier 7..8 "a"
    Comma 8..9 ","
    Identifier 10..11 "b"
    RParen 11..12 ")"
    NewLine 12..13 "\n"
    Indent 13..17 "    "
    Return 17..19 "->"
    ...

### `un ast` / `un desugar` / `un opt`

AST의 들여쓴 pretty-print입니다. `un ast`는 파싱된 트리를, `un desugar`는
desugar 이후 트리를, `un opt`는 옵티마이저 패스 수와 fold된 트리를 출력합니다.
예시(`un opt add.un`):

    passes: 1
    Block 0..36
      Function 0..25
        Identifier 3..6 "add"
        Tuple 6..12
          Parameter 7..8
            Identifier 7..8 "a"
          Parameter 10..11
            Identifier 10..11 "b"
        Block 17..25
          Return 17..25
            Binary 20..25
              Identifier 20..21 "a"
              Identifier 24..25 "b"
      Call 27..36
        Identifier 27..30 "add"
        Tuple 30..36
          Integer 31..32 1
          Integer 34..35 2

(이 입력에서는 `un desugar`가 같은 트리를 출력합니다. 옵티마이저는 상수 표현식만
fold 하므로 단순한 프로그램은 영향이 없습니다.)

### `un dis`

모든 proto의 스택 바이트코드를 덤프합니다(`names`, `consts`, proto별 코드),
명령어별 라인 번호 포함:

    $ un dis add.un
    names: ["<module>", "add", "a", "b", "a", "b", "add"]
    consts: [Int(1), Int(2)]
    proto 0 name "<module>" flags 0x0000 code 11
       0 MakeFunc flags 000001 op     0 depth   1 line 1
       1 Unwrap flags 000000 op     0 depth   1 line 1
       2 Pop flags 000000 op     0 depth   0 line 1
       3 LoadName flags 000000 op     0 depth   1 line 4
       ...
    proto 1 name "add" flags 0x0001 code 7
       0 LoadName flags 000000 op     0 depth   1 line 2
       ...

### `un dis-reg`

같은 프로그램을 VM이 실제로 실행하는 register plan 형태로 변환한 것입니다
(scratch base, register 수, 명령어별 operand):

    $ un dis-reg add.un
    proto 0 name "<module>" scratch_base 0 reg_count 4 code 11
      params: []
      locals raw: [] names arena: ["add"]
       0 MakeFunc a 0 b 0 c 0 d 0 flags 000001
       ...

## 환경 변수

| 변수 | 의미 |
|---|---|
| (없음) | 기본값: 적격 Int/Float 부분집합에 임베디드 Cranelift JIT 활성화 |
| `UV_JIT=0` | 인터프리터만 사용 (`scripts/differential.ps1`이 JIT on/off를 바이트 단위로 비교할 때 사용) |
| `UV_JIT=2` | strict canary: JIT 적격 proto가 하나라도 인터프리터로 fallback 되면 프로세스가 종료 코드 2로 죽는다 (CI의 JIT 회귀 감지용) |

## 실험적 (개발 중, 변경될 수 있음)

아래는 진행 중인 compile mode에서 나온 명령들로, 완전한 타입 어노테이션이 붙은
시그니처를 요구하며 아직 안정적 CLI 계약의 일부가 아닙니다.

### `un check <file.un>`

정적 타입 분석만 수행 — 실행하지 않습니다. 타입 검사에 실패하면
`compile error:` 진단을 출력하고 종료 코드 1, 성공하면 0을 반환합니다.

    $ un check hello.un
    <hello.un>, line [1], column [8]
        fn add(a, b)
               ^
    compile error: parameter 'a' of 'add' requires a type annotation
    ...
    un: 3 compile-mode error(s)

### `un native <file.un>`

정적 분석 + native 실행 (compile mode의 `un run`에 해당). `un check`와 동일하게
어노테이션 요구사항이 적용됩니다.

### `un obj <file.un|file.unbc> [-o out.obj] [--entry name]`

소스 또는 바이트코드를 재배치 가능한 네이티브 오브젝트 파일(`.obj`,
Windows/GNU는 COFF)로 되돌립니다. `un compile`이 생성한 `.unbc`를 읽거나
(`core Int` 부분집합만), `.un` 소스의 경우 `parse → optimize → typeck → TIR`
파이프라인을 거쳐 core subset을 Cranelift로 기계어로 하강합니다. 소스 경로는
`if`/`while`/`for` 루프도 네이티브로 컴파일합니다. 진입 proto(기본 `main`)는
`un_main`으로 export되며 C ABI(`() -> i64`)를 따르므로 실행형으로 링크할 수
있습니다. Un의 모든 값은 i64로 하강합니다. 부분집합 밖을 쓰는 proto는
`skipped:` 경고와 함께 제외되며, 정적 스크린은 통과했지만 lowering에 실패한
proto는 trap stub으로 정의되어 링크를 유지합니다(호출 시 트랩). 마일스톤은
`docs/compile/OBJ_MILESTONES.md`를 참고하세요.

### `un build <file.un> [-o out.exe]`

소스에서 독립 실행형 네이티브 실행형까지 한 명령으로 수행합니다. 위 과정으로
오브젝트 파일을 방출한 뒤, `un_main()`을 호출하고 i64 결과를 출력하는 작은
드라이버를 생성해 rustup의 self-contained mingw gcc로 링크합니다. 오브젝트가
VM 런타임을 import하면(예: core 경로에서 `io.write` 호출) 링크가 미해결 심볼로
실패합니다 — 현재 Int core에는 런타임 import가 없어서 독립 빌드가 가능합니다.

`--embed`를 쓰면 같은 명령으로 프로그램의 `.unbc` 이미지를 드라이버에 구워
`libunvm.a`(전체 VM 런타임, 정적 링크)와 함께 링크합니다. embed 모드는
**모든 Un 프로그램**(class, collection, IO, 문자열 보간 등)에서 완전하게
동작하며, 단일 자체 완결 실행형을 만듭니다. 자세한 것은
`docs/compile/OBJ_MILESTONES.md`를 참고하세요.

## 구현 참고

- Rust 2021, 단일 외부 의존성 트리(JIT용 Cranelift). 스크립트 실행에는 런타임
  의존성이 필요 없습니다.
- VM은 64 MiB 스택의 워커 스레드에서 실행되므로, 깊은 재귀는 프로세스 스택을
  오버플로하지 않고 `error: maximum recursion depth exceeded`로 종료됩니다.
- 저장소 루트의 `main.un`은 사용자 스크래치 파일로, 제품 진입점이 아니며 이
  문서의 CLI 동작 범위에서 제외됩니다.
