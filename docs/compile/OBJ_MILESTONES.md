# OBJ_MILESTONES.md — `un obj` / `un build` 진행 상황

> 목표: `.unbc` 바이트코드 또는 `.un` 소스를 재배치 가능한 네이티브
> 오브젝트 파일(`.obj`, Windows/GNU는 COFF)로 번역해 링커로 독립 실행형
> `.exe`를 만든다. 이 문서는 진행 상황과 검증 방법을 기록한다.

## 배경

`un native`는 core 부분집합의 함수를 **실행 중 메모리에** Cranelift로 컴파일해
VM이 호출한다 (in-process). `un obj`/`un build`는 같은 Cranelift를 사용하되
디스크에 오브젝트(및 실행형)를 출력하는 **산출물** 모드다. 링커는 rustup
self-contained mingw gcc를 경유한 `rustc`가 담당한다.

## 설계

- 입력 1: `.unbc` → Int core 부분집합만 지원 (`objify.rs`).
- 입력 2: `.un` 소스 → parse → optimize → typeck → TIR → `.obj` (`natcode::obj_compile`). `if`/`while`/`for` 루프까지 네이티브.
- 모든 un 값은 기계에서는 **i64** 하나로 표현한다 (int = i64, bool = 0/1, none = 0).
  파라미터는 i64 블록 파라미터(슬롯 순서대로), 함수 본문의 지역 변수는
  `proto.locals`(또는 TIR `locals`)에 대응하는 Cranelift `Variable`들로 매핑한다.
- 함수 내부 호출은 같은 모듈의 다른 proto로 **직접 call**을 내보낸다.
  `LoadName` → 파라미터/로컬/동일모듈 proto 순서로 해석한다.
- 정상 작동 못하는 proto는 경고와 함께 skip된다. 정적 스크린은 통과했지만
  lowering에 실패한 proto는 **trap stub**(호출 시 트랩)으로 정의해 링커가
  항상 유효한 심볼을 갖도록 한다.
- 진입 proto(기본 `main`)는 `un_main`으로 export된다 (C ABI, `() -> i64`).
- float 값은 f64로 하강하지만 엔트리 포인트는 i64만 export하므로, 반환을
  출력하는 데모 드라이버는 int 결과만 출력한다.

## 단계별 상태

| 단계 | 내용 | 상태 |
|---|---|---|
| 1 | 수직 슬라이스: core 부분집합(Int 상수/지역변수/산술/비교/`JmpIf`/직접 호출/`Return`) → COFF `.obj`, fib 데모 | **완료** |
| 2 | `for i in a..b` / `while` 반복 하강 | **완료** (소스 경로: TIR ForRange/While을 그대로 하강; 루프 경계 가드 포함) |
| 3 | VM 정적 링크 ABI — 생성 코드가 VM 런타임을 정적 링크해 Object 호출 | **3b 진행 중** (3a: `unvm`이 `staticlib`로 빌드되고 `unrt`(`rust/src/unrt.rs`)가 C ABI 제공. `un build --embed`는 임의의 Un 프로그램을 독립 exe로 빌드 — class/예외 포함. 3b: 생성 코드가 `unrt_call_native`/`unrt_obj_int` 등으로 VM 런타임을 직접 호출 — `io.write`, Str 리터럴/concat/`str == str`, `list[int]`/`list[float]`(리터럴, `for ... in`, `-> list[int]`, `io.write` 인자, `xs[i]` read/write, `len`)와 `dict[str, int]`/`dict[str, float]`(리터럴, `d["k"]` read/write, 함수 인자/반환), **`try`/`catch`/`finally`와 `throw`**까지 네이티브 컴파일. 인터프리터와 출력 바이트 동일(보통 함수 body에서 `->`/`break`/`skip`이 안 쓰일 때). 클래스를 사용하는 함수는 plain native에서 명시적으로 거부되며 `un build --embed`로 빌드) |
| 4 | `.un` 소스에서 바로 `un obj` (typeck/TIR 재사용) | **완료** |
| 5 | `un build` — 링크까지 자동화 (self-contained mingw gcc 자동 탐지) | **완료** |

## 검증

