# BYTECODE.md — Un 바이트코드 설계

`unbc` = Un bytecode. 이 문서는 opcode 집합, 인코딩, 함수 chunk, constant pool, local slot,
jump offset, source mapping, stack effect, 검증 규칙, `.unbc` 파일 레이아웃을 정의한다.

원칙:
1. **의미론은 `ANALYSIS.md`의 Reference 동작을 1:1로 재현**한다. opcode는 그 동작을 표현하는 최소 단위다.
2. **`.unbc` 직렬화를 염두에 두고 설계**한다. 재구축 없이 확장할 수 있도록 모든 테이블은
   length-prefixed, little-endian, version 헤더를 가진다.
3. jump offset은 **relative(i32, next-ip 기준)**로 둔다. 패치가 단순하고, 함수 chunk가
   이관되는 `.unbc` layout과 잘 맞는다.

## 1. Instruction 형식

```text
Instr { op: u8, operand: i32, flags: u16 }
```

- `flags`는 opcode별 보조 비트(optional-chain, localOnly, has-value 등)에 쓴다.
- 인코딩: `op(1) | flags(2) | operand(4)`, little-endian, 7 bytes/instruction.
  VM은 `code: Vec<Instr>`를 직접 쓰고, `.unbc` writer가 bytes로 직렬화한다.
- `operand`의 해석은 opcode에 따라: constant index, name index, proto index, jump delta,
  count, entry index, slot index.

## 2. 값 로드/저장

| opcode | operand | stack | 설명 |
|---|---|---|---|
| `LOAD_NONE` | — | +1 | `Obj.None` |
| `LOAD_NULL` | — | +1 | `Obj.Null` |
| `LOAD_TRUE` | — | +1 | `Bool(true)` |
| `LOAD_FALSE` | — | +1 | `Bool(false)` |
| `LOAD_CONST` | const idx | +1 | int/float/str/bool 상수 (intern 규칙 적용) |
| `LOAD_NAME` | name idx | +1 | compile-time 미해석 이름: global registry → class registry(TObj) → `undefined variable '{name}'` |
| `LOAD_LOCAL` | slot | +1 | current scope slot |
| `LOAD_UPV` | depth<<16 \| slot | +1 | parent chain `depth`번 후 slot |
| `STORE_NAME` | name idx | -1 | 런타임 라우팅 (`BindName`): FieldRef면 `.Value`, `localOnly || is_function_scope`면 `SetLocalOrDeclare`, 아니면 `Set`(parent 전파) |
| `STORE_LOCAL` | slot | -1 | current scope slot 직접 (`STORE_NAME`의 specialized 형태) |
| `STORE_UPV` | depth<<16 \| slot | -1 | parent chain slot 직접 |
| `DUP` | — | +1 | 스택 top 복사 |
| `POP` | — | -1 | |

flag `LOCAL_ONLY`는 `STORE_NAME`의 `localOnly: true` 변형(`for` 루프 변수)에 쓴다.

## 3. 객체 구성

| opcode | operand | stack | 설명 |
|---|---|---|---|
| `BUILD_LIST` | n | -n, +1 | `List` |
| `BUILD_DICT` | n | -2n, +1 | `Dict`(key,value n 쌍) |
| `BUILD_SET` | n | -n, +1 | `Set` |
| `BUILD_TUPLE` | n | -n, +1 | `Tup`(이름 없음) |
| `BUILD_TUPLE_NAMED` | names idx, n | -n, +1 | tuple items의 `name = value` Pair 보존(names table) |
| `FSTRING` | n | -n, +1 | n 조각 concat, 비-string 조각은 `ToStr()`(실패 시 `f-string expression must be a string`) |
| `TAG` | n | -(n+1), +1 | tag + strings/values → `tagged template` call |

## 4. 연산자

C#의 dunder dispatch + super fallback + fallback message를 그대로 재현한다.

