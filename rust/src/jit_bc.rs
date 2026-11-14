//! `.unbc` -> native translation for a strictly Int-compatible subset.
//!
//! Works directly on the validated stack bytecode that the register VM
//! executes (stack instructions lowered 1:1 by `vm/transfer.rs`). A function
//! proto whose body falls entirely inside the supported subset compiles to a
//! native function taking the same positional Int arguments as raw i64 and
//! returning i64; anything else returns `None` and the VM falls back to the
//! interpreter.
//!
//! Supported subset:
//!   - parameters of runtime Int type (max 8), essential or optional positional
//!   - integer consts (Int, Bool literals), `LoadName` of a parameter
//!   - integer `+ - *`; `/` produces a Float in Un, so it is excluded outright;
//!     `//` and `%` only with a non-zero integer constant divisor loaded into
//!     the right operand register by a preceding `LoadConst`
//!   - `< <= > >= == !=` producing Int 0/1 booleans
//!   - `if` / `while` via `JmpIf` (jump when the condition is falsy) and `Jmp`
//!   - `for i in iter.range(lo, hi)` loops over Int bounds
//!   - `FusedIntMulAdd` (the optimizer's accumulator shape)
//!   - `return expr`
//!
//! Rejected (the whole function routes to the interpreter): floats, strings,
//! objects, members, methods, calls other than `iter.range`, bitwise ops,
//! logical `and`/`or` when the `JmpIf` operand is not a comparison result,
//! division by a non-constant divisor, spreads, exceptions, enum/pattern
//! matching, `go`/`wait`, spread/varargs parameters.
//!
//! Compiled functions follow the Un fast-call ABI: positional Int arguments
//! are passed as raw i64 values, one i64 result is returned. The VM checks
//! all argument payloads are `Int` before using this path.

use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::{AbiParam, BlockArg, InstBuilder, UserFuncName, Value, types};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::ir::Block;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_native::builder as native_builder;

use crate::bytecode::Constant;
use crate::bytecode::Program;
use crate::bytecode::Proto;
use crate::bytecode::instr::*;
use crate::vm::{Ins, ProtoPlan};

pub const MAX_ARITY: usize = 8;

/// Translate the eligible protos of `program` into the JIT cache keyed by
/// `program_ptr`. Called once per program when the VM first plans it.
pub fn register_eligible(program: &Program, program_ptr: usize) {
    if !crate::jit_cache::jit_enabled() {
        return;
    }
    for proto_index in 0..program.protos.len() {
        if let Some((code_ptr, arity)) = try_compile_fn(program, proto_index) {
            let key = crate::jit_cache::unbc_key(program_ptr, proto_index);
            crate::jit_cache::register_jit_entry(
                &key,
                crate::jit_cache::JitEntry { ptr: code_ptr as *const u8, arity: arity as u32 },
            );
        }
    }
}

/// Translator state. All per-function data is gathered under one borrow.
struct Tr<'a> {
    program: &'a Program,
    proto: &'a Proto,
    plan: &'a ProtoPlan,
    arena: Vec<String>,
    param_pos: HashMap<String, usize>,
    patches: HashMap<usize, RangePatch>,
    arity: usize,
    locals_len: usize,
    // params [0..arity), locals [arity..arity+locals_len), registers after
    vars: Vec<Variable>,
    is_bool: Vec<bool>,
    is_none: Vec<bool>,
    local_is_bool: Vec<bool>,
    local_is_none: Vec<bool>,
    blocks: HashMap<usize, Block>,
    loop_inc: HashMap<usize, (Block, usize)>,
}

impl<'a> Tr<'a> {
    fn rv(&self, reg: usize) -> Variable {
        self.vars[self.arity + self.locals_len + reg]
    }

    fn lv(&self, slot: usize) -> Variable {
        self.vars[self.arity + slot]
    }

    fn emit_stream(&mut self, builder: &mut FunctionBuilder) -> Option<()> {
        let code = &self.plan.code;
        let mut ip = 0usize;
        let mut terminated = false;
        builder.switch_to_block(self.blocks[&0]);
        while ip < code.len() {
            let ins = code[ip];

            match self.blocks.get(&ip).copied() {
                Some(blk) => {
                    builder.switch_to_block(blk);
                    terminated = false;
                }
                None => {
                    if terminated {
                        // unreachable instruction after a Return — skip it
                        ip += 1;
                        continue;
                    }
                }
            }

            if let Some(patch) = self.patches.get(&ip).copied() {
                self.emit_range_for_next(builder, code, ip, patch)?;
                terminated = false;
                ip = patch.body_start_ip;
                continue;
            }

            if self.emit_ins(builder, code, ip, ins).is_none() {
                return None;
            }
            if matches!(ins.op, Op::Return | Op::Jmp | Op::JmpIf) {
                terminated = true;
            }
            ip += 1;
        }
        Some(())
    }

