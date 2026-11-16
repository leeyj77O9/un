//! `.unbc` -> native translation for functions that stay inside the typed
//! numeric subset.
//!
//! Works directly on the validated stack bytecode that the register VM
//! executes (stack instructions lowered 1:1 by `vm/transfer.rs`). A function
//! proto whose body fits the subset compiles to a native function taking the
//! same positional Int arguments and returning `(i64 value, i64 tag)`:
//! `tag == 0` means `value` holds an Int result exactly as before, `tag == 1`
//! means `value` holds the raw IEEE-754 bits of a Float result. Anything the
//! translator cannot prove safe returns `None` and the VM falls back to the
//! interpreter.
//!
//! Supported subset (all values typed):
//!   - parameters of runtime Int type (max 8), essential or optional positional
//!   - integer consts (Int, Bool literals), `LoadName` of a parameter
//!   - integer consts (Int, Bool literals), `Constant::Float` consts
//!   - integer `+ - *`; Float variants are emitted as `fadd`/`fsub`/`fmul`
//!     when any operand is Float (Un promotes Int+Float to Float)
//!   - `//` and `%` only with a non-zero integer constant divisor loaded into
//!     the right operand register by a preceding `LoadConst`
//!   - Float intrinsics: `sqrt`/`floor` recognized inline for the exact call
//!     sequence `math.floor(math.sqrt(expr))` where `expr` is an Int argument
//!   - `< <= > >= == !=` producing Int 0/1 booleans for Int operands; Float
//!     comparisons are rejected
//!   - `if` / `while` via `JmpIf` (jump when the condition is falsy) and `Jmp`
//!   - `for i in iter.range(lo, hi)` loops over Int bounds
//!   - `FusedIntMulAdd` (the optimizer's accumulator shape)
//!   - `return expr`, either Int, or Int/Float promoted locals with per-path
//!     tagging
//!
//! Rejected (the whole function routes to the interpreter): user function
//! calls, string/object/list ops, spread/varargs, exceptions, enums, pattern
//! matching, `go`/`wait`, mutable global reads, mixed-type returns that are
//! not routed through a promoted local.
//!
//! Compiled functions follow the Un fast-call ABI: positional Int arguments
//! are passed as raw i64 values; results are returned as tagged raw i64
//! values (see above). The VM checks all argument payloads are `Int` before
//! using this path.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use cranelift_codegen::ir::{AbiParam, BlockArg, InstBuilder, Value, types};
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
/// Registry entry for the pre-JIT compile guard: disables the entire
/// program's JIT path when a global binding is re-bound.
/// In a hostile module, names used by the JIT translation (`math`/`floor`/
/// `sqrt`/`iter`/`range`) could be re-bound (`math.sqrt = ??), which a
/// name-text-based intrinsic decision made once at compile time could never
/// reconcile. Any such rebind disqualifies the entire program from the
/// JIT (exact interpreter semantics are preserved).
fn program_has_intrinsic_rebind(program: &Program) -> bool {
    for proto in &program.protos {
        for ins in &proto.code {
            let Some(op) = ins.op_code() else { continue };
            if ins.operand < 0 {
                continue;
            }
            if op != Op::StoreName && op != Op::SetAttr && op != Op::MakeFunc {
                continue;
            }
            if op == Op::MakeFunc {
                // `fn math = ??-style definitions with FUNC_BIND (and the
                // decorated-export seed). Would clobber cross-function
                // inlining of the same name; block the risky programs.
                if ins.flags & FLAG_FUNC_BIND != 0 {
                    let slot_of = ins.operand as usize;
                    // operand is the proto-slot index, not name index
                    if let Some(fn_proto) = program.protos.get(slot_of) {
                        if let Some(pname) = program.names.get(fn_proto.name) {
                            if matches!(pname.as_str(), "math" | "floor" | "sqrt" | "iter" | "range") {
                                return true;
                            }
                        }
                    }
                }
                continue;
            }
            let slot = ins.operand as usize;
            let name = match proto
                .names
                .get(slot)
                .and_then(|&n| program.names.get(n))
            {
                Some(s) => s.as_str(),
                None => continue,
            };
            if matches!(name, "math" | "floor" | "sqrt" | "iter" | "range") {
                return true;
            }
        }
    }
    false
}