| opcode | operand | stack | 설명 |
|---|---|---|---|
| `ADD` `SUB` `MUL` `DIV` `IDIV` `MOD` `POW` | — | -2, +1 | dunder → super → `unsupported operand type(s) for {sym}: '{L}' and '{R}'` |
| `RANGE` | — | -2, +1 | `a..b` → `__range__` → super → int/float bound `Iter.Range` → Err |
| `BAND` `BOR` `BXor` `LSH` `RSH` | — | -2, +1 | |
| `EQ` `NE` `LT` `GT` `LE` `GE` | — | -2, +1 | `Gt`/`LE`/`GE`는 `Lt`+`Eq` 조합 fallthrough |
| `IS` | flag | -2, +1 | TObj가 왼쪽/오른쪽에 있으면 방향 반전 (`right.Is(left)`) |
| `IS_NOT` | flag | -2, +1 | `NegateIs` (Bool이 아니면 negate하지 않음) |
| `IN` | — | -2, +1 | `right.In(left)` |
| `XOR` | — | -2, +1 | `ToBool`이 Bool 아님 → `left/right operand must be a boolean` |
| `AND` | jump delta | -1 | 좌변 truthy: 우변 실행, **좌변 truthy일 때 좌변값 아님 → 우변 값**; falsy면 좌변 값 |
| `OR` | jump delta | -1 | truthy면 좌변 값, falsy면 우변 실행 |
| `COALESCE` | jump delta | -1 | `IsNone()`면 우변, 아니면 좌변 |
| `NOT` `POS` `NEG` `BNOT` | — | -1, +1 | dunder dispatch |
| `SPREAD` | — | -1, +1 | `__spread__` → `Err("unsupported operand type(s) for *: '{T}'")` |
| `TO_BOOL` | — | -1, +1 | `__bool__` + Unwrap |

`AND`/`OR`은 short-circuit이 값 반환 semantics와 결합하므로 **jump 기반**로만 표현한다.
`JMP_IF_FALSE_KEEP`(truthy면 pop하지 않고 jump) 같은 변형은 flag로 통합한다.

## 5. 속성/인덱스/슬라이스

| opcode | operand | stack | 설명 |
|---|---|---|---|
| `GET_ATTR` | name idx | -1, +1 | `?.` flag 설정 시 target None && chain → None. TObj면 `Global.GetClass(t).GetAttr` |
| `SET_ATTR` | name idx | -2, +1 | `SetAttrFrom` 순서 재현, 값 반환 |
| `GET_ITEM` | — | -2, +1 | `__getitem__` → super → Err |
| `SET_ITEM` | — | -3, +1 | `SetItem`, 값 반환 |
| `SLICE` | flag(bit0=start,bit1=end,bit2=step 존재) | -1..-4, +1 | 생략 bound는 `None` 인자 |
| `SET_SLICE` | flag | -1..-5, +1 | list 전용, `step != 1` 등 6개 메시지 재현 |
| `JMP_NONE` | jump delta | | target None && optional-chain면 jump (None push) |

## 6. 제어 흐름

| opcode | operand | stack | 설명 |
|---|---|---|---|
| `JMP` | delta | | |
| `JMP_FALSE` | delta | -1 | `ToBool` |
| `JMP_TRUE` | delta | -1 | |
| `WHILE_NOT` | delta | -1 | `while` 조건은 strict bool (`As<Bool>`), truthy 아님 |
| `NEW_SCOPE` | kind flags | | block scope push (`for`/`match`/`catch`) |
| `POP_SCOPE` | — | | `context.Scope = outerScope` (for의 finally 복원) |
| `ENTER_SCOPE` | — | | `context.Scope = scope` (match/catch 전환) |
| `PREP_ITER` | text idx | -1, +1 | `.Iter()` 실패 → `'{원문}' is not iterable` |
| `FOR_NEXT` | delta | +1 | exhausted → jump; 아니면 next value push |
| `BREAK` | — | | loop frame search. 최상위까지 없으면 `'break' outside loop` |
| `SKIP` | — | | 같은 방식으로 `'skip' outside loop` |
| `RETURN` | — | -1 | 함수 경계까지 unwind, defers/usings flush |

