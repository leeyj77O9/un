# JIT Float 수학 intrinsic 설계

상태: **Accepted** (2026-11-18 구현 완료, 커밋 `4ba4f14`). 상세는 §7 결정 목록과
`docs/DECISIONS.md` D-078·`docs/REFACTOR_LOG.md` Pass 23 참고.
관련: `docs/CODE_NORMALIZATION_REPORT.md` §6·§7 (item 2, Done), `docs/DECISIONS.md` D-078,
`docs/TECHNICAL_DEBT.md` TD-51, `test/jit/strict.un`

## 0. 요약

`math` 모듈의 수학 함수(`sin`, `cos`, `sqrt`, `floor`, …)를 JIT에서 **벤치마크 모양이
아닌 일반적인 Float intrinsic**으로 낮춘다. 인자가 Int/Float이 아니거나 수학적 정의역을
벗어나면 JIT은 tag=2로 **bail**하고 interpreter가 그대로 다시 실행되어 Un의 정확한
오류(예: `cannot compute sqrt of a negative number`)를 발생시킨다. 결과 태그 규칙은
interpreter의 반환 규칙을 그대로 따른다(Int→Int identity, Float→Float 등).
`math10000` / `sin20000`급 수치 루프를 정직하게 JIT으로 실행하는 것이 목표다.

제거된 `match_floor_sqrt`/`match_math_sin`(이름 텍스트 13-instruction 윈도우 매칭)은
이 설계의 반례다. 이 설계는 (1) 후보 호출 형태가 bytecode 상 name-text로 명확히 식별되고,
(2) 도메인 위반은 전부 bail로 빠지며, (3) 어떤 Int/Float 입력에 대해서도 interpreter와
byte-identical 출력을 내는 것만 목표로 한다.

## 1. 배경과 문제 정의

정규화 패스(`424831f`, `ef61e07`)에서 벤치마크 전용 JIT intrinsic을 제거한 뒤,
`math.*` 호출이 포함된 proto는 전부 JIT 후보에서 탈락한다. 실측:

```
$ UV_JIT_DEBUG=1 un.exe run bench/floatmath.un
JITREJECT emit_ins ip=21 op=LoadName
JITREJECT proto[1] sin_sum (panic in JIT codegen, fallback stub)
$ UV_JIT_DEBUG=1 un.exe run bench/math.un
JITREJECT proto[1] math_work (panic in JIT codegen, fallback stub)
```

`ip=21`은 proto `sin_sum`의 `LoadName "math"`다. `LoadName` arm이 알려진 이름이
아니라고 `None`을 반환하고, 그 proto는 전체가 interpreter로 넘어간다. `iter.range`는
peephole로 살아 있으므로 루프 본체가 `math.sin(i)` 하나라도 포함되면 루프 전체가
interpreter에서 실행된다.

이것은 의미론 위반이 아니라 **성능 회귀**다(interpreter가 항상 정답). 이 설계는
semantics를 바꾸지 않고 그 회귀를 되돌린다.

## 2. 목표 / 비목표

목표:

- `math` 모듈의 숫자 함수 부분집합을 JIT에서 f64 산술 + FFI 호출로 낮춘다.
- interpreter의 **오류 의미론과 byte-identical 출력** 유지 (UV_JIT=0 differential 통과).
- 반환 태그 규칙을 interpreter와 동일하게:
  - `floor`/`ceil`/`trunc`: Int 인자 → 입력과 동일 값(identity)을 Int로 반환.
  - `trunc`: Float 인자 → `obj::int(v.trunc() as i64)`(포화 변환)과 동일하게 Int.
  - `sign`: 항상 Int (`NaN → 0` 포함).
  - `pow`: 두 인자가 Int && `e >= 0.0` && 결과가 유한 && `fract() == 0.0` → Int, 아니면 Float.
  - 그 외 전부 Float.