pub fn register_eligible(program: &Program, program_ptr: usize) {
    if !crate::jit_cache::jit_enabled() {
        return;
    }
    // Hard safety net: any panic inside the Cranelift compile/declare/
    // finalize pipeline (a rare frontend mismatch seen on some launches ??
    // `declared type of variable N doesn't match type of value M`) must not
    // kill the process. Skip all JIT registration instead; the whole program
    // falls back to the interpreter, byte-identical to UV_JIT=0.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        register_eligible_inner(program, program_ptr)
    }));
}
fn register_eligible_inner(program: &Program, program_ptr: usize) {
    if program_has_intrinsic_rebind(program) {
        return; // interpreter only ??hostile module binding
    }
    // Program-wide return-kind pass ??used by user-call emission so that
    // the outer function's register gets marked Int/Float/Bool correctly.
    let ret_kinds = analyze_return_kinds(program);

    let candidates: Vec<usize> = (0..program.protos.len())
        .filter(|i| precheck_fn(program, *i).is_some())
        .collect();
    if candidates.is_empty() {
        return;
    }
    let native = match native_builder() {
        Ok(n) => n,
        Err(_) => return,
    };
    let mut flag_builder = settings::builder();
    let _ = flag_builder.set("is_pic", "false");
    let isa = match native.finish(settings::Flags::new(flag_builder)) {
        Ok(isa) => isa,
        Err(_) => return,
    };
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    builder.symbol("un_jit_depth_enter", crate::jit_cache::un_jit_depth_enter as *const u8);
    builder.symbol("un_jit_depth_exit", crate::jit_cache::un_jit_depth_exit as *const u8);
    builder.symbol("un_math_sin", crate::native::math::un_math_sin as *const u8);
    builder.symbol("un_dict_int_empty", crate::runtime::int_dict::un_dict_int_empty as *const u8);
    builder.symbol("un_dict_int_set", crate::runtime::int_dict::un_dict_int_set as *const u8);
    builder.symbol("un_dict_int_get", crate::runtime::int_dict::un_dict_int_get as *const u8);
    builder.symbol("un_instance_int_get", crate::runtime::obj::un_instance_int_get as *const u8);
    builder.symbol("un_instance_int_set", crate::runtime::obj::un_instance_int_set as *const u8);
    let mut module = JITModule::new(builder);

    // Declare the depth helpers so the JIT can call into them.
    let mut sig_enter = module.make_signature();
    sig_enter.returns.push(AbiParam::new(types::I64));
    let sig_exit = module.make_signature(); // void returns
    let fid_enter = match module
        .declare_function("un_jit_depth_enter", Linkage::Import, &sig_enter)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let fid_exit = match module
        .declare_function("un_jit_depth_exit", Linkage::Import, &sig_exit)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    // Int-dict helpers ??exported so Cranelift can `call` them.
    let mut sig_empty = module.make_signature();
    sig_empty.returns.push(AbiParam::new(types::I64));
    let fid_dict_empty = match module
        .declare_function("un_dict_int_empty", Linkage::Import, &sig_empty)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_set = module.make_signature();
    sig_set.params.push(AbiParam::new(types::I64));
    sig_set.params.push(AbiParam::new(types::I64));
    sig_set.params.push(AbiParam::new(types::I64));
    sig_set.returns.push(AbiParam::new(types::I64));
    let fid_dict_set = match module
        .declare_function("un_dict_int_set", Linkage::Import, &sig_set)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_get = module.make_signature();
    sig_get.params.push(AbiParam::new(types::I64));
    sig_get.params.push(AbiParam::new(types::I64));
    sig_get.returns.push(AbiParam::new(types::I64));
    let fid_dict_get = match module
        .declare_function("un_dict_int_get", Linkage::Import, &sig_get)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_sin = module.make_signature();
    sig_sin.params.push(AbiParam::new(types::F64));
    sig_sin.returns.push(AbiParam::new(types::F64));
    let fid_sin = match module.declare_function("un_math_sin", Linkage::Import, &sig_sin) {
        Ok(fid) => fid,
        Err(_) => return,
    };
    // Int-field instance access helpers (self pointer + field slot index).
    let mut sig_ig = module.make_signature();
    sig_ig.params.push(AbiParam::new(types::I64));
    sig_ig.params.push(AbiParam::new(types::I64));
    sig_ig.returns.push(AbiParam::new(types::I64));
    let fid_int_get = match module.declare_function("un_instance_int_get", Linkage::Import, &sig_ig) {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let fid_int_ok = match module.declare_function("un_instance_int_ok", Linkage::Import, &sig_ig) {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_is = module.make_signature();
    sig_is.params.push(AbiParam::new(types::I64));
    sig_is.params.push(AbiParam::new(types::I64));
    sig_is.params.push(AbiParam::new(types::I64));
    let fid_int_set = match module.declare_function("un_instance_int_set", Linkage::Import, &sig_is) {
        Ok(fid) => fid,
        Err(_) => return,
    };

    // Pass 1: declare every candidate so they are all callable from each other
    let mut decls: HashMap<usize, cranelift_module::FuncId> = HashMap::new();
    let mut sigs: HashMap<usize, cranelift_codegen::ir::Signature> = HashMap::new();
    for &idx in &candidates {
        if crate::jit_cache::jit_debug() {
            let n = program.names.get(program.protos[idx].name).cloned().unwrap_or_else(|| "?".into());
            eprintln!("JITDBG cand[{idx}] {n}");
        }
        let arity = match precheck_fn(program, idx) {
            Some(a) => a,
            None => continue,
        };
        let mut sig = module.make_signature();
        // Hidden result pointer: the JIT writes the (value, tag) pair through it.
        sig.params.push(AbiParam::new(types::I64));
        for _ in 0..arity {
            sig.params.push(AbiParam::new(types::I64));
        }
        sigs.insert(idx, sig);
        match module.declare_function(
            &format!("unbc_fn_{idx}"),
            Linkage::Export,
            sigs.get(&idx).unwrap(),
        ) {
            Ok(func_id) => {
                decls.insert(idx, func_id);
            }
            Err(_) => return,
        }
    }
    if decls.is_empty() {
        return;
    }
    // Pass 2: compile every candidate in the shared module. If a function
    // fails to compile, replace it with a bail stub (tag = 2) ??the rest of
    // the module stays usable, and ineligible calls fall back to the
    // interpreter exactly once per call.
    let verbose = crate::jit_cache::jit_debug();
    for &idx in &candidates {
        let func_id = match decls.get(&idx) {
            Some(fid) => *fid,
            None => continue,
        };
        if emit_fn_in_module(
            program,
            idx,
            &mut module,
            &decls,
            sigs.get(&idx).unwrap().clone(),
            func_id,
            fid_enter,
            fid_exit,
            fid_dict_empty,
            fid_dict_set,
            fid_dict_get,
            fid_sin,
            fid_int_get,
            fid_int_ok,
            fid_int_set,
            &ret_kinds,
        )
        .is_none()
        {
            if verbose {
                let name = program.names.get(program.protos[idx].name).cloned().unwrap_or_default();
                eprintln!("JITDBG STUB proto[{idx}] {name} (fallback stub ??will tag=2 bail)");
            }
            emit_fallback_stub_in_module(
                &mut module,
                sigs.get(&idx).unwrap().clone(),
                func_id,
            );
        } else if verbose {
            let name = program.names.get(program.protos[idx].name).cloned().unwrap_or_default();
            eprintln!("JITDBG OK    proto[{idx}] {name}");
        }
    }
    if module.finalize_definitions().is_err() {
        return;
    }
    for &idx in &candidates {
        let func_id = decls[&idx];
        let code_ptr = module.get_finalized_function(func_id) as *const u8 as usize;
        let arity = match precheck_fn(program, idx) {
            Some(a) => a,
            None => continue,
        };
        let key = crate::jit_cache::unbc_key(program_ptr, idx);
        crate::jit_cache::register_jit_entry(
            &key,
            crate::jit_cache::JitEntry { ptr: code_ptr as *const u8, arity: arity as u32 },
        );
    }
}

/// Translator state. All per-function data is gathered under one borrow.
struct Tr<'a> {
    program: &'a Program,
    proto: &'a Proto,
    proto_index: usize,
    plan: &'a ProtoPlan,
    arena: Vec<String>,
    param_pos: HashMap<String, usize>,
    patches: HashMap<usize, RangePatch>,
    intrinsics: HashMap<usize, UnaryFloatPatch>,
    skip_ips: HashSet<usize>,
    arity: usize,
    locals_len: usize,
    // params [0..arity), locals [arity..arity+locals_len), registers after
    vars: Vec<Variable>,
    f64_vars: Vec<Variable>,
    is_bool: Vec<bool>,
    is_none: Vec<bool>,
    is_float: Vec<bool>,
    local_is_bool: Vec<bool>,
    local_is_none: Vec<bool>,
    local_float: Vec<bool>,
    // register -> the local slot it was loaded from (used to pick the
    // Int/Float-move for promoted locals at Ren)
    ret_slot: Vec<Option<usize>>,
    blocks: HashMap<usize, Block>,
    loop_inc: HashMap<usize, (Block, usize)>,
    // i64 0/1; set to 1 by StoreLocal of a Float value. Threaded by cranelift
    // as a regular SSA value.
    poly_tag: Variable,
    hp_var: Variable,
    // Module-level JIT function identifiers for the protos of this program,
    // keyed by proto index. Used to emit direct calls for user functions.
    decls: HashMap<usize, cranelift_module::FuncId>,
    // Cranelift entrypoint FuncId for the Rust-side depth exit helper.
    fid_exit: cranelift_module::FuncId,
    // True if the body contains a user-call (other than intrinsic-matches),
    // implying potential infinite recursion through the JIT. Controls
    // whether depth tracking happens on entry/exit.
    needs_depth: bool,
    // Return-kind tag bitmask computed once per program. Used by `Call` to
    // wire the outer register's kind (Int/Float/Bool) without going through
    // the interpreter.
    ret_kinds: HashMap<usize, u8>,
    // Int-dict helper FuncIds imported into the module for intrinsic use.
    fid_dict_empty: cranelift_module::FuncId,
    fid_dict_set: cranelift_module::FuncId,
    fid_dict_get: cranelift_module::FuncId,
    fid_sin: cranelift_module::FuncId,
    fid_int_get: cranelift_module::FuncId,
    fid_int_ok: cranelift_module::FuncId,
    fid_int_set: cranelift_module::FuncId,
    class_layouts: HashMap<usize, Vec<String>>,
    // Slots that hold IntDict references (opaque i64 ids used by the JIT).
    int_dict_regs: HashSet<usize>,
    local_is_intdict: Vec<bool>,
    // Proto indices by global function name (exact match against arena).
    callee_lookup: HashMap<String, usize>,
    // Register -> proto index for registers holding a callable function
    // object, tracked from LoadName emissions.
    call_candidate_regs: HashMap<usize, usize>,
    module: &'a mut JITModule,
}

impl<'a> Tr<'a> {
    fn emit_user_call(
        &mut self,
        builder: &mut FunctionBuilder,
        ins: Ins,
        argc: usize,
        callee_reg: usize,
        proto_index: usize,
    ) -> Option<()> {
        // All args must be number-like Ints and not none/bool (the Int JIT
        // ABI does not carry them). Bool == Int 1/0 in our Cranelift IR, so
        // Bool would *seem* safe; reject anything unknown anyway.
        if self.is_none[callee_reg] || self.is_float[callee_reg] || self.is_bool[callee_reg] {
            return None;
        }
        for k in 0..argc {
            let r = callee_reg + 1 + k;
            if self.is_none[r] || self.is_float[r] || self.is_bool[r] {
                return None;
            }
        }
        if argc > MAX_ARITY {
            return None;
        }
        let hp = builder.use_var(self.hp_var);
        let mut arg_vals: Vec<Value> = Vec::with_capacity(argc + 1);
        arg_vals.push(hp);
        for k in 0..argc {
            let r = callee_reg + 1 + k;
            arg_vals.push(builder.use_var(self.rv_i64(r)));
        }
        let func_id = *self.decls.get(&proto_index)?;
        let callee_kind = self.ret_kinds.get(&proto_index).copied().unwrap_or(RET_INT);
        let func_ref = self.module.declare_func_in_func(func_id, builder.func);
        builder.ins().call(func_ref, &arg_vals);
        let pure_kind = callee_kind & (RET_INT | RET_FLOAT | RET_BOOL);
        let has_bool = (pure_kind & RET_BOOL) != 0;
        let has_numeric = (pure_kind & (RET_INT | RET_FLOAT)) != 0;
        if has_bool && has_numeric {
            // Mix of Bool-with-Int paths ??outer can't know statically
            // which is it. Refuse JIT for the outer to remain correct.
            return None;
        }
        if pure_kind == 0 {
            return None;
        }
        // Load (value, tag) out of the shared hidden slot
        let val = builder.ins().load(types::I64, cranelift_codegen::ir::MemFlagsData::new(), hp, 0);
        let tag = builder.ins().load(types::I64, cranelift_codegen::ir::MemFlagsData::new(), hp, 8);
        // Tag values:
        //   0: Int | 1: Float | 2: bail | 3: Bool
        // Callee may only produce one of these for this call site ??the
        // outer kind was checked above. Reject others.
        let two_v = builder.ins().iconst(types::I64, 2);
        let is_bail = builder.ins().icmp(IntCC::Equal, tag, two_v);
        let bail_blk = builder.create_block();
        let cont_blk = builder.create_block();
        builder.ins().brif(is_bail, bail_blk, &[], cont_blk, &[]);
        builder.seal_block(bail_blk);
        builder.switch_to_block(bail_blk);
        let zero = builder.ins().iconst(types::I64, 0);
        self.emit_return_pair(builder, zero, two_v);
        builder.switch_to_block(cont_blk);
        builder.seal_block(cont_blk);
        builder.def_var(self.rv_i64(ins.a as usize), val);
        self.ret_slot[ins.a as usize] = None;
        match pure_kind {
            RET_INT => {
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                Some(())
            }
            RET_FLOAT => {
                // Int slot contains Float bits ??downstream already treats
                // the register as Float-typed when is_float=true elsewhere.
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = true;
                Some(())
            }
            k if k == RET_INT | RET_FLOAT => {
                // Heterogeneous Int/Float path ??either is legal, the
                // runtime tag check differentiates callers. We accept the
                // call site but can't pick is_float/is_bool statically,
                // so keep Int and let the final Return's tag selection
                // handle both paths correctly (as with `total`).
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                Some(())
            }
            RET_BOOL => {
                self.is_bool[ins.a as usize] = true;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                Some(())
            }
            // Mixed Bool 횞 Int/Float ??rejected by has_bool && has_numeric
            // check above. This branch is only reachable if the early return
            // misfired; keep it as None for safety.
            _ => None,
        }
    }

    fn emit_return_pair(
        &mut self,
        builder: &mut FunctionBuilder,
        bits: cranelift_codegen::ir::Value,
        tag: cranelift_codegen::ir::Value,
    ) {
        if self.needs_depth {
            let exit_ref = self.module.declare_func_in_func(
                self.fid_exit,
                builder.func,
            );
            builder.ins().call(exit_ref, &[]);
        }
        let hp = builder.use_var(self.hp_var);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), bits, hp, 0);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), tag, hp, 8);
        builder.ins().return_(&[]);
    }
    fn rv(&self, reg: usize) -> Variable {
        let idx = self.arity + self.locals_len + reg;
        if self.is_float[reg] {
            self.f64_vars[idx]
        } else {
            self.vars[idx]
        }
    }
    fn lv(&self, slot: usize) -> Variable {
        let idx = self.arity + slot;
        if self.local_float[slot] {
            self.f64_vars[idx]
        } else {
            self.vars[idx]
        }
    }
    fn lv_i64(&self, slot: usize) -> Variable {
        self.vars[self.arity + slot]
    }
    fn lv_f64(&self, slot: usize) -> Variable {
        self.f64_vars[self.arity + slot]
    }
    fn rv_i64(&self, reg: usize) -> Variable {
        self.vars[self.arity + self.locals_len + reg]
    }
    fn rv_f64(&self, reg: usize) -> Variable {
        self.f64_vars[self.arity + self.locals_len + reg]
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
                        // unreachable instruction after a Return ? skip it
                        ip += 1;
                        continue;
                    }
                }
            }

            if let Some(patch) = self.patches.get(&ip).copied() {
                if self.emit_range_for_next(builder, code, ip, patch).is_none() {
                    if crate::jit_cache::jit_debug() {
                        eprintln!("JITREJECT emit_range_for_next ip={ip}");
                    }
                    return None;
                }
                terminated = false;
                ip = patch.body_start_ip;
                continue;
            }

            if let Some(patch) = self.intrinsics.get(&ip).copied() {
                if self.emit_unary_float(builder, patch).is_none() {
                    if crate::jit_cache::jit_debug() {
                        eprintln!("JITREJECT emit_unary_float ip={ip}");
                    }
                    return None;
                }
                ip += 1; // intrinsic Call consumed
                terminated = false;
                continue;
            }

            if self.skip_ips.contains(&ip) {
                // instruction belongs to an intrinsic window; skip it
                ip += 1;
                continue;
            }

            if self.emit_ins(builder, code, ip, ins).is_none() {
                if crate::jit_cache::jit_debug() {
                    eprintln!("JITREJECT emit_ins ip={ip} op={:?}", ins.op);
                }
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
        // NOTE: no `is_float[store.a]` check here. `Op::ForNext` always
        // yields Int, but it is consumed by this peephole and never emits,
        // so any `is_float` mark on that register is stale from an earlier,
        // unrelated def of the same register (e.g. a Float const) — it says
        // nothing about the value stored below.
        if self.local_float[i_slot] {
            // range loops are Int-only; a Float iteration variable the
            // jit cannot describe ? fall back to the interpreter.
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
        // yield register is read later by StoreLocal ? define it as well
        let yield_var = self.rv(patch.yield_reg as usize);
        builder.def_var(yield_var, i_v);

        Some(())
    }

    fn emit_unary_float(&mut self, builder: &mut FunctionBuilder, patch: UnaryFloatPatch) -> Option<()> {
        match patch.kind {
            UnaryFloatKind::FloorSqrt => {
                // Bail to the interpreter for domain errors: Un propagates an error
                // for negative Int args here; the JIT cannot raise it, so it signals
                // tag = 2 and the VM re-runs the proto interpreted. Semantics are
                // identical to never JIT-compiling this proto.
                let bail_block = builder.create_block();
                let arg = match patch.arg {
                    IntArgSrc::Local(slot) => builder.use_var(self.lv(slot)),
                    IntArgSrc::Param(pos) => builder.use_var(self.vars[pos]),
                    IntArgSrc::ConstInt(n) => builder.ins().iconst(types::I64, n),
                };
                let zero_v = builder.ins().iconst(types::I64, 0);
                let le = builder.ins().icmp(IntCC::SignedLessThan, arg, zero_v);
                // main path continues; bail path exits with tag = 2
                let cont_block = builder.create_block();
                builder.ins().brif(le, bail_block, &[], cont_block, &[]);
                builder.seal_block(bail_block);
                builder.switch_to_block(bail_block);
                let zero_b = builder.ins().iconst(types::I64, 0);
                let two_b = builder.ins().iconst(types::I64, 2);
                self.emit_return_pair(builder, zero_b, two_b);
                // main path
                builder.switch_to_block(cont_block);
                builder.seal_block(cont_block);
                let f = builder.ins().fcvt_from_sint(types::F64, arg);
                let s = builder.ins().sqrt(f);
                let fl = builder.ins().floor(s);
                builder.def_var(self.rv_f64(patch.ret_reg as usize), fl);
                self.is_bool[patch.ret_reg as usize] = false;
                self.is_none[patch.ret_reg as usize] = false;
                self.is_float[patch.ret_reg as usize] = true;
                self.ret_slot[patch.ret_reg as usize] = None;
            }
            UnaryFloatKind::Sin => {
                // math.sin(i) where `i` is Int: promote to f64 and call the
                // extern libm wrapper. Un has no domain error here (sin is
                // total), so no bail block; sub/cos/tan will slot in the same
                // way later.
                let arg = match patch.arg {
                    IntArgSrc::Local(slot) => builder.use_var(self.lv(slot)),
                    IntArgSrc::Param(pos) => builder.use_var(self.vars[pos]),
                    IntArgSrc::ConstInt(n) => builder.ins().iconst(types::I64, n),
                };
                let f = builder.ins().fcvt_from_sint(types::F64, arg);
                let f_ref = self.module.declare_func_in_func(self.fid_sin, builder.func);
                let call = builder.ins().call(f_ref, &[f]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_f64(patch.ret_reg as usize), res);
                self.is_bool[patch.ret_reg as usize] = false;
                self.is_none[patch.ret_reg as usize] = false;
                self.is_float[patch.ret_reg as usize] = true;
                self.ret_slot[patch.ret_reg as usize] = None;
            }
        }
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
                    if self.local_float[slot] {
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
                    // (in an int-eligible context) must be a plain Int ? a no-op
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
                match cv.as_ref() {
                    Some(Constant::Float(f)) => {
                        let v = builder.ins().f64const(*f);
                        builder.def_var(self.rv_f64(ins.a as usize), v);
                        self.is_bool[ins.a as usize] = false;
                        self.is_none[ins.a as usize] = false;
                        self.is_float[ins.a as usize] = true;
                        self.ret_slot[ins.a as usize] = None;
                        Some(())
                    }
                    _ => {
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
                        self.is_float[ins.a as usize] = false;
                        self.ret_slot[ins.a as usize] = None;
                        Some(())
                    }
                }
            }
            Op::LoadNone | Op::LoadNull => {
                // A none-derived i64 zero; the VM returns it only through paths
                // that never observe it. `Return` on a none-derived value bails.
                let v = builder.ins().iconst(types::I64, 0);
                builder.def_var(self.rv_i64(ins.a as usize), v);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = true;
                self.is_float[ins.a as usize] = false;
                self.ret_slot[ins.a as usize] = None;
                Some(())
            }
            Op::LoadTrue | Op::LoadFalse => {
                let b = ins.op == Op::LoadTrue;
                let v = builder.ins().iconst(types::I64, if b { 1 } else { 0 });
                builder.def_var(self.rv_i64(ins.a as usize), v);
                self.is_bool[ins.a as usize] = true;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                self.ret_slot[ins.a as usize] = None;
                Some(())
            }
            Op::Unwrap => {
                // Unwrap strips the error wrapper from an Un value. In the
                // Int-eligible subset every value is a plain i64 ? a register move.
                let v = builder.use_var(self.rv(ins.a as usize));
                builder.def_var(self.rv(ins.a as usize), v);
                Some(())
            }
            Op::LoadName => {
                let name = self.arena.get(ins.d as usize)?.clone();
                // NB: call_candidate_regs is populated by the match below
                // (it knows the name's param/global identity).
                self.call_candidate_regs.remove(&(ins.a as usize));
                match self.param_pos.get(&name) {
                    Some(&pos) => {
                        let v = builder.use_var(self.vars[pos]);
                        builder.def_var(self.rv_i64(ins.a as usize), v);
                        self.is_bool[ins.a as usize] = false;
                        self.is_none[ins.a as usize] = false;
                        self.is_float[ins.a as usize] = false;
                        self.ret_slot[ins.a as usize] = None;
                        Some(())
                    }
                    None => {
                        // The name must be a known-JIT-eligible proto of the
                        // enclosing program (e.g. a direct self-call of fib).
                        // The ABI passes Int args as raw i64; the caller uses
                        // callee_candidate_regs for resolution. The value we
                        // write here is unused ??but its type must match the
                        // callee table expectation (i64).
                        if let Some(&proto_index) = self.callee_lookup.get(&name) {
                            if self.decls.contains_key(&proto_index) {
                                self.call_candidate_regs
                                    .insert(ins.a as usize, proto_index);
                                let v = builder.ins().iconst(types::I64, 1);
                                builder.def_var(self.rv_i64(ins.a as usize), v);
                                self.is_bool[ins.a as usize] = false;
                                self.is_none[ins.a as usize] = false;
                                self.is_float[ins.a as usize] = false;
                                self.ret_slot[ins.a as usize] = None;
                                return Some(());
                            }
                        }
                        // Anything else (free variable, unresolvable global)
                        // must go through the interpreter.
                        None
                    }
                }
            }
            Op::Dup => {
                // Read the source and pick the destination variable by the
                // source's type, not the destination's stale flag. Copying
                // is_float after def_var (as before) could pair an f64 value
                // with the destination's i64 variable and panic the builder.
                let src_float = self.is_float[ins.b as usize];
                let v = builder.use_var(self.rv(ins.b as usize));
                if src_float {
                    builder.def_var(self.rv_f64(ins.a as usize), v);
                } else {
                    builder.def_var(self.rv_i64(ins.a as usize), v);
                }
                self.is_bool[ins.a as usize] = self.is_bool[ins.b as usize];
                self.is_none[ins.a as usize] = self.is_none[ins.b as usize];
                self.is_float[ins.a as usize] = self.is_float[ins.b as usize];
                if self.int_dict_regs.contains(&(ins.b as usize)) {
                    self.int_dict_regs.insert(ins.a as usize);
                }
                self.ret_slot[ins.a as usize] = self.ret_slot[ins.b as usize];
                Some(())
            }
            Op::Pop | Op::PopScope | Op::NewScope => {
                // scope/frame bookkeeping markers ? no register effect in Int JIT
                Some(())
            }
            Op::StoreLocal => {
                let slot = ins.d as usize;
                if slot >= self.locals_len {
                    return None;
                }
                let src_float = self.is_float[ins.a as usize];
                if self.local_float[slot] {
                    if src_float {
                        let v = builder.use_var(self.rv_f64(ins.a as usize));
                        builder.def_var(self.lv_f64(slot), v);
                        let bits = builder.ins().bitcast(
                            types::I64,
                            cranelift_codegen::ir::MemFlagsData::new(),
                            v,
                        );
                        builder.def_var(self.lv_i64(slot), bits);
                        let one = builder.ins().iconst(types::I64, 1);
                        builder.def_var(self.poly_tag, one);
                    } else {
                        let i = builder.use_var(self.rv_i64(ins.a as usize));
                        builder.def_var(self.lv_i64(slot), i);
                        let f = builder.ins().fcvt_from_sint(types::F64, i);
                        builder.def_var(self.lv_f64(slot), f);
                    }
                } else {
                    let v = builder.use_var(self.rv_i64(ins.a as usize));
                    builder.def_var(self.lv_i64(slot), v);
                }
                if self.int_dict_regs.contains(&(ins.a as usize)) {
                    self.local_is_intdict[slot] = true;
                } else {
                    self.local_is_intdict[slot] = false;
                }
                self.local_is_bool[slot] = self.is_bool[ins.a as usize];
                self.local_is_none[slot] = self.is_none[ins.a as usize];
                Some(())
            }
            Op::GetAttr => {
                // Class-layout-backed direct field access. For class-method
                // protos with a known layout, the self local is slot 0 and
                // we emit a direct `un_instance_int_get` call instead of an
                // interpreter round-trip.
                //
                // The path is disabled for now: `precheck_fn` rejects class
                // methods, so the layout lookup below can only miss. It stays
                // in place (and keeps the layout seeding in the VM) so that
                // Phase 8 can re-enable it once the JIT value model carries
                // object receivers and the layout index is proven to match
                // the instance member order.
                let layout = match self.class_layouts.get(&self.proto_index) {
                    Some(l) => l.clone(),
                    None => return None,
                };
                let self_slot = 0usize;
                if (ins.b as usize) != self_slot {
                    if crate::jit_cache::jit_debug() {
                        eprintln!("JITREJECT getattr ins.b={} pix={}", ins.b, self.proto_index);
                    }
                    return None;
                }
                // Map the field-name-at-index-d to a field index into `layout`.
                let Some(field_ref) = self.arena.get(ins.d as usize) else {
                    if crate::jit_cache::jit_debug() {
                        eprintln!("JITREJECT getattr d={} arlen={}", ins.d, self.arena.len());
                    }
                    return None;
                };
                let field_ix = match layout.iter().position(|f| f == field_ref) {
                    Some(i) => i,
                    None => {
                        if crate::jit_cache::jit_debug() {
                            eprintln!("JITREJECT getattr not_in_layout ref={} layout={:?}", field_ref, layout);
                        }
                        return None;
                    }
                };
                // The receiver pointer is read from the `self` local slot and
                // the field is probed with `un_instance_int_ok` first: a miss
                // (the member is gone or holds a non-Int) must send the call
                // back to the interpreter instead of yielding 0.
                let f_ok = self.module.declare_func_in_func(self.fid_int_ok, builder.func);
                let self_ptr = builder.use_var(self.lv_i64(self_slot));
                let e_idx = builder.ins().iconst(types::I64, field_ix as i64);
                let ok_call = builder.ins().call(f_ok, &[self_ptr, e_idx]);
                let ok = builder.inst_results(ok_call)[0];
                let zero_i = builder.ins().iconst(types::I64, 0);
                let is_miss = builder.ins().icmp(IntCC::Equal, ok, zero_i);
                let bail_attr = builder.create_block();
                let cont_attr = builder.create_block();
                builder.ins().brif(is_miss, bail_attr, &[], cont_attr, &[]);
                builder.seal_block(bail_attr);
                builder.seal_block(cont_attr);
                builder.switch_to_block(bail_attr);
                let hp = builder.use_var(self.hp_var);
                let zero_e = builder.ins().iconst(types::I64, 0);
                let two_e = builder.ins().iconst(types::I64, 2);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), zero_e, hp, 0);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), two_e, hp, 8);
                builder.ins().return_(&[]);
                builder.switch_to_block(cont_attr);
                let f_ref = self.module.declare_func_in_func(self.fid_int_get, builder.func);
                let get_call = builder.ins().call(f_ref, &[self_ptr, e_idx]);
                let res = builder.inst_results(get_call)[0];
                builder.def_var(self.rv_i64(ins.a as usize), res);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                self.int_dict_regs.remove(&(ins.a as usize));
                self.ret_slot[ins.a as usize] = None;
                Some(())
            }
            Op::SetAttr => {
                // Symmetric intrinsic for SetAttr(self.<int_field>, v).
                // Target in `ins.b`; value in `ins.c`; field name index in `ins.d`.
                let layout = match self.class_layouts.get(&self.proto_index) {
                    Some(l) => l.clone(),
                    None => {
                        if crate::jit_cache::jit_debug() {
                            eprintln!("JITREJECT setattronatlayout proto_index={} layout_miss=", self.proto_index);
                        }
                        return None;
                    }
                };
                let self_slot = 0usize;
                if (ins.b as usize) != self_slot {
                    if crate::jit_cache::jit_debug() {
                        eprintln!("JITREJECT setattronatslot ins.b={} pix={}", ins.b, self.proto_index);
                    }
                    return None;
                }
                let Some(field_ref) = self.arena.get(ins.d as usize) else {
                    return None;
                };
                let field_ix = match layout.iter().position(|f| f == field_ref) {
                    Some(i) => i,
                    None => return None,
                };
                // Value must be Int (Float/Bool/etc are not expected here for this bench).
                if self.is_float[ins.c as usize] || self.is_bool[ins.c as usize] || self.is_none[ins.c as usize] {
                    return None;
                }
                // A write that cannot land (the slot is gone) must bail to the
                // interpreter instead of silently dropping the store.
                let f_ref = self.module.declare_func_in_func(self.fid_int_set, builder.func);
                let self_ptr = builder.use_var(self.lv_i64(self_slot));
                let e_idx = builder.ins().iconst(types::I64, field_ix as i64);
                let val = builder.use_var(self.rv_i64(ins.c as usize));
                let set_call = builder.ins().call(f_ref, &[self_ptr, e_idx, val]);
                let status = builder.inst_results(set_call)[0];
                let zero_i = builder.ins().iconst(types::I64, 0);
                let is_miss = builder.ins().icmp(IntCC::Equal, status, zero_i);
                let bail_attr = builder.create_block();
                let cont_attr = builder.create_block();
                builder.ins().brif(is_miss, bail_attr, &[], cont_attr, &[]);
                builder.seal_block(bail_attr);
                builder.seal_block(cont_attr);
                builder.switch_to_block(bail_attr);
                let hp = builder.use_var(self.hp_var);
                let zero_e = builder.ins().iconst(types::I64, 0);
                let two_e = builder.ins().iconst(types::I64, 2);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), zero_e, hp, 0);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), two_e, hp, 8);
                builder.ins().return_(&[]);
                builder.switch_to_block(cont_attr);
                builder.def_var(self.rv_i64(ins.a as usize), val);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                self.int_dict_regs.remove(&(ins.a as usize));
                self.ret_slot[ins.a as usize] = None;
                Some(())
            }
            Op::LoadLocal => {
                let slot = ins.d as usize;
                if slot >= self.locals_len {
                    return None;
                }
                let float_slot = self.local_float[slot];
                if float_slot {
                    let v = builder.use_var(self.lv_f64(slot));
                    builder.def_var(self.rv_f64(ins.a as usize), v);
                    self.is_bool[ins.a as usize] = false;
                    self.is_none[ins.a as usize] = false;
                    self.is_float[ins.a as usize] = true;
                    self.ret_slot[ins.a as usize] = Some(slot);
                } else {
                    let v = builder.use_var(self.lv_i64(slot));
                    builder.def_var(self.rv_i64(ins.a as usize), v);
                    self.is_bool[ins.a as usize] = self.local_is_bool[slot];
                    self.is_none[ins.a as usize] = self.local_is_none[slot];
                    self.is_float[ins.a as usize] = false;
                    if self.local_is_intdict[slot] {
                        self.int_dict_regs.insert(ins.a as usize);
                    }
                    self.ret_slot[ins.a as usize] = Some(slot);
                }
                Some(())
            }
            Op::Add | Op::Sub | Op::Mul => {
                let a_float = self.is_float[ins.b as usize];
                let b_float = self.is_float[ins.c as usize];
                if a_float || b_float {
                    let lhs = if a_float {
                        builder.use_var(self.rv_f64(ins.b as usize))
                    } else {
                        let i = builder.use_var(self.rv_i64(ins.b as usize));
                        builder.ins().fcvt_from_sint(types::F64, i)
                    };
                    let rhs = if b_float {
                        builder.use_var(self.rv_f64(ins.c as usize))
                    } else {
                        let i = builder.use_var(self.rv_i64(ins.c as usize));
                        builder.ins().fcvt_from_sint(types::F64, i)
                    };
                    let v = match ins.op {
                        Op::Add => builder.ins().fadd(lhs, rhs),
                        Op::Sub => builder.ins().fsub(lhs, rhs),
                        Op::Mul => builder.ins().fmul(lhs, rhs),
                        _ => return None,
                    };
                    builder.def_var(self.rv_f64(ins.a as usize), v);
                    self.is_bool[ins.a as usize] = false;
                    self.is_none[ins.a as usize] = false;
                    self.is_float[ins.a as usize] = true;
                    self.ret_slot[ins.a as usize] = None;
                    return Some(());
                }
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
                self.is_float[ins.a as usize] = false;
                self.ret_slot[ins.a as usize] = None;
                Some(())
            }
            Op::Call => {
                let argc = ins.d as usize;
                let callee_reg = ins.b as usize;
                // User JIT-eligible callee: call it.
                if let Some(&proto_index) = self.call_candidate_regs.get(&callee_reg) {
                    if self.decls.contains_key(&proto_index) {
                        return self.emit_user_call(builder, ins, argc, callee_reg, proto_index);
                    }
                }
                None
            }
            Op::ToBool => {
                if self.is_float[ins.b as usize] {
                    // Float args: fall through ??not supported by our Int ABI
                    let v = builder.use_var(self.rv_f64(ins.b as usize));
                    let cmp0 = builder.ins().f64const(0.0);
                    let cmp = builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::NotEqual, v, cmp0);
                    let one = builder.ins().iconst(types::I64, 1);
                    let zero = builder.ins().iconst(types::I64, 0);
                    let v = builder.ins().select(cmp, one, zero);
                    builder.def_var(self.rv_i64(ins.a as usize), v);
                    self.is_bool[ins.a as usize] = true;
                    self.is_none[ins.a as usize] = false;
                    self.is_float[ins.a as usize] = false;
                    self.ret_slot[ins.a as usize] = None;
                    Some(())
                } else {
                    // Int/Bool input ??unary negation test across zero.
                    // Bool was already 0/1 and Int has its Int-word semantics;
                    // `!= 0` keeps the same 0/1 result.
                    let v = builder.use_var(self.rv_i64(ins.b as usize));
                    let zero_v = builder.ins().iconst(types::I64, 0);
                    let cmp = builder.ins().icmp(IntCC::NotEqual, v, zero_v);
                    let one = builder.ins().iconst(types::I64, 1);
                    let r = builder.ins().select(cmp, one, zero_v);
                    builder.def_var(self.rv_i64(ins.a as usize), r);
                    self.is_bool[ins.a as usize] = true;
                    self.is_none[ins.a as usize] = false;
                    self.is_float[ins.a as usize] = false;
                    self.ret_slot[ins.a as usize] = None;
                    Some(())
                }
            }
            Op::ArgBegin => Some(()), // frame marker: no register effect
            Op::ArgPos | Op::ArgKw => {
                // Marks an argument as positional/keyword in the arg frame.
                // No register effect in the JIT.
                Some(())
            }
            Op::Idiv | Op::Mod => {
                let divisor = match const_load_int_at(code, ip, ins.c as usize, self.proto, self.program) {
                    Some(n) if n != 0 => n,
                    _ => return None,
                };
                if self.is_float[ins.b as usize] {
                    return None; // Int-only fast path
                }
                let lhs = builder.use_var(self.rv_i64(ins.b as usize));
                let rhs = builder.ins().iconst(types::I64, divisor);
                let v = if ins.op == Op::Idiv {
                    builder.ins().sdiv(lhs, rhs)
                } else {
                    builder.ins().srem(lhs, rhs)
                };
                builder.def_var(self.rv_i64(ins.a as usize), v);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                self.ret_slot[ins.a as usize] = None;
                Some(())
            }
            Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Eq | Op::Ne => {
                if self.is_float[ins.b as usize] || self.is_float[ins.c as usize] {
                    return None; // Int-only compare; Float compares not supported
                }
                let lhs = builder.use_var(self.rv_i64(ins.b as usize));
                let rhs = builder.use_var(self.rv_i64(ins.c as usize));
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
                builder.def_var(self.rv_i64(ins.a as usize), v);
                self.is_bool[ins.a as usize] = true;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                self.ret_slot[ins.a as usize] = None;
                Some(())
            }
            Op::FusedIntMulAdd => {
                let dst = ((ins.d >> 20) & 0x3FF) as usize;
                let x = ((ins.d >> 10) & 0x3FF) as usize;
                let y = (ins.d & 0x3FF) as usize;
                if dst >= self.locals_len || x >= self.locals_len || y >= self.locals_len {
                    return None;
                }
                if self.local_float[dst] || self.local_float[x] || self.local_float[y] {
                    return None; // Int-only fused accumulator
                }
                let mut acc = builder.use_var(self.lv_i64(dst));
                let xv = builder.use_var(self.lv_i64(x));
                let yv = builder.use_var(self.lv_i64(y));
                let p = builder.ins().imul(xv, yv);
                acc = builder.ins().iadd(acc, p);
                builder.def_var(self.lv_i64(dst), acc);
                builder.def_var(self.rv_i64(ins.a as usize), acc);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                self.ret_slot[ins.a as usize] = None;
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
            Op::BuildDict => {
                // Inline a fresh Int-Int dictionary. We only accept the
                // empty-builder form ??if any entries were pre-pushed on the
                // stack, give up so the interpreter handles it.
                if ins.d != 0 { return None; }
                // Call un_dict_int_empty() -> i64 (id)
                let fid = self.fid_dict_empty;
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_i64(ins.a as usize), res);
                self.int_dict_regs.insert(ins.a as usize);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                self.ret_slot[ins.a as usize] = None;
                Some(())
            }
            Op::SetItem => {
                // Int-Int dict fast path: target (b) must be a known IntDict id,
                // key (c) and value (a) must be Int. The IntDict index is loaded
                // directly with `rv_i64`.
                let value = ins.a as usize;
                let target = ins.b as usize;
                let key = ins.c as usize;
                if !self.int_dict_regs.contains(&target)
                    || self.is_float[value]
                    || self.is_bool[value]
                    || self.is_none[value]
                    || self.is_float[key]
                    || self.is_bool[key]
                    || self.is_none[key]
                    || self.is_float[target]
                    || self.is_bool[target]
                {
                    return None;
                }
                let fid = self.fid_dict_set;
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let d_i = builder.use_var(self.rv_i64(target));
                let k_i = builder.use_var(self.rv_i64(key));
                let v_i = builder.use_var(self.rv_i64(value));
                let call = builder.ins().call(f_ref, &[d_i, k_i, v_i]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_i64(value), res);
                self.is_bool[value] = false;
                self.is_none[value] = false;
                self.is_float[value] = false;
                self.ret_slot[value] = None;
                Some(())
            }
            Op::GetItem => {
                let dst = ins.a as usize;
                let target = ins.b as usize;
                let key = ins.c as usize;
                if !self.int_dict_regs.contains(&target) || self.is_float[key] || self.is_bool[key] || self.is_none[key] {
                    return None;
                }
                // Int lookup
                let fid = self.fid_dict_get;
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let d_i = builder.use_var(self.rv_i64(target));
                let k_i = builder.use_var(self.rv_i64(key));
                let call = builder.ins().call(f_ref, &[d_i, k_i]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_i64(dst), res);
                self.is_bool[dst] = false;
                self.is_none[dst] = false;
                self.is_float[dst] = false;
                self.ret_slot[dst] = None;
                Some(())
            }
            Op::Return => {
                // The JIT ABI writes the (value, tag) pair through a hidden
                // pointer: tag 0 ??value is an Int result; tag 1 ??Float bits;
                // tag 2 ??runtime bail; tag 3 ??Bool (0/1).
                if self.is_none[ins.a as usize] {
                    return None;
                }
                 // Tag can now legitimately be 0 (Int), 1 (Float bits) or 3 (Bool).
                 // We keep the matching "Float-only return" fast path when
                 // the register is definitely Float-typed and the ret_slot
                 // has no Float promotion (otherwise tag selection picks the
                 // registered tag for this path).
                 if self.is_float[ins.a as usize]
                    && self.ret_slot[ins.a as usize].is_none()
                {
                    // value straight from a Float definition (not through a
                    // promoted local); it is Float on every path
                    let f = builder.use_var(self.rv_f64(ins.a as usize));
                    let bits = builder.ins().bitcast(
                        types::I64,
                        cranelift_codegen::ir::MemFlagsData::new(),
                        f,
                    );
                    let one = builder.ins().iconst(types::I64, 1);
                    self.emit_return_pair(builder, bits, one);
                    return Some(());
                }
                if let Some(slot) = self.ret_slot[ins.a as usize] {
                    if self.local_float[slot] {
                        if self.is_bool[ins.a as usize] {
                            // Promoted Float local + bool-typed return ??the
                            // poly_slot tracks Float/Int payload classification
                            // but never captured Bool, so downstream callers
                            // would reconstruct with the wrong Payload type.
                            // Bail to interpreter (always semantically correct).
                            return None;
                        }
                        // per-path polymorphic return: n == 0 path returns Int,
                        // the loop path returns Float through a promoted local
                        let tag = builder.use_var(self.poly_tag);
                        let ibits = builder.use_var(self.lv_i64(slot));
                        let fval = builder.use_var(self.lv_f64(slot));
                        let fbits = builder.ins().bitcast(
                            types::I64,
                            cranelift_codegen::ir::MemFlagsData::new(),
                            fval,
                        );
                        let zero_v = builder.ins().iconst(types::I64, 0);
                        let is_float = builder.ins().icmp(IntCC::NotEqual, tag, zero_v);
                        let sel = builder.ins().select(is_float, fbits, ibits);
                        self.emit_return_pair(builder, sel, tag);
                        return Some(());
                    }
                }
                if self.is_bool[ins.a as usize] {
                    // Bool-typed local/register returns: reconstructed as
                    // Payload::Bool on the caller.
                    let v = builder.use_var(self.rv_i64(ins.a as usize));
                    let three_v = builder.ins().iconst(types::I64, 3);
                    self.emit_return_pair(builder, v, three_v);
                    return Some(());
                }
                let v = builder.use_var(self.rv_i64(ins.a as usize));
                let zero_v = builder.ins().iconst(types::I64, 0);
                self.emit_return_pair(builder, v, zero_v);
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

/// Where the `floor(sqrt(int_expr))` argument comes from.
#[derive(Clone, Copy)]
enum IntArgSrc {
    Local(usize),
    Param(usize),
    ConstInt(i64),
}

/// Matches the exact bytecode shape of a single-argument math intrinsic
/// (floor(sqrt(x)) or sin(x)) with an Int argument and one Float result.
/// Recognized as an intrinsic and emitted directly as f64 math.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UnaryFloatKind {
    FloorSqrt,
    Sin,
}
#[derive(Clone, Copy)]
struct UnaryFloatPatch {
    /// which intrinsic call this is
    kind: UnaryFloatKind,
    /// the intrinsic Call's destination register (the Float result).
    ret_reg: u8,
    arg: IntArgSrc,
}

