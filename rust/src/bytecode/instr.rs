//! `Instr { op: u8, flags: u16, operand: i32 }` — the fixed-size instruction of BYTECODE.md §1.

/// opcode set. the numeric values are part of the `.unbc` contract, so every variant is
/// written with an explicit discriminant. `0x60..=0x7F` is the reserved extension area of
/// BYTECODE.md §16 and `0x00` is the unused sentinel, so no opcode lives in either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    // §2 value load/store
    LoadNone = 0x01,
    LoadNull = 0x02,
    LoadTrue = 0x03,
    LoadFalse = 0x04,
    LoadConst = 0x05,
    LoadName = 0x06,
    LoadLocal = 0x07,
    LoadUpv = 0x08,
    StoreName = 0x09,
    StoreLocal = 0x0A,
    StoreUpv = 0x0B,

    // stack shuffles (not in BYTECODE.md's table; they are the minimal units the
    // `Property`/`Index`/`Slice` assignment and tuple-pattern forms need)
    Dup = 0x0C,
    Dup2 = 0x0D,
    Swap = 0x0E,
    Rot = 0x0F,
    Pop = 0x10,
    PopN = 0x11,

    // §3 object builders
    BuildList = 0x12,
    BuildDict = 0x13,
    BuildSet = 0x14,
    BuildTuple = 0x15,
    BuildTupleNamed = 0x16,
    FString = 0x17,
    Tag = 0x18,

    // §4 operators
    Add = 0x19,
    Sub = 0x1A,
    Mul = 0x1B,
    Div = 0x1C,
    Idiv = 0x1D,
    Mod = 0x1E,
    Pow = 0x1F,
    Band = 0x20,
    Bor = 0x21,
    Bxor = 0x22,
    Lsh = 0x23,
    Rsh = 0x24,
    Range = 0x25,

    // §4 comparisons and membership
    Eq = 0x26,
    Ne = 0x27,
    Lt = 0x28,
    Gt = 0x29,
    Le = 0x2A,
    Ge = 0x2B,
    Is = 0x2C,
    IsNot = 0x2D,
    In = 0x2E,
    Xor = 0x2F,

    // §4 unary
    Not = 0x30,
    Pos = 0x31,
    Neg = 0x32,
    Bnot = 0x33,
    Spread = 0x34,
    ToBool = 0x35,

    // §6 jumps (`JMP_IF` carries the condition shape in flags)
    JmpIf = 0x36,
    Jmp = 0x37,
    PushTryProp = 0x38,
    Propagate = 0x39,

    // §5 attribute / item / slice
    GetAttr = 0x3A,
    SetAttr = 0x3B,
    GetItem = 0x3C,
    SetItem = 0x3D,
    Slice = 0x3E,
    SetSlice = 0x3F,

    // §7 call
    ArgBegin = 0x40,
    ArgPos = 0x41,
    ArgKw = 0x42,
    ArgSpread = 0x43,
    ArgKwSpread = 0x44,
    Call = 0x45,
    Unwrap = 0x46,

    // §6 scopes
    NewScope = 0x47,
    PopScope = 0x48,

    // §6 loops and function boundary
    PrepIter = 0x49,
    ForNext = 0x4A,
    Break = 0x4B,
    Skip = 0x4C,
    Return = 0x4D,

    // §9 exceptions, defer, using
    PushTry = 0x4E,
    PopTry = 0x4F,
    Raise = 0x50,
    PushDefer = 0x51,
    PushUsing = 0x52,
    FlushDefers = 0x53,
    FlushUsings = 0x54,

    // §8 class / enum
    MakeClass = 0x55,
    LoadBaseClass = 0x56,
    SetBases = 0x57,
    AddMember = 0x58,
    AddMethod = 0x59,
    MakeProperty = 0x5A,
    RegisterClass = 0x5B,
    MakeEnum = 0x5C,

    // §7 function creation. it sits at the tail of the assigned range so `0x60..=0x7F`
    // stays free for the extension fields of §16.
    MakeFunc = 0x5D,

    // `use` statement. the reference resolves the module during evaluation, not compilation.
    Use = 0x5E,

    /// the reference `Eval` throws `unsupported node kind {Kind}` for every kind it has no
    /// case for. the compiler keeps that behaviour observable instead of failing the build.
    Unsupported = 0x5F,

    /// tuple pattern unpack. operand is the number of values to pop, the last one is
    /// deepest, so the binder runs the targets in reverse.
    /// `Evaluator.Unpack` (`Evaluator.cs:1309`) is the reference this opcode implements.
    Unpack = 0x60,

    /// `ToStr()` conversion. `EvalFString` (`Evaluator.cs:1087`) converts every expression
    /// part with `Eval(child).ToStr()` and requires the result to be a `Str`, so the
    /// conversion is an opcode and the check is part of `FSTRING`.
    /// `0x61` is the second slot of the §16 extension area.
    ToStr = 0x61,

    /// match pattern test. the operand is the index of the pattern in the proto's
    /// pattern table, the value is on the stack and stays there, and the result is a
    /// boolean. the pattern binds its names in the current scope when it matches.
    /// `0x62` is the third slot of the §16 extension area.
    MatchPattern = 0x62,

    /// `go expr` — the operand is the proto slot of the expression, the future runs it at
    /// the `wait` point.
    MakeFuture = 0x63,

    /// `wait expr` — the future on the stack is replaced by its value.
    Await = 0x64,
}

