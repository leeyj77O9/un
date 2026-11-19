# COMPILE_FINDINGS.md — Compile Mode 분석에서 발견한 문제와 기술적 결정

> `COMPILE_DESIGN.md`의 결정이 나온 근거나, 설계와 코드가 어긋날 때의 기록.

## F-001. 기존 CLI와의 충돌 — `un compile`은 이미 `.unbc` 산출이다

- 사실: `main.rs:318 print_help() unpacks`, `compile()` (main.rs:318)은 `.unbc` 파일을 쓴다 (`runner::compile_source` → `unbc::write`).
- 결정: Compile Mode는 **`un native`** (실행) + **`un check`** (검사만) 두 개의 새 명령으로 추가. `un compile`의 기존 의미를 바꾸지 않는다. (`COMPILE_DESIGN.md` §2)

## F-002. 반환 타입 어노테이션은 파서가 AST에 보관하지만 컴파일러는 완전히 무시한다

- 사실: `parse_function`은 반환 타입을 `Function.children[3]`으로 밀어 넣는다. `compiler.rs code_function`은 `children[0]`(name), `[1]`(params), `[2]`(body)만 읽고 `children[3]`은 어디서도 읽지 않는다. VM은 항상 `return_type: Types::Single("any")` (`vm/mod.rs` `Op::MakeFunc`).
- 결정: 컴파일 모드에서는 typeck가 이 자식을 읽어 반환 타입 검사에 사용한다. 인터프리터 동작(`any`로 표시되는 것)은 그대로 둔다.
- 영향: 기존 VM 테스트 `"f fn(int, str) -> any"` 기대값은 인터프리터 모드에서만 유지. 컴파일 모드에서 `fn(int, str) -> any` 출력이 달라지는 것은 문서화 대상.

## F-003. 매개변수 타입 어노테이션은 metadata로만 존재하고 호출 시 검사되지 않는다

- 사실: `compile_params`가 `Param { type_name }`를 Proto에 넣고, `.unbc`에 직렬화되고, MakeFunc 시 `ArgData`로 함수 객체에 실린다. 그러나 호출 경로(`bind`)는 타입을 비교하지 않는다. `FLAG_ARG_CHECK`는 err 검사만 한다.
- 결정: 인터프리터 모드는 그대로. 컴파일 모드만 typeck에서 검사한다.

## F-004. 정적 타입은 어디에도 없다 — AST에 annotations가 있는 것뿐

- 사실: `Types` (runtime/types.rs)는 문자열 union 집합으로 `is`/`in`/err 경로에만 쓰인다. 정적 분석용 표현은 존재하지 않는다.
- 결정: 컴파일 모드 전용 `Ty` 표현을 만든다. 런타임 `Types`는 중복이 아니라 역할이 다른다(동적 검사 전용). 변환하지 않고 공존시킨다. (`COMPILE_DESIGN.md` §4.1)

## F-005. `un native`의 의미: "전부 네이티브"가 아니라 "정적으로 확정된 core만 네이티브"

- 사실: 인터프리터는 `Op` dispatch + Rc<RefCell<Obj>> 값 모델. 모든 값이 Obj이고 primitive도 Obj이므로, 전부 네이티브화하려면 런타임 전체를 Object-free로 바꾸는 대규모 재작성이 필요하다.
- 결정: **core 함수만 TIR→Cranelift로 내리고, 호출 경계를 넘는 것은 기존 VM/JIT/인터프리터가 담당**한다. core 판정은 컴파일 타임에 끝난다(런타임 bail이 아님). 태그 불일치가 실제로 발생할 수 있는 경우(any 경계)는 진입 가드의 TAG_BAIL → 인터프리터로 안전 착지. (`COMPILE_DESIGN.md` §3, §5.2)
- 성능 주장은 이 범위에서만 한다: int/float/bool 산술·비교·제어흐름·직접 호출이 unboxed 네이티브.

## F-006. `/`는 항상 float를, `//`는 항상 int를, `%`는 int%int만 int를 반환한다

- 사실: `int_int_binop` (obj.rs:1635): `Div` → `float(a as f64 / b as f64)`, `IDiv` → `int(a.wrapping_div(b))`, `Mod` → int. Float 배치 경로도 `IDiv` → `cast_long`, `Mod` → `float`.
- 결정: typeck의 연산 규칙을 여기에 맞춘다. `int / int → float`은 네이티브에서 `fdiv`로 내릴 수 있는 신호가 아니라 **승격**이다 (피연산자 int→f64 후 fdiv). TIR 코드젠은 피연산자 종류에 따라 fcvt 삽입.

