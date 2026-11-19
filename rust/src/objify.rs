
use std::collections::HashMap;
use std::path::Path;

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{types, AbiParam, InstBuilder, UserFuncName};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::ir::TrapCode;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::bytecode::{Constant, Op, Program, Proto};

const FLAG_JUMP_TRUE: u16 = 1 << 0;

pub struct EmitOutcome {
    pub emitted: Vec<String>,
    pub skipped: Vec<String>,
    pub output: std::path::PathBuf,
}

#[derive(Clone)]
enum V {
    Val(cranelift_codegen::ir::Value),
    Func(usize),
    ArgBegin,
    ArgEntry(cranelift_codegen::ir::Value),
}

fn static_screen(program: &Program, proto: &Proto) -> bool {
    proto.code.iter().all(|instr| {
        instr.op_code().map(static_supported_op).unwrap_or(false)
    }) && proto
        .consts
        .iter()
        .filter_map(|&gi| program.consts.get(gi))
        .all(|c| matches!(c, Constant::Int(_) | Constant::Bool(_)))
}

fn static_supported_op(op: Op) -> bool {
    matches!(
        op,
        Op::LoadNone
            | Op::LoadTrue
            | Op::LoadFalse
            | Op::LoadConst
            | Op::LoadLocal
            | Op::StoreLocal
            | Op::LoadName
            | Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Idiv
            | Op::Mod
            | Op::Lt
            | Op::Le
            | Op::Gt
            | Op::Ge
            | Op::Eq
            | Op::Ne
            | Op::ToBool
            | Op::Jmp
            | Op::JmpIf
            | Op::Return
            | Op::Dup
            | Op::Pop
            | Op::ArgBegin
            | Op::ArgPos
            | Op::Call
            | Op::Unwrap
    )
}

fn program_name<'a>(program: &'a Program, index: usize) -> Option<&'a str> {
    program.names.get(index).map(String::as_str)
}

fn proto_local_name<'a>(program: &'a Program, proto: &Proto, index: usize) -> Option<&'a str> {
    proto
        .names
        .get(index)
        .and_then(|&gi| program.names.get(gi))
        .map(String::as_str)
}

fn proto_const<'a>(program: &'a Program, proto: &Proto, index: usize) -> Option<&'a Constant> {
    proto
        .consts
        .get(index)
        .and_then(|&gi| program.consts.get(gi))
}