- 이름 텍스트 + argc 만으로 인식한다. 입력 값이나 반복 횟수에 의존하는 매칭은 금지.
- `use math`만 있는 일반 수치 프로그램이 대상이다. 라이브러리 이름을 바꿔 끼운 프로그램은 interpreter로 간다.

비목표:

- 오류 메시지 변경 없음. `obj::err("expected number type")` 등 interpreter가 발생시키는
  메시지는 JIT이 직접 만들지 않고 bail 후 interpreter 재실행으로 나온다.
- NaN payload 변경 없음. 양쪽 경로가 같은 Rust `f64` 메서드를 호출하므로 동일하다.
- Float 파라미터/비교/분기 지원 추가 없음 (이미 부분 지원: `fadd/fsub/fmul`,
  `ToBool`의 fcmp 등). 새 지원을 설계하지 않는다.
- `use math`되지 않은 bare `sin(1)` 호출 등 global 경유 호출 추가 지원 없음.
- `math.min`/`math.max`, `math.round(v, digits)`, `math.isqrt`, `math.gcd`/`lcm`, `degrees`/`radians`(신규): Phase 2.
- 두 인자 이상의 optional 인자를 받는 함수의 keyword 호출: Phase 1 제외.

## 3. 확인된 사실 (증거)

Bytecode 형태 — `fn t(n) ... total += math.sin(i)` 의 proto 1 (`un.exe dis`):

```
21 LoadName 3 (math)     22 GetAttr 4 (sin)    23 Unwrap    24 ArgBegin
25 LoadLocal 1 (i)       26 ArgPos             27 Call 1    28 Add
```

- `LoadName` arm(`jit_bc.rs` ~1720): 이름이 파라미터·`len`·클래스 ctor·JIT 후보 proto가
  아니면 `None` → proto reject. `"math"`를 넣을 자리가 이미 있다(`len` 선례).
- `GetAttr` arm(`jit_bc.rs` ~1888): receiver register 마킹 기반의 marker class 발화
  지점. `(LoadName, GetAttr, Call)` 시퀀스(`jit_bc.rs` ~1612 주석)에 이미 대응 구조가 있다.
- `Op::Call` arm(`jit_bc.rs` 2169): `callee_reg`를 검사하는 if-chain
  (class ctor → `len_regs` → `list_add_regs` → `method_regs` → `call_candidate_regs`).
  새 분기는 `len_regs` 분기 뒤, user-call 분기 앞에 넣는다.
- 결합 규칙: 인자는 `callee_reg + 1..`(연속 register), argc는 `ins.d`.
- FFI 선례: `un_dict_int_has`(`rust/src/runtime/int_dict.rs:143`) —
  `#[no_mangle] pub extern "C" fn`, `jit_bc.rs:175`에서 `builder.symbol`,
  `jit_bc.rs:251`에서 `Linkage::Import`로 declare, emitted 코드가 `ins().call`.
- bail 패턴: `create_block`/`brif`/`terminated = true`/`emit_bail_exit`(`jit_bc.rs`
  1264) — "Cannot raise an Un error in JIT code"의 표준 escape hatch.
- Float 반환: `Op::Return` arm(`jit_bc.rs` 2823) — Float 값은 F64 Variable,
  반환 시 `bitcast(I64, f)` 후 tag=1. promoted Float local은 `poly_tag`로 per-path 태깅.
  Int compare on Float는 `fcmp(FloatCC::NotEqual, …)` + `select`으로 이미 구현됨
  (`Op::Lt => FloatCC::LessThan` 등, ~2362).
- `is_float` / `ret_slot` / `local_float` 추적은 레지스터별로 이미 존재 (emit_user_call
  경로가 RET_FLOAT을 처리).
- rebind guard: `program_has_intrinsic_rebind`(`jit_bc.rs` ~60)는 `Op::StoreName` /
  `Op::SetAttr` / `Op::MakeFunc`만 스캔하고(`compiler.rs:1731`의 `Use`는 무관),
  store 대상 name exact match(`"iter"|"range"|"len"`)로 **프로그램 전체** JIT veto를 한다.
  이 설계에서는 4.1의 조건부 규칙으로 확장한다.