    fn emit_range_for_next(
        &mut self,
        builder: &mut FunctionBuilder,
        code: &[Ins],
        _ip: usize,
        patch: RangePatch,
    ) -> Option<()> {
        let (min_v, _end_min) = self.emit_bound_expr(builder, code, patch.min_start)?;
        let (stop_v, _end_max) = self.emit_bound_expr(builder, code, patch.argpos1 + 1)?;

        // The loop index lives in local slot `i_slot`. It is the only value the
        // range loop must thread through the control-flow merge. We make the
        // loop-carried value an explicit block parameter (BlockArg) rather than
        // relying on the variable-based SSA pick-up.
        let store = code.get(patch.fornext_ip + 1)?;
        if store.op != Op::StoreLocal || store.a != patch.yield_reg {
            return None;
        }
        let i_slot = store.d as usize;
        if i_slot >= self.locals_len {
            return None;
        }
        let i_var = self.lv(i_slot);

        let (inc_blk, _inc_i_var) = self.loop_inc.get(&patch.fornext_ip).copied()?;
        let header_blk = self.blocks.get(&patch.fornext_ip).copied()?;
        let body_blk = self.blocks.get(&patch.body_start_ip).copied()?;
        let exit_blk = self.blocks.get(&patch.exit_ip).copied()?;

        // blocks were pre-allocated without params; declare the param now
        let _param0 = builder.append_block_param(header_blk, types::I64);
        let _param1 = builder.append_block_param(body_blk, types::I64);
        let _param2 = builder.append_block_param(inc_blk, types::I64);

        // i = min initially; jump into header passing i
        builder.def_var(i_var, min_v);
        let i0 = builder.use_var(i_var);
        builder.ins().jump(header_blk, &[BlockArg::Value(i0)]);

        // header: i < stop ? body : exit
        builder.switch_to_block(header_blk);
        let i_v = builder.block_params(header_blk)[0];
        builder.def_var(i_var, i_v);
        let cmp = builder.ins().icmp(IntCC::SignedLessThan, i_v, stop_v);
        builder.ins().brif(cmp, body_blk, &[BlockArg::Value(i_v)], exit_blk, &[]);

        // increment block: i = i + 1; jumps back to header
        builder.switch_to_block(inc_blk);
        let i_v = builder.block_params(inc_blk)[0];
        builder.def_var(i_var, i_v);
        let one = builder.ins().iconst(types::I64, 1);
        let inc = builder.ins().iadd(i_v, one);
        builder.ins().jump(header_blk, &[BlockArg::Value(inc)]);

        // continue emission in the body block
        builder.switch_to_block(body_blk);
        let i_v = builder.block_params(body_blk)[0];
        builder.def_var(i_var, i_v);
        // yield register is read later by StoreLocal — define it as well
        let yield_var = self.rv(patch.yield_reg as usize);
        builder.def_var(yield_var, i_v);

        Some(())
    }

    fn emit_bound_expr(
        &mut self,
        builder: &mut FunctionBuilder,
        code: &[Ins],
        start_ip: usize,
    ) -> Option<(Value, usize)> {
        let mut stack: Vec<Value> = Vec::new();
        let mut i = start_ip;
        loop {
            let ins = code.get(i)?;
            match ins.op {
                Op::LoadConst => {
                    let cv = self
                        .proto
                        .consts
                        .get(ins.d as usize)
                        .and_then(|&ci| self.program.consts.get(ci).cloned());
                    match cv.as_ref()? {
                        Constant::Int(n) => {
                            stack.push(builder.ins().iconst(types::I64, *n));
                        }
                        _ => return None,
                    }
                }
                Op::LoadName => {
                    let name = self.arena.get(ins.d as usize)?.clone();
                    let &pos = self.param_pos.get(&name)?;
                    stack.push(builder.use_var(self.vars[pos]));
                }
                Op::LoadLocal => {
                    let slot = ins.d as usize;
                    if slot >= self.locals_len {
                        return None;
                    }
                    stack.push(builder.use_var(self.lv(slot)));
                }
                Op::Add | Op::Sub | Op::Mul => {
                    let rhs = stack.pop()?;
                    let lhs = stack.pop()?;
                    let v = match ins.op {
                        Op::Add => builder.ins().iadd(lhs, rhs),
                        Op::Sub => builder.ins().isub(lhs, rhs),
                        Op::Mul => builder.ins().imul(lhs, rhs),
                        _ => return None,
                    };
                    stack.push(v);
                }
                Op::Unwrap => {
                    // Unwrap just strips the error wrapper from a value that
                    // (in an int-eligible context) must be a plain Int — a no-op
                    // for Cranelift i64 values.
                }
                Op::ArgPos => {
                    return Some((stack.pop()?, i + 1));
                }
                _ => return None,
            }
            i += 1;
        }
    }