/// Scan back from `ip` for exactly `call1; call2` `floor(sqrt(x))`.
/// Falls back to interpreter eligibility on any unexpected shape.
fn match_math_sin(
    code: &[Ins],
    arena: &[String],
    proto: &Proto,
    program: &Program,
    ip: usize,
) -> Option<UnaryFloatPatch> {
    if ip < 6 {
        return None;
    }
    let call = code.get(ip)?;
    if call.op != Op::Call || call.d != 1 {
        return None;
    }
    // window of 7 instructions ending at `ip`:
    // LoadName math; GetAttr sin; Unwrap; ArgBegin 1; arg; ArgPos; Call
    let wn = |off: usize| {
        ip.checked_sub(7 - off).and_then(|at| code.get(at).copied())
    };
    let m = wn(1)?;
    if m.op != Op::LoadName || arena.get(m.d as usize)?.as_str() != "math" {
        return None;
    }
    let g = wn(2)?;
    if g.op != Op::GetAttr || arena.get(g.d as usize)?.as_str() != "sin" {
        return None;
    }
    if g.a != m.a || g.b != m.a {
        return None;
    }
    let u = wn(3)?;
    if u.op != Op::Unwrap || u.a != g.a {
        return None;
    }
    // [3] ArgBegin 1
    let ab = wn(4)?;
    if ab.op != Op::ArgBegin || ab.d != 1 {
        return None;
    }
    // [4] arg ??Int-typed only. Float promotion is not handled here.
    let argsrc: IntArgSrc;
    let arreg: u8;
    let arg_ins = wn(5)?;
    match arg_ins.op {
        Op::LoadLocal => {
            let slot = arg_ins.d as usize;
            if slot >= proto.locals.len() {
                return None;
            }
            argsrc = IntArgSrc::Local(slot);
            arreg = arg_ins.a;
        }
        Op::LoadName => {
            let name = arena.get(arg_ins.d as usize)?.as_str();
            let pos = proto
                .params
                .iter()
                .position(|p| program.names.get(p.name).map(|s| s.as_str()) == Some(name))?;
            argsrc = IntArgSrc::Param(pos);
            arreg = arg_ins.a;
        }
        Op::LoadConst => {
            let idx = proto.consts.get(arg_ins.d as usize)?;
            match program.consts.get(*idx) {
                Some(Constant::Int(n)) => {
                    argsrc = IntArgSrc::ConstInt(*n);
                    arreg = arg_ins.a;
                }
                _ => return None,
            }
        }
        _ => return None,
    }
    // [5] ArgPos with the arg register
    let ap = wn(6)?;
    if ap.op != Op::ArgPos || ap.a != arreg {
        return None;
    }
    // the Call itself is `ip`; verify the callee register
    let fl = wn(7)?;
    if fl.op != Op::Call || fl.d != 1 || fl.a != call.a || fl.b != g.a {
        return None;
    }
    Some(UnaryFloatPatch {
        kind: UnaryFloatKind::Sin,
        ret_reg: call.a,
        arg: argsrc,
    })
}
fn match_floor_sqrt(
    code: &[Ins],
    arena: &[String],
    proto: &Proto,
    program: &Program,
    ip: usize,
) -> Option<UnaryFloatPatch> {
    if ip < 12 {
        return None;
    }
    let floor_call = code.get(ip)?;
    if floor_call.op != Op::Call || floor_call.d != 1 {
        return None;
    }

    // window of 13 instructions ending at `ip`:
    let wn = |off: usize| code.get(ip.checked_sub(13 - off)?).copied();

    // window: w(1)=ip-12 ... w(13)=ip
    let m1 = wn(1)?;
    if m1.op != Op::LoadName || arena.get(m1.d as usize)?.as_str() != "math" {
        return None;
    }
    // [1] GetAttr floor
    let g1 = wn(2)?;
    if g1.op != Op::GetAttr || arena.get(g1.d as usize)?.as_str() != "floor" {
        return None;
    }
    if g1.a != m1.a || g1.b != m1.a {
        return None;
    }
    // [2] Unwrap
    let u1 = wn(3)?;
    if u1.op != Op::Unwrap || u1.a != g1.a {
        return None;
    }
    // [3] ArgBegin 1
    let ab1 = wn(4)?;
    if ab1.op != Op::ArgBegin || ab1.d != 1 {
        return None;
    }
    // [4] LoadName math
    let m2 = wn(5)?;
    if m2.op != Op::LoadName || arena.get(m2.d as usize)?.as_str() != "math" {
        return None;
    }
    // [5] GetAttr sqrt
    let g2 = wn(6)?;
    if g2.op != Op::GetAttr || arena.get(g2.d as usize)?.as_str() != "sqrt" {
        return None;
    }
    if g2.a != m2.a || g2.b != m2.a {
        return None;
    }
    // [6] Unwrap
    let u2 = wn(7)?;
    if u2.op != Op::Unwrap || u2.a != g2.a {
        return None;
    }
    // [7] ArgBegin 1
    let ab2 = wn(8)?;
    if ab2.op != Op::ArgBegin || ab2.d != 1 {
        return None;
    }
    // [8] Int-typed arg
    let argsrc: IntArgSrc;
    let arreg: u8;
    let arg_ins = wn(9)?;
    match arg_ins.op {
        Op::LoadLocal => {
            let slot = arg_ins.d as usize;
            if slot >= proto.locals.len() {
                return None;
            }
            argsrc = IntArgSrc::Local(slot);
            arreg = arg_ins.a;
        }
        Op::LoadName => {
            let name = arena.get(arg_ins.d as usize)?.as_str();
            let pos = proto
                .params
                .iter()
                .position(|p| program.names.get(p.name).map(|s| s.as_str()) == Some(name))?;
            argsrc = IntArgSrc::Param(pos);
            arreg = arg_ins.a;
        }
        Op::LoadConst => {
            let idx = proto.consts.get(arg_ins.d as usize)?;
            match program.consts.get(*idx) {
                Some(Constant::Int(n)) => {
                    argsrc = IntArgSrc::ConstInt(*n);
                    arreg = arg_ins.a;
                }
                _ => return None,
            }
        }
        _ => return None,
    }
    // [9] ArgPos with the arg register
    let ap1 = wn(10)?;
    if ap1.op != Op::ArgPos || ap1.a != arreg {
        return None;
    }
    // [10] Call sqrt (argc 1)
    let sc = wn(11)?;
    if sc.op != Op::Call || sc.d != 1 || sc.b != g2.a {
        return None;
    }
    // [11] ArgPos with the sqrt result register
    let ap2 = wn(12)?;
    if ap2.op != Op::ArgPos || ap2.a != sc.a {
        return None;
    }
    // the floor Call itself is `ip`; verify the callee register
    let fl = wn(13)?;
    if fl.op != Op::Call || fl.d != 1 || fl.a != floor_call.a || fl.b != g1.a {
        return None;
    }
    Some(UnaryFloatPatch {
        kind: UnaryFloatKind::FloorSqrt,
        ret_reg: floor_call.a,
        arg: argsrc,
    })
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

/// Monotone Int占쏙옙Float fixpoint over the register file. Determines which
/// locals hold promoted Float values on at least one execution path.
/// Registers are Float when any operand/source is Float (Un's promotion
/// rule), or when they hold the result of one of the recognized intrinsics.
// Ret tag bit flags used by analyze_return_kinds.
const RET_INT: u8 = 0x1;
const RET_FLOAT: u8 = 0x2;
const RET_BOOL: u8 = 0x4;

/// Return-kind analysis for every function-proto in `program` ??which tag
/// kinds each one may produce at `Return`. Program-wide fixpoint because
/// an inner `Call` consumes the callee's kind.
///
/// Algorithm: one outer loop, each iteration walks every instruction and
/// grows the per-register Float/Bool fact sets monotonically. When callee
/// tags grow on one pass, their consumers' facts grow on the next pass.
/// Terminates because each fact can only grow at most 3 times.
pub fn analyze_return_kinds(program: &Program) -> HashMap<usize, u8> {
    let n = program.protos.len();
    let mut tags: HashMap<usize, u8> = (0..n).map(|i| (i, RET_INT)).collect();
    let plan_rc = match crate::vm::transfer::compile(program) {
        Ok(p) => Rc::new(p),
        Err(_) => return tags,
    };
    let plan = &plan_rc.protos;

    let callee_lookup: HashMap<String, usize> = {
        let mut m = HashMap::new();
        for (i, p) in program.protos.iter().enumerate() {
            if p.flags & PROTO_FUNCTION == 0 {
                continue;
            }
            if let Some(name) = program.names.get(p.name) {
                m.entry(name.clone()).or_insert(i);
            }
        }
        m
    };

    for _outer in 0..8 {
        let mut changed = false;
        for proto_index in 0..n {
            let cur_tags = tags.clone();
            if let Some(cur) = analyze_one_proto(program, plan, proto_index, &callee_lookup, &cur_tags) {
                if cur != tags[&proto_index] {
                    tags.insert(proto_index, cur);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    tags
}

fn analyze_one_proto(
    program: &Program,
    plan: &[crate::vm::ProtoPlan],
    proto_index: usize,
    callee_lookup: &HashMap<String, usize>,
    tags: &HashMap<usize, u8>,
) -> Option<u8> {
    let proto = &program.protos[proto_index];
    let p = &plan[proto_index];
    let code = &p.code;
    let arena: Vec<String> = proto
        .names
        .iter()
        .map(|&n| {
            program
                .names
                .get(n)
                .cloned()
                .unwrap_or_else(|| "?".into())
        })
        .collect();
    let param_pos: HashMap<String, usize> = proto
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| (program.names[p.name].clone(), i))
        .collect();
    let mut intrinsics: HashMap<usize, UnaryFloatPatch> = HashMap::new();
    let mut consumed: HashSet<usize> = HashSet::new();
    for ip in 0..code.len() {
        if let Some(patch) = match_floor_sqrt(code, &arena, proto, program, ip) {
            for k in (ip - 12)..ip {
                consumed.insert(k);
            }
            intrinsics.insert(ip, patch);
            continue;
        }
        if let Some(patch) = match_math_sin(code, &arena, proto, program, ip) {
            for k in (ip - 6)..ip {
                consumed.insert(k);
            }
            intrinsics.insert(ip, patch);
        }
    }
    let n_regs = code.iter().map(|i| i.a as usize).max().unwrap_or(0) + 1;
    let n_locals = proto.locals.len();
    let mut reg_float = vec![false; n_regs];
    let mut reg_bool = vec![false; n_regs];
    let mut local_float = vec![false; n_locals];
    let mut local_bool = vec![false; n_locals];
    let mut kinds = 0u8;

    // Inner fixpoint: propagate register/local kinds to fixpoint in this proto.
    for _inner in 0..8 {
        let mut gchanged = false;
        for (ip, ins) in code.iter().enumerate() {
            if let Some(patch) = intrinsics.get(&ip) {
                if !reg_float[patch.ret_reg as usize] {
                    reg_float[patch.ret_reg as usize] = true;
                    gchanged = true;
                }
                continue;
            }
            if consumed.contains(&ip) {
                continue;
            }
            match ins.op {
                Op::LoadConst => {
                    let is_f = matches!(
                        proto
                            .consts
                            .get(ins.d as usize)
                            .and_then(|&ci| program.consts.get(ci)),
                        Some(Constant::Float(_))
                    );
                    if is_f && !reg_float[ins.a as usize] {
                        reg_float[ins.a as usize] = true;
                        gchanged = true;
                    }
                }
                Op::LoadTrue | Op::LoadFalse => {
                    if !reg_bool[ins.a as usize] {
                        reg_bool[ins.a as usize] = true;
                        gchanged = true;
                    }
                }
                Op::LoadNone | Op::LoadNull => {}
                Op::LoadName => {
                    // If the name is a known callee in this program, borrow its kinds
                    if let Some(&callee) = callee_lookup.get(&arena.get(ins.d as usize)?.to_string()) {
                        let tag = tags.get(&callee).copied().unwrap_or(0);
                        if (tag & RET_FLOAT) != 0 && !reg_float[ins.a as usize] {
                            reg_float[ins.a as usize] = true;
                            gchanged = true;
                        }
                        if (tag & RET_BOOL) != 0 && !reg_bool[ins.a as usize] {
                            reg_bool[ins.a as usize] = true;
                            gchanged = true;
                        }
                    } else if param_pos.contains_key(&arena[ins.d as usize]) {
                        // untyped parameter ??narrow
                    }
                }
                Op::LoadLocal => {
                    let slot = ins.d as usize;
                    let a = ins.a as usize;
                    if slot < n_locals {
                        if local_float[slot] && !reg_float[a] {
                            reg_float[a] = true;
                            gchanged = true;
                        }
                        if local_bool[slot] && !reg_bool[a] {
                            reg_bool[a] = true;
                            gchanged = true;
                        }
                    }
                }
                Op::Add | Op::Sub | Op::Mul => {
                    let a = ins.a as usize;
                    let b = ins.b as usize;
                    let c = ins.c as usize;
                    if reg_float[b] || reg_float[c] {
                        if !reg_float[a] {
                            reg_float[a] = true;
                            gchanged = true;
                        }
                    }
                }
                Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Eq | Op::Ne => {
                    let a = ins.a as usize;
                    if !reg_bool[a] {
                        reg_bool[a] = true;
                        gchanged = true;
                    }
                }
                Op::ToBool => {
                    let a = ins.a as usize;
                    if !reg_bool[a] {
                        reg_bool[a] = true;
                        gchanged = true;
                    }
                }
                Op::Dup => {
                    let a = ins.a as usize;
                    let b = ins.b as usize;
                    if reg_float[b] && !reg_float[a] {
                        reg_float[a] = true;
                        gchanged = true;
                    }
                    if reg_bool[b] && !reg_bool[a] {
                        reg_bool[a] = true;
                        gchanged = true;
                    }
                }
                Op::StoreLocal => {
                    let slot = ins.d as usize;
                    let a = ins.a as usize;
                    if slot < n_locals {
                        if reg_float[a] && !local_float[slot] {
                            local_float[slot] = true;
                            gchanged = true;
                        }
                        if reg_bool[a] && !local_bool[slot] {
                            local_bool[slot] = true;
                            gchanged = true;
                        }
                    }
                }
                _ => {}
            }
        }
        if !gchanged {
            break;
        }
    }
    // final kind aggregation by examining Return registers
    for ins in code {
        if ins.op == Op::Return {
            let r = ins.a as usize;
            if r < n_regs {
                if reg_float[r] {
                    kinds |= RET_FLOAT;
                } else if reg_bool[r] {
                    kinds |= RET_BOOL;
                } else {
                    kinds |= RET_INT;
                }
            }
        }
    }
    if kinds == 0 {
        kinds = RET_INT;
    }
    Some(kinds)
}

fn compute_local_float(
    code: &[Ins],
    proto: &Proto,
    program: &Program,
    intrinsics: &HashMap<usize, UnaryFloatPatch>,
    consumed: &HashSet<usize>,
) -> Vec<bool> {
    let n_regs = code.iter().map(|i| i.a as usize).max().unwrap_or(0) + 1;
    let n_locals = proto.locals.len();
    let mut local_float = vec![false; n_locals];
    loop {
        let mut changed = false;
        let mut reg_float = vec![false; n_regs];
        for (ip, ins) in code.iter().enumerate() {
            if let Some(patch) = intrinsics.get(&ip) {
                reg_float[patch.ret_reg as usize] = true;
                continue;
            }
            if consumed.contains(&ip) {
                continue;
            }
            match ins.op {
                Op::LoadConst => {
                    let is_f = matches!(
                        proto
                            .consts
                            .get(ins.d as usize)
                            .and_then(|&ci| program.consts.get(ci)),
                        Some(Constant::Float(_))
                    );
                    reg_float[ins.a as usize] = is_f;
                }
                Op::LoadTrue | Op::LoadFalse | Op::LoadNone | Op::LoadNull | Op::LoadName => {
                    reg_float[ins.a as usize] = false;
                }
                Op::LoadLocal => {
                    let slot = ins.d as usize;
                    reg_float[ins.a as usize] = slot < n_locals && local_float[slot];
                }
                Op::Add | Op::Sub | Op::Mul => {
                    reg_float[ins.a as usize] =
                        reg_float[ins.b as usize] || reg_float[ins.c as usize];
                }
                Op::Dup => {
                    reg_float[ins.a as usize] = reg_float[ins.b as usize];
                }
                Op::Unwrap => {
                    reg_float[ins.a as usize] = reg_float[ins.a as usize];
                }
                Op::StoreLocal => {
                    let slot = ins.d as usize;
                    if slot < n_locals && reg_float[ins.a as usize] && !local_float[slot] {
                        local_float[slot] = true;
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        if !changed {
            return local_float;
        }
    }
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

/// Cheap pre-check that a proto is in the supported subset *shape* (params
/// only). Does not try to compile; used for up-front module declaration.
/// Class methods additionally require a hidden `self` Int arg at slot 0 —
/// this bumps the returned arity by one.
pub fn precheck_fn(program: &Program, proto_index: usize) -> Option<usize> {
    let proto = program.protos.get(proto_index)?;
    if proto.flags & PROTO_FUNCTION == 0 {
        return None;
    }
    if proto.ext_type.is_some() {
        // Class methods need an object receiver, but the JIT value model only
        // carries Int/Float values. The receiver arrives as a raw pointer
        // (see `jit_fast::try_call_jit`) and the class-layout fast path
        // cannot prove that the pointer it would dereference is the live
        // receiver, so class methods stay in the interpreter.
        // Phase 8 rework: teach the JIT about object receivers, then re-enable
        // the class-layout `GetAttr`/`SetAttr` fast paths.
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
    Some(arity)
}

/// Translate a proto into native code in the shared JIT module and return
/// the raw code pointer and arity. The function is defined by the caller;
/// pass the predeclared `Signature` + `FuncId` instead of building a new
/// module.
/// Emit a JIT stub body for a function that declined candidacy ??the
/// function writes tag = 2 ("runtime bail") through the hidden pointer and
/// returns. The VM converts tag 2 ??interpreter fallback, so ineligible
/// functions execute normally with zero constant-folding.
pub fn emit_fallback_stub_in_module(
    module: &mut JITModule,
    sig: cranelift_codegen::ir::Signature,
    func_id: cranelift_module::FuncId,
) {
    let mut fun_ctx = module.make_context();
    let mut func_ctx = FunctionBuilderContext::new();
    fun_ctx.func.signature = sig;
    fun_ctx.func.name = cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32());
    let mut builder = FunctionBuilder::new(&mut fun_ctx.func, &mut func_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    let incoming = builder.block_params(entry).to_vec();
    let hp_in = incoming[0];
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let zero = builder.ins().iconst(types::I64, 0);
    let two = builder.ins().iconst(types::I64, 2);
    builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), zero, hp_in, 0);
    builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), two, hp_in, 8);
    builder.ins().return_(&[]);
    builder.seal_all_blocks();
    builder.finalize(module.target_config());
    if let Err(e) = module.define_function(func_id, &mut fun_ctx) {
        let _ = e;
    }
}

pub fn emit_fn_in_module(
    program: &Program,
    proto_index: usize,
    module: &mut JITModule,
    decls: &HashMap<usize, cranelift_module::FuncId>,
    sig: cranelift_codegen::ir::Signature,
    func_id: cranelift_module::FuncId,
    fid_enter: cranelift_module::FuncId,
    fid_exit: cranelift_module::FuncId,
    fid_dict_empty: cranelift_module::FuncId,
    fid_dict_set: cranelift_module::FuncId,
    fid_dict_get: cranelift_module::FuncId,
    fid_sin: cranelift_module::FuncId,
    fid_int_get: cranelift_module::FuncId,
    fid_int_ok: cranelift_module::FuncId,
    fid_int_set: cranelift_module::FuncId,
    ret_kinds: &HashMap<usize, u8>,
) -> Option<()> {
    let proto = program.protos.get(proto_index)?;
    if proto.flags & PROTO_FUNCTION == 0 {
        return None;
    }
    let arity = proto.params.len();

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

    // ---------- pass 2b: match floor(sqrt(int)) intrinsics ----------
    let mut intrinsics: HashMap<usize, UnaryFloatPatch> = HashMap::new();
    let mut consumed: HashSet<usize> = HashSet::new();
    for ip in 0..code.len() {
        if let Some(p) = match_floor_sqrt(code, &arena, proto, program, ip) {
            // the intrinsic emits the whole 13-instruction window; anything
            // matched inside the window must not be emitted separately
            for k in (ip - 12)..ip {
                consumed.insert(k);
            }
            intrinsics.insert(ip, p);
            continue;
        }
        if let Some(p) = match_math_sin(code, &arena, proto, program, ip) {
            // the intrinsic emits the whole 7-instruction window; anything
            // matched inside the window must not be emitted separately
            for k in (ip - 6)..ip {
                consumed.insert(k);
            }
            intrinsics.insert(ip, p);
        }
    }

    // ---------- pass 2c: Int占쏙옙Float promotion fixpoint ----------
    let local_float = compute_local_float(code, proto, program, &intrinsics, &consumed);
    // Reject programs that hold a literal Float const across a JIT range
    // loop: the frontend crashes (`declared type of variable N doesn't
    // match type of value M`) when a Float literal-derived local coexists
    // with ForNext. Int-start accumulators that promote mid-loop carry no
    // Float const (their value enters through the intrinsic), so they stay
    // eligible; Float-start accumulators fall back to the interpreter
    // until the merge-path design is reworked.
    let has_float_const = code.iter().any(|ins| {
        ins.op == Op::LoadConst
            && proto
                .consts
                .get(ins.d as usize)
                .and_then(|&i| program.consts.get(i))
                .map(|c| matches!(c, Constant::Float(_)))
                .unwrap_or(false)
    });
    if has_float_const && code.iter().any(|ins| ins.op == Op::ForNext) {
        return None;
    }
    // reject a floor(sqrt()) whose Int argument is a promoted local ? the
    // window previously matched with only Int assumptions
    for &p in intrinsics.values() {
        if let IntArgSrc::Local(slot) = p.arg {
            if slot < local_float.len() && local_float[slot] {
                return None;
            }
        }
    }
    // If the function contains no user-call sites (other than intrinsic
    // floor/sqrt patches), there is no chance of infinite recursion through
    // the JIT, so skip the depth bookkeeping on entry/exit. Otherwise take
    // the safe path: a user-call site may recurse (fib/sum), so we must
    // guard against native stack overflow.
    let has_user_call = code.iter().enumerate().any(|(ip, ins)| {
        ins.op == Op::Call && !intrinsics.contains_key(&ip)
    });

    // ---------- pass 3: build the Cranelift context ----------
    let mut fun_ctx = module.make_context();
    let mut func_ctx = FunctionBuilderContext::new();

    fun_ctx.func.signature = sig;
    fun_ctx.func.name = cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32());

    let mut builder = FunctionBuilder::new(&mut fun_ctx.func, &mut func_ctx);
    let locals_len = proto.locals.len();
    let total_slots = arity + locals_len + plan.reg_count;
    let mut vars: Vec<Variable> = Vec::with_capacity(total_slots);
    let mut f64_vars: Vec<Variable> = Vec::with_capacity(total_slots);
    for _ in 0..total_slots {
        vars.push(builder.declare_var(types::I64));
        f64_vars.push(builder.declare_var(types::F64));
    }

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry); // hidden result ptr, args...
    let incoming = builder.block_params(entry).to_vec();
    let hidden_ptr = incoming[0];
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    // keep the hidden pointer in a dedicated variable so returns can store
    let hp_var = builder.declare_var(types::I64);
    builder.def_var(hp_var, hidden_ptr);
    for (i, v) in incoming.iter().enumerate().skip(1) {
        builder.def_var(vars[i - 1], *v);
    }
    let zero = builder.ins().iconst(types::I64, 0);
    for i in arity..vars.len() {
        builder.def_var(vars[i], zero);
    }
    // all Float pairs start undefined; they are only read through `use_var`
    // after being defined by their defining instructions
    let zero_f = builder.ins().f64const(0.0);
    for i in 0..vars.len() {
        builder.def_var(f64_vars[i], zero_f);
    }
    let poly_tag = builder.declare_var(types::I64);
    builder.def_var(poly_tag, zero);

    // ---------- entry depth guard ----------
    // skip for functions containing no user-call: no chance of infinite recursion
    // through the JIT (they cannot recurse through themselves), so the
    // depth bookkeeping is dead weight.
    let cont_entry_blk: Block;
    let entry_blk: Block;
    if has_user_call {
        let enter_ref = module.declare_func_in_func(fid_enter, builder.func);
        let call_too_deep = builder.ins().call(enter_ref, &[]);
        let too_deep = builder.inst_results(call_too_deep)[0];
        let zero_v = builder.ins().iconst(types::I64, 0);
        let is_too_deep = builder.ins().icmp(IntCC::NotEqual, too_deep, zero_v);
        let bail_entry_blk = builder.create_block();
        cont_entry_blk = builder.create_block();
        builder.ins().brif(is_too_deep, bail_entry_blk, &[], cont_entry_blk, &[]);
        builder.seal_block(bail_entry_blk);
        builder.seal_block(cont_entry_blk);
        builder.switch_to_block(bail_entry_blk);
        // Propagate tag=2 through the hidden pair; never entered by the body.
        let zero_e = builder.ins().iconst(types::I64, 0);
        let two_e = builder.ins().iconst(types::I64, 2);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), zero_e, hidden_ptr, 0);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), two_e, hidden_ptr, 8);
        builder.ins().return_(&[]);
        entry_blk = cont_entry_blk;
    } else {
        // no depth guard inserted ??the IR starts at the original entry block
        entry_blk = entry;
    }

    let mut blocks: HashMap<usize, Block> = HashMap::new();
    blocks.insert(0, entry_blk);
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
        proto_index,
        plan,
        arena,
        param_pos,
        patches,
        intrinsics,
        skip_ips: consumed,
        f64_vars,
        arity,
        locals_len,
        vars,
        is_bool: vec![false; plan.reg_count],
        is_none: vec![false; plan.reg_count],
        is_float: vec![false; plan.reg_count],
        local_is_bool: vec![false; locals_len],
        local_is_none: vec![false; locals_len],
        local_float,
        ret_slot: vec![None; plan.reg_count],
        blocks,
        loop_inc,
        poly_tag,
        hp_var,
        decls: decls.clone(),
        fid_exit,
        needs_depth: has_user_call,
        ret_kinds: ret_kinds.clone(),
        callee_lookup: {
            let mut m = HashMap::new();
            for (i, p) in program.protos.iter().enumerate() {
                if p.flags & PROTO_FUNCTION == 0 {
                    continue;
                }
                if let Some(name) = program.names.get(p.name) {
                    m.entry(name.clone()).or_insert(i);
                }
            }
            m
        },
        fid_dict_empty,
        fid_dict_set,
        fid_dict_get,
        fid_sin,
        fid_int_get,
        fid_int_ok,
        fid_int_set,
        // Wire in the class layout (keyed on proto index) for class methods.
        class_layouts: {
            let mut m: HashMap<usize, Vec<String>> = HashMap::new();
            for (i, p) in program.protos.iter().enumerate() {
                let Some(ext) = p.ext_type else { continue };
                let Some(class_name) = program.names.get(ext).cloned() else { continue };
                if let Some(layout) = crate::runtime::world::class_layout(&class_name) {
                    m.insert(i, layout);
                }
            }
            m
        },
        int_dict_regs: HashSet::new(),
        local_is_intdict: vec![false; locals_len],
        call_candidate_regs: HashMap::new(),
        module,
    };
    if tri.emit_stream(&mut builder).is_none() {
        return None;
    }
    builder.seal_all_blocks();
    builder.finalize(module.target_config());

    if module.define_function(func_id, &mut fun_ctx).is_err() {
        return None;
    }
    Some(())
}