impl Op {
    /// `.unbc` validation rejects an opcode outside this set (BYTECODE.md §14.2).
    pub fn from_u8(v: u8) -> Option<Op> {
        match v {
            0x01 => Some(Op::LoadNone),
            0x02 => Some(Op::LoadNull),
            0x03 => Some(Op::LoadTrue),
            0x04 => Some(Op::LoadFalse),
            0x05 => Some(Op::LoadConst),
            0x06 => Some(Op::LoadName),
            0x07 => Some(Op::LoadLocal),
            0x08 => Some(Op::LoadUpv),
            0x09 => Some(Op::StoreName),
            0x0A => Some(Op::StoreLocal),
            0x0B => Some(Op::StoreUpv),
            0x0C => Some(Op::Dup),
            0x0D => Some(Op::Dup2),
            0x0E => Some(Op::Swap),
            0x0F => Some(Op::Rot),
            0x10 => Some(Op::Pop),
            0x11 => Some(Op::PopN),
            0x12 => Some(Op::BuildList),
            0x13 => Some(Op::BuildDict),
            0x14 => Some(Op::BuildSet),
            0x15 => Some(Op::BuildTuple),
            0x16 => Some(Op::BuildTupleNamed),
            0x17 => Some(Op::FString),
            0x18 => Some(Op::Tag),
            0x19 => Some(Op::Add),
            0x1A => Some(Op::Sub),
            0x1B => Some(Op::Mul),
            0x1C => Some(Op::Div),
            0x1D => Some(Op::Idiv),
            0x1E => Some(Op::Mod),
            0x1F => Some(Op::Pow),
            0x20 => Some(Op::Band),
            0x21 => Some(Op::Bor),
            0x22 => Some(Op::Bxor),
            0x23 => Some(Op::Lsh),
            0x24 => Some(Op::Rsh),
            0x25 => Some(Op::Range),
            0x26 => Some(Op::Eq),
            0x27 => Some(Op::Ne),
            0x28 => Some(Op::Lt),
            0x29 => Some(Op::Gt),
            0x2A => Some(Op::Le),
            0x2B => Some(Op::Ge),
            0x2C => Some(Op::Is),
            0x2D => Some(Op::IsNot),
            0x2E => Some(Op::In),
            0x2F => Some(Op::Xor),
            0x30 => Some(Op::Not),
            0x31 => Some(Op::Pos),
            0x32 => Some(Op::Neg),
            0x33 => Some(Op::Bnot),
            0x34 => Some(Op::Spread),
            0x35 => Some(Op::ToBool),
            0x36 => Some(Op::JmpIf),
            0x37 => Some(Op::Jmp),
            0x38 => Some(Op::PushTryProp),
            0x39 => Some(Op::Propagate),
            0x3A => Some(Op::GetAttr),
            0x3B => Some(Op::SetAttr),
            0x3C => Some(Op::GetItem),
            0x3D => Some(Op::SetItem),
            0x3E => Some(Op::Slice),
            0x3F => Some(Op::SetSlice),
            0x40 => Some(Op::ArgBegin),
            0x41 => Some(Op::ArgPos),
            0x42 => Some(Op::ArgKw),
            0x43 => Some(Op::ArgSpread),
            0x44 => Some(Op::ArgKwSpread),
            0x45 => Some(Op::Call),
            0x46 => Some(Op::Unwrap),
            0x47 => Some(Op::NewScope),
            0x48 => Some(Op::PopScope),
            0x49 => Some(Op::PrepIter),
            0x4A => Some(Op::ForNext),
            0x4B => Some(Op::Break),
            0x4C => Some(Op::Skip),
            0x4D => Some(Op::Return),
            0x4E => Some(Op::PushTry),
            0x4F => Some(Op::PopTry),
            0x50 => Some(Op::Raise),
            0x51 => Some(Op::PushDefer),
            0x52 => Some(Op::PushUsing),
            0x53 => Some(Op::FlushDefers),
            0x54 => Some(Op::FlushUsings),
            0x55 => Some(Op::MakeClass),
            0x56 => Some(Op::LoadBaseClass),
            0x57 => Some(Op::SetBases),
            0x58 => Some(Op::AddMember),
            0x59 => Some(Op::AddMethod),
            0x5A => Some(Op::MakeProperty),
            0x5B => Some(Op::RegisterClass),
            0x5C => Some(Op::MakeEnum),
            0x5D => Some(Op::MakeFunc),
            0x5E => Some(Op::Use),
            0x5F => Some(Op::Unsupported),
            0x60 => Some(Op::Unpack),
            0x61 => Some(Op::ToStr),
            0x62 => Some(Op::MatchPattern),
            0x63 => Some(Op::MakeFuture),
            0x64 => Some(Op::Await),
            _ => None,
        }
    }
}

