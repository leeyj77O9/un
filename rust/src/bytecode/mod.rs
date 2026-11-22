pub mod instr;
pub mod unbc;
    pub use instr::{
    Instr, Op, PARAM_DEFAULT, PARAM_ESSENTIAL, PARAM_KEYWORD, PARAM_OPTIONAL, PARAM_POSITIONAL,
    FLAG_ARG_CHECK, FLAG_CLASS_BASES, FLAG_CLASS_PARAMS, FLAG_FUNC_BIND, FLAG_FUNC_EXT,
    FLAG_FUNC_PROP, FLAG_HOST_ERROR, FLAG_JUMP_NONE, FLAG_JUMP_POP, FLAG_JUMP_TRUE,
    FLAG_LOCAL_ONLY, FLAG_OPT_CHAIN, FLAG_SCOPE_FUNCTION, FLAG_SCOPE_LOOP, FLAG_SLICE_END,
    FLAG_SLICE_START, FLAG_SLICE_STEP, FLAG_STRICT, FLAG_SCOPE_SET, PROTO_FUNCTION,
    PROTO_HAS_SELF, PROTO_HAS_SUPER, MAX_REGS, FUSED_MULADD_SLOT_MASK, jump_target,
    jump_target_i64, pack_fused_muladd, slice_bound_count, unpack_fused_muladd,
};
pub const MAGIC: [u8; 4] = *b"UNB1";
pub const VERSION: u16 = 1;
/// Shared nesting limit for pattern validation (bytecode and .unbc reader).
pub const MAX_PATTERN_DEPTH: usize = 64;
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    None,
    Null,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: usize,

    pub type_name: Option<usize>,
    pub flags: u16,
    pub default: Option<usize>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcKind {

    Catch,

    Finally,

    Propagate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExcEntry {
    pub kind: ExcKind,

    pub start: usize,
    pub end: usize,

    pub handler: usize,

    pub var_slot: Option<usize>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {

    Wildcard,

    Bind(usize),

    Literal(usize),

    Tuple(Vec<Pattern>),

    /// `Enum.variant(inner)` — name indexes refer to the proto's name table.
    Variant(usize, usize, Box<Pattern>),

    /// `A | B` pattern: matches when any alternative matches.
    /// Alternatives are ranked in source order; the first match wins.
    Or(Vec<Pattern>),

    /// `Name(...)` class pattern. The name indexes `program.names`; the inner
    /// pattern is normally `Fields` and is matched against the instance itself.
    Class(usize, Box<Pattern>),

    /// Named/positional attribute destructuring. `None` means positional (use
    /// the instance's declaration order); `Some(idx)` indexes `program.names`.
    Fields(Vec<(Option<usize>, Pattern)>),
}
#[derive(Debug, Clone, PartialEq)]
pub struct Proto {

    pub name: usize,
    pub arity: usize,

    pub locals: Vec<usize>,
    pub params: Vec<Param>,
    pub code: Vec<Instr>,

    pub consts: Vec<usize>,

    pub names: Vec<usize>,

    pub protos: Vec<usize>,

    pub call_names: Vec<Vec<i32>>,

    pub source_map: Vec<(usize, usize)>,
    pub exc: Vec<ExcEntry>,
    pub flags: u16,

    pub patterns: Vec<Pattern>,

    pub ext_type: Option<usize>,

    pub fn_span: Option<(usize, usize)>,
}
impl Proto {
    pub fn new(name: usize, flags: u16) -> Self {
        Self {
            name,
            arity: 0,
            locals: Vec::new(),
            params: Vec::new(),
            code: Vec::new(),
            consts: Vec::new(),
            names: Vec::new(),
            protos: Vec::new(),
            call_names: Vec::new(),
            source_map: Vec::new(),
            exc: Vec::new(),
            flags,
            patterns: Vec::new(),
            ext_type: None,
            fn_span: None,
        }
    }

    pub fn is_function(&self) -> bool {
        self.flags & PROTO_FUNCTION != 0
    }

    pub fn instruction(&self, ip: usize) -> Option<(Op, u16, i32)> {
        let instr = self.code.get(ip)?;
        Some((instr.op_code()?, instr.flags, instr.operand))
    }
    pub fn span(&self, ip: usize) -> (usize, usize) {
        self.source_map.get(ip).copied().unwrap_or((0, 0))
    }

    pub fn jump_target(&self, ip: usize, delta: i32) -> usize {
        instr::jump_target(ip, delta)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub source_path: String,

    pub source: Option<String>,
    pub names: Vec<String>,
    pub consts: Vec<Constant>,
    pub protos: Vec<Proto>,

    pub msgs: Vec<String>,
    pub root: usize,

    pub id: u64,
}

static PROGRAM_ID_SEQ: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

pub fn next_program_id() -> u64 {
    PROGRAM_ID_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
impl Program {
    pub fn name(&self, proto: &Proto, index: usize) -> &str {
        let module_index = proto.names[index];
        &self.names[module_index]
    }
    pub fn constant(&self, proto: &Proto, index: usize) -> &Constant {
        &self.consts[proto.consts[index]]
    }
    pub fn message(&self, index: usize) -> &str {
        &self.msgs[index]
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.protos.is_empty() {
            return Err("program has no proto".to_string());
        }
        if self.root >= self.protos.len() {
            return Err(format!("root proto index {} out of range", self.root));
        }
        for (proto_index, proto) in self.protos.iter().enumerate() {
            let label = format!("proto {proto_index}");
            if proto.name >= self.names.len() {
                return Err(format!("{label}: name index {} out of range", proto.name));
            }
            if let Some(text) = &self.source {
                let char_len = text.chars().count();
                for ip in 0..proto.source_map.len() {
                    let (start, length) = proto.source_map[ip];
                    let end = start.checked_add(length).ok_or_else(|| {
                        format!("{label}: source_map span at ip {ip} overflows")
                    })?;
                    if end > char_len {
                        return Err(format!(
                            "{label}: source_map span at ip {ip} [{start}..{end}) out of embedded source ({char_len} chars)"
                        ));
                    }
                }
            }
            if proto.source_map.len() != proto.code.len() {
                return Err(format!(
                    "{label}: source_map len {} != code len {}",
                    proto.source_map.len(),
                    proto.code.len()
                ));
            }

            for index in &proto.consts {
                if *index >= self.consts.len() {
                    return Err(format!("{label}: constant index {index} out of range"));
                }
            }
            for index in &proto.names {
                if *index >= self.names.len() {
                    return Err(format!("{label}: name index {index} out of range"));
                }
            }
            for index in &proto.protos {
                if *index >= self.protos.len() {
                    return Err(format!("{label}: proto index {index} out of range"));
                }
            }
            for index in &proto.locals {
                if *index >= proto.names.len() {
                    return Err(format!("{label}: local name index {index} out of range"));
                }
            }
            for pattern in &proto.patterns {
                validate_pattern(&label, pattern, self, 0)?;
            }
            for param in &proto.params {
                if param.name >= self.names.len() {
                    return Err(format!("{label}: parameter name index {} out of range", param.name));
                }
                if let Some(type_name) = param.type_name {
                    if type_name >= self.names.len() {
                        return Err(format!(
                            "{label}: parameter type index {type_name} out of range"
                        ));
                    }
                }
                if let Some(default) = param.default {
                    if default >= self.consts.len() {
                        return Err(format!("{label}: parameter default index {default} out of range"));
                    }
                }
            }
            if let Some(ext) = proto.ext_type {
                if ext >= self.names.len() {
                    return Err(format!("{label}: extension type index {ext} out of range"));
                }
            }
            for entry in &proto.exc {
                if entry.start >= entry.end {
                    return Err(format!("{label}: exception range {} >= {}", entry.start, entry.end));
                }
                if entry.end > proto.code.len() {
                    return Err(format!("{label}: exception end {} beyond code", entry.end));
                }
                if entry.handler >= proto.code.len() {
                    return Err(format!("{label}: exception handler {} beyond code", entry.handler));
                }
                if let Some(slot) = entry.var_slot {
                    if slot >= proto.names.len() {
                        return Err(format!("{label}: exception var slot {slot} out of range"));
                    }
                }
            }
            for ip in 0..proto.code.len() {
                let (op, flags, operand) = proto.instruction(ip).ok_or(format!(
                    "{label}: opcode {:#04x} is not defined",
                    proto.code[ip].op
                ))?;
                if matches!(
                    op,
                    Op::LoadUpv
                        | Op::StoreUpv
                        | Op::Dup2
                        | Op::Rot
                        | Op::PopN
                        | Op::Tag
                        | Op::FlushDefers
                        | Op::FlushUsings
                ) {
                    return Err(format!("{label}: opcode {op:?} is not supported"));
                }
                check_indices(&label, proto, self, op, flags, operand)?;
            }

            let depths = crate::vm::transfer::analyze(proto).map_err(|e| format!("{label}: {e}"))?;
            let scratch_base = proto.locals.len();
            let max_depth = depths.iter().flatten().copied().max().unwrap_or(0).max(0) as usize;
            let reg_count = scratch_base + max_depth + 1;
            if reg_count > MAX_REGS {
                return Err(format!(
                    "{label}: function uses {reg_count} registers (locals {scratch_base} + max depth {max_depth} + 1), exceeds limit {MAX_REGS}",
                ));
            }
        }
        Ok(())
    }
}
fn check_indices(
    label: &str,
    proto: &Proto,
    program: &Program,
    op: Op,
    flags: u16,
    operand: i32,
) -> Result<(), String> {
    let operand = operand as i64;
    if operand < 0 {

        let is_jump = matches!(op, Op::Jmp | Op::JmpIf | Op::ForNext | Op::Break | Op::Skip);
        if !is_jump {
            return Err(format!("{label}: negative operand {operand} for {op:?}"));
        }
        return Ok(());
    }
    let operand = operand as usize;
    match op {
        Op::LoadConst => index_range(label, "constant", operand, proto.consts.len()),
        Op::LoadName => index_range(label, "name", operand, proto.names.len()),

        Op::GetAttr
        | Op::SetAttr
        | Op::ArgKw
        | Op::LoadBaseClass
        | Op::RegisterClass
        | Op::MakeEnum
        | Op::AddMember => index_range(label, "name", operand, proto.names.len()),
        Op::ArgBegin => {
            if operand > MAX_REGS {
                return Err(format!("{label}: argument count {operand} exceeds {MAX_REGS}"));
            }
            Ok(())
        }
        Op::MakeClass => {
            index_range(label, "name", operand, proto.names.len())?;
            let fields = (flags >> 8) as usize;
            if operand + 1 + fields > proto.names.len() {
                return Err(format!("{label}: class field name slot out of range"));
            }
            Ok(())
        }
        Op::BuildTupleNamed => {
            let first = flags as usize;
            index_range(label, "names", first, proto.names.len())?;
            if operand > MAX_REGS {
                return Err(format!("{label}: tuple name count {operand} exceeds {MAX_REGS}"));
            }
            if first + operand > proto.names.len() {
                return Err(format!("{label}: tuple name slot out of range"));
            }
            Ok(())
        }
        Op::LoadLocal | Op::StoreLocal => {
            index_range(label, "slot", operand, proto.locals.len())
        }
        Op::FusedIntMulAdd => {
            let (dst, x, y) = unpack_fused_muladd(operand as i32);
            if dst >= proto.locals.len() {
                return Err(format!("{label}: destination slot {dst} out of range"));
            }
            if x >= proto.locals.len() {
                return Err(format!("{label}: source slot {x} out of range"));
            }
            if y >= proto.locals.len() {
                return Err(format!("{label}: source slot {y} out of range"));
            }
            Ok(())
        }

        Op::LoadUpv | Op::StoreUpv => {
            let slot = operand & 0xFFFF;
            let depth = operand >> 16;
            if slot >= proto.locals.len() {
                return Err(format!("{label}: upvalue slot {slot} out of range"));
            }

            index_range(label, "upvalue depth", depth, usize::MAX)
        }
        Op::Call => index_range(label, "call names", flags as usize, proto.call_names.len()),

        Op::Use => index_range(label, "name", operand, proto.names.len()),
        Op::PushTry | Op::PushTryProp => index_range(label, "exception entry", operand, proto.exc.len()),
        Op::MakeFunc => index_range(label, "proto", operand, proto.protos.len()),

        Op::MakeFuture => index_range(label, "proto", operand, proto.protos.len()),
        Op::MatchPattern => index_range(label, "pattern", operand, proto.patterns.len()),
        Op::Unsupported => index_range(label, "message", operand, program.msgs.len()),
        Op::PrepIter => index_range(label, "message", operand, program.msgs.len()),
        Op::Jmp | Op::JmpIf | Op::ForNext | Op::Break | Op::Skip => Ok(()),
        Op::Slice | Op::SetSlice => {
            if flags & !(FLAG_SLICE_START | FLAG_SLICE_END | FLAG_SLICE_STEP) != 0 {
                return Err(format!("{label}: unknown slice flags {flags:#06x}"));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
fn index_range(label: &str, what: &str, index: usize, len: usize) -> Result<(), String> {
    if index >= len {
        return Err(format!("{label}: {what} index {index} out of range ({len})"));
    }
    Ok(())
}
fn validate_pattern(label: &str, pattern: &Pattern, program: &Program, depth: usize) -> Result<(), String> {
    if depth > MAX_PATTERN_DEPTH {
        return Err(format!("{label}: pattern nesting exceeds {MAX_PATTERN_DEPTH}"));
    }
    match pattern {
        Pattern::Wildcard => Ok(()),
        Pattern::Bind(index) => index_range(label, "pattern bind", *index, program.names.len()),
        Pattern::Literal(index) => index_range(label, "pattern literal", *index, program.consts.len()),
        Pattern::Tuple(items) => {
            for item in items {
                validate_pattern(label, item, program, depth + 1)?;
            }
            Ok(())
        }
        Pattern::Variant(enum_name, variant_name, inner) => {
            index_range(label, "pattern enum", *enum_name, program.names.len())?;
            index_range(label, "pattern variant", *variant_name, program.names.len())?;
            validate_pattern(label, inner, program, depth + 1)
        }
        Pattern::Or(alternatives) => {
            if alternatives.is_empty() {
                return Err(format!("{label}: or-pattern has no alternatives"));
            }
            for alt in alternatives {
                validate_pattern(label, alt, program, depth + 1)?;
            }
            Ok(())
        }
        Pattern::Class(name, inner) => {
            index_range(label, "pattern class", *name, program.names.len())?;
            validate_pattern(label, inner, program, depth + 1)
        }
        Pattern::Fields(fields) => {
            for (name, pat) in fields {
                if let Some(name) = name {
                    index_range(label, "pattern field", *name, program.names.len())?;
                }
                validate_pattern(label, pat, program, depth + 1)?;
            }
            Ok(())
        }
    }
}
pub fn stack_effect(op: Op, flags: u16, operand: i32) -> i64 {
    let count = operand as i64;
    match op {
        Op::LoadNone
        | Op::LoadNull
        | Op::LoadTrue
        | Op::LoadFalse
        | Op::LoadConst
        | Op::LoadName
        | Op::LoadLocal
        | Op::LoadUpv
        | Op::Dup
        | Op::MakeClass
        | Op::LoadBaseClass
        | Op::MakeEnum
        | Op::Use
        | Op::FusedIntMulAdd => 1,
        Op::Dup2 => 2,
        Op::StoreLocal | Op::StoreUpv | Op::Return => -1,

        Op::StoreName => {
            if flags & FLAG_SCOPE_SET != 0 {
                0
            } else {
                -1
            }
        }
        Op::Pop => -1,
        Op::PopN => -count,

        Op::SetBases => -count,

        Op::RegisterClass => 0,

        Op::Unpack => count - 1,
        Op::PrepIter => 0,

        Op::MatchPattern => 1,

        Op::MakeFuture => 1,
        Op::Await => 0,

        Op::ForNext => 1,
        Op::Swap | Op::Rot => 0,
        Op::BuildList | Op::BuildSet | Op::BuildTuple | Op::BuildTupleNamed | Op::FString => 1 - count,
        Op::BuildDict => 1 - 2 * count,
        Op::Tag => -count,
        Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Idiv
        | Op::Mod
        | Op::Pow
        | Op::Band
        | Op::Bor
        | Op::Bxor
        | Op::Lsh
        | Op::Rsh
        | Op::Range
        | Op::Eq
        | Op::Ne
        | Op::Lt
        | Op::Gt
        | Op::Le
        | Op::Ge
        | Op::Is
        | Op::IsNot
        | Op::In
        | Op::Xor
        | Op::GetItem
        | Op::ArgKwSpread => -1,

        Op::ArgPos | Op::ArgKw | Op::ArgSpread => 0,
        Op::Not
        | Op::Pos
        | Op::Neg
        | Op::Bnot
        | Op::Spread
        | Op::ToBool
        | Op::ToStr
        | Op::Jmp
        | Op::PushTryProp
        | Op::Propagate
        | Op::GetAttr
        | Op::NewScope
        | Op::PopScope
        | Op::Break
        | Op::Skip
        | Op::ArgBegin
        | Op::PushTry
        | Op::PopTry
        | Op::Raise
        | Op::PushDefer
        | Op::PushUsing
        | Op::FlushDefers
        | Op::FlushUsings
        | Op::Unsupported => 0,

        Op::Unwrap => 0,
        Op::SetAttr | Op::AddMember | Op::AddMethod => -1,

        Op::MakeFunc => 1 - ((flags >> 8) as i64),
        Op::SetItem => -2,

        Op::MakeProperty => -2,

        Op::Slice => -(slice_bound_count(flags) as i64),
        Op::SetSlice => -(slice_bound_count(flags) as i64 + 1),
        Op::JmpIf => {
            if flags & FLAG_JUMP_POP != 0 {
                -1
            } else {
                0
            }
        }
        Op::Call => -count,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_stack_effect_matches_the_original_bound_formula() {
        for bits in 0u16..8 {
            let flags = if bits & 1 != 0 { FLAG_SLICE_START } else { 0 }
                | if bits & 2 != 0 { FLAG_SLICE_END } else { 0 }
                | if bits & 4 != 0 { FLAG_SLICE_STEP } else { 0 };
            let old_bounds = 1
                + (flags & FLAG_SLICE_START != 0) as i64
                + (flags & FLAG_SLICE_END != 0) as i64
                + (flags & FLAG_SLICE_STEP != 0) as i64;
            assert_eq!(stack_effect(Op::Slice, flags, 0), -old_bounds + 1);
            assert_eq!(stack_effect(Op::SetSlice, flags, 0), -old_bounds);
        }
    }

    #[test]
    fn validate_rejects_too_many_locals() {
        let mut proto = Proto::new(0, PROTO_FUNCTION);
        proto.locals = (0..256).collect();
        proto.arity = 0;
        proto.names = vec![0; 256];
        proto.consts = vec![0];
        proto.code.push(Instr::new(Op::LoadConst, 0));
        proto.code.push(Instr::new(Op::Return, 0));
        proto.source_map = vec![(1, 0); 2];

        let prog = Program {
            source_path: "test".into(),
            source: None,
            names: vec!["f".into()],
            consts: vec![Constant::Int(0)],
            protos: vec![proto],
            msgs: vec![],
            root: 0,
            id: next_program_id(),
        };
        let err = prog.validate().expect_err("must error");
        assert!(
            err.contains("registers") || err.contains("limit"),
            "expected register count error, got: {err}"
        );
    }
}