fn internal_signature(module: &ObjectModule, arity: usize) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    for _ in 0..arity {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn emit_trap_stub(
    module: &mut ObjectModule,
    fid: cranelift_module::FuncId,
    arity: usize,
) -> Result<(), String> {
    let sig = internal_signature(module, arity);
    let mut ctx = module.make_context();
    ctx.func.signature = sig;
    ctx.func.name = UserFuncName::user(0, fid.as_u32());
    let mut bctx = FunctionBuilderContext::new();
    let mut b = FunctionBuilder::new(&mut ctx.func, &mut bctx);
    let blk = b.create_block();
    b.append_block_params_for_function_params(blk);
    b.switch_to_block(blk);
    b.ins().trap(TrapCode::STACK_OVERFLOW);
    b.seal_all_blocks();
    b.finalize(module.target_config());
    module.define_function(fid, &mut ctx).map_err(|e| format!("trap stub: {e:?}"))
}

fn lower_proto(
    module: &mut ObjectModule,
    program: &Program,
    index: usize,
    fid: cranelift_module::FuncId,
    fids: &HashMap<usize, cranelift_module::FuncId>,
) -> Result<(), String> {
    let proto = &program.protos[index];
    let is_entry = false;
    let _ = is_entry;

    let mut func_ctx = module.make_context();
    func_ctx.func.signature = internal_signature(module, proto.params.len());
    func_ctx.func.name = UserFuncName::user(0, fid.as_u32());
    let mut builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut func_ctx.func, &mut builder_ctx);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    let incoming = builder.block_params(entry).to_vec();
    builder.switch_to_block(entry);

    let param_count = proto.params.len();
    let mut param_vars: Vec<Variable> = Vec::with_capacity(param_count);
    for i in 0..param_count {
        let var = builder.declare_var(types::I64);
        builder.def_var(var, incoming[i]);
        param_vars.push(var);
    }

    let mut local_vars: Vec<Variable> = Vec::with_capacity(proto.locals.len());
    for _ in 0..proto.locals.len() {
        let var = builder.declare_var(types::I64);
        let zero = builder.ins().iconst(types::I64, 0);
        builder.def_var(var, zero);
        local_vars.push(var);
    }

    let end_blk = builder.create_block();

    let mut leaders: HashMap<usize, cranelift_codegen::ir::Block> = HashMap::new();
    for (ip, instr) in proto.code.iter().enumerate() {
        if let Some(op) = instr.op_code() {
            match op {
                Op::Jmp | Op::JmpIf => {
                    let target = proto.jump_target(ip, instr.operand);
                    if target < proto.code.len() {
                        leaders.entry(target).or_insert_with(|| builder.create_block());
                    }
                    if ip + 1 < proto.code.len() {
                        leaders.entry(ip + 1).or_insert_with(|| builder.create_block());
                    }
                }
                Op::Return => {
                    if ip + 1 < proto.code.len() {
                        leaders.entry(ip + 1).or_insert_with(|| builder.create_block());
                    }
                }
                _ => {}
            }
        }
    }
    leaders.insert(0, entry);
    let blocks = leaders;

    let mut stack: Vec<V> = Vec::with_capacity(16);
    let mut ip = 0usize;
    while ip < proto.code.len() {
        if let Some(block) = blocks.get(&ip).copied() {
            builder.switch_to_block(block);
            builder.seal_block(block);
        }
        let instr = &proto.code[ip];
        let Some(op) = instr.op_code() else {
            return Err(format!("unknown opcode at instruction {ip}"));
        };
        match op {
            Op::LoadNone | Op::LoadFalse => {
                let v = builder.ins().iconst(types::I64, 0);
                stack.push(V::Val(v));
            }
            Op::LoadTrue => {
                let v = builder.ins().iconst(types::I64, 1);
                stack.push(V::Val(v));
            }
            Op::LoadConst => {
                match proto_const(program, proto, instr.operand as usize) {
                    Some(Constant::Int(v)) => {
                        let c = builder.ins().iconst(types::I64, *v);
                        stack.push(V::Val(c));
                    }
                    Some(Constant::Bool(b)) => {
                        let c = builder.ins().iconst(types::I64, if *b { 1 } else { 0 });
                        stack.push(V::Val(c));
                    }
                    _ => return Err("non-Int/Bool constant".to_string()),
                }
            }
            Op::LoadLocal => {
                let slot = instr.operand as usize;
                let Some(&var) = local_vars.get(slot) else {
                    return Err(format!("LoadLocal slot {slot} out of range"));
                };
                let v = builder.use_var(var);
                stack.push(V::Val(v));
            }
            Op::StoreLocal => {
                let slot = instr.operand as usize;
                let Some(&var) = local_vars.get(slot) else {
                    return Err(format!("StoreLocal slot {slot} out of range"));
                };
                let Some(value) = stack.pop() else {
                    return Err("StoreLocal: empty stack".to_string());
                };
                match value {
                    V::Val(v) => builder.def_var(var, v),
                    _ => return Err("StoreLocal: non-value on stack".to_string()),
                }
            }
            Op::LoadName => {
                let Some(name) = proto_local_name(program, proto, instr.operand as usize) else {
                    return Err("LoadName: bad name index".to_string());
                };
                let param_idx = proto
                    .params
                    .iter()
                    .position(|p| program_name(program, p.name) == Some(name));
                if let Some(slot) = param_idx {
                    let v = builder.use_var(param_vars[slot]);
                    stack.push(V::Val(v));
                    ip += 1;
                    continue;
                }
                let local_idx = proto
                    .locals
                    .iter()
                    .position(|&li| proto_local_name(program, proto, li) == Some(name));
                if let Some(slot) = local_idx {
                    let v = builder.use_var(local_vars[slot]);
                    stack.push(V::Val(v));
                    ip += 1;
                    continue;
                }
                let target = program
                    .protos
                    .iter()
                    .position(|p| program_name(program, p.name) == Some(name));
                if let Some(fi) = target {
                    stack.push(V::Func(fi));
                    ip += 1;
                    continue;
                }
                return Err(format!("LoadName: unresolved name '{name}'"));
            }
            Op::Add | Op::Sub | Op::Mul | Op::Idiv | Op::Mod => {
                let Some(rhs) = stack.pop() else {
                    return Err("binary op: empty stack".to_string());
                };
                let Some(lhs) = stack.pop() else {
                    return Err("binary op: empty stack".to_string());
                };
                let (V::Val(a), V::Val(b)) = (lhs, rhs) else {
                    return Err("binary op: non-value operand".to_string());
                };
                let v = match op {
                    Op::Add => builder.ins().iadd(a, b),
                    Op::Sub => builder.ins().isub(a, b),
                    Op::Mul => builder.ins().imul(a, b),
                    Op::Idiv => builder.ins().sdiv(a, b),
                    Op::Mod => builder.ins().srem(a, b),
                    _ => unreachable!(),
                };
                stack.push(V::Val(v));
            }
            Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Eq | Op::Ne => {
                let Some(rhs) = stack.pop() else {
                    return Err("comparison: empty stack".to_string());
                };
                let Some(lhs) = stack.pop() else {
                    return Err("comparison: empty stack".to_string());
                };
                let (V::Val(a), V::Val(b)) = (lhs, rhs) else {
                    return Err("comparison: non-value operand".to_string());
                };
                let cond = match op {
                    Op::Lt => IntCC::SignedLessThan,
                    Op::Le => IntCC::SignedLessThanOrEqual,
                    Op::Gt => IntCC::SignedGreaterThan,
                    Op::Ge => IntCC::SignedGreaterThanOrEqual,
                    Op::Eq => IntCC::Equal,
                    Op::Ne => IntCC::NotEqual,
                    _ => unreachable!(),
                };
                let c = builder.ins().icmp(cond, a, b);
                let v = builder.ins().uextend(types::I64, c);
                stack.push(V::Val(v));
            }
            Op::ToBool => {
                let Some(value) = stack.pop() else {
                    return Err("ToBool: empty stack".to_string());
                };
                let V::Val(v) = value else {
                    return Err("ToBool: non-value operand".to_string());
                };
                let zero = builder.ins().iconst(types::I64, 0);
                let c = builder.ins().icmp(IntCC::NotEqual, v, zero);
                let v = builder.ins().uextend(types::I64, c);
                stack.push(V::Val(v));
            }
            Op::JmpIf => {
                let target = proto.jump_target(ip, instr.operand);
                let Some(cond_v) = stack.pop() else {
                    return Err("JmpIf: empty stack".to_string());
                };
                let V::Val(cond) = cond_v else {
                    return Err("JmpIf: non-value condition".to_string());
                };
                let fallthrough = ip + 1;
                let then_blk = blocks.get(&target).copied().unwrap_or(end_blk);
                let else_blk = blocks.get(&fallthrough).copied().unwrap_or(end_blk);
                if instr.flags & FLAG_JUMP_TRUE != 0 {
                    builder.ins().brif(cond, then_blk, &[], else_blk, &[]);
                } else {
                    builder.ins().brif(cond, else_blk, &[], then_blk, &[]);
                }
                ip = fallthrough;
                continue;
            }
            Op::Jmp => {
                let target = proto.jump_target(ip, instr.operand);
                let blk = blocks.get(&target).copied().unwrap_or(end_blk);
                builder.ins().jump(blk, &[]);
                ip = target;
                continue;
            }
            Op::Return => {
                let Some(value) = stack.pop() else {
                    return Err("Return: empty stack".to_string());
                };
                match value {
                    V::Val(v) => builder.ins().return_(&[v]),
                    _ => return Err("Return: non-value returned".to_string()),
                };
                ip += 1;
                continue;
            }
            Op::Dup => {
                let Some(value) = stack.last().cloned() else {
                    return Err("Dup: empty stack".to_string());
                };
                stack.push(value);
            }
            Op::Pop => {
                let _ = stack.pop();
            }
            Op::ArgBegin => {
                stack.push(V::ArgBegin);
            }
            Op::ArgPos => {
                let Some(value) = stack.pop() else {
                    return Err("ArgPos: empty stack".to_string());
                };
                match value {
                    V::Val(v) => stack.push(V::ArgEntry(v)),
                    _ => return Err("ArgPos: non-value argument".to_string()),
                }
            }
            Op::Call => {
                let arity = instr.operand as usize;
                let mut args: Vec<cranelift_codegen::ir::Value> = Vec::with_capacity(arity);
                for _ in 0..arity {
                    match stack.pop() {
                        Some(V::ArgEntry(v)) => args.push(v),
                        _ => {
                            return Err(
                                "Call: arguments are not a valid positional sequence".to_string()
                            )
                        }
                    }
                }
                args.reverse();
                match stack.pop() {
                    Some(V::ArgBegin) => {}
                    _ => return Err("Call: missing ArgBegin".to_string()),
                }
                match stack.pop() {
                    Some(V::Func(callee)) => {
                        let Some(&callee_id) = fids.get(&callee) else {
                            return Err(format!(
                                "Call: callee proto {callee} was not statically supported"
                            ));
                        };
                        let Some(callee_proto) = program.protos.get(callee) else {
                            return Err("Call: callee index out of range".to_string());
                        };
                        if callee_proto.params.len() != arity {
                            return Err(format!(
                                "Call: callee expects {} args, got {}",
                                callee_proto.params.len(),
                                arity
                            ));
                        }
                        let callee_ref = module.declare_func_in_func(callee_id, builder.func);
                        let call = builder.ins().call(callee_ref, &args);
                        let results = builder.inst_results(call).to_vec();
                        stack.push(V::Val(results[0]));
                    }
                    _ => return Err("Call: callee is not a same-module proto".to_string()),
                }
            }
            Op::Unwrap => {
            }
            _ => return Err(format!("unsupported opcode {op:?}")),
        }
        ip += 1;
    }

    builder.switch_to_block(end_blk);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().return_(&[zero]);

    builder.seal_all_blocks();
    builder.finalize(module.target_config());
    module
        .define_function(fid, &mut func_ctx)
        .map_err(|e| format!("define: {e:?}"))?;
    Ok(())
}

