# BYTECODE.md — Un 바이트코드 / `.unbc` 명세

이 문서는 **현재 Rust 구현**(`rust/src/bytecode/`)을 기준으로 한 opcode 집합, 인코딩,
Proto 구조, `.unbc` 직렬화 형식, 검증 규칙을 정의한다.

>历史 참고: 초기 문서는 C# 참조 구현의 opcode 이름(`LOAD_NONE`, `AND`, `WHILE_NOT`,
> `JMP_FALSE` 등)을 사용했으나 현재 Rust 바이트코드는 다른 이름/인코딩을 쓴다.
> 2026-11-20 감사(AUDIT-003)에서 문서를 현재 구현 기준으로 재작성했다.
> C# 구현은 git 이력에만 남아 있고 현재 산출물에는 포함되지 않는다.

원칙:
1. opcode는 `rust/src/bytecode/instr.rs::Op`에 `#[repr(u8)]`으로 고정되어 있다.
   **opcode 값을 바꾸면 기존 `.unbc`가 깨진다.**
2. 모든 테이블은 length-prefixed, little-endian이며 version 헤더를 가진다.
3. jump operand는 현재 instruction 다음 IP 기준 **상대 delta(i32)** 다.

## 1. Instruction 형식

```text
Instr { op: u8, flags: u16, operand: i32 }
인코딩: op(1) | flags(2) | operand(4)  = 7 bytes, little-endian
```

VM은 `code: Vec<Instr>`를 직접 사용하고, `.unbc` writer가 7-byte 레코드로 직렬화한다.
`operand` 해석은 opcode에 따라 constant/name/proto index, jump delta, count, slot index 등이다.

## 2. Opcode table (권위 있는 목록)

출처: `rust/src/bytecode/instr.rs::Op`. 이름과 값은 코드와 1:1이다.

| 값 | 이름 | 값 | 이름 |
|---:|---|---:|---|
| 0x01 | `LoadNone` | 0x33 | `Bnot` |
| 0x02 | `LoadNull` | 0x34 | `Spread` |
| 0x03 | `LoadTrue` | 0x35 | `ToBool` |
| 0x04 | `LoadFalse` | 0x36 | `JmpIf` |
| 0x05 | `LoadConst` | 0x37 | `Jmp` |
| 0x06 | `LoadName` | 0x38 | `PushTryProp` |
| 0x07 | `LoadLocal` | 0x39 | `Propagate` |
| 0x08 | `LoadUpv` | 0x3A | `GetAttr` |
| 0x09 | `StoreName` | 0x3B | `SetAttr` |
| 0x0A | `StoreLocal` | 0x3C | `GetItem` |
| 0x0B | `StoreUpv` | 0x3D | `SetItem` |
| 0x0C | `Dup` | 0x3E | `Slice` |
| 0x0D | `Dup2` | 0x3F | `SetSlice` |
| 0x0E | `Swap` | 0x40 | `ArgBegin` |
| 0x0F | `Rot` | 0x41 | `ArgPos` |
| 0x10 | `Pop` | 0x42 | `ArgKw` |
| 0x11 | `PopN` | 0x43 | `ArgSpread` |
| 0x12 | `BuildList` | 0x44 | `ArgKwSpread` |
| 0x13 | `BuildDict` | 0x45 | `Call` |
| 0x14 | `BuildSet` | 0x46 | `Unwrap` |
| 0x15 | `BuildTuple` | 0x47 | `NewScope` |
| 0x16 | `BuildTupleNamed` | 0x48 | `PopScope` |
| 0x17 | `FString` | 0x49 | `PrepIter` |
| 0x18 | `Tag` | 0x4A | `ForNext` |
| 0x19 | `Add` | 0x4B | `Break` |
| 0x1A | `Sub` | 0x4C | `Skip` |
| 0x1B | `Mul` | 0x4D | `Return` |
| 0x1C | `Div` | 0x4E | `PushTry` |
| 0x1D | `Idiv` | 0x4F | `PopTry` |
| 0x1E | `Mod` | 0x50 | `Raise` |
| 0x1F | `Pow` | 0x51 | `PushDefer` |
| 0x20 | `Band` | 0x52 | `PushUsing` |
| 0x21 | `Bor` | 0x53 | `FlushDefers` |
| 0x22 | `Bxor` | 0x54 | `FlushUsings` |
| 0x23 | `Lsh` | 0x55 | `MakeClass` |
| 0x24 | `Rsh` | 0x56 | `LoadBaseClass` |
| 0x25 | `Range` | 0x57 | `SetBases` |
| 0x26 | `Eq` | 0x58 | `AddMember` |
| 0x27 | `Ne` | 0x59 | `AddMethod` |
| 0x28 | `Lt` | 0x5A | `MakeProperty` |
| 0x29 | `Gt` | 0x5B | `RegisterClass` |
| 0x2A | `Le` | 0x5C | `MakeEnum` |
| 0x2B | `Ge` | 0x5D | `MakeFunc` |
| 0x2C | `Is` | 0x5E | `Use` |
| 0x2D | `IsNot` | 0x5F | `Unsupported` |
| 0x2E | `In` | 0x60 | `Unpack` |
| 0x2F | `Xor` | 0x61 | `ToStr` |
| 0x30 | `Not` | 0x62 | `MatchPattern` |
| 0x31 | `Pos` | 0x63 | `MakeFuture` |
| 0x32 | `Neg` | 0x64 | `Await` |
| — | — | 0x7F | `FusedIntMulAdd` |

