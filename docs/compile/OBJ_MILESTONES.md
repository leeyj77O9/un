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
| 3 | VM 정적 링크 ABI — 생성 코드가 VM 런타임을 정적 링크해 Object 호출 | **3b 진행 중** (3a: `unvm`이 `staticlib`로 빌드되고 `unrt`(`rust/src/unrt.rs`)가 C ABI 제공. `un build --embed`는 임의의 Un 프로그램을 독립 exe로 빌드 — class/예외 포함. 3b: 생성 코드가 `unrt_call_native`/`unrt_obj_int` 등으로 VM 런타임을 직접 호출 — `io.write`, Str 리터럴/concat/`str == str`, `list[int]`/`list[float]`/`list[class]`(리터럴, `for ... in`, `-> list[int]`, `io.write` 인자, `xs[i]` read/write, `len`, list 메서드)와 `dict[str, int]`/`dict[str, float]`(리터럴, `d["k"]` read/write, 함수 인자/반환), **`try`/`catch`/`finally`와 `throw`와 클래스**까지 네이티브 컴파일. 인터프리터와 출력 바이트 동일(보통 함수 body에서 `->`/`break`/`skip`이 안 쓰일 때). 클래스는 `<classes_init>` 스파인을 exe에 임베드해 시작 시 한 번 등록(`unrt_register_classes`); 임의 annotation 기본값/모듈 데코레이터도 동작) |
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
plain `un build`도 임의의 annotated 클래스를 지원한다(`<classes_init>`을 exe 초기화 시 한 번 등록).

`scripts\obj-demo.ps1`가 PASS(`obj-demo: fib = 55` + `obj-demo (un build): 55` + `obj-demo (un build class): 37` + `obj-demo (un build class list): 37`)를
반환한다. 오브젝트 심볼은 `llvm-nm`으로 확인할 수 있다: `un_main`(export),
`un_proto_<i>`/`un_tir_<i>`(Local).