    fn emit_ins(
        &mut self,
        builder: &mut FunctionBuilder,
        code: &[Ins],
        ip: usize,
        ins: Ins,
    ) -> Option<()> {
        match ins.op {
            Op::LoadConst => {
                let cv = self
                    .proto
                    .consts
                    .get(ins.d as usize)
                    .and_then(|&ci| self.program.consts.get(ci).cloned());
                let (v, bool_lit) = match cv.as_ref() {
                    Some(Constant::Int(n)) => (builder.ins().iconst(types::I64, *n), false),
                    Some(Constant::Bool(b)) => {
                        (builder.ins().iconst(types::I64, if *b { 1 } else { 0 }), true)
                    }
                    _ => return None,
                };
                builder.def_var(self.rv(ins.a as usize), v);
                self.is_bool[ins.a as usize] = bool_lit;
                self.is_none[ins.a as usize] = false;
                Some(())
            }
            Op::LoadNone | Op::LoadNull => {
                // A none-derived i64 zero; the VM returns it only through paths
                // that never observe it. `Return` on a none-derived value bails.
                let v = builder.ins().iconst(types::I64, 0);
                builder.def_var(self.rv(ins.a as usize), v);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = true;
                Some(())
            }
            Op::LoadTrue | Op::LoadFalse => {
                let b = ins.op == Op::LoadTrue;
                let v = builder.ins().iconst(types::I64, if b { 1 } else { 0 });
                builder.def_var(self.rv(ins.a as usize), v);
                self.is_bool[ins.a as usize] = true;
                self.is_none[ins.a as usize] = false;
                Some(())
            }
            Op::Unwrap => {
                // Unwrap strips the error wrapper from an Un value. In the
                // Int-eligible subset every value is a plain i64 — a register move.
                let v = builder.use_var(self.rv(ins.a as usize));
                builder.def_var(self.rv(ins.a as usize), v);
                Some(())
            }
            Op::LoadName => {
                let name = self.arena.get(ins.d as usize)?.clone();
                match self.param_pos.get(&name) {
                    Some(&pos) => {
                        let v = builder.use_var(self.vars[pos]);
                        builder.def_var(self.rv(ins.a as usize), v);
                        self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                        Some(())
                    }
                    None => None,
                }
            }
            Op::Dup => {
                let v = builder.use_var(self.rv(ins.b as usize));
                builder.def_var(self.rv(ins.a as usize), v);
                self.is_bool[ins.a as usize] = self.is_bool[ins.b as usize];
                self.is_none[ins.a as usize] = self.is_none[ins.b as usize];
                Some(())
            }
            Op::Pop | Op::PopScope | Op::NewScope => {
                // scope/frame bookkeeping markers — no register effect in Int JIT
                Some(())
            }
            Op::StoreLocal => {
                let slot = ins.d as usize;
                if slot >= self.locals_len {
                    return None;
                }
                let v = builder.use_var(self.rv(ins.a as usize));
                builder.def_var(self.lv(slot), v);
                self.local_is_bool[slot] = self.is_bool[ins.a as usize];
                self.local_is_none[slot] = self.is_none[ins.a as usize];
                Some(())
            }
            Op::LoadLocal => {
                let slot = ins.d as usize;
                if slot >= self.locals_len {
                    return None;
                }
                let v = builder.use_var(self.lv(slot));
                builder.def_var(self.rv(ins.a as usize), v);
                self.is_bool[ins.a as usize] = self.local_is_bool[slot];
                self.is_none[ins.a as usize] = self.local_is_none[slot];
                Some(())
            }
            Op::Add | Op::Sub | Op::Mul => {
                let lhs = builder.use_var(self.rv(ins.b as usize));
                let rhs = builder.use_var(self.rv(ins.c as usize));
                let v = match ins.op {
                    Op::Add => builder.ins().iadd(lhs, rhs),
                    Op::Sub => builder.ins().isub(lhs, rhs),
                    Op::Mul => builder.ins().imul(lhs, rhs),
                    _ => return None,
                };
                builder.def_var(self.rv(ins.a as usize), v);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                Some(())
            }
            Op::Idiv | Op::Mod => {
                let divisor = match const_load_int_at(code, ip, ins.c as usize, self.proto, self.program) {
                    Some(n) if n != 0 => n,
                    _ => return None,
                };
                let lhs = builder.use_var(self.rv(ins.b as usize));
                let rhs = builder.ins().iconst(types::I64, divisor);
                let v = if ins.op == Op::Idiv {
                    builder.ins().sdiv(lhs, rhs)
                } else {
                    builder.ins().srem(lhs, rhs)
                };
                builder.def_var(self.rv(ins.a as usize), v);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                Some(())
            }
            Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Eq | Op::Ne => {
                let lhs = builder.use_var(self.rv(ins.b as usize));
                let rhs = builder.use_var(self.rv(ins.c as usize));
                let cc = match ins.op {
                    Op::Lt => IntCC::SignedLessThan,
                    Op::Le => IntCC::SignedLessThanOrEqual,
                    Op::Gt => IntCC::SignedGreaterThan,
                    Op::Ge => IntCC::SignedGreaterThanOrEqual,
                    Op::Eq => IntCC::Equal,
                    Op::Ne => IntCC::NotEqual,
                    _ => return None,
                };
                let cmp = builder.ins().icmp(cc, lhs, rhs);
                let one = builder.ins().iconst(types::I64, 1);
                let zero_v = builder.ins().iconst(types::I64, 0);
                let v = builder.ins().select(cmp, one, zero_v);
                builder.def_var(self.rv(ins.a as usize), v);
                self.is_bool[ins.a as usize] = true;
                self.is_none[ins.a as usize] = false;
                Some(())
            }
            Op::FusedIntMulAdd => {
                let dst = ((ins.d >> 20) & 0x3FF) as usize;
                let x = ((ins.d >> 10) & 0x3FF) as usize;
                let y = (ins.d & 0x3FF) as usize;
                if dst >= self.locals_len || x >= self.locals_len || y >= self.locals_len {
                    return None;
                }
                let mut acc = builder.use_var(self.lv(dst));
                let xv = builder.use_var(self.lv(x));
                let yv = builder.use_var(self.lv(y));
                let p = builder.ins().imul(xv, yv);
                acc = builder.ins().iadd(acc, p);
                builder.def_var(self.lv(dst), acc);
                builder.def_var(self.rv(ins.a as usize), acc);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.local_is_bool[dst] = false;
                self.local_is_none[dst] = false;
                Some(())
            }
            Op::Jmp => {
                let target = jump_target(ip, ins.d);
                // loop back-edges route through the increment block
                if let Some(&(inc_blk, i_slot)) = self.loop_inc.get(&target) {
                    let i_v = builder.use_var(self.lv(i_slot));
                    builder.ins().jump(inc_blk, &[BlockArg::Value(i_v)]);
                    return Some(());
                }
                let blk = match self.blocks.get(&target).copied() {
                    Some(b) => b,
                    None => return None,
                };
                builder.ins().jump(blk, &[]);
                Some(())
            }
            Op::JmpIf => {
                let target = jump_target(ip, ins.d);
                if !self.is_bool[ins.a as usize] {
                    return None;
                }
                let v = builder.use_var(self.rv(ins.a as usize));
                let zero_v = builder.ins().iconst(types::I64, 0);
                let cond = builder.ins().icmp(IntCC::NotEqual, v, zero_v);
                let then_blk = self.blocks.get(&target).copied()?;
                let else_blk = self.blocks.get(&(ip + 1)).copied()?;
                let jump_on_true = (ins.flags & FLAG_JUMP_TRUE) != 0;
                if jump_on_true {
                    builder.ins().brif(cond, then_blk, &[], else_blk, &[]);
                } else {
                    builder.ins().brif(cond, else_blk, &[], then_blk, &[]);
                }
                Some(())
            }
            Op::Return => {
                if self.is_none[ins.a as usize] {
                    // interpreter would return `none`; we cannot in an i64 ABI
                    return None;
                }
                let v = builder.use_var(self.rv(ins.a as usize));
                builder.ins().return_(&[v]);
                Some(())
            }
            _ => None,
        }
    }
}