pub fn emit(program: &Program, output: &Path, entry: Option<&str>) -> Result<EmitOutcome, String> {
    let entry_name = entry.unwrap_or("main");
    let entry_index = program
        .protos
        .iter()
        .position(|p| program.names[p.name] == entry_name)
        .ok_or_else(|| format!("entry proto '{entry_name}' not found"))?;

    let mut flag_builder = settings::builder();
    let _ = flag_builder.set("is_pic", "false");
    let native = cranelift_native::builder()
        .map_err(|e| format!("native ISA unsupported: {e:?}"))?;
    let isa = native
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| format!("ISA finish: {e:?}"))?;
    let object_builder = ObjectBuilder::new(isa, "un_object", default_libcall_names())
        .map_err(|e| format!("object builder: {e:?}"))?;
    let mut module = ObjectModule::new(object_builder);

    let mut skipped: Vec<String> = Vec::new();

    let mut fids: HashMap<usize, cranelift_module::FuncId> = HashMap::new();
    for (index, proto) in program.protos.iter().enumerate() {
        if !static_screen(program, proto) {
            continue;
        }
        let name = program.names[proto.name].clone();
        let export = if index == entry_index { Some("un_main") } else { None };
        let symbol = export.map(str::to_string).unwrap_or_else(|| format!("un_proto_{index}"));
        let linkage = if export.is_some() { Linkage::Export } else { Linkage::Local };
        let sig = internal_signature(&module, proto.params.len());
        match module.declare_function(&symbol, linkage, &sig) {
            Ok(fid) => {
                fids.insert(index, fid);
            }
            Err(e) => skipped.push(format!("{name}: declare {symbol}: {e:?}")),
        }
    }

    let mut emitted: Vec<String> = Vec::new();
    for (index, _) in program.protos.iter().enumerate() {
        let name = program.names[program.protos[index].name].clone();
        let Some(&fid) = fids.get(&index) else {
            skipped.push(format!("{name}: not statically supported"));
            continue;
        };
        match lower_proto(&mut module, program, index, fid, &fids) {
            Ok(()) => emitted.push(name),
            Err(reason) => {
                skipped.push(format!("{name}: {reason}"));
                let _ = emit_trap_stub(&mut module, fid, program.protos[index].params.len());
            }
        }
    }

    if emitted.iter().all(|n| *n != entry_name) {
        let reasons = skipped.join("; ");
        return Err(format!(
            "entry proto '{entry_name}' was rejected (check warnings). {reasons}"
        ));
    }

    let product = module.finish();
    let bytes = product
        .emit()
        .map_err(|e| format!("object emission failed: {e}"))?;
    std::fs::write(output, bytes).map_err(|e| format!("cannot write {}: {e}", output.display()))?;

    Ok(EmitOutcome {
        emitted,
        skipped,
        output: output.to_path_buf(),
    })
}
