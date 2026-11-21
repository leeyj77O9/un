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
| 3 | 런타임 ABI — 생성 코드의 `unrt_*` 호출 | **완료** (`un build --embed`는 `libunvm.a`(VM), plain `un build`는 `libnrt.a`(독립 런타임)를 링크. 두 경로 모두 `io.write`, Str, list/dict, 클래스(상속 포함), 예외, 기본값 인자, 람다/closure, 단순 모듈 import/전역, byte-identical traceback 지원. 아래 "No-VM plain executable" 섹션 참고) |
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
plain `un build`도 임의의 annotated 클래스를 지원한다(클래스는 TIR 메타데이터로
네이티브 등록되며, 이전 경로가 쓰던 `<classes_init>` 바이트코드는 현재 plain exe에 없다).

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
검사한다 (현재 32 checks: embed 9 + plain 22 + no-VM 마커 1). Part B는 바이트
동일성만 확인하므로, Part C가 `bare_for.un`에서 no-VM 마커와 exe 크기(<10 MB)를
별도로 검증한다. 새 기능을 지원할 때마다 이 디렉터리에 빌드 가능 픽스처를 추가하면
plain exe 회귀를 잡을 수 있다.

## 제한 사항

아래 초기 제한 목록은 역사적 기록이다. 현재 상태는 "No-VM plain executable" 섹션을
참고할 것 — Float/딕셔너리/클래스/클로저/예외/모듈 로딩/전역 상태 대부분이 이제
지원된다. 남은 제한은 동적 타입(`Any`/Union), 시간 시드 `random`, 모듈 최상위
부수 효과, Float 인자/반환을 가진 closure/메서드(ABI 제약), 기본값이 있는 base
  클래스 ctor 파라미터의 인자 순서, 메서드 오버라이드 시 기본값 선택이다.
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
  list 조작은 NRT 런타임 함수(`unrt_*`)를 거치고, `for v in xs`의 `v`는 Class Obj 핸들이다.
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
- `unrt_dump_uncaught`는 인터프리터의 `report()`와 마찬가지로 stdout에 출력한다.
  NRT는 소스/프레임/에러 위치를 등록하므로 plain exe도 소스 컨텍스트 traceback을
  `un run`과 byte-identical하게 생성한다(이전 "frozen exe에는 없다"는 제한은 해소됨).
- 클래스 지원 (plain exe): plain `un build`(`un obj`)가 클래스도 네이티브로
  지원한다. 클래스 템플릿은 TIR 메타데이터에서 네이티브 등록되고, 메서드는
  `un_tir_N` 함수 포인터로 직접 호출된다. 이전 구현이 쓰던 `<classes_init>`
  바이트코드 실행과 `unrt_register_classes`는 plain 경로에서 제거됐다.
  `ClassNew`→`__init__`, field get/set, method/property dispatch, 상속 체인
  lookup은 모두 NRT(`libnrt.a`)가 기계어로 수행하며, VM 브릿지로 재진입하지
  않는다. 상속받은 메서드/필드도 base 체인 lookup을 통해 정적 체커가 인지한다.
  자세한 현재 구현은 아래 "No-VM plain executable" 섹션 참고.
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
- 네이티브 모듈의 Int/Float/Bool/Str 결과 멤버들(`math.sign/isqrt/is_nan/is_infinite/is_finite`,
  `os`/`sys`의 pid/name/arch/etc, `time.epoch`/`epoch_ms`/`monotonic(_ms)`,
  `fs`의 exists/file/dir/readonly/hidden/size/created/modified/accessed/
  join/parent/name/stem/ext/absolute/relative/normalize/cwd/temp/home/exe/app,
  `codes.bin/oct/hex/chr/ord/ascii`, `random.int/float/bool/chance/uuid`, `re.test/escape`)는
  `TirExpr::NativeCallExpr`(native module table → VM bridge)로 하강한다.
  반환값이 Int/Float/Bool/Str-String이어야 하며(string은 Obj 포인터 그대로 유지),
  다른 타입을 반환하는 멤버는 정적 타입을 정할 수 없으므로 core에서 거부된다.
- 로컬 변수 어노테이션 초기화(`dp: list[int] = [0]` 같은 `x: T = e` 문장)와 반복 변수 없는
  `for 0..n`(디스카드 슬롯에는 `'$'` 접두 합성 이름을 씀)은 compile mode에서 지원된다
  (2026-11-20). interpreter와 동작이 같다. `x: T`만 단독으로 쓰는 어노테이션 문장은 여전히
  거부된다.