```powershell
# fib 데모: un compile → un obj → rustc link → 실행 → "fib = 55" + un build 단일 명령
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\obj-demo.ps1

# 개별 실행
.\rust\target\release\un.exe compile fib.un
.\rust\target\release\un.exe obj fib.un.unbc -o fib.obj
.\rust\target\release\un.exe obj fib.un -o fib_src.obj     # 소스 → .obj (루프 지원)
.\rust\target\release\un.exe build fib.un -o hello.exe     # 소스 → .exe 단일 명령
.\rust\target\release\un.exe build --embed embed_test.un   # 임의 프로그램(class/dict/문자열/IO)을 embed 런타임으로 .exe

# --embed 검증: 출력이 un run과 바이트 동일
$env:TEMP\embed_test.exe
```

`un build --embed`는 클래스를 사용하는 프로그램도 원본과 동일한 출력을 낸다.

`scripts\obj-demo.ps1`가 PASS(`obj-demo: fib = 55` + `obj-demo (un build): 55`)를
반환한다. 오브젝트 심볼은 `llvm-nm`으로 확인할 수 있다: `un_main`(export),
`un_proto_<i>`/`un_tir_<i>`(Local).

## 제한 사항

- 상수: `Int`/`Bool`/`Str` 리터럴을 지원한다 (`Float`/딕셔너리/집합/튜플/클래스/클로저/예외/모듈 로딩은 skip).
  네이티브 exe가 지원하는 문자열은 `unrt_*`를 통해 VM Str을 사용한다.
- `list[int]`/`list[float]`: 리터럴(`[1, 2, 3]`), 파라미터/렛/대입, `for v in xs` 또는
  `for v in [1, 2, 3]`(리터럴 직접 반복),
  호출 인자/반환(`-> list[int]`), `io.write(xs)` 네이티브 출력까지 지원한다.
  빈 리스트(타입 추론 불가)와 중첩 리스트(`list[list[int]]`)는 **거부**한다 —
  `un build`는 exit 1로 실패하고 프로그램은 인터프리터에서 계속 실행된다.
  **인덱싱**: `xs[i]` read, `xs[i] = v` write(음수 인덱스 wrap), 범위 밖이면
  Obj 경로는 user trap `user2`로 프로세스를 종료한다(`un run`은 exit 1 + traceback).
- `list[int]`/`list[float]` 메서드: `add(v)`(append), `insert(v, i)`(인덱스 clamp),
  `reverse()`, `sort()`(VM comparator), `pop()`/`pop(i)`(원소 반환),
  `contains(v)`, `index_of(v)`까지 네이티브 지원한다. 나머지 리스트 메서드는
  인터프리터로 넘긴다. 정적 체커(`un check`/`un native`)는
  등록된 list/dict 멤버 시그니처를 알고 있으며, 없는 멤버를 호출하면 컴파일 오류다.
- `dict[str, int]`/`dict[str, float]`: 리터럴(`{"k": v}`), `d["k"]` 인덱스 **read**,
  `d["k"] = v` **write**(없는 키는 삽입),
  파라미터/렛/대입/함수 반환, `io.write(d)`까지 지원한다. 빈 dict(추론 불가)와
  중첩 dict(value가 dict), 비 str 키는 **거부**한다. `d["missing"] = v`처럼 새 키를 넣는
  패턴도 인터프리터와 같이 동작한다. Obj 경로에서 `d["missing"]` read는
  user trap `user1`으로 프로세스를 종료한다
  (`un run`은 `error: key '{key}' not found in dictionary`를 내며 exit 1).
- `len(x)`: `str`(UTF-16 코드 유닛, 인터프리터와 동일)과 `list[int]`/`list[float]`/
  `dict[str, int]`/`dict[str, float]`의 크기를 반환한다. 다른 타입은 인터프리터로 넘긴다.
  `un native`에서는 len 포함 함수도 모두 인터프리터 fallback.