- 태그 상수: `jit_cache.rs:20-37` — `TAG_INT=0` / `TAG_FLOAT=1`(IEEE-754 bits) /
  `TAG_BAIL=2` / `TAG_BOOL=3` / `TAG_NONE=5` / `TAG_INTLIST=6`.
- 숨겨진 결과 셀: callee signature은 `(hidden: I64, args: I64…) -> ()`이고, `hp[0]=bits`,
  `hp[1]=tag`를 `store`로 기록(`emit_return_pair`/`jit_fast.rs`의 `jit_call` unsafe
  extern "C" transmute 경로와 짝). JIT fn은 Float 인자를 받지 않으며, `try_call_jit`는
  `Payload::Int`만 전달한다.
- native는 Un 오류를 raise할 수 없다 — `fn(&Scope) -> Obj` 시그니처를 갖지만 JIT에서 부를 수
  있는 FFI shim은 scope/World에 접근할 수 없고 sentinel만 반환할 수 있다. Un 오류가 필요한
  상황은 반드시 tag=2 bail → pristine 인자로 interpreter 재실행이다. (이 설계도 같은 구조.)
- `assert_err`는 try/catch로 감싸고 텍스트를 비교하지 않는 라벨일 뿐(`test/assert.un:65-71`)
  이므로, `test/unit/native/math.un`의 `sqrt(-1)` 케이스는 JIT/interpreter 양 lane에서
  label 일치만 확인한다. `log(0)` 케이스도 동일. negative manifest에는 math 오류 케이스가
  없다. `test/jit/strict.un`(UV_JIT=2 canary)은 math 호출을 포함하지 않는다 — math intrinsic은
  이 canary에 넣지 않고 differential 게이트에 의존한다.
- interpreter 수학 의미론(`rust/src/native/math.rs`):
  - `get_number`: Int/Float만, else `err("expected number type")`
  - `sqrt`: `v < 0.0` → `err("cannot compute sqrt of a negative number")`
  - `log`: `v <= 0.0` → `err("expected value greater than 0")`
  - `floor`/`ceil`: Int → 입력 값 그대로, Float → `obj::float(v.floor())`
  - `trunc`: Int → 입력 값 그대로, Float → `obj::int(v.trunc() as i64)` (포화)
  - `sign`: NaN 또는 0 → 0, else ±1 (항상 Int)
  - `pow`: 양쪽 Int && `e >= 0.0` && 결과가 유한 && `fract() == 0.0` → Int else Float
  - `abs`: `wrap()` — Int → `obj::int(v.abs() as i64)`, Float → Float
  - `hypot`: `obj::float(x.hypot(y))` (f64::hypot, TD-45에서 고쳐짐)
- 검증 게이트: `test/unit/native/math.un`에 `sqrt(-1)` 오류 케이스 포함
  (`assert_err(fn() -> math.sqrt(-1), "cannot compute sqrt of a negative number")`) —
  UV_JIT=0 vs default differential이 intrinsic의 오류 경로까지 검증해 준다.
- bench가 `iter.range` + `math.floor(math.sqrt(i))` / `math.sin(i)` 형태다
  (`bench/math.un`, `bench/floatmath.un`). `assert.assert_eq(math_work(1000), 20615)`는
  Float==Int 비교가 필요한데 Float `Op::Eq`는 현재 JIT에서 reject되므로 그 assert 함수는
  계속 interpreter로 돈다(무해).

## 4. 설계

### 4.1 인식 경로 (intent: `len` 마커의 확장)

1. **LoadName arm** (`name == "math"`):
   - `param_pos` miss 이후, marker 후보에 `"math"` 추가 → `math_regs.insert(ins.a)`,
     dummy `iconst(I64, 0)` emission (len과 동일 처리).
