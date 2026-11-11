//! bytecode format: instruction stream, constant pool, name table, proto table, exception
//! table, source mapping, and the load-time validation rules of BYTECODE.md §14.
//!
//! the tables are count-prefixed and every instruction is fixed-size, so `.unbc` (phase 15)
//! can serialize this layout without restructuring it.

pub mod instr;

pub use instr::{
    Instr, Op, PARAM_DEFAULT, PARAM_ESSENTIAL, PARAM_KEYWORD, PARAM_OPTIONAL, PARAM_POSITIONAL,
    FLAG_ARG_CHECK, FLAG_CLASS_BASES, FLAG_CLASS_PARAMS, FLAG_FUNC_BIND, FLAG_FUNC_EXT,
    FLAG_FUNC_PROP, FLAG_JUMP_NONE, FLAG_JUMP_POP, FLAG_JUMP_TRUE, FLAG_LOCAL_ONLY,
    FLAG_OPT_CHAIN, FLAG_SCOPE_FUNCTION, FLAG_SCOPE_LOOP, FLAG_SLICE_END, FLAG_SLICE_START,
    FLAG_SLICE_STEP, FLAG_STRICT, PROTO_FUNCTION, PROTO_HAS_SELF, PROTO_HAS_SUPER,
};

/// `.unbc` magic (BYTECODE.md §13).
pub const MAGIC: [u8; 4] = *b"UNB1";
/// minor/major version, appended fields bump `minor`.
pub const VERSION: u16 = 1;

/// module-level constant pool entry. `Constant` is deduplicated in `.unbc`, and the `Str`
/// intern pool rule is applied at load time, not at compile time.
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    None,
    Null,
}

/// one parameter of a `Proto`. `Arg` of `src/Core/Object/Function/Arg.cs` resolves to this
/// shape at compile time; `default` is a module constant pool index.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: usize,
    /// type annotation name, `None` when the parameter is untyped
    pub type_name: Option<usize>,
    pub flags: u16,
    pub default: Option<usize>,
}

/// exception table entry (BYTECODE.md §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcKind {
    /// `Error` handler, one per `catch` clause
    Catch,
    /// `finally` body
    Finally,
    /// `?` operator: any `Error`/`Err` inside the range becomes a `PropagateFlow`
    Propagate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExcEntry {
    pub kind: ExcKind,
    /// instruction range `[start, end)` the handler covers
    pub start: usize,
    pub end: usize,
    /// handler entry point, always an instruction boundary
    pub handler: usize,
    /// the `catch (e)` variable slot, `None` for a bare `catch`
    pub var_slot: Option<usize>,
}

/// a compiled code unit: the module and every nested function. `protos` is the flat
/// function table `.unbc` stores; nested protos are reached by index.
#[derive(Debug, Clone, PartialEq)]
pub struct Proto {
    /// name table index
    pub name: usize,
    pub arity: usize,
    /// compile-time slot order, identical to the runtime `Scope` slot order
    pub locals: Vec<usize>,
    pub params: Vec<Param>,
    pub code: Vec<Instr>,
    /// module constant pool indices
    pub consts: Vec<usize>,
    /// module name table indices
    pub names: Vec<usize>,
    /// nested protos, module proto indices
    pub protos: Vec<usize>,
    /// per call site: the argument names, `-1` for a positional argument
    pub call_names: Vec<Vec<i32>>,
    /// `(start, length)` of the source span each instruction comes from
    pub source_map: Vec<(usize, usize)>,
    pub exc: Vec<ExcEntry>,
    pub flags: u16,
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
        }
    }

    /// `source_map` is the only way `.unbc` can rebuild an error position and a trace line,
    /// so a mapping that is shorter than the code is treated as degraded (BYTECODE.md §12).
    pub fn is_function(&self) -> bool {
        self.flags & PROTO_FUNCTION != 0
    }

    /// resolves one instruction. the caller has validated the program, so `op_code` cannot
    /// fail here; `None` is returned for a hole so the VM can report it instead of panicking.
    pub fn instruction(&self, ip: usize) -> Option<(Op, u16, i32)> {
        let instr = self.code.get(ip)?;
        Some((instr.op_code()?, instr.flags, instr.operand))
    }

    pub fn span(&self, ip: usize) -> (usize, usize) {
        self.source_map.get(ip).copied().unwrap_or((0, 0))
    }

    /// jump target of an instruction, relative to the next ip (BYTECODE.md §1 rule 3).
    pub fn jump_target(&self, ip: usize, delta: i32) -> usize {
        (ip as i64 + 1 + delta as i64) as usize
    }
}