- 예외(`try`/`catch`/`finally`): 네이티브 exe가 `unrt_*`로 연결하는 **모든** VM 오류를
  현재 함수의 활성 `catch`로 전달한다. 없는 키(`d[missing]`), 범위 밖 리스트 인덱스,
  `throw(expr)`, 그리고 하강된 `try`의 body/finally에서 나는 모든 오류가 포함된다.
  `int / int`에서 제수가 0이면 Obj 경로도 `emit_int_zero_guard`가
  `unrt_make_int_div_zero`로 인터프리터와 동일한 `int division by zero` 에러 Obj을
  `pending`에 주차해 활성 핸들러로 분기한다. 핸들러가 없으면 `unrt_dump_uncaught`가
  인터프리터와 동일하게 stdout에 `error: ...` 한 줄을 출력하고 exit 1.
  `//`도 같은 규칙. `INT_MIN // -1`은 인터프리터의 wrapping_div와 같이
  `INT_MIN`으로 wrap한다(`b_safe = 1`로 치환해 hardware #DE 트랩을 원천 차단).
  `int / int`는 `Float`으로 승격되어 `Float`을 반환하지만
  (VM과 동일), 나누기 전 0 검사는 유지된다.
- int `%` 0 나누기는 **catch 불가**다. 인터프리터는 `divide_by_zero()`로
  `Payload::Panic`(`System.DivideByZeroException: Attempted to divide by zero.`)을 만들고
  `try`/`Unwrap`은 `Payload::Error`만 잡으므로 항상 프로세스가 종료된다.
  Obj 경로도 `emit_mod_zero_guard`가 활성 핸들러를 무시하고
  `unrt_make_mod_zero_panic` → `unrt_dump_uncaught`로 stdout 한 줄 출력 + exit 1을
  강제한다(인터프리터와 바이트 동일).
- 네이티브 `try` 제약: `body`/`finally` 블록 안에서 `return`/`->`/`break`/`skip`은
  사용 불가 (컴파일 오류). 중첩 `try`는 정상적으로 동작한다.
  활성 핸들러가 없는 상황에서 leaf 오류가 나면 `unrt_dump_uncaught`가 exit(1)로
  종료한다. `Vm` 모드에서 try/catch/throw를 쓰는 함수는 인터프리터로 bail되어
  항상 인터프리터와 동일하게 동작한다.
- `unrt_dump_uncaught`는 인터프리터의 `report()`와 마찬가지로 stdout에 출력한다
  (소스 컨텍스트 traceback은 frozen exe에 없다).
- class/프로퍼티/동적 디스패치는 네이티브 미지원이며, plain `un obj`/`un build`는
  클래스 타입을 사용하는 함수를 **셋 엔트리보다 먼저** (`skipped`에 등재하지 않고)
  폐기하고 빌드를 거부한다:
  `un build: class-typed functions are not supported in a native executable yet; rebuild with un build --embed`.
  `--embed`는 전체 VM을 정적 링크하므로 임의의 클래스 프로그램이 동작한다.
  (plain 경로로 클래스를 지원하려면 exe에 클래스 등록 테이블 + `Body::Native` 디스패치
  기계 장치가 필요하다 — 별도 단계로 보고 있다.)
- `str == str`은 VM의 `key_eq`로 네이티브 비교한다.
- `//` 및 `Int / Int`에서 제수가 0이면 **catchable exception**이며, int `%` 0은
  **uncatchable Panic**이다 (위 예외 항목 참고). 두 경우 모두 exit code는 1이고
  출력은 인터프리터와 바이트 동일하다.
- 재귀는 Depth 가드 없이 그냥 스택 재귀다 (스택 오버플로는 프로세스를 죽인다). VM 경로는 VM depth 가드가 있다.
- `un build`의 드라이버는 `unrt_init()` → `un_main()` → `unrt_flush_stdout()` 순서로
  호출한다. Str 출력은 VM runtime의 stdout 버퍼를 통해 나간다. 엔트리 반환이
  float/bool이면 값을 출력하지 않고 숫자 그대로/0으로 출력한다.
- `use`로 가져와야 하는 모듈이라도 `io.write`처럼 **네이티브 테이블에 등록된** native 호출은
  `unrt_call_native`로 연결되므로 독립 exe에서 바로 동작한다. 일반 모듈 import(사용자 `.un` 파일 등)는
  여전히 embed 런타임(`--embed`)을 쓴다.