`scripts\compile-check.ps1`도 함께 쓴다 — `test/compile/*.un` 픽스처 6개가 `un run`/`un native` 바이트 동일 + exit 0인지,
그리고 `test/compile/errors/*.un` 부정 픽스처가
`un check`/`un native` 양쪽에서 exit 1 + 기대 진단 문자열을 내는지 검사한다
(현재 18 checks, errors/* 11개 + untyped-source-rejected 포함). 거부 케이스를 추가하려면 픽스처 + `test/compile/errors/manifest.txt`
한 행만 추가하면 된다.

`scripts\exe-diff.ps1`은 **exe 검증**을 담당한다 — `test/compile/*.un`을 `un build --embed`로
링크한 exe와, `test/compile/build/*.un`(plain native 빌드 가능 프로그램만 두는 디렉터리)을
plain `un build`로 링크한 exe가 각각 `un run`과 stdout/stderr/exit code 바이트 동일한지
검사한다 (현재 7 checks). 새 코어 기능을 지원할 때마다 이 디렉터리에 빌드 가능 픽스처를
추가하면 plain exe 회귀를 잡을 수 있다.

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
- `list[class]`: `list[int]`/`list[float]`와 같은 모든 위치(리터럴, 파라미터/렛/대입,
  `for v in xs`, 호출 인자/반환, `xs[i]` read/write, `add`/`insert`/`reverse`/`sort`/`pop`/`pop(i)`/
  `contains`/`index_of`, `len(xs)`)에서 native 컴파일된다. 정적 타입이 element 타입이므로
  list 조작은 VM 런타임 브릿지를 거치고, `for v in xs`의 `v`는 Class Obj 핸들이다.
  `list[int]`/`list[float]`가 거부하는 것(빈 리스트, 중첩 리스트, 비 str 키 등)은 동일하게 거부한다.
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
  `list[class]`/`dict[str, int]`/`dict[str, float]`의 크기를 반환한다. 다른 타입은 인터프리터로 넘긴다.
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
  `Payload::Panic`(`error: Attempted to divide by zero.`)을 만들고
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
- 클래스 지원 (Obj 경로): plain `un build`(`un obj`)가 클래스도 네이티브로
  지원한다. 프로그램 시작 시 exe에 박혀 있는 `<classes_init>` 프로토가 VM의
  `unrt_register_classes`에 의해 실행되어 인터프리터와 동일하게 클래스 템플릿을
  world에 등록한다(한 번만 실행). 네이티브 코드의 클래스 연산은 전부 VM
  브릿지 경유: `ClassNew`→`tobj_call`(clone_obj + `__init__`), field get/set과
  method call은 `get_attr`/`set_attr`과 동일한 의미론(프로퍼티, `__getattr__`,
  super/parent 체인 포함)으로 `unrt_obj_int`류 브릿지처럼 수행된다. 상속받은
  메서드/필드도 base 체인 lookup을 통해 정적 체커가 인지한다.
  class/property/method dispatch는 전부 VM 런타임으로 재진입하는 브릿지이므로
  Int-catchable 외의 mis-run 위험이 없다.
  - 임의 기본값은 항상 바인딩된 클래스의 등록 스파인(임의 표현식 허용)을 쓰므로
    `@decorator`류 참조도 동작한다(기본값 표현식/데코레이터가 쓰는 모듈 함수는
    `bind_module_functions`가 미리 바인딩).
  - 한계: plain native exe의 클래스 프로퍼티는 getter의 반환 타입이 정적 타입
    (`Int`/`Float`/`Bool`)으로 단정되지 않으므로(`any`), 순수 getter 읽기는
    거부된다(annotate된 backing field는 정상). dict 값의 class 타입, 중첩된
    클래스 선언, 클래스 등록 스파인의 기본값 표현식이 모듈 전역 변수를
    참조하는 경우는 지원하지 않고 `--embed`를 써야 한다.
    `list[ClassName]`은 native로 지원된다(리터럴/`for v in xs`/`xs[i]` read+write/
    `add`·`insert`·`pop`·`pop(i)`·`reverse`·`sort`·`contains`·`index_of`/`len(xs)`);
    중첩된 클래스 리스트(`list[list[Point]]`)와 빈 리스트는 거부하므로 `--embed`가 필요하다.
  - 클래스를 쓰는 함수는 Vm(`un native`)에서는 여전히 인터프리터 fallback.
- `str == str`은 VM의 `key_eq`로 네이티브 비교한다.
- `//` 및 `Int / Int`에서 제수가 0이면 **catchable exception**이며, int `%` 0은
  **uncatchable Panic**이다 (위 예외 항목 참고). 두 경우 모두 exit code는 1이고
  출력은 인터프리터와 바이트 동일하다.
- 재귀는 Depth 가드 없이 그냥 스택 재귀다 (스택 오버플로는 프로세스를 죽인다). VM 경로는 VM depth 가드가 있다.
- `iter.array(v, n)` — `iter` 모듈의 고정 서명을 타이프체커가 인식하는 유일한 iter intrinsic.
  `v`가 int면 `list[int]`, float이면 `list[float]` 반환. `n`이 음수면 interpreter와 같은
  size 오류. 다차원 size(`[d1, d2, ...]`)는 지원하지 않는다 (core는 flat list만).
- 로컬 변수 어노테이션 초기화(`dp: list[int] = [0]` 같은 `x: T = e` 문장)와 반복 변수 없는
  `for 0..n`(디스카드 슬롯에는 `'$'` 접두 합성 이름을 씀)은 compile mode에서 지원된다
  (2026-11-20). interpreter와 동작이 같다. `x: T`만 단독으로 쓰는 어노테이션 문장은 여전히
  거부된다.
- `un build`의 드라이버는 `unrt_init()` → `un_main()` → `unrt_flush_stdout()` 순서로
  호출한다. Str 출력은 VM runtime의 stdout 버퍼를 통해 나간다. 엔트리 반환이
  float/bool이면 값을 출력하지 않고 숫자 그대로/0으로 출력한다.
  **plain exe는 모듈 최상위 문장을 실행하지 않는다** — 진입 proto(`fn main`, `un_main`으로
  export)만 실행된다. 이에 반해 `un run` / `un native` / `--embed`는 인터프리터 의미론이라
  모듈 최상위 문장이 한 번 실행되고 `main()`은 파일이 직접 호출할 때만 실행된다.
  함정 예: top-level의 `io.write("boot")`는 plain exe에서 조용히 무시된다.
- `use`로 가져와야 하는 모듈이라도 `io.write`처럼 **네이티브 테이블에 등록된** native 호출은
  `unrt_call_native`로 연결되므로 독립 exe에서 바로 동작한다. 일반 모듈 import(사용자 `.un` 파일 등)는
  여전히 embed 런타임(`--embed`)을 쓴다.