2. **GetAttr arm**: receiver가 `math_regs`에 있고 field name이 대상 함수 집합에 있으며
   이후 `ArgBegin…ArgPos…Call`이 실제 함수 호출을 형성하면 `mathfn_regs[dst] = MathFn`.
   마커는 `call` 시점에 소비되어 제거된다. 벗어나면 기존대로 `None`.
3. **Call arm**: `mathfn_regs`에 callee가 있으면 math intrinsic 분기. argc가 함수
   signature와 불일치하면 `None` (가드 함수 케이스 포함).

재정의 차단은 컴파일-타임 결정이라 실행 전(module body)에 일어나는 rebind를 잡아야 한다.
`program_has_intrinsic_rebind`를 아래 조건부 규칙으로 확장한다:

- **규칙 1 (무조건 veto)**: store/definition 대상 name이 정확히 `"math"`이면
  프로그램 전체 interpreter. (`math = 5`는 global을 갈아치운다. `use math`는
  `Op::Use`라 스캔 대상이 아니다.) MakeFunc의 math-proto도 동일.
- **규칙 2 (조건부 veto)**: 어떤 `SetAttr`의 대상 name이 Phase-1 함수 이름 집합
  (`"sqrt"|"sin"|…`)에 속하고 **프로그램 어딘가에 `LoadName "math"`가 있으면** veto.
  - 대상은 store 대상 attr 이름으로만 판단한다 — 수신자가 math인지 구분하지 못하는
    텍스트 규칙이므로 이것이 정확성의 대가다. 대신 math를 쓰지 않는 프로그램은 영향 없음.
  - 별칭 경로(`m = math; m.sin = spy`)도 `LoadName "math"`가 source로 나타나므로 포착된다.
  - `self.sin = 3`처럼 math와 무관한 instance 속성을 math 사용 프로그램에서 재정의하면
    JIT이 꺼지는 collateral이 남는다 — `len`/`iter`의 문서화된 bluntness와 같은 범주이며,
    정확성보다 우선할 수 없는 trade-off다. (런타임 probe로 해결할 수는 있으나, jit은
    plan time에 program global scope를 만지지 않으므로 엔진 구조 변경이 필요하다.)

어느 규칙에도 걸리지 않으면 marker 자체는 `emit_user_call`처럼 **런타임 probe로
재확인하지 않는다**(name-text만으로 인식). 즉
"이름이 정확히 `math`인 global에서 정확히 `sin`인 멤버를 읽어 호출하는 형태"만 빠르고,
나머지 전부는 기존 동작 그대로다.

### 4.2 인자 → f64

- `is_none[arg]` 또는 `is_bool[arg]` → `None` (interpreter가 Un 오류 발생).
- `is_float[arg]` → `rv_f64(arg)` 직접 사용.
- Int → `fcvt_from_sint(F64, v)` (Un promotes Int+Float→Float이므로 승격).
- `is_none`/Bool은 FFI 경계에서 절대 넘기지 않는다.

### 4.3 가드 (bail 블록)

| 함수 | 조건 | 위반 시 |
|---|---|---|
| `sqrt` | `x < 0.0` | bail (interpreter가 `cannot compute sqrt of a negative number`) |
| `ln`, `log2`, `log10`, `log(v[, base])` | `x <= 0.0` | bail (interpreter가 `expected value greater than 0`) |
| 그 외 | 없음 | — |

가드는 JIT이 `fcmp(FloatCC::LessThan(OrEqual), x, zero_f64)`로 직접 만들어 bail 블록으로
`brif`한다. bail 블록은 `emit_bail_exit` 패턴 그대로: tag=2, `terminated = true`.
entry의 hidden pointer 호출자는 tag=2를 보고 interpreter로 pristine 인자를 다시 실행한다.

`log`의 base 인자(`base`)는 검사하지 않는다 — interpreter도 `b.ln()`을 그대로 수행하므로
base ≤ 0은 NaN/-inf로 동일하게 흐른다. FFI가 같은 Rust `f64::ln`을 호출하므로 동일하다.

