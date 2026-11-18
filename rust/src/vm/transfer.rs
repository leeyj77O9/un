//! Translation of validated stack bytecode into register instructions.
//!
//! The translation is 1:1 ? one stack instruction becomes exactly one `Ins` ?
//! so jump targets, exception handlers and `Break`/`Skip` offsets carry over
//! unchanged (ip values are identical). A stack-depth analysis identical to
//! the verifier's assigns static depth slots; the loaded program carries the
//! table so the VM never tracks depth at runtime.
//!
//! Call arguments are *not* placed in registers at all: they are collected in
//! a per-frame value/name buffer by the `Arg*` instructions, which is what
//! allows dynamically-sized spreads. `Arg*` consume the argument values from
//! the top of the virtual stack (`depth -= 1` each), so all argument values
//! share one register per value at a time, freed immediately after movement.

use crate::bytecode::instr::*;
use crate::bytecode::{Program, Proto};
use crate::vm::{Ins, Plan, ProtoPlan};

pub fn compile(program: &Program) -> Result<Plan, String> {
    let mut protos = Vec::with_capacity(program.protos.len());
    for proto in &program.protos {
        protos.push(compile_proto(program, proto)?);
    }
    Ok(Plan { protos })
}

fn compile_proto(program: &Program, proto: &Proto) -> Result<ProtoPlan, String> {
    let depths = analyze(proto)?;
    let scratch_base = proto.locals.len();
    let max_depth = depths.iter().flatten().copied().max().unwrap_or(0).max(0) as usize;
    let reg_count = scratch_base + max_depth + 1;
    if reg_count > 256 {
        return Err(format!(
            "{}: function too complex: {} registers",
            proto_name(program, proto),
            reg_count
        ));
    }

    let mut code = Vec::with_capacity(proto.code.len());
    for ip in 0..proto.code.len() {
        let (op, flags, operand) = proto
            .instruction(ip)
            .ok_or_else(|| format!("{}: opcode {:#06x} at {ip} is not defined", proto_name(program, proto), proto.code[ip].op))?;
        let depth = depths[ip].unwrap_or(0);
        let s = |k: i64| if !(0..=255).contains(&k) { scratch_base } else { scratch_base + k as usize };
        let emit = |op, a, b, c, d, flags| Ins { op, a, b, c, d, flags };
        let ins = match op {
            Op::LoadNone
            | Op::LoadNull
            | Op::LoadTrue
            | Op::LoadFalse
            | Op::LoadConst
            | Op::LoadName
            | Op::LoadLocal => {
                emit(op, s(depth) as u8, 0, 0, operand, flags)
            }
            Op::StoreName | Op::StoreLocal => {
                emit(op, s(depth - 1) as u8, 0, 0, operand, flags)
            }
            Op::Dup => emit(op, s(depth) as u8, s(depth - 1) as u8, 0, 0, flags),
            Op::Pop => emit(op, s(depth - 1) as u8, 0, 0, 0, flags),
            Op::Swap => emit(op, s(depth - 2) as u8, s(depth - 1) as u8, 0, 0, flags),
            Op::Unpack => emit(op, s(depth - 1) as u8, 0, 0, operand, flags),
            Op::BuildList | Op::BuildSet | Op::BuildTuple | Op::BuildTupleNamed | Op::FString => {
                let n = operand as i64;
                emit(op, s(depth - n) as u8, 0, 0, operand, flags)
            }
            Op::BuildDict => {
                let n = operand as i64;
                emit(op, s(depth - 2 * n) as u8, 0, 0, operand, flags)
            }

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
            | Op::In
            | Op::Is
            | Op::IsNot
            | Op::Xor
            | Op::Eq
            | Op::Ne
            | Op::Lt
            | Op::Gt
            | Op::Le
            | Op::Ge
            | Op::GetItem => {
                emit(op, s(depth - 2) as u8, s(depth - 2) as u8, s(depth - 1) as u8, operand, flags)
            }
            Op::SetAttr => {
                let a = s(depth - 2);
                // [value][target] ? value deepest; the target (on top) is reused as the result slot
                emit(op, a as u8, s(depth - 1) as u8, a as u8, operand, flags)
            }
            Op::SetItem => {
                emit(op, s(depth - 3) as u8, s(depth - 2) as u8, s(depth - 1) as u8, operand, flags)
            }

            Op::FusedIntMulAdd => {
                // operand packs dst/x/y slots (10 bits each); result also lands at s(depth)
                emit(op, s(depth) as u8, 0, 0, operand, flags)
            }

            Op::Not | Op::Pos | Op::Neg | Op::Bnot | Op::Spread | Op::ToStr | Op::ToBool | Op::GetAttr => {
                emit(op, s(depth - 1) as u8, s(depth - 1) as u8, 0, operand, flags)
            }

            Op::Jmp => emit(op, 0, 0, 0, operand, flags),
            Op::JmpIf => emit(op, s(depth - 1) as u8, 0, 0, operand, flags),
            Op::PushTry | Op::PushTryProp => emit(op, 0, 0, 0, operand, flags),
            Op::Propagate => emit(op, 0, 0, 0, operand, flags),
            Op::PopTry => emit(op, 0, 0, 0, 0, flags),
            Op::Raise => emit(op, 0, 0, 0, 0, flags),

            Op::Slice => {
                // result replaces the consumed [target] at depth+effect-1
                let eff = crate::bytecode::stack_effect(op, flags, operand) as i64;
                let dst = scratch_base as i64 + depth as i64 + eff - 1;
                if dst < 0 || dst > 255 {
                    return Err(format!(
                        "{}: slice destination slot {dst} out of register range",
                        proto_name(program, proto)
                    ));
                }
                emit(op, dst as u8, dst as u8, 0, operand, flags)
            }
            Op::SetSlice => {
                // [value][target][bounds...]: value is re-pushed at depth+effect-1
                let eff = crate::bytecode::stack_effect(op, flags, operand) as i64;
                let dst = scratch_base as i64 + depth as i64 + eff - 1;
                if dst < 0 || dst > 255 {
                    return Err(format!(
                        "{}: slice destination slot {dst} out of register range",
                        proto_name(program, proto)
                    ));
                }
                let bb = slice_bounds(flags) - 1;
                emit(op, dst as u8, s(depth - 1 - bb) as u8, 0, operand, flags)
            }

            Op::ArgBegin => emit(op, 0, 0, 0, operand, flags),
            Op::ArgPos | Op::ArgKw | Op::ArgSpread | Op::ArgKwSpread => {
                emit(op, s(depth - 1) as u8, 0, 0, operand, flags)
            }
            Op::Call => {
                let argc = operand as i64;
                let dst = s(depth - 1 - argc);
                debug_assert!(dst <= s(depth - 1));
                emit(op, dst as u8, dst as u8, 0, operand, flags)
            }

            Op::NewScope | Op::PopScope => emit(op, 0, 0, 0, operand, flags),
            Op::PrepIter => emit(op, s(depth - 1) as u8, 0, 0, operand, flags),
            Op::ForNext => emit(op, s(depth) as u8, s(depth - 1) as u8, 0, operand, flags),
            Op::Break | Op::Skip => emit(op, 0, 0, 0, operand, flags),
            Op::Return => emit(op, s(depth - 1) as u8, 0, 0, operand, flags),

            Op::MakeFunc => {
                let defaults = (flags >> 8) as i64;
                // result replaces the consumed default values
                emit(op, s(depth - defaults) as u8, 0, 0, operand, flags)
            }
            Op::MakeClass => emit(op, s(depth) as u8, 0, 0, operand, flags),
            Op::RegisterClass => emit(op, s(depth - 1) as u8, 0, 0, operand, flags),
            Op::LoadBaseClass => emit(op, s(depth) as u8, 0, 0, operand, flags),
            Op::SetBases => {
                let count = operand as i64;
                // [created][base0..baseN] ? bases consumed in reverse; class obj stays
                emit(op, s(depth - 1 - count) as u8, 0, 0, operand, flags)
            }
            Op::AddMember | Op::AddMethod => {
                emit(op, s(depth - 2) as u8, s(depth - 1) as u8, 0, operand, flags)
            }
            Op::MakeProperty => {
                emit(op, s(depth - 3) as u8, 0, 0, operand, flags)
            }
            Op::MakeEnum => emit(op, s(depth) as u8, 0, 0, operand, flags),
            Op::MatchPattern => emit(op, s(depth) as u8, s(depth - 1) as u8, 0, operand, flags),
            Op::MakeFuture => emit(op, s(depth) as u8, 0, 0, operand, flags),
            Op::Await => emit(op, s(depth - 1) as u8, s(depth - 1) as u8, 0, operand, flags),
            Op::PushDefer => emit(op, 0, 0, 0, operand, flags),
            Op::PushUsing => emit(op, s(depth - 1) as u8, 0, 0, operand, flags),

            Op::Unwrap => emit(op, s(depth - 1) as u8, 0, 0, operand, flags),
            Op::Use => emit(op, 0, 0, 0, operand, flags),
            Op::Unsupported => emit(op, 0, 0, 0, operand, flags),

            _ => {
                return Err(format!(
                    "{}: opcode {op:?} is not supported by vm2",
                    proto_name(program, proto)
                ))
            }
        };
        code.push(ins);
    }
    // depth table for the runtime (one entry per ip, plus end)
    let depth_table: Vec<u8> = depths
        .iter()
        .map(|d| d.unwrap_or(0).max(0) as u8)
        .collect();
    Ok(ProtoPlan { scratch_base, reg_count, code, depths: depth_table })
}