/// `JMP_IF` shape: how the popped condition decides the jump.
pub const FLAG_JUMP_TRUE: u16 = 1 << 0;
pub const FLAG_JUMP_POP: u16 = 1 << 1;
/// strict boolean test (`while` uses `As<Bool>`, not `ToBool`)
pub const FLAG_STRICT: u16 = 1 << 2;
/// jump when the top is `None` (`??` coalesce), no boolean conversion
pub const FLAG_JUMP_NONE: u16 = 1 << 3;

/// optional-chain flag of `?.` (BYTECODE.md §5 `JMP_NONE` is folded into the flag)
pub const FLAG_OPT_CHAIN: u16 = 1 << 4;

/// `STORE_NAME` with `localOnly: true` — the `for`/`match`/`catch` loop variable form
pub const FLAG_LOCAL_ONLY: u16 = 1 << 5;

/// `STORE_NAME` with the `Scope.Set` rule — the chain of the scopes is walked and the
/// binding is updated where the name is declared. the function definitions use it.
pub const FLAG_SCOPE_SET: u16 = 1 << 8;

/// `ARG_POS` inside a call site: `EvalArguments` checks `value is Err` and rethrows with
/// the header preserved. container literals (`EvalList` / `EvalDict` / `EvalTuple`) do
/// not check, so they emit `ARG_POS` without this flag.
pub const FLAG_ARG_CHECK: u16 = 1 << 6;

/// `UNSUPPORTED` raises a host exception instead of a language error. the message
/// table entry is the .NET text `Name: Message`, the way `Exception.ToString()` prints
/// it. the reference reaches this state when it indexes a missing child node.
pub const FLAG_HOST_ERROR: u16 = 1 << 7;

/// slice bound presence, bit0 = start, bit1 = end, bit2 = step
pub const FLAG_SLICE_START: u16 = 1 << 0;
pub const FLAG_SLICE_END: u16 = 1 << 1;
pub const FLAG_SLICE_STEP: u16 = 1 << 2;

/// `NEW_SCOPE` shape
pub const FLAG_SCOPE_FUNCTION: u16 = 1 << 0;
pub const FLAG_SCOPE_LOOP: u16 = 1 << 1;

/// `Proto.flags`
pub const PROTO_FUNCTION: u16 = 1 << 0;
pub const PROTO_HAS_SELF: u16 = 1 << 1;
pub const PROTO_HAS_SUPER: u16 = 1 << 2;

/// `Param.flags` — mirrors `Arg.IsEssential/IsOptional/IsPositional/IsKeyword`
pub const PARAM_ESSENTIAL: u16 = 1 << 0;
pub const PARAM_OPTIONAL: u16 = 1 << 1;
pub const PARAM_POSITIONAL: u16 = 1 << 2;
pub const PARAM_KEYWORD: u16 = 1 << 3;
pub const PARAM_DEFAULT: u16 = 1 << 4;

/// `MAKE_FUNC` flags — what the reference does with the created `LFn`
pub const FLAG_FUNC_BIND: u16 = 1 << 0;
pub const FLAG_FUNC_EXT: u16 = 1 << 1;
pub const FLAG_FUNC_PROP: u16 = 1 << 2;

/// `MAKE_CLASS` flags
pub const FLAG_CLASS_PARAMS: u16 = 1 << 0;
pub const FLAG_CLASS_BASES: u16 = 1 << 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instr {
    pub op: u8,
    pub flags: u16,
    pub operand: i32,
}

impl Instr {
    pub fn new(op: Op, operand: i32) -> Self {
        Self { op: op as u8, flags: 0, operand }
    }

    pub fn with_flags(op: Op, flags: u16, operand: i32) -> Self {
        Self { op: op as u8, flags, operand }
    }