### 4.4 결과 태그 규칙

| 함수 | Int → | Float → | 비고 |
|---|---|---|---|
| `sin` `cos` `tan` `asin` `acos` `atan` | Float | Float | |
| `sqrt` `cbrt` `exp` `ln` `log2` `log10` | Float | Float | sqrt/ln류는 가드 |
| `log(v, base?)` | **base 포함.** `base`가 `is_none` → bail(interpreter의 `is_none(&base)` 분기 — `v.ln()` 결과가 interpreter와 동일). `base`가 Bool/비수치 → bail("expected number type"). 그 외 `un_math_log(v, b) = v.ln() / b.ln()` → Float. argc 1/2 분기, 가드 `x>0` | Float | interpreter와 동일 경로 |
| `floor` `ceil` | **identity Int** | Float | JIT에서 identity 분기(파라미터별) |
| `trunc` | **identity Int** | **Int** (`x.trunc() as i64`) | FFI `un_math_trunc_i64` |
| `sign` | Int | Int | `x.is_nan() → 0` 포함, fcmp unordered 사용 |
| `pow(x, e)` | both-Int → **bail** (tag=2, interpreter가 Int 의미론 그대로 계산 — poly-tag 없이 nameless 레지스터에 Int/Float를 섞어 담을 수 없기 때문). 어느 한쪽이라도 Float → FFI `powf` → Float | Float | interpreter 규칙: `both_int && e≥0.0 && fract(r)==0.0 && is_finite()` → Int, 아니면 Float. both-Int만 그 규칙 적용이므로 bail로 정확히 재현 |
| `hypot` | Float | Float | |
| `abs` | **bail** | Float | Int 경계: `i64::MIN.abs()`는 디버그에서 panic이므로 안전하게 interpreter로 |

identity 케이스(`Int→Int`)는 FFI 호출 없이 `def_var(rv_i64(dst), src)`로 처리하고
`is_float=false`·tag=0을 기록한다. interpreter가 입력 값을 그대로 돌려주는 의미론과
정확하다.

Float 결과는 항상 `bitcast(I64, f)` + tag=1로 `rv_f64(dst)`에 기록된다(Return arm과 동일).

### 4.5 FFI shim (`rust/src/native/math.rs`)

total function만(no panic). `pub extern "C"`를 math.rs에 같이 둔다
(`un_dict_int_*`가 `runtime/int_dict.rs`에 같이 있는 것과 같은 배치).

```rust
#[no_mangle] pub extern "C" fn un_math_sqrt(x: f64) -> f64 { x.sqrt() }
#[no_mangle] pub extern "C" fn un_math_cbrt(x: f64) -> f64 { x.cbrt() }
#[no_mangle] pub extern "C" fn un_math_exp(x: f64) -> f64 { x.exp() }
#[no_mangle] pub extern "C" fn un_math_ln(x: f64) -> f64 { x.ln() }
#[no_mangle] pub extern "C" fn un_math_log2(x: f64) -> f64 { x.log2() }
#[no_mangle] pub extern "C" fn un_math_log10(x: f64) -> f64 { x.log10() }
#[no_mangle] pub extern "C" fn un_math_log(x: f64, b: f64) -> f64 { x.ln() / b.ln() }
#[no_mangle] pub extern "C" fn un_math_sin(x: f64) -> f64 { x.sin() }
#[no_mangle] pub extern "C" fn un_math_cos(x: f64) -> f64 { x.cos() }
#[no_mangle] pub extern "C" fn un_math_tan(x: f64) -> f64 { x.tan() }
#[no_mangle] pub extern "C" fn un_math_asin(x: f64) -> f64 { x.asin() }
#[no_mangle] pub extern "C" fn un_math_acos(x: f64) -> f64 { x.acos() }
#[no_mangle] pub extern "C" fn un_math_atan(x: f64) -> f64 { x.atan() }
#[no_mangle] pub extern "C" fn un_math_floor(x: f64) -> f64 { x.floor() }
#[no_mangle] pub extern "C" fn un_math_ceil(x: f64) -> f64 { x.ceil() }
#[no_mangle] pub extern "C" fn un_math_abs(x: f64) -> f64 { x.abs() }
#[no_mangle] pub extern "C" fn un_math_hypot(x: f64, y: f64) -> f64 { x.hypot(y) }
#[no_mangle] pub extern "C" fn un_math_powf(x: f64, e: f64) -> f64 { x.powf(e) }
#[no_mangle] pub extern "C" fn un_math_trunc_i64(x: f64) -> i64 { x.trunc() as i64 }
```