## F-007. int `//`/`%`는 truncated(C 의미), 0 나누기는 panic 객체

- 사실: `wrapping_div`/`wrapping_rem`은 truncation, div-zero → 인터프리터의 `System.DivideByZeroException`. `INT_MIN / -1`은 `wrapping_div`가 INT_MIN으로 순환.
- 결정: TIR 코드젠에서 `//`/`%`는 (1) 상수 0 → bail(인터프리터가 정확한 에러 발생), (2) `b == -1` → wrapping 규칙으로 특별 처리, (3) 그 외 `sdiv`/`srem` 전 div-0 분기에서 bail. bail이면 인터프리터가 그대로 재실행하여 정확한 에러를 낸다.

## F-008. `iter.range(a, b)` 루프는 jit_bc의 핵심 지원 패턴이다

- 사실: jit_bc는 `iter.range`를 특수 intrinsic으로 인식해 Int 루프를 네이티브로 내린다. 범위 상한은 exclusive.
- 결정: TIR core도 `for i in iter.range(lo, hi[, step])`만 for 네이티브로 인정한다. 일반 이터레이터(list 순회 등)는 core에서 제외. range 바운드는 typeck에서 int를 요구(인터프리터의 `range_float.un`/`range_str.un` negative 테스트와 동일한 엄격성).

## F-009. `un native` 프로그램도 기존 모듈 시스템/네이티브 Init을 그대로 필요로 한다

- 사실: `un run`은 `native::init(cwd)` 후 `compile_source` → `VM::new`. 모듈 import는 VM이 실행 중 `module::load`로 처리.
- 결정: `un native`도 동일하게 `native::init` → 프로그램을 평소처럼 컴파일(unbc) → **typeck/TIR 통과한 core proto만** `jit_cache`에 TIR 네이티브 엔트리로 등록 → VM 실행. 즉 모듈 로딩/초기화/네이티브 함수는 인터프리터와 100% 동일 경로.

## F-010. jit_fast::try_call_jit는 Float/Bool 인자를 전달할 수 없다

- 사실: `try_call_jit`는 `Payload::Int`/`Payload::List`만 받아들이고 나머지는 decline → 인터프리터. Float/Bool 인자는 항상 인터프리터로 빠진다.
- 결정: Compile Mode에서 TIR 엔트리를 위해 일반화한다: `JitEntry`에 `arg_kinds: Vec<ArgKind>` 추가. `Unbc`(기존 시맨틱스) / `Int` / `Float` / `Bool`. 기대 타입과 payload가 어긋나면 decline(인터프리터). Float 기대 & Int 인자 → `f64` 비트로 승격해 전달. `call_proto`(Op::Call 경유)와 `fn_call`(host 경유) 양쪽에서 동작. VM에는 숨겨진 분기 외 변경 없음.
- 위험: `JitEntry` 확장은 jit_bc 엔트리 생성 코드(`register_jit_entry` 호출부 2곳)도 함께 수정해야 함.

## F-011. class 메서드는 `vm/mod.rs call_proto` 경로로는 jit에 못 들어간다 (원래 동작)

- 사실: `call_proto`는 `bound_receiver=None`을 넘기고, `try_call_jit`는 class method(`ext_type.is_some()`)에서 None receiver면 decline. 즉 Un 바이트코드에서 `obj.method()`는 항상 인터프리터 실행. 반면 `fn_call`(host 경유)은 receiver를 넘긴다.
- 결정: Compile Mode에서도 이 동작을 바꾸지 않는다. core에서 class/메서드는 어차피 네이티브 제외.

## F-012. Proto에는 span이 없다 — TIR↔proto 매칭 수단 필요

- 사실: `Proto`는 `source_map`만 가지고 proto 자체의 함수 span은 없다.
- 결정: `Proto.fn_span: Option<(usize, usize)>`를 추가한다(unbc 직렬화 제외 — `read_proto`는 None으로 초기화). `begin_proto`의 호출부에서 span을 넘긴다. core 함수 판정 후 이 span으로 proto를 찾는다.

## F-013. `try_call_jit`의 `ID_BASE` IntVec 충돌 검사는 list 인자 전용이다