    /// decodes `op` back into the typed opcode. a `Program` that fails this check is
    /// malformed and never reaches the VM.
    pub fn op_code(&self) -> Option<Op> {
        Op::from_u8(self.op)
    }

    /// `op(1) | flags(2) | operand(4)`, little-endian, 7 bytes (BYTECODE.md §1)
    pub fn encode(&self) -> [u8; 7] {
        let mut out = [0u8; 7];
        out[0] = self.op;
        out[1..3].copy_from_slice(&self.flags.to_le_bytes());
        out[3..7].copy_from_slice(&self.operand.to_le_bytes());
        out
    }

    pub fn decode(bytes: &[u8; 7]) -> Self {
        Self {
            op: bytes[0],
            flags: u16::from_le_bytes([bytes[1], bytes[2]]),
            operand: i32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_values_are_the_contract() {
        // the numbers are written down in BYTECODE.md §16 as the reserved split, so pin
        // the boundaries explicitly instead of trusting the enum order
        assert_eq!(Op::LoadNone as u8, 0x01);
        assert_eq!(Op::StoreUpv as u8, 0x0B);
        assert_eq!(Op::Dup as u8, 0x0C);
        assert_eq!(Op::PopN as u8, 0x11);
        assert_eq!(Op::BuildList as u8, 0x12);
        assert_eq!(Op::Tag as u8, 0x18);
        assert_eq!(Op::Add as u8, 0x19);
        assert_eq!(Op::Range as u8, 0x25);
        assert_eq!(Op::Eq as u8, 0x26);
        assert_eq!(Op::Xor as u8, 0x2F);
        assert_eq!(Op::Not as u8, 0x30);
        assert_eq!(Op::ToBool as u8, 0x35);
        assert_eq!(Op::JmpIf as u8, 0x36);
        assert_eq!(Op::Propagate as u8, 0x39);
        assert_eq!(Op::GetAttr as u8, 0x3A);
        assert_eq!(Op::SetSlice as u8, 0x3F);
        assert_eq!(Op::ArgBegin as u8, 0x40);
        assert_eq!(Op::Unwrap as u8, 0x46);
        assert_eq!(Op::NewScope as u8, 0x47);
        assert_eq!(Op::PopScope as u8, 0x48);
        assert_eq!(Op::PrepIter as u8, 0x49);
        assert_eq!(Op::Return as u8, 0x4D);
        assert_eq!(Op::PushTry as u8, 0x4E);
        assert_eq!(Op::FlushUsings as u8, 0x54);
        assert_eq!(Op::MakeClass as u8, 0x55);
        assert_eq!(Op::MakeEnum as u8, 0x5C);
        assert_eq!(Op::MakeFunc as u8, 0x5D);
        assert_eq!(Op::Use as u8, 0x5E);
        assert_eq!(Op::Unsupported as u8, 0x5F);
        assert_eq!(Op::Unpack as u8, 0x60);
        assert_eq!(Op::ToStr as u8, 0x61);
        assert_eq!(Op::MatchPattern as u8, 0x62);
        assert_eq!(Op::MakeFuture as u8, 0x63);
        assert_eq!(Op::Await as u8, 0x64);
    }

    #[test]
    fn reserved_area_is_free() {
        // `0x60` .. `0x64` are the first slots of the reserved extension area, and
        // `UNPACK` / `TO_STR` / `MATCH_PATTERN` / `MAKE_FUTURE` / `AWAIT` take them.
        // `0x65..=0x7F` must stay free so a `.unbc` written by an older compiler is
        // readable.
        for v in 0x65u8..=0x7F {
            assert_eq!(Op::from_u8(v), None, "{v:#04x} must stay reserved");
        }
        for v in 0x65u8..=0x7F {
            let bytes = [v, 0, 0, 0, 0, 0, 0];
            let instr = Instr::decode(&bytes);
            assert_eq!(instr.op_code(), None);
        }
    }

    #[test]
    fn encoding_is_little_endian_round_trip() {
        let instr = Instr::with_flags(Op::JmpIf, 0xABCD, -12345);
        let bytes = instr.encode();
        assert_eq!(bytes[0], 0x36);
        assert_eq!(bytes[1..3], [0xCD, 0xAB]);
        assert_eq!(bytes[3..7], (-12345i32).to_le_bytes());
        assert_eq!(Instr::decode(&bytes), instr);
    }

    #[test]
    fn every_assigned_opcode_is_decodable() {
        for v in 0x01u8..=0x60 {
            assert!(Op::from_u8(v).is_some(), "{v:#04x} is a hole in the opcode table");
        }
        assert_eq!(Op::from_u8(0x00), None);
    }
}