`return`는 함수 경계(`LFn.Call`)에서 `Flow::Return(value)`로 처리하고, `break`/`skip`은
루프 경계에서 잡힌다. `try`의 `finally`는 unwind 중에 실행된다(C#의 `finally` 순서 유지).

## 7. 함수

| opcode | operand | stack | 설명 |
|---|---|---|---|
| `MAKE_FUNC` | proto idx | +1 | 현재 `Context`를 캡처해 `LClosure` 생성. flag: `BIND`(scope에 `name` 바인딩), `EXT`(extension → `Global.TrySetOriginalClass`), `PROP` |
| `ARG_BEGIN` | — | | arg accumulator 초기화 |
| `ARG_POS` | — | -1 | value pop, positional push |
| `ARG_KW` | name idx | -1 | value pop, keyword push |
| `ARG_SPREAD` | — | -1 | `Unpack`, `Err`면 throw, positional push |
| `ARG_KWSPREAD` | — | -1 | `Dict` 아니면 `** argument must be dict`, pair push |
| `CALL` | n | -(n+1), +1 | callable pop, accumulator n개 → `Tup(values,names)` → `PushFrame(FrameInfo(call node 원문))` → `Call` → pop frame → `Unwrap` |
| `UNWRAP` | — | | top이 `Err`면 `Error(msg, node)` throw |
| `PUSH_TRY_PROP` | entry idx | | `?` 연산용: 실행 중 `Error`/`Err` → `Frames.Count>0`이면 `PropagateFlow`, 최상위면 `Error` |

`CALL`의 frame 이름은 call node의 **원문 텍스트**(source mapping에서 span으로 복원)다.

## 8. 클래스/enum

| opcode | operand | stack | 설명 |
|---|---|---|---|
| `MAKE_CLASS` | name idx, flag(has params, has bases) | +1 | `Obj(UnType.Create(name))` 또는 `Stru` + fields(names idx) |
| `LOAD_BASE_CLASS` | name idx | +1 | 미등록 base → `unknown base class '{name}'` |
| `SET_BASES` | n | -(n+1) | 첫 base는 `Super`+`Parents[0]`+`Types` 합, 이후 base는 `Parents`+`Types` |
| `ADD_MEMBER` | name idx | -2 | `Members[name] = value` |
| `ADD_METHOD` | name idx, proto idx, flag | -1 | accessor conflict 검사 포함 |
| `MAKE_PROPERTY` | name idx | -3 | getter/setter proto + default, `"$"+name` backing, `CreateAccessor` 이름 `get {n}`/`set {n}` |
| `REGISTER_CLASS` | name idx | -1 | `Global.SetClass(name, type)` |
| `MAKE_ENUM` | name idx, members names idx, flag(값 목록 idx) | +1 | `Enu` 생성 + 등록 |

`invalid initialize`, `duplicate getter/setter for property`,
`property '{n}' conflicts with a method`, `invalid enum`, `invalid pattern` 메시지 순서를 유지한다.

## 9. 예외/defer/using

`Proto`에 **exception table**를 둔다.

```text
Entry { kind: Catch | Finally | Propagate, start: usize, end: usize, handler: usize, var_slot: Option<usize> }
```

| opcode | operand | 설명 |
|---|---|---|
| `PUSH_TRY` | entry idx | `start..end` 구간에 handler 등록 |
| `POP_TRY` | — | 구간 정상 종료 |
| `RAISE` | — | `Err`를 `Error`로 승격하고 throw (catch search: `catch(Error)`만, `Propagate`는 통과) |
| `PUSH_DEFER` | proto idx | defer body Proto + 현재 scope를 defers stack에 push |
| `PUSH_USING` | — | `__entry__` Unwrap 후 usings stack push |
| `FLUSH_DEFERS` | — | LIFO 실행 |
| `FLUSH_USINGS` | — | LIFO `__exit__` |

catch는 **catch마다 새 scope**를 만들고 `var_slot`에 `Err(e.Message, e.Header)`를 바인딩한다.
catch body가 정상 종료하면 `caught=true`, 미caught면 원시 오류 재throw,
`finally`가 있으면 finally 실행 후 `hasError && pendingError`면 `throw pendingError`.

## 10. 함수 chunk (Proto)

```text
Proto {
  name: NameId,
  arity: usize,
  locals: Vec<NameId>,           // slot index = locals index
  params: Vec<Arg>,              // Fn.GetArgs와 같은 해석 결과 (essential/optional/positional/keyword/default)
  code: Vec<Instr>,
  consts: Vec<Constant>,         // 모듈 constant pool 참조 index
  names: Vec<NameId>,            // 모듈 name table 참조 index
  protos: Vec<ProtoId>,          // 중첩 함수
  call_names: Vec<Vec<Option<NameId>>>,  // call site의 인자 이름 목록
  source_map: Vec<(u32, u32)>,   // instruction별 (start, length)
  exc: Vec<ExcEntry>,
  flags: u16,                    // is_function_scope, has_self, ...
}
```

- `locals`는 compile-time slot 할당과 런타임 Scope slot 순서가 같다.
- `params`는 정의 시점에 평가되는 default 값(constant pool index)을 가진다.
- nested `Proto`는 parent Proto의 `protos`에 저장하고, `.unbc`는 평탄화된 proto table로 저장한다.

## 11. Constant pool / Name table

```text
Constant { kind: u8, ... }   // int(i64) | float(f64) | str(String) | bool | none | null
NameTable: Vec<String>        // 식별자/타입 이름/property backing key
```

- `Constant`와 `Name`은 모듈 전역이며, `.unbc`에서 **dedup**한다. `UnType`의 intern 규칙과
  name table dedup이 일치해야 `is` semantics가 유지된다.
- 상수 문자열은 `Str` intern pool 규칙(§7.4)을 **실행 시점**에 적용한다(컴파일 시 아님).

## 12. Source mapping

- instruction별 `(start, length)`는 오류 위치(`Error(start,length,source)`)와 trace(`GetLineText`)에 쓰인다.
- call frame의 이름은 call node의 원문 span으로 복원한다.
- `.unbc`는 source **원문**을 넣지 않고 `(source path, mapping table)`만 넣는다.
  mapping이 없으면 trace가 degraded되므로, `.unbc` validation은 mapping과 code 길이가
  같은 것을 요구한다.

## 13. `.unbc` 파일 레이아웃

```text
magic   "UNB1"          (4 bytes)
version u16             (minor/major)
flags   u16             (source embed 여부, sandbox hints)
source path: length-prefixed UTF-8
name table: count u32 + length-prefixed UTF-8 strings
constant pool: count u32 + entries
proto table: count u32 + protos
  proto: { name u32, arity u32, flags u16, locals: count+u32s, params: count+entries,
           code: count+instrs(7 bytes each), consts: count+u32s, names: count+u32s,
           protos: count+u32s, call_names: count+(count+u32s), source_map: count+(u32,u32),
           exc: count+(kind u8, start u32, end u32, handler u32, var_slot u32=NONE) }
```

- 모든 다바이트 필드는 little-endian. 모든 테이블은 count-prefixed.
- unknown opcode/version/flag는 **validation 실패**로 처리한다(침묵하지 않는다).
- `flags` 비트는 하위 버전 호환성을 위해 추가 비트만 허용하고, read 시 unknown bit는 reject한다.

## 14. Validation 규칙 (load 시)

1. magic/version 일치.
2. opcode가 정의된 집합에 속하고 operand 타입이 opcode table과 일치.
3. jump delta가 같은 proto의 `[0, code.len()]` 범위에 있고, jump 대상이 instruction 경계다.
4. constant/name/proto index가 범위 내다.
5. `source_map.len() == code.len()`, `call_names.len() == call site count`.
6. exception table의 `start < end`, `handler`가 instruction 경계, `kind`가 유효.
7. stack effect simulation: proto의 code를 선형 simulate하여 stack depth가 음수가 되지 않고,
   jump 타겟에서 depth가 예측 consistent해야 한다(함수 boundary에서 depth 0).

## 15. Stack effect 표 (주요 군)

- expression producing: +1
- statement consuming: -1 (식 문장은 마지막에 `POP`을 emit한다)
- `CALL n`: -(n+1) +1
- `MAKE_FUNC`: +1
- `NEW_SCOPE`/`POP_SCOPE`: 0
- control jump: 조건 pop 포함 표기대로

`POP`은 expression statement의 value를 버리는 전용 opcode로, C#의
`EvalBlock`이 각 문장 결과를 last result로 유지하는 것과 달리 **블록의 마지막 문장 값은
block의 값**이므로(`EvalBlock`의 `result`), 마지막 문장에는 `POP`을 emit하지 않는다.
`for`/`while` body는 값이 버려지므로 body 문장마다 `POP`을 emit한다.

## 16. 확장을 위한 자리

- `flags`의 상위 비트, opcode table의 예약 영역(`0x60..0x7F`), `.unbc` footer 섹션은
  추가 필드(dis, profile, coverage, gc metadata)용 예약으로 둔다.
- serialization을 추가할 때 **proto layout에 필드를 append**하고 version을 bump한다.
  재구축이 필요 없는 이유는 모든 테이블이 count-prefixed이고 instruction이 fixed-size이기 때문이다.