- 사실: `Payload::Int(v >= id_base())`면 decline. 이 규칙은 list 핸들 오인 방지용. TIR core는 list 인자를 받지 않으므로 영향 없음. jit_bc 경로는 그대로 유지한다 (F-010의 `Unbc` kind).

## F-014. exhaustive checking을 구현하지 않는 이유

- 사실: Union의 구체 분해·`is` narrowing·match 패턴의 도달 가능 분석이 모두 필요하다. v1에서 잘못 구현하면 거짓 양성으로 정상 프로그램을 거부해 검증 가치를 해친다.
- 결정: v1은 match arm 타입을 "union 후보 → 그 타입으로 좁혀지는" 정도의 좁히기만 하고(실제: arm body는 scrutinee 타입 그대로 + 패턴 바인딩은 해당 후보 타입), exhaustiveness는 검사하지 않는다. v2 항목으로 명문화. (`COMPILE_DESIGN.md` §4.6)

## F-015. `/`/`pow` 결과 타입과 인터프리터의 err 객체

- 사실: `int ** int` (negative exponent), `int pow overflow` 등은 err **객체**로 반환되어 호출자가 err 처리하면 예외가 된다. 즉 타입은 int지만 런타임 예외 경로가 있다.
- 결정: typeck는 `int ** int → int`로 보고, core 네이티브에서는 `**` 전체를 core에서 제외(정확한 오버플로/음수 지수 에러를 인터프리터가 내도록 bail-by-exclusion). `+ - * // %`와 비교만 core에 포함한다.

## F-016. 검증기가 "core가 아닌데 core로 판정"하는 경우를 반드시 막아야 한다

- 사실: TIR→Cranelift는 FFI로 VM에 등록된다. core로 잘못 내리면 i64 비트를 잘못 해석해 엉뚱한 값이 나온다.
- 결정: TIR 검증기가 **core 조건을 독립적으로 재확인**한다(판정 로직과 검증 로직을 의도적으로 이중화). 판정과 검증이 같은 결론을 내지 않으면 내부 오류로 중단한다(silent wrong code 금지).

## F-017. Pass-1 선언 수집이 함수 본문의 지역 변수를 모듈 전역으로 등록했다

- 사실: `collect_stmt`가 `NodeKind::Function | Extension`에서 본문으로
  `collect_block(body)`를 호출했고, 그 안의 모든 `Assign`이
  `collect_module_var`로 `self.globals`에 `Ty::Unknown`으로 등록됐다.
  그 결과 stage-3(모듈 본체 검사)의 globals seeding이 함수 지역 변수까지
  전역으로 보급했고, `bind_name`은 이들을 재대입(outer scope)으로 처리하여
  실제 타입 추론(`x = 0` → int)이 전혀 일어나지 않았다. `locals`가 비어
  TIR의 `collect_locals`/`body_is_core`가 `sum_to`/`fact` 등 단순 함수마저
  전부 거부했다. 증거: `TIRDBG globals after stage2: [x, y, ...]`.
- 결정: Pass 1은 함수 본문을 **선언만** 순회한다 (`collect_nested_block`/
  `collect_nested_stmt`). 본문 안의 `Assign`/`Typed`는 지역 바인딩이므로
  모듈 전역으로 등록하지 않는다. 함수/람다/클래스/enum/use는 여전히 순회한다.
- 파급: `un native`/`un check`에서만 관찰되는 버그. 인터프리터 경로는 무관.

## F-018. TIR 검증기의 `locals` 시드 오류 — 본문 지역 변수 이중 계산

- 사실: `validate()`가 `f.locals` 전체(params + body locals)를 `locals`에
  시드한 뒤 `collect_decls`가 각 `Let`마다 placeholder를 하나씩 더 push하여
  "local slot 1 does not follow the declaration order (expected 3)" 오류가
  났다. 또한 `ForRange`의 암묵적 루프 변수(int)를 replay가 인식하지 못해
  "range loop variable must be an int" 오류가 났다.
- 결정: `locals`는 params만 시드하고, `collect_decls`/`validate_stmt` 모두
  `ForRange.var`을 암묵적 int `Let`으로 인식한다. `Let`은 replay 중에 실제
  타입으로 덮어쓴다 (placeholder는 타입 결정에 사용되지 않는다).