`0x00`은 사용하지 않는다. `0x65..=0x7E`는 **예약 영역**이며
`instr.rs`의 `reserved_area_is_free` 테스트가 비어 있음을 고정한다.
`0x7F`(`FusedIntMulAdd`)는 JIT 누적기용으로 실제 사용 중이므로 예약 영역에 포함되지 않는다.

### 2.1 opcode별 요약

- **로드/저장 (0x01–0x0B)**: 리터럴·상수·이름·로컬 슬롯·업값. `LoadUpv`/`StoreUpv`의
  operand는 `depth << 16 | slot`이다.
- **스택 (0x0C–0x11)**: `Dup`, `Dup2`, `Swap`, `Rot`, `Pop`, `PopN`.
- **집합 생성 (0x12–0x18)**: list/dict/set/tuple/named-tuple, `FString`, `Tag`.
- **산술 (0x19–0x25)**: `Add`…`Pow`, 비트연산 `Band/Bor/Bxor/Lsh/Rsh`, `Range`.
- **비교 (0x26–0x2F)**: `Eq/Ne/Lt/Gt/Le/Ge/Is/IsNot/In/Xor`.
- **단항 (0x30–0x35)**: `Not/Pos/Neg/Bnot/Spread/ToBool`.
- **제어 (0x36–0x37, 0x38–0x39)**: `JmpIf`는 flag에 따라 조건부 분기,
  `Jmp`는 무조건 분기, `PushTryProp`/`Propagate`는 `?` 연산용.
- **속성/인덱스 (0x3A–0x3F)**: `GetAttr/SetAttr/GetItem/SetItem/Slice/SetSlice`.
- **호출 (0x40–0x46)**: `ArgBegin`으로 인자 accumulator를 초기화한 뒤
  `ArgPos/ArgKw/ArgSpread/ArgKwSpread`로 기록하고 `Call n`이 실행한다.
- **스코프/반복 (0x47–0x4C)**: `NewScope/PopScope/PrepIter/ForNext/Break/Skip`.
- **반환 (0x4D)**: `Return`은 현재 proto만 종료한다(바깥 루프로 unwind하지 않음).
- **예외/정리 (0x4E–0x54)**: try/finally/propagate, defer, using.
- **클래스/enum (0x55–0x5C)**: 클래스 정의·상속·멤버/메서드/프로퍼티·등록, enum.
- **기타 (0x5D–0x64)**: `MakeFunc`, `Use`, `Unsupported`, `Unpack`, `ToStr`,
  `MatchPattern`, `MakeFuture`(go), `Await`(wait).

### 2.2 단축회로 / None-jump 인코딩

C# 참조 구현에 있던 `AND`/`OR`/`COALESCE`/`JMP_FALSE`/`JMP_TRUE`/`WHILE_NOT`/`JMP_NONE`
opcode는 **현재 Rust 바이트코드에 존재하지 않는다.** 대신 단일 `JmpIf`(0x36)에
아래 flag를 조합해 표현한다.

| flag | 값 | 의미 |
|---|---:|---|
| `FLAG_JUMP_TRUE` | `1 << 0` | 조건이 truthy일 때 분기 |
| `FLAG_JUMP_POP` | `1 << 1` | 분기 시 조건 값을 pop (stack effect -1) |
| `FLAG_STRICT` | `1 << 2` | strict bool 요구 |
| `FLAG_JUMP_NONE` | `1 << 3` | None 대상 optional-chain 분기 |
| `FLAG_OPT_CHAIN` | `1 << 4` | `?.` 연쇄 |
| `FLAG_LOCAL_ONLY` | `1 << 5` | `for` 루프 변수 등 local 전용 |
| `FLAG_ARG_CHECK` | `1 << 6` | 인자 검사 |
| `FLAG_HOST_ERROR` | `1 << 7` | host error 표식 |
| `FLAG_SCOPE_SET` | `1 << 8` | 스코프 설정 |

`slice` 계열은 같은 flag 비트를 재사용한다:
`FLAG_SLICE_START = 1<<0`, `FLAG_SLICE_END = 1<<1`, `FLAG_SLICE_STEP = 1<<2`.
`NewScope`는 `FLAG_SCOPE_FUNCTION = 1<<0`, `FLAG_SCOPE_LOOP = 1<<1`을 쓴다.
전체 목록은 `instr.rs`의 `FLAG_*` 상수와 `bytecode/mod.rs`의 재수출을 참고한다.

## 3. Proto (함수 chunk)

