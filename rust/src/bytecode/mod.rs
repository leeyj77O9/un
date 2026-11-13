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
            }
            for ip in 0..proto.code.len() {
                let (op, flags, operand) = proto.instruction(ip).ok_or(format!(
                    "{label}: opcode {:#04x} is not defined",
                    proto.code[ip].op
                ))?;
                check_indices(&label, proto, self, op, flags, operand)?;
            }

            let mut depths: Vec<Option<i64>> = vec![None; proto.code.len() + 1];
            let mut work = vec![0usize];
            depths[0] = Some(0);
            while let Some(ip) = work.pop() {
                let mut depth = depths[ip].expect("queued depth");
                let mut ip = ip;
                while ip < proto.code.len() {
                    if let Some(seen) = depths[ip] {
                        if seen != depth {
                            return Err(format!(
                                "{label}: stack depth mismatch at ip {ip} ({seen} vs {depth})"
                            ));
                        }
                    } else {
                        depths[ip] = Some(depth);
                    }
                    let (op, flags, operand) = proto.instruction(ip).ok_or(format!(
                        "{label}: opcode {:#04x} is not defined",
                        proto.code[ip].op
                    ))?;
                    let effect = stack_effect(op, flags, operand);
                    let next_depth = depth + effect;
                    if next_depth < 0 {
                        return Err(format!("{label}: stack depth {next_depth} at ip {ip}"));
                    }
                    if matches!(op, Op::Jmp | Op::JmpIf | Op::ForNext | Op::Break | Op::Skip) {
                        let target = ip as i64 + 1 + operand as i64;
                        if target < 0 || target > proto.code.len() as i64 {
                            return Err(format!("{label}: jump target {target} out of range"));
                        }
                        let target = target as usize;
                        let target_depth = if op == Op::ForNext { depth - 1 } else { next_depth };
                        if target_depth < 0 {
                            return Err(format!("{label}: stack depth {target_depth} at ip {ip}"));
                        }
                        match depths[target] {
                            Some(seen) if seen != target_depth => {
                                return Err(format!(
                                    "{label}: jump to {target} expects depth {seen}, reached depth {target_depth}"
                                ));
                            }
                            None => {
                                depths[target] = Some(target_depth);
                                work.push(target);
                            }
                            _ => {}
                        }
                        if matches!(op, Op::Jmp | Op::Break | Op::Skip) {
                            break;
                        }

                        if op == Op::Return || op == Op::Raise {
                            break;
                        }
                        depth = next_depth;
                        ip += 1;
                        continue;
                    }
                    depth = next_depth;
                    ip += 1;
                }
                if ip == proto.code.len() {
                    match depths[ip] {
                        Some(seen) if seen != depth => {
                            return Err(format!(
                                "{label}: stack depth mismatch at end ({seen} vs {depth})"
                            ));
                        }
                        None => depths[ip] = Some(depth),
                        _ => {}
                    }
                }
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
        | Op::Use => 1,
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