- `un build`의 생성 드라이버는 소스/프레임/클래스/closure/전역을 NRT에 등록한 뒤
  `un_main()`을 호출하고 그 반환값을 exit code로 쓴다. `io.write`는 NRT가 stdout에
  직접 쓴다(VM stdout 버퍼 아님).
  **plain exe는 메인 파일의 모듈 최상위 문장을 실행하지 않는다** — 진입 proto(`fn main`,
  `un_main`으로 export)만 실행된다. 인라인된 사용자 모듈의 최상위 전역 초기화는
  `main` 본문 앞에 삽입되어 실행된다. `un run` / `un native` / `--embed`는 인터프리터
  의미론이라 모듈 최상위 문장이 한 번 실행되고 `main()`은 파일이 직접 호출할 때만 실행된다.
  함정 예: 메인 파일 top-level의 `io.write("boot")`는 plain exe에서 조용히 무시된다.
- `use`로 가져와야 하는 모듈이라도 `io.write`처럼 **네이티브 테이블에 등록된** native 호출은
  `unrt_call_native`로 연결되므로 독립 exe에서 바로 동작한다. 사용자 `.un` 모듈 import도
  빌드 시 텍스트 인라인으로 지원한다(단순 함수/전역 초기화만; 전역 재할당 등 부수 효과는
  `--embed` 전용).

## No-VM plain executable (2026-11-21, 전체 네이티브 런타임 이식)

- **plain `un build`는 이제 `libunvm.a`를 절대 링크하지 않는다.** `rust/nrt/`
  (`libnrt.a`)가 VM 브릿지 43개(`unrt_*`)와 `un_math_*`를 전부 독립 구현한다.
  Cranelift가 만든 `.obj` + `libnrt.a`만 링크하고, 결과 exe는 약 5 MB(VM 링크 시
  약 17 MB), `un run`과 stdout/stderr/exit code가 바이트 단위로 같다.
- 이식 범위: Int/Float/Bool/Str, `list[int|float]`, `dict[str,int|float]`,
  클래스(필드·메서드·`__init__`), `try`/`catch`/`finally`/`throw`, pending error,
  그리고 `io`/`math`/`iter`/`os`/`sys`/`fs`/`codes`/`re`/`time`/`types`/`random`
  모듈 멤버. `re`는 VM의 순수 Rust regex 엔진(`native/regex/`)을 그대로 복사해
  사용하므로 의미론이 같다.
- 클래스: `obj_compile`이 TIR의 `class_names`/`class_fields`/`class_methods`를 읽어
  `(class, method, TIR index, arity, return kind)` 목록을 `EmitOutcome`으로 반환하고,
  생성 드라이버가 `nrt_register_class/field/method`를 호출한다. 메서드는 `.obj`의
  `un_tir_N` 함수 포인터로 등록되고, `<classes_init>` 바이트코드와
  `unrt_register_classes`는 plain 경로에서 제거됐다. 메서드가 Int/Float/Bool를
  raw로 반환하므로 NRT가 Obj 핸들로 boxing한다(`ret_kind`).
- `tir::core_rejection_reason`이 core-subset 이탈 사유(파라미터/로컬 타입, 어노테이션
  힌트)를 알려주고, `un build` 실패 시 출력된다. `UV_TIR_DEBUG=1`에서도 표시된다.
- 기본값 인자: `TirFn.defaults: Vec<Option<TirExpr>>`가 파라미터별 기본값을 들고
  있고, `TirProgram::pad_default_args()`가 모든 함수 lowering 후 호출부에서 빠진
  인자를 채운다. `unrt_class_new`/`unrt_method_call`도 네이티브 메서드에
  파라미터 종류(`kinds`)를 전달해 Obj 핸들을 raw Int/Float/Bool로 unbox한다
  (명시적 `__init__`이 있는 클래스도 VM과 바이트 동일). 파라미터를 참조하는
  기본값(예: `fn f(a, b = a + 1)`)은 호출부에서 슬롯이 달라지므로 TIR이 거부한다.
- Lambda(정적 해석 서브셋): `synthesize_lambda`가 lambda 본문을 별도 `TirFn`으로
  합성하고, 외부 함수의 지역변수를 캡처하면 합성 함수의 leading 파라미터로 넘긴다.
  `f = fn(...)`의 `f(…)` 호출과 `(fn(…))(…)` 즉시 호출은 `TirExpr::Call`로 rewrite
  되므로 NRT/natcode 변경 없이 기계어 직접 호출이 된다. lambda 값을 컨테이너에
  저장하거나 반환하는 형태(escape)는 거부한다.