fn jump_target(ip: usize, delta: i32) -> usize {
    (ip as i64 + 1 + delta as i64) as usize
}

fn const_load_int_at(
    code: &[Ins],
    ip: usize,
    reg: usize,
    proto: &Proto,
    program: &Program,
) -> Option<i64> {
    let mut i = ip;
    let mut back = 0usize;
    while i > 0 && back < 32 {
        i -= 1;
        back += 1;
        let ins = code[i];
        match ins.op {
            Op::Jmp | Op::JmpIf | Op::ForNext | Op::Return | Op::Break | Op::Skip => return None,
            Op::LoadConst => {
                if ins.a as usize == reg {
                    let cv = proto
                        .consts
                        .get(ins.d as usize)
                        .and_then(|&ci| program.consts.get(ci).cloned());
                    return match cv.as_ref()? {
                        Constant::Int(n) => Some(*n),
                        _ => None,
                    };
                }
            }
            _ => {}
        }
    }
    None
}

// ============================================================================
// range(lo, hi) peephole
// ============================================================================

#[derive(Clone, Copy)]
struct RangePatch {
    body_start_ip: usize,
    fornext_ip: usize,
    yield_reg: u8,
    exit_ip: usize,
    min_start: usize,
    argpos1: usize,
}

fn match_range_for_next(code: &[Ins], arena: &[String], ip: usize) -> Option<RangePatch> {
    let l = code.get(ip)?;
    if l.op != Op::LoadName || arena.get(l.d as usize)?.as_str() != "iter" {
        return None;
    }
    let g = code.get(ip + 1)?;
    if g.op != Op::GetAttr || g.a != l.a || g.b != l.a || arena.get(g.d as usize)?.as_str() != "range" {
        return None;
    }
    let u = code.get(ip + 2)?;
    if u.op != Op::Unwrap || u.a != l.a {
        return None;
    }
    let ab = code.get(ip + 3)?;
    if ab.op != Op::ArgBegin || ab.d != 2 {
        return None;
    }
    let a1 = find_argpos(code, ip + 4)?;
    let a2 = find_argpos(code, a1 + 1)?;
    let call = code.get(a2 + 1)?;
    if call.op != Op::Call || call.d != 2 {
        return None;
    }
    let prep = code.get(a2 + 2)?;
    if prep.op != Op::PrepIter || prep.a != call.a {
        return None;
    }
    let mut pos = a2 + 3;
    if code.get(pos).map(|i| i.op) == Some(Op::NewScope) {
        pos += 1;
    }
    let fx = code.get(pos)?;
    if fx.op != Op::ForNext || fx.b as usize != prep.a as usize {
        return None;
    }
    Some(RangePatch {
        body_start_ip: pos + 1,
        fornext_ip: pos,
        yield_reg: fx.a,
        exit_ip: jump_target(pos, fx.d),
        min_start: ip + 4,
        argpos1: a1,
    })
}