/// Stack depth analysis ? the same fixpoint as `Program::validate`, but
/// returning the table instead of only checking consistency.
pub(crate) fn analyze(proto: &Proto) -> Result<Vec<Option<i64>>, String> {
    let mut depths: Vec<Option<i64>> = vec![None; proto.code.len() + 1];
    let mut work = vec![0usize];
    depths[0] = Some(0);
    run_worklist(&mut depths, &mut work, proto)?;
    // Exception handlers enter with the stack height recorded at the PushTry;
    // they are never reached by normal jumps, so seed them here with the
    // now-computed depth of each PushTry instruction. (Handlers are entered
    // with the stack truncated to this depth, so a mismatch is impossible.)
    for ip in 0..proto.code.len() {
        if let Some((op, _flags, operand)) = proto.instruction(ip) {
            if op == Op::PushTry || op == Op::PushTryProp {
                let handler = proto.exc[operand as usize].handler;
                if depths[handler].is_none() {
                    depths[handler] = Some(depths[ip].unwrap_or(0));
                    work.push(handler);
                }
            }
        }
    }
    run_worklist(&mut depths, &mut work, proto)?;
    Ok(depths)
}

fn run_worklist(
    depths: &mut Vec<Option<i64>>,
    work: &mut Vec<usize>,
    proto: &Proto,
) -> Result<(), String> {
    while let Some(ip) = work.pop() {
        let mut depth = depths[ip].expect("queued depth");
        let mut ip = ip;
        while ip < proto.code.len() {
            if let Some(seen) = depths[ip] {
                if seen != depth {
                    let dump = (744..=796)
                        .map(|i| format!("{i}:{:?}", depths[i].map(|d| d.to_string()).unwrap_or_else(|| "?".into())))
                        .collect::<Vec<_>>()
                        .join(" ");
                    return Err(format!("stack depth mismatch at ip {ip} ({seen} vs {depth}) dump: {dump}"));
                }
            } else {
                depths[ip] = Some(depth);
            }
            let (op, flags, operand) = proto
                .instruction(ip)
                .ok_or_else(|| format!("opcode {:#06x} at {ip} is not defined", proto.code[ip].op))?;
            let effect = crate::bytecode::stack_effect(op, flags, operand);
            let next_depth = depth + effect;
            if next_depth < 0 {
                return Err(format!("stack depth {next_depth} at ip {ip}"));
            }
            if matches!(op, Op::Jmp | Op::JmpIf | Op::ForNext | Op::Break | Op::Skip) {
                let target = ip as i64 + 1 + operand as i64;
                if target < 0 || target > proto.code.len() as i64 {
                    return Err(format!("jump target {target} out of range"));
                }
                let target = target as usize;
                let target_depth = if op == Op::ForNext { depth - 1 } else { next_depth };
                if target_depth < 0 {
                    return Err(format!("stack depth {target_depth} at ip {ip}"));
                }
                match depths[target] {
                    Some(seen) if seen != target_depth => {
                        return Err(format!("jump to {target} expects depth {seen}, reached depth {target_depth}"));
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
                depth = next_depth;
                ip += 1;
                continue;
            }
            if op == Op::Return || op == Op::Raise {
                break;
            }
            depth = next_depth;
            ip += 1;
        }
        if ip == proto.code.len() {
            match depths[ip] {
                Some(seen) if seen != depth => {
                    return Err(format!("stack depth mismatch at end ({seen} vs {depth})"));
                }
                None => depths[ip] = Some(depth),
                _ => {}
            }
        }
    }
    Ok(())
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

pub fn dump_register_plan(program: &Program) -> Result<String, String> {
    let mut out = String::new();
    let plan = compile(program)?;
    for (pi, p) in plan.protos.iter().enumerate() {
        let proto = &program.protos[pi];
        let params: Vec<String> = proto
            .params
            .iter()
            .map(|p| program.names.get(p.name).cloned().unwrap_or_else(|| "?".into()))
            .collect();
        let locals: Vec<String> = proto
            .locals
            .iter()
            .map(|&n| program.names.get(n).cloned().unwrap_or_else(|| "?".into()))
            .collect();
        out.push_str(&format!(
            "proto {pi} name {:?} scratch_base {} reg_count {} code {}\n  params: {:?}\n  locals raw: {:?} names arena: {:?}\n",
            program.names[program.protos[pi].name],
            p.scratch_base,
            p.reg_count,
            p.code.len(),
            params,
            proto.locals.clone(),
            proto
                .names
                .iter()
                .map(|&n| program.names.get(n).cloned().unwrap_or_else(|| "?".into()))
                .collect::<Vec<_>>(),
        ));
        let _ = locals;
        for (ip, ins) in p.code.iter().enumerate() {
            out.push_str(&format!(
                "{ip:4} {:?} a {} b {} c {} d {} flags {:06x}\n",
                ins.op, ins.a, ins.b, ins.c, ins.d, ins.flags
            ));
        }
    }
    Ok(out)
}

fn proto_name(program: &Program, proto: &Proto) -> String {
    program
        .names
        .get(proto.name)
        .cloned()
        .unwrap_or_else(|| "<proto>".to_string())
}