- x86_64(Win64/SysV) 모두 f64는 xmm으로 전달 — calling convention 문제 없음.
- wasm32: libm은 wasm libcall로 해석됨 — cfg 분기 불필요.
- `powf`는 결과를 JIT이 검사하지 않고 통째로 반환하고, tag 판정은 JIT 쪽에서 한다.
- FFI 추가에 따라 `jit_bc.rs`의 `register_eligible` FFI declare 목록과
  `declare_function` 섹션을 보수적으로 확장(함수당 `make_signature` 1개씩).

Phase 1 비목표를 유지하기 위해 `un_math_powf`와 `pow` 태그 판정은 설계에 포함하되,
리뷰에서 범위를 더 좁히고 싶다면 pow만 Phase 2로 뺄 수 있다(그 경우 `math_work`류에는
영향 없음).

### 4.6 Phase 1 대상 / 비대상

Phase 1 (module 함수 20개): `sqrt` `cbrt` `exp` `sin` `cos` `tan` `asin` `acos` `atan`
`ln` `log2` `log10` `log(v, base?)` `floor` `ceil` `trunc` `sign` `pow` `hypot` `abs(Float만)`

Phase 2 (설계만 기록, 미구현):

- `round(v[, digits])` — digits 경로가 복잡(`round_to_even`, `10f64.powi`)하고
  반환이 Int|Float 혼합이라 별도 설계 필요.
- `isqrt` Int 도메인 전용 — Int 호출 ABI로 처리 가능하지만 별도 intrinsic 계열.
- `min`/`max` — 가변 argc 가드 함수.
- `gcd`/`lcm` — 정수 도메인 + overflow 의미론 정리 필요.
- `degrees`/`radians` — math 모듈에 아직 없음 (추가하면 신규 기능).

### 4.7 math 이외의 커버리지: 별칭과 재정의

실측 확인(`dis`/`run`, 2026-11-18): `use math` + `math.sin = spy` + `io.write(math.sin())`는
실행 시 `42`를 출력한다 — 모듈 멤버 재정의는 로드 타임에 일어나고, interpreter도 그것을
존중한다. JIT은 plan 타임에 그 사실을 알 수 없으므로, 4.1의 조건부 veto가 이를 잡는다.

- `m = math; m.sin = spy` → `LoadName "math"`가 source로 나타나고 `SetAttr "sin"`가
  있으므로 규칙 2로 veto. ✓
- `math = 5` → 규칙 1로 veto. ✓
- `m = math; m.sin(1)` (재정의 없이 별칭만) → `LoadName "m"`이라 marker가 안 달려
  interpreter fallback. ✓ (정확성 유지)
- `math.sin = spy`를 하되 어디서도 `math`를 읽지 않는 프로그램 → veto 없음, intrinsic도
  발화 불가하므로 안전. ✓
- 함수 파라미터가 `math`인 경우(`fn f(math) -> math.sin(1)`): `LoadName`이 param 경로를
  타고 marker가 달리지 않으므로 interpreter. (기존 동작과 동일 — `math`라는 이름의 param은
  지금도 JIT이 거부함을 유지.)