/// the compiled module: the flat proto table plus the shared constant and name tables and
/// the message table `UNSUPPORTED` uses.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub source_path: String,
    pub names: Vec<String>,
    pub consts: Vec<Constant>,
    pub protos: Vec<Proto>,
    /// messages for `UNSUPPORTED` operands. the reference keeps `unsupported node kind X`
    /// observable, so the string lives in a table and not in the opcode.
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

    /// BYTECODE.md §14 rules 1..7, run before the VM touches a program.
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

            // rule 4/6: every referenced index is in range, exception ranges are sane
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

            // rule 3/7: operand ranges, jump targets, stack depth simulation
            let mut depth: i64 = 0;
            let mut ip = 0usize;
            let mut targets: Vec<i64> = vec![-1; proto.code.len() + 1];
            targets[0] = 0;
            while ip < proto.code.len() {
                let (op, flags, operand) = proto
                    .instruction(ip)
                    .ok_or(format!("{label}: opcode {:#04x} is not defined", proto.code[ip].op))?;

                check_indices(&label, proto, self, op, flags, operand)?;

                let target = ip as i64 + 1 + operand as i64;
                if op == Op::Jmp || op == Op::JmpIf {
                    if target < 0 || target > proto.code.len() as i64 {
                        return Err(format!("{label}: jump target {target} out of range"));
                    }
                    let target = target as usize;
                    let expected = depth + stack_effect(op, flags, operand);
                    if expected < 0 {
                        return Err(format!("{label}: stack depth {expected} at ip {ip}"));
                    }
                    if targets[target] >= 0 && targets[target] != expected {
                        return Err(format!(
                            "{label}: jump to {target} expects depth {}, reached depth {expected}",
                            targets[target]
                        ));
                    }
                    targets[target] = expected;
                    depth = expected;
                    ip = target;
                    continue;
                }

                depth += stack_effect(op, flags, operand);
                if depth < 0 {
                    return Err(format!("{label}: stack depth {depth} at ip {ip}"));
                }
                ip += 1;
            }
        }

        Ok(())
    }
}

/// operand/flag index checks for one instruction.
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
        // jumps take a negative delta, every other index operand is non-negative
        let is_jump = op == Op::Jmp || op == Op::JmpIf;
        if !is_jump {
            return Err(format!("{label}: negative operand {operand} for {op:?}"));
        }
        return Ok(());
    }
    let operand = operand as usize;

    match op {
        Op::LoadConst => index_range(label, "constant", operand, proto.consts.len()),
        Op::LoadName => index_range(label, "name", operand, proto.names.len()),
        // the doc table puts the name index in the operand for these, `BUILD_TUPLE_NAMED`
        // keeps the count in the operand and the names table index in the flags
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
        // `depth << 16 | slot` in one operand
        Op::LoadUpv | Op::StoreUpv => {
            let slot = operand & 0xFFFF;
            let depth = operand >> 16;
            if slot >= proto.locals.len() {
                return Err(format!("{label}: upvalue slot {slot} out of range"));
            }
            // the parent chain depth is only knowable across protos, so the slot is the
            // part this proto can check at load time
            index_range(label, "upvalue depth", depth, usize::MAX)
        }
        Op::Call => index_range(label, "call names", flags as usize, proto.call_names.len()),
        // the `use` operand is the path name index, the alias and pairs follow it
        Op::Use => index_range(label, "name", operand, proto.names.len()),
        Op::PushTry | Op::PushTryProp => index_range(label, "exception entry", operand, proto.exc.len()),
        Op::MakeFunc => index_range(label, "proto", operand, proto.protos.len()),
        Op::Unsupported => index_range(label, "message", operand, program.msgs.len()),
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

/// BYTECODE.md §15 stack effects. `JMP_IF` pops only when `FLAG_JUMP_POP` is set, because
/// `and`/`or` keep the side they short-circuit to.
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
        | Op::PrepIter
        | Op::ForNext
        | Op::MakeClass
        | Op::LoadBaseClass
        | Op::MakeEnum
        | Op::MakeFunc
        | Op::Use => 1,
        Op::Dup2 => 2,
        Op::StoreName | Op::StoreLocal | Op::StoreUpv | Op::Raise | Op::Return => -1,
        Op::Pop => -1,
        Op::PopN => -count,
        // `UNPACK` pops `count` values and hands them to the following binders one by one,
        // so the net depth change is `count - 1` (the count itself lives in the operand).
        Op::Unpack => count - 1,
        Op::Swap | Op::Rot => 0,
        Op::BuildList | Op::BuildSet | Op::BuildTuple | Op::BuildTupleNamed | Op::FString => 1 - count,
        Op::BuildDict => 1 - 2 * count,
        Op::Tag | Op::SetBases => -count,
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
        // the ARG accumulators keep the value on the stack and only record the name,
        // so they do not change the depth. `ARG_KW_SPREAD` pops the dict it spreads.
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
        | Op::PushDefer
        | Op::PushUsing
        | Op::FlushDefers
        | Op::FlushUsings
        | Op::Unsupported => 0,
        // `UNWRAP` checks the top of the stack and leaves it in place, so it does not change
        // the depth. the name index of `ADD_MEMBER` lives in flags, so only the value is popped,
        // and `ADD_METHOD` pops the `MAKE_FUNC` result it consumes.
        Op::Unwrap => 0,
        Op::SetAttr | Op::AddMember | Op::AddMethod => -1,
        Op::SetItem => -2,
        Op::MakeProperty => -3,
        Op::RegisterClass => -1,
        // `SLICE` pops `k` bounds and the target, pushes the slice: net `-k`.
        // `SET_SLICE` pops the bounds, the target and the value, and pushes the value
        // back for the pattern result, so the net is `-(k + 1)`.
        // `slice_bounds(flags)` counts the target plus the bounds, so the target is
        // subtracted separately here.
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
