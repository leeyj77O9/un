pub mod instr;
pub mod unbc;
    pub use instr::{
    Instr, Op, PARAM_DEFAULT, PARAM_ESSENTIAL, PARAM_KEYWORD, PARAM_OPTIONAL, PARAM_POSITIONAL,
    FLAG_ARG_CHECK, FLAG_CLASS_BASES, FLAG_CLASS_PARAMS, FLAG_FUNC_BIND, FLAG_FUNC_EXT,
    FLAG_FUNC_PROP, FLAG_HOST_ERROR, FLAG_JUMP_NONE, FLAG_JUMP_POP, FLAG_JUMP_TRUE,
    FLAG_LOCAL_ONLY, FLAG_OPT_CHAIN, FLAG_SCOPE_FUNCTION, FLAG_SCOPE_LOOP, FLAG_SLICE_END,
    FLAG_SLICE_START, FLAG_SLICE_STEP, FLAG_STRICT, FLAG_SCOPE_SET, PROTO_FUNCTION,
    PROTO_HAS_SELF, PROTO_HAS_SUPER,
};
pub const MAGIC: [u8; 4] = *b"UNB1";
pub const VERSION: u16 = 1;
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
        (ip as i64 + 1 + delta as i64) as usize
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

    /// Unique per-program id (assigned at compile/read time). Per-program JIT
    /// state is keyed by this id instead of by pointer identity, which
    /// eliminates the stale-entry hazard when a program is dropped and a new
    /// program lands at the same address.
    pub id: u64,
}

static PROGRAM_ID_SEQ: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

/// Draw the next program id. Called at every fresh `Program` creation.
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
                // Opcodes that the verifier understands but the register-VM
                // translation does not implement. Reject here rather than
                // letting `transfer::compile` fail downstream (which would
                // otherwise panic in `plan_for`).
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

            // Stack-depth analysis is delegated to the register-VM translator
            // (`transfer::analyze`), which is the single source of truth for
            // handler seeding, dead code after `Return`/`Raise`, and jump-target
            // checks. Delegating here prevents the verifier and the translator
            // from drifting apart.
            let depths = crate::vm::transfer::analyze(proto).map_err(|e| format!("{label}: {e}"))?;
            // Register-VM invariant: the loaded program must fit within the
            // 8-bit register file (locals + stack max depth + 1 <= 256).
            // Enforce it here so downstream code (`vm::transfer::compile`) does
            // not need to panic on a too-complex function.
            let scratch_base = proto.locals.len();
            let max_depth = depths.iter().flatten().copied().max().unwrap_or(0).max(0) as usize;
            let reg_count = scratch_base + max_depth + 1;
            if reg_count > 256 {
                return Err(format!(
                    "{label}: function uses {reg_count} registers (locals {scratch_base} + max depth {max_depth} + 1), exceeds limit 256",
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

        Op::BuildTupleNamed => index_range(label, "names", flags as usize, proto.names.len()),
        Op::GetAttr
        | Op::SetAttr
        | Op::ArgKw
        | Op::MakeClass
        | Op::LoadBaseClass
        | Op::RegisterClass
        | Op::MakeEnum
        | Op::AddMember => index_range(label, "name", operand, proto.names.len()),
        Op::LoadLocal | Op::StoreLocal => {
            index_range(label, "slot", operand, proto.locals.len())
        }
        Op::FusedIntMulAdd => {
            let dst = ((operand >> 20) & 0x3FF) as usize;
            let x = ((operand >> 10) & 0x3FF) as usize;
            let y = (operand & 0x3FF) as usize;
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
/// Validate a match pattern tree: depth must stay small (a crafted file
/// could otherwise nest `Pattern::Tuple` until the stack overflows during a
/// later recursive pass), and every index it names must be in range. Pattern
/// indices are resolved against the program-wide tables (`match_pattern`
/// reads `self.program.consts` / `self.program.names`).
fn validate_pattern(label: &str, pattern: &Pattern, program: &Program, depth: usize) -> Result<(), String> {
    const MAX_PATTERN_DEPTH: usize = 64;
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

        Op::Slice => -slice_bounds(flags) + 1,
        Op::SetSlice => -slice_bounds(flags),
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
fn slice_bounds(flags: u16) -> i64 {
    let mut n = 1;
    if flags & FLAG_SLICE_START != 0 {
        n += 1;
    }
    if flags & FLAG_SLICE_END != 0 {
        n += 1;
    }
    if flags & FLAG_SLICE_STEP != 0 {
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_too_many_locals() {
        // Construct a proto with 256 locals and a body with stack depth 1.
        // 256 + 1 + 1 = 258 > 256 → must be rejected.
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