- `math.sqrt`를 `sqrt`라는 global에 저장(`use math` 후 `sqrt = math.sqrt`) → bare `sqrt(1)`은
  `call_candidate_regs`에서 "같은 프로그램 proto 이름" 매칭이라 marker가 아니므로 interpreter. ✓

### 4.8 Soundness: "bail = 재실행"의 대가

tag=2 bail은 interpreter 재실행이다. 같은 입력에 대해 동일 결과를 보장해야 의미론이
유지된다:

- interpreter 재실행은 `get_number`부터 다시 타므로, 잘못된 인자 타입의 오류 메시지가
  그대로 재현된다 ✓
- f64 산술은 결정적이고, FFI shim은 total function이므로 재실행이 diverge할 수 없다 ✓
- `math` 멤버가 실행 중에 바뀌는 경우는 4.1의 whole-program veto가 차단한다 ✓
- 부분 부작용(FFI는 순수 f64 연산, arena를 사용하지 않음)이 없으므로
  "이미 실행된 부분"이 잘못된 상태를 남기지 않는다 ✓

### 4.9 검증 게이트 (구현 완료의 정의)

1. `scripts/differential.ps1` — UV_JIT=0 vs default, 전 파일 byte-identical.
2. `scripts/test.ps1` — Positive 1419 passed / Negative 41 matched / Known 0 유지.
3. `scripts/unbc-roundtrip.ps1 -IncludeNegative` — 컴파일↔실행 비교.
4. `cargo test --release` — 205 passed 유지.
5. `test/unit/native/math.un` — sqrt(-1) 오류 경로 포함, 양 lane byte-identical.
6. `bench/math.un`, `bench/floatmath.un` — JIT lane이 `UV_JIT=0`과 byte-identical 출력이고
   interpreter lane보다 빠름을 확인.
7. 회귀 가드: 신규 `.un` 스크립트나 기존 테스트가 math 결과를 직접 비교하는 경우는 상기 1~3이 덮는다.
   선택 사항: `test/unit/native/math_jit.un`에 Int→Int identity 및 trunc-into-Int와
   `sign(NaN)`(가능한 입력이 있다면)을 명시하는 테스트 추가.

## 5. 리스크와 대책

| 리스크 | 확률 | 대책 |
|---|---|---|
| NaN bit 불일치 | 낮음 | 양 경로 동일 Rust `f64` 메서드 사용; `assert_close`류는 실수 비교라 무해, `assert_eq` byte 비교는 NaN 입력을 안 씀 |
| rebind guard 오차단 | 낮음 | 규칙 2의 조건부 veto(math 사용 프로그램만)로 국한. `self.sin=3` collateral은 문서화 (4.1) |
| `i64::MIN.trunc()` | None | trunc는 Float→i64 FFI(포화)이고 Int identity 경로만 tag=0 |
| `pow` Int 판정 오차 | 중간 | interpreter 규칙을 그대로 복제(4.4); 판정 실패 시 Float tag — interpreter는 Int로 반환하므로 **다르면 안 됨**. 이 분기는 differential 테스트의 `pow` 케이스로 고정. 판정이 어렵다면 pow를 Phase 2로 미룸 |
| Float 비교 reject로 `assert_eq`가 interpreter行 | 무해 | 성능만 못 얻을 뿐 의미론 동일 |
| JIT strict canary(`UV_JIT=2`) | 검토 필요 | math intrinsic은 f64 전용이라 `test/jit/strict.un`의 Int 부분집합 검사에는 포함시키지 않음. 단 math intrinsic이 컴파일 실패하면 candidate가 stub이 되므로 strict canary 범위 확장 여부는 별도 결정 |
| 기존 JITREJECT 덤프 노이즈 | 낮음 | `emit_ins ... op=Call` 거부 로그가 math call 발견 후에는 사라지는지 확인 (정상 동작) |

## 6. 구현 결과 (2026-11-18)