- `collect_decls`는 `Then/Else`와 loop body를 같은 순서로 replay하므로,
  양쪽 arm이 각각 선언하는 지역 변수의 slot 할당도 선언 순서와 일치한다.

## F-019. `for i in a..b`의 `..`는 Binary(DotDot)이고, num+Any는 Any로 전파돼야 한다

- 사실: `un ast`/`un desugar`에서 `for i in 0..n`의 이터레이터는
  `iter.range(a, b)` Call이 아니라 `Binary(DotDot)`으로 나타난다.
  `check_iter_elem`은 `iter.range(...)` 패턴만 인식하여 `i`의 타입이
  `Any`로 남았고, `check_binop(+, int, Any)`는 `is_numeric()`(Any 포함) 경로를
  타 "int op int → int; mixed → float" 으로 `float` 승격시켜
  `total += i`가
  "cannot assign value of type 'float' to 'total' of type 'int'" 오류를 냈다.
- 결정: (1) `check_iter_elem`에 `Binary(DotDot)` 분기를 추가해 두 경계가
  integer-ish면 `Int`를 반환한다. (2) `check_binop(+,-,*)`는 한쪽이 `Any`면
  `Any`를 전파한다 (`int op int → int`, numeric 나머지 → float, Any → Any).
- 부수: `/`/`//`/`%`는 이미 `Any`를 명시적으로 처리하므로 변경하지 않았다.

## F-020. Cranelift SSA construction은 seal 시점에 알려진 선행 블록만 본다

- 사실: cranelift-frontend는 `seal_block` 시점에 알려진 모든 선행 블록을
  기준으로 SSA를 구성한다. `While`/`ForRange`의 헤더를 `brif` 방출 직후에
  seal하면 back-edge(본문 말단 jump / skip_blk jump)가 반영되지 않아 루프
  변수의 SSA phi가 생성되지 않는다. 결과적으로 native code가 `count != 3`을
  영원히 만족하여 **무한 루프**가 되었다 (countdown(5) hang).
- 결정: 헤더는 back-edge 방출(= body_blk sealed → skip_blk sealed) 이후에
  seal한다. `body_blk`/`skip_blk`는 선행 블록이 확정된 시점(본문 방출 후,
  skip_blk는 `jump(header)` 방출 후)에 seal한다.

## F-021. 빈 블록은 seal해도 finalize에서 삭제되므로 dangling reference가 된다

- 사실: Cranelift의 finalize는 instruction이 없는 sealed 블록을 제거한다
  (예: `If`의 merge_blk). 분기에서 그 블록을 참조하는 `brif`/`jump`가 있으면
  `invalid block reference blockN`으로 define_function이 실패한다.
- 결정: (1) `If` arm에서 terminator가 없던 `else` arm은 항상 `jump merge_blk`를
  방출하고, 양쪽 arm이 모두 종료한 merge는 `jump bail_blk`(unreachable)를 채운다.
  (2) `emit_exit_depth`의 do/skip 블록처럼 guard 실패 경로도 terminator로
  채워진다. 어느 블록이든 선행이 있으면 반드시 채워지거나, 없으면 적어도
  terminator는 갖는다.
- 참고: `emit_exit_depth`는 블록 2개(do/skip)를 더 생성하므로 블록 번호와
  생성 순서가 출력 IR과 어긋나 보일 수 있지만, Cranelift가 번호를 재부여하지
  않으므로 무해하다.

## F-022. `cargo test --release`의 기존 테스트가 가끔 깨진다 (pre-existing flake)

- 사실: `vm::tests::a_cleared_jit_program_runs_identically`가
  "the JIT should have compiled at least one proto" 단언으로 1회 실패했으나,
  재실행/전체 스위트 재실행에서 모두 green이다. 본 변경과 무관하다 —
  기존 테스트에서 `a_full_clear_lets_jit_run_again`이 `jit_cache::clear()`를
  호출해 같은 프로세스의 다른 테스트가 방금 등록한 entry를 지울 수 있는
  순서 경쟁이 있다 (CI에서도 scheduling에 따라 재현 가능).
- 결정: 테스트를 수정하지 않는다 (AGENTS.md: 1538 passed/0 crashed 기준이며,
  이 루트는 cargo test 결과 213 passed). 다음 유지보수 세션에서
  `--test-threads=1` 또는 테스트별 program_id 격리로 고정하는 것을 권장한다.