- 클래스 상속: typeck의 `collect_class`가 base 클래스의 ctor 파라미터와 required
  개수를 파생 클래스에 병합하도록 수정(VM은 base 인자를 먼저 받음). 생성 드라이버의
  `un_tir_N` extern 선언을 중복 제거해, base 메서드가 여러 클래스에서 참조돼도 링크된다.
- 사용자 모듈 import: `inline_user_modules_text`가 빌드 시 `use a.b as alias`를
  텍스트 기반으로 인라인하고 함수명을 `alias_`로 mangle한 뒤 `alias.member(`를
  `alias_member(`로 치환한다. 모듈 최상위에 함수 선언 외 문장(전역 상태 등)이
  있으면 거부한다.
- VM이 필요한 나머지 기능(동적 타입, lambda escape, 모듈 전역 효과)은 plain
  build가 `un build --embed` 안내와 함께 거부한다. 거부된 프로그램이 조용히 VM을
  링크하는 일은 없다.
- 드라이버 ABI 주의: `unrt_len`은 raw i64가 아닌 **Obj 핸들**을 반환하고,
  `io.write`는 값 사이에 `" "`를 넣고 마지막에 `"\n"`을 붙인다.
  `unrt_take_error()`는 pending error가 없으면 `0`을 반환한다.
- 검증: `scripts/exe-diff.ps1` Part C가 "no-VM native runtime" 마커와 10 MB 미만
  exe 크기를 확인하고, Part B의 `bare_for`/`iter_array`/`iter_minmax`/
  `native_modules`/`class_list`가 모두 no-VM 경로로 `un run`과 바이트 비교된다.
  `scripts/obj-demo.ps1`의 클래스 exe도 같은 경로로 통과한다.
- Traceback: 드라이버가 `nrt_register_source`로 소스 텍스트/경로를 등록하고,
  `nrt_push_frame`으로 `<module>` 프레임과 호출부 프레임을 기록한다. TIR의
  `TirExpr::Located { span, inner }`가 에러 발생식의 위치를 natcode에 전달하고,
  `nrt_set_error_site`가 caret 위치를 기록한다. NRT `unrt_dump_uncaught`가
  VM의 `trace_string` + `LangError::display`와 동일한 형식으로 출력한다.
  uncatchable Panic(`% 0`)은 tag 10으로 구분해 위치 없이 메시지만 출력한다
  (`test/compile/build/traceback.un`으로 byte 비교 검증).
- Lambda escape: lambda가 NRT의 tag 8 closure 객체(코드 포인터 + 캡처 값 +
  파라미터 종류)로 생성된다. 드라이버가 `nrt_register_lambda`로 각 lambda 함수
  포인터를 등록하고, `TirExpr::MakeClosure`가 `unrt_obj_closure`를 호출한다.
  호출부는 `TirExpr::CallObj` → `unrt_call_obj`로 인자를 boxing/unescaping해
  간접 호출한다. `list[lambda]` 요소도 지원하므로 `xs = [f]; xs[0](4)`가 동작한다
  (`test/compile/build/lambda_escape.un`). NRT에 `iter.map`/`filter`의 closure
  호출 구현도 추가했다.
- 모듈 전역 상태: `inline_user_modules_text`가 모듈 최상위 `name = ...`을 수집해
  `alias_name`으로 mangle하고 본문의 참조도 word-boundary로 치환한다. 드라이버가
  `nrt_register_global`로 전역을 등록하고, TIR의 `GlobalGet`/`GlobalSet`이 NRT의
  `unrt_global_get`/`unrt_global_set`으로 읽기/쓰기한다. 모듈 init 문장은 `main`
  본문 앞에 삽입된다(`test/compile/build/module_globals.un`은 init + 읽기를
  검증한다; 함수 내 전역 재할당은 VM의 `StoreLocal` 의미론과 달라 아직 미지원).
- 알려진 제한: lambda를 `any` 타입 파라미터로 넘기거나 반환하는 형태, 동적 타입
  코드(`Ty::Any`/Union, 타입 없는 함수 파라미터)는 여전히 `un build --embed`
  전용이다. `test/assert.un`처럼 타입 없는 함수가 있는 모듈은 전역 상태가
  인라인돼도 typeck에서 거부된다.
  이들은 TIR의 정적 타입 모델을 넘어서는 기능으로, 완전 지원은 별도 백엔드
  확장(closure 객체 + 간접 호출, 전역 슬롯, 태그 기반 동적 연산)이 필요하다.