1. ~~이 문서를 `Proposed → Accepted`로 바꾸고~~ 완료, `docs/DECISIONS.md` D-078 등록.
2. FFI shims + declare 코드 추가 (호출 없음, 빌드 green).
3. LoadName/GetAttr/Call arm + `MathFn` enum + identity 분기(태그 규칙) 구현.
4. `compute_local_float`에 math intrinsic result kinds 학습 — 이게 없으면 promoted
   Float local이 태깅되지 않아 루프가 0을 반환했다(구현 중 실측으로 발견·수정).
5. rebind guard 확장 (규칙 1: StoreName math → 무조건 veto, 규칙 2: math 사용 프로그램의
   SetAttr∈MATH_FNS → veto). `self.sin` 등 math 비사용 프로그램에는 영향 없음을 실측 확인.
6. 게이트 전부 통과: differential 88/0, test.ps1 1493/0·Negative 41, unbc-roundtrip
   47/47+34/34, cargo test 205, `UV_JIT=2` OK.
7. `bench/math.un` JIT 0.053ms(3/3), `bench/floatmath.un` JIT 0.54ms(2/2)
   (interpreter lane ≈22/27ms). `BENCHMARK.md`는 커밋된 트리에서 재측정 후 별도 docs
   커밋으로 갱신.

## 7. 결정 (2026-11-18, Reviewer 위임 하에 확정)

1. **`pow` → Phase 1 포함, both-Int 인자는 bail.** nameless 레지스터에는 Int/Float 구분을
   담을 수 없고(poly-tag는 promoted local 전용), interpreter 규칙상 both-Int && integral &&
   finite일 때만 Int 태그가 된다. 둘 다 Int면 bail → interpreter가 Int 의미론 그대로 재현.
   하나라도 Float면 interpreter도 무조건 Float이므로 FFI `powf` 사용. mixed-Float는 포함.
2. **`log`의 base를 Phase 1에 포함.** `base=none`/Bool/비수치 → bail(interpreter의
   `is_none(&base)` → `v.ln()` 분기 또는 "expected number type"을 그대로 재현). 그 외
   `un_math_log(v, b)` = `v.ln() / b.ln()`으로 interpreter와 동일 경로.
3. **`UV_JIT=2` strict canary를 Float에도 신설했다.** `strict.un`(Int)을 그대로 두고
   `test/jit/strict_float.un`을 같은 형태로 추가하고 CI `JIT Float strict canary (UV_JIT=2)`
   스텝을 넣었다. bring-up 과정에서 unary `Op::Neg`(Float 상수 부정)이 JIT 미지원임이
   실측으로 드러났다 — 그 사실은 **TD-53**으로 등록했다가, 곧바로 해결했다
   (`ineg`(Int, wrapping) / `fneg`(Float) lower, Bool/none/arena handle은 bail).
   `strict_float.un`은 이제 `-0.5`/`-7`을 써서 Neg arm을 상시 CI 검증한다.
4. **rebind 규칙 2(조건부 veto) 채택.** math 사용 프로그램은 member 이름이 정확히
   Phase-1 함수 이름인 `SetAttr`이 어디든 있으면 interpreter — 정확성 우선, `len`과 같은
   문서화된 bluntness. math를 쓰지 않는 프로그램은 영향 없음. (`use math` + `self.sin = x`
   조합만 JIT이 꺼짐.) 런타임 probe 정밀화는 global scope 접근이 필요해 별도 설계.
5. FFI shim 바인딩은 21개의 명명된 fid 필드 대신 `HashMap<MathFn, FuncId>`를 `Tr`에
   하나로 넘긴다 — 선언/심볼 등록/발화 모두 일관된 단일 경로, 리뷰 용이.
6. `jit_bc.rs`의 낡은 모듈 주석("user function calls rejected", "Float comparisons rejected")을
   구현과 함께 현행화한다.
7. ~~Accepted 후 `docs/DECISIONS.md`에 D-07x로 이 문서를 요약한다.~~ **Done** — D-078로 등록.