```text
Proto {
  name: NameId, arity: usize, flags: u16,
  locals: Vec<NameId>,          // slot index = locals index
  params: Vec<Param>,           // name/type_name/default/flags
  code: Vec<Instr>,
  consts: Vec<ConstId>, names: Vec<NameId>, protos: Vec<ProtoId>,
  call_names: Vec<Vec<Option<NameId>>>,
  source_map: Vec<(u32, u32)>,
  exc: Vec<ExcEntry>,           // kind(Catch|Finally|Propagate), start, end, handler, var_slot
  patterns: Vec<Pattern>,
  ext_type: Option<NameId>,     // 확장 메서드의 수신 타입
}
```

`patterns`의 항목은 `Wildcard`, `Bind(NameId)`, `Literal(ConstId)`,
`Tuple(Vec<Pattern>)`, `Variant(EnumNameId, VariantNameId, Box<Pattern>)` 5종이며
이름/상수 인덱스는 모듈 전역이다. `Variant`는 `Enum.variant(inner)` 패턴을 뜻하고
`.unbc` 태그 4로 직렬화된다(기존 0–3 태그는 그대로).
`ext_type`이 있는 proto는 클래스 메서드이며, JIT에서 receiver를 local slot 0에 바인딩한다.

## 4. Constant pool / Name table

```text
Constant = Int(i64) | Float(f64) | Str(String) | Bool(bool) | None | Null
NameTable: Vec<String>
```

모듈 전역이며 `.unbc`에서 dedup한다. `MAGIC = *b"UNB1"`, `VERSION: u16 = 1`.

## 5. Source mapping

instruction별 `(start, length)`로 오류 위치·trace를 복원한다.
`.unbc` flags bit 0이 source 원문 포함 여부이며, `un compile`은 항상 포함한다.
`source path`는 컴파일 시 전달된 전체 경로를 그대로 저장해(D-072) `.unbc` 재실행 시
모듈 해석이 direct run과 동일하게 동작하게 한다.

## 6. `.unbc` 파일 레이아웃

```text
magic   "UNB1"        (4 bytes)
version u16
flags   u16              (bit 0: source embed)
source path   : length-prefixed UTF-8
source        : length-prefixed UTF-8   (flags bit 0일 때만)
name table    : count u32 + length-prefixed UTF-8 strings
constant pool : count u32 + entries (kind u8 + payload)
message table : count u32 + length-prefixed UTF-8 strings
proto table   : count u32 + protos
  proto: name u32, arity u32, flags u16,
         locals: count+u32s,
         params: count+(name u32, type_name u32=NONE, flags u16, default u32=NONE),
         code: count+(op u8, flags u16, operand i32),
         consts/names/protos: 각 count+u32s,
         call_names: count+(count+u32s),
         source_map: count+(u32,u32),
         exc: count+(kind u8, start u32, end u32, handler u32, var_slot u32=NONE),
         patterns: count+entries,
         ext_type: u32=NONE
```

모든 다바이트 필드는 little-endian이고 모든 테이블은 count-prefixed다.

## 7. 검증 (load 시)

`bytecode/unbc.rs::Reader`(length/bounds 검사)와 `bytecode/mod.rs::Program::validate`가 담당한다.

1. magic/version 일치 (불일치 시 `not a .unbc file` / `unsupported .unbc version N`).
2. opcode가 정의된 집합에 속하고 operand 타입이 opcode와 일치.
3. jump delta가 같은 proto의 `[0, code.len()]` 범위이고 대상이 instruction 경계.
4. constant/name/proto/local/upvalue/예외 슬롯 index가 범위 내.
5. `source_map.len() == code.len()`, call site 수와 `call_names` 일치.
6. exception table의 `start < end`, handler가 instruction 경계, kind 유효.
7. 256-register 제한( locals + max stack depth + 1 ).
8. pattern 중첩 깊이 ≤ 64 (`MAX_PATTERN_DEPTH`).

손상된 입력은 panic/abort가 아니라 exit 1 + 명확한 오류 메시지로 거부된다.
`unbc-roundtrip.ps1 -IncludeNegative`가 Positive 48/48, compilable Negative 42/42을 검증한다.

## 8. JIT와의 관계

- `FusedIntMulAdd`(0x7F)는 Int-JIT가 누적기 패턴을 인식하기 위한 융합 opcode다.
- bytecode JIT 엔트리(`jit_bc.rs`)와 Compile Mode TIR 엔트리(`natcode.rs`)는
  `jit_cache.rs`에서 **별도 맵**(`JIT_FNS` / `TIR_FNS`)에 저장된다(2026-11-20 수정).
  두 종류가 같은 맵을 공유하면 인자 있는 proto의 2회차 호출이 인터프리터로 조용히
  퇴화한다(AUDIT-001).

## 9. 확장 예약 영역

`0x65..=0x7E`와 flags 상위 비트는 향후 확장용으로 비워 둔다.
새 serialization 필드는 proto layout에 append하고 version을 bump한다.
테이블이 count-prefixed이고 instruction이 fixed-size이므로 재구축 없이 확장 가능하다.