fn find_argpos(code: &[Ins], start: usize) -> Option<usize> {
    let mut i = start;
    while i < code.len() {
        match code[i].op {
            Op::ArgPos => return Some(i),
            Op::ArgBegin | Op::Jmp | Op::JmpIf | Op::ForNext | Op::Return | Op::Break | Op::Skip => {
                return None;
            }
            _ => i += 1,
        }
    }
    None
}

/// Translate a compiled function proto. Returns the raw code pointer and
/// arity, or `None` outside the supported subset.
pub fn try_compile_fn(program: &Program, proto_index: usize) -> Option<(usize, usize)> {
    let proto = program.protos.get(proto_index)?;
    if proto.flags & PROTO_FUNCTION == 0 {
        return None;
    }
    let arity = proto.params.len();
    if arity == 0 || arity > MAX_ARITY {
        return None;
    }
    const ALLOWED: u16 = PARAM_ESSENTIAL | PARAM_OPTIONAL;
    if proto.params.iter().any(|p| p.flags & !ALLOWED != 0) {
        return None;
    }

    let plan = crate::vm::transfer::compile(program).ok()?;
    let plan = &plan.protos[proto_index];
    let code = &plan.code;

    let arena: Vec<String> = proto
        .names
        .iter()
        .map(|&n| program.names.get(n).cloned().unwrap_or_else(|| "?".into()))
        .collect();
    let param_pos: HashMap<String, usize> = proto
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| (program.names[p.name].clone(), i))
        .collect();

    // ---------- pass 1: collect jump targets ----------
    let mut leaders: HashSet<usize> = HashSet::new();
    leaders.insert(0);
    for (ip, ins) in code.iter().enumerate() {
        match ins.op {
            Op::Jmp | Op::JmpIf | Op::Break | Op::Skip => {
                leaders.insert(jump_target(ip, ins.d));
                if matches!(ins.op, Op::JmpIf) {
                    leaders.insert(ip + 1);
                }
            }
            Op::ForNext => {
                leaders.insert(ip + 1);
                leaders.insert(jump_target(ip, ins.d));
            }
            _ => {}
        }
    }

    // ---------- pass 2: match range-loop peepholes ----------
    let mut patches: HashMap<usize, RangePatch> = HashMap::new();
    for ip in 0..code.len() {
        if let Some(p) = match_range_for_next(code, &arena, ip) {
            leaders.insert(p.body_start_ip);
            leaders.insert(p.exit_ip);
            patches.insert(ip, p);
        }
    }

    // ---------- pass 3: build the Cranelift context ----------
    let native = native_builder().ok()?;
    let mut flag_builder = settings::builder();
    let _ = flag_builder.set("is_pic", "false");
    let isa = native
        .finish(settings::Flags::new(flag_builder))
        .ok()?;
    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let mut fun_ctx = module.make_context();
    let mut func_ctx = FunctionBuilderContext::new();

    let mut sig = module.make_signature();
    for _ in 0..arity {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));

    let func_id = module.declare_function("unbc_fn", Linkage::Export, &sig).ok()?;
    fun_ctx.func.signature = sig;
    fun_ctx.func.name = UserFuncName::user(0, func_id.as_u32());

    let mut builder = FunctionBuilder::new(&mut fun_ctx.func, &mut func_ctx);
    let locals_len = proto.locals.len();
    let mut vars: Vec<Variable> = Vec::with_capacity(arity + locals_len + plan.reg_count);
    for _ in 0..arity {
        vars.push(builder.declare_var(types::I64));
    }
    for _ in 0..locals_len {
        vars.push(builder.declare_var(types::I64));
    }
    for _ in 0..plan.reg_count {
        vars.push(builder.declare_var(types::I64));
    }

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    let incoming = builder.block_params(entry).to_vec();
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    for (i, v) in incoming.iter().enumerate() {
        builder.def_var(vars[i], *v);
    }
    let zero = builder.ins().iconst(types::I64, 0);
    for i in arity..vars.len() {
        builder.def_var(vars[i], zero);
    }

    let mut blocks: HashMap<usize, Block> = HashMap::new();
    blocks.insert(0, entry); // ip 0 is the entry block (params/zeros live there)
    for &ip in &leaders {
        if ip < code.len() && ip != 0 {
            blocks.insert(ip, builder.create_block());
        }
    }
    let mut loop_inc: HashMap<usize, (Block, usize)> = HashMap::new();
    for patch in patches.values() {
        if let Some(s) = code.get(patch.fornext_ip + 1) {
            if s.op == Op::StoreLocal && s.a == patch.yield_reg {
                loop_inc.insert(patch.fornext_ip, (builder.create_block(), s.d as usize));
            }
        }
    }

    let mut tri = Tr {
        program,
        proto,
        plan,
        arena,
        param_pos,
        patches,
        arity,
        locals_len,
        vars,
        is_bool: vec![false; plan.reg_count],
        is_none: vec![false; plan.reg_count],
        local_is_bool: vec![false; locals_len],
        local_is_none: vec![false; locals_len],
        blocks,
        loop_inc,
    };
    if tri.emit_stream(&mut builder).is_none() {
        return None;
    }
    builder.seal_all_blocks();
    builder.finalize(module.target_config());

    if let Err(e) = module.define_function(func_id, &mut fun_ctx) {
        let _ = e;
        return None;
    }
    module.finalize_definitions().ok()?;
    let code_ptr = module.get_finalized_function(func_id) as *const u8 as usize;
    Some((code_ptr, arity))
}















