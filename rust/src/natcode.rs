
use crate::jit_cache::{self, ArgKind, JitEntry, TAG_BAIL, TAG_BOOL, TAG_FLOAT, TAG_INT, TAG_NONE};
use crate::bytecode::Program;
use crate::objify::EmitOutcome;
use crate::tir::{abi_kind, elem_of_tir, tir_is_ref, tir_of_elem, TirBinOp, TirCmp, TirExpr, TirFn, TirProgram, TirStmt, TirTy, TirUnOp};
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::TrapCode;
use cranelift_codegen::ir::{types, Block, AbiParam, InstBuilder, MemFlagsData, UserFuncName};
use cranelift_codegen::settings::{self, Configurable};
use std::path::Path;

use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use cranelift_native;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NativeMode {
    Vm,
    Obj,
}

impl ArgKind {
    /// Derive the JIT parameter class from the single `TirTy` ABI table.
    /// Object-handle kinds are passed as i64 bits, exactly like Int.
    pub(crate) fn from_tir(ty: TirTy) -> Self {
        match abi_kind(ty) {
            0 => ArgKind::Int,
            1 => ArgKind::Float,
            2 => ArgKind::Bool,
            _ => ArgKind::Int,
        }
    }
}

/// Conservative "this function can never reach an uncaught runtime error"
/// analysis over the TIR call graph.
///
/// `TirExpr::Call` in `NativeMode::Obj` normally brackets each direct call with
/// `nrt_push_frame` / `nrt_pop_frame` so an uncaught error can print a full
/// traceback. A function whose body is built purely from scalar arithmetic,
/// control flow and calls to other such functions can never have a traceback
/// dumped while its frame is live, so the two FFI calls are pure overhead
/// there. The analysis is a least fixpoint: a function is pure only when every
/// local operation is pure and every callee is pure as well (cycles such as
/// recursive `fib` settle to "pure" if no impure operation exists anywhere in
/// the strongly connected component). Unknown/foreign operations are treated
/// as impure, so this never removes a frame that could be observable.
fn compute_error_free(fns: &[TirFn]) -> Vec<bool> {
    fn expr_error_free(e: &TirExpr, pure: &[bool]) -> bool {
        match e {
            TirExpr::Int(_) | TirExpr::Float(_) | TirExpr::Bool(_) | TirExpr::Local(_)
            | TirExpr::Promote(_) | TirExpr::CastInt(_) | TirExpr::CastFloat(_)
            | TirExpr::Cmp(_, _, _) | TirExpr::Un(_, _) => true,
            TirExpr::Bin(op, a, b) => {
                // Div/Mod can raise divide-by-zero; Str concat goes through NRT.
                matches!(
                    op,
                    TirBinOp::Add
                        | TirBinOp::Sub
                        | TirBinOp::Mul
                        | TirBinOp::BAnd
                        | TirBinOp::BOr
                        | TirBinOp::BXor
                        | TirBinOp::Shl
                        | TirBinOp::Shr
                ) && expr_error_free(a, pure)
                    && expr_error_free(b, pure)
            }
            TirExpr::Select { cond, then_expr, else_expr } => {
                expr_error_free(cond, pure)
                    && expr_error_free(then_expr, pure)
                    && expr_error_free(else_expr, pure)
            }
            TirExpr::Located { inner, .. } => expr_error_free(inner, pure),
            TirExpr::Call(i, args) => {
                pure.get(*i as usize).copied().unwrap_or(false)
                    && args.iter().all(|a| expr_error_free(a, pure))
            }
            // Everything else touches the heap / NRT / an error gate.
            _ => false,
        }
    }

    fn stmt_error_free(s: &TirStmt, pure: &[bool]) -> bool {
        match s {
            TirStmt::Let(_, e) | TirStmt::Set(_, e) | TirStmt::ExprStmt(e) => expr_error_free(e, pure),
            TirStmt::Return(Some(e)) => expr_error_free(e, pure),
            TirStmt::Return(None) | TirStmt::Break | TirStmt::Skip => true,
            TirStmt::If { cond, then_body, else_body } => {
                expr_error_free(cond, pure)
                    && then_body.iter().all(|s| stmt_error_free(s, pure))
                    && else_body.iter().all(|s| stmt_error_free(s, pure))
            }
            TirStmt::While { cond, body } => {
                expr_error_free(cond, pure) && body.iter().all(|s| stmt_error_free(s, pure))
            }
            TirStmt::ForRange { start, end, step, body, .. } => {
                expr_error_free(start, pure)
                    && expr_error_free(end, pure)
                    && expr_error_free(step, pure)
                    && body.iter().all(|s| stmt_error_free(s, pure))
            }
            // Collection/class/native/try statements all carry error paths.
            _ => false,
        }
    }

    let mut pure = vec![true; fns.len()];
    loop {
        let mut changed = false;
        for (i, f) in fns.iter().enumerate() {
            if !pure[i] {
                continue;
            }
            let ok = f.body.iter().all(|s| stmt_error_free(s, &pure))
                && f.defaults.iter().flatten().all(|e| expr_error_free(e, &pure));
            if !ok {
                pure[i] = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    pure
}

pub fn native_compile(program_id: u64, program: &Program, tir: &TirProgram) -> Vec<String> {
    let debug_trace = std::env::var_os("UV_TIR_DEBUG").is_some();
    if tir.functions.is_empty() {
        if debug_trace {
            eprintln!("TIRDBG no core functions found");
        }
        return Vec::new();
    }
    let mut failed: Vec<String> = Vec::new();
    let native = match cranelift_native::builder() {
        Ok(n) => n,
        Err(_) => return failed,
    };
    let mut flag_builder = settings::builder();
    let _ = flag_builder.set("is_pic", "false");
    let isa = match native.finish(settings::Flags::new(flag_builder)) {
        Ok(isa) => isa,
        Err(_) => return failed,
    };
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    builder.symbol(
        "un_jit_depth_enter",
        jit_cache::un_jit_depth_enter as *const u8,
    );
    builder.symbol(
        "un_jit_depth_exit",
        jit_cache::un_jit_depth_exit as *const u8,
    );
    builder.symbol("un_math_sin", crate::native::math::un_math_sin as *const u8);
    builder.symbol("un_math_cos", crate::native::math::un_math_cos as *const u8);
    builder.symbol("un_math_tan", crate::native::math::un_math_tan as *const u8);
    builder.symbol("un_math_asin", crate::native::math::un_math_asin as *const u8);
    builder.symbol("un_math_acos", crate::native::math::un_math_acos as *const u8);
    builder.symbol("un_math_atan", crate::native::math::un_math_atan as *const u8);
    builder.symbol("un_math_sqrt", crate::native::math::un_math_sqrt as *const u8);
    builder.symbol("un_math_cbrt", crate::native::math::un_math_cbrt as *const u8);
    builder.symbol("un_math_exp", crate::native::math::un_math_exp as *const u8);
    builder.symbol("un_math_ln", crate::native::math::un_math_ln as *const u8);
    builder.symbol("un_math_log2", crate::native::math::un_math_log2 as *const u8);
    builder.symbol("un_math_log10", crate::native::math::un_math_log10 as *const u8);
    builder.symbol("un_math_floor", crate::native::math::un_math_floor as *const u8);
    builder.symbol("un_math_ceil", crate::native::math::un_math_ceil as *const u8);
    builder.symbol("un_math_powf", crate::native::math::un_math_powf as *const u8);
    builder.symbol("un_math_log", crate::native::math::un_math_log as *const u8);
    builder.symbol("un_math_trunc", crate::native::math::un_math_trunc as *const u8);
    builder.symbol("un_math_abs", crate::native::math::un_math_abs as *const u8);
    let mut module = JITModule::new(builder);

    let mut internal_fids: HashMap<usize, cranelift_module::FuncId> = HashMap::new();
    let mut ffi_fids: HashMap<usize, cranelift_module::FuncId> = HashMap::new();
    for f in &tir.functions {
        let internal_sig = internal_signature(&module, f, NativeMode::Vm);
        match module.declare_function(
            &format!("tir_core_{}", f.index),
            Linkage::Export,
            &internal_sig,
        ) {
            Ok(fid) => {
                internal_fids.insert(f.index, fid);
            }
            Err(_) => {
                failed.push(f.name.clone());
                continue;
            }
        }
        let ffi_sig = ffi_signature(&module, f);
        match module.declare_function(&format!("tir_entry_{}", f.index), Linkage::Export, &ffi_sig) {
            Ok(fid) => {
                ffi_fids.insert(f.index, fid);
            }
            Err(_) => {
                failed.push(f.name.clone());
            }
        }
    }

    for f in &tir.functions {
        let heap = crate::tir::fn_uses_heap(f);
        let native_call = crate::tir::fn_uses_native_call(f);
        if crate::tir::fn_uses_str(f)
            || heap
            || native_call
            || crate::tir::fn_uses_dynamic(f)
        {
            // Both Str-only and heap-using functions are expected to run in the
            // interpreter in Vm mode; a bail stub is enough, no user-facing
            // warning (it would also break run/native output equivalence).
            if let (Some(_), Some(ffi)) = (internal_fids.get(&f.index), ffi_fids.get(&f.index)) {
                let ffi_sig = ffi_signature(&module, f);
                emit_bail_ffi(&mut module, *ffi, &ffi_sig);
                let internal_sig = internal_signature(&module, f, NativeMode::Vm);
                let internal_fid = *internal_fids.get(&f.index).unwrap();
                let _ = emit_internal_bail(&mut module, f, internal_fid, &internal_sig);
            }
            continue;
        }
        let (Some(internal), Some(ffi)) = (internal_fids.get(&f.index), ffi_fids.get(&f.index)) else {
            continue;
        };
        let internal_sig = internal_signature(&module, f, NativeMode::Vm);
        let ffi_sig = ffi_signature(&module, f);
        let guard = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            emit_internal(f, &mut module, &internal_fids, *internal, &internal_sig, &tir.functions, NativeMode::Vm, true)
        }));
        if std::env::var_os("UV_TIR_DEBUG").is_some() {
            match &guard {
                Ok(Ok(())) => eprintln!("TIRDBG emit_internal OK {}", f.name),
                Ok(Err(())) => eprintln!("TIRDBG emit_internal Err {}", f.name),
                Err(payload) => {
                    let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = payload.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "<non-string panic payload>".to_string()
                    };
                    eprintln!("TIRDBG emit_internal PANIC {}: {}", f.name, msg);
                }
            }
        }
        let internal_ok = matches!(&guard, Ok(Ok(())));
        if !internal_ok {
            if !failed.iter().any(|n| n == &f.name) {
                failed.push(f.name.clone());
            }
            emit_bail_ffi(&mut module, *ffi, &ffi_sig);
            let _ = emit_internal_bail(&mut module, f, *internal, &internal_sig);
            continue;
        }
        let ffi_guard = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            emit_ffi(f, &mut module, *internal, *ffi, &ffi_sig)
        }));
        if std::env::var_os("UV_TIR_DEBUG").is_some() {
            match &ffi_guard {
                Ok(Ok(())) => eprintln!("TIRDBG emit_ffi     OK {}", f.name),
                Ok(Err(())) => eprintln!("TIRDBG emit_ffi     Err {}", f.name),
                Err(payload) => {
                    let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = payload.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "<non-string panic payload>".to_string()
                    };
                    eprintln!("TIRDBG emit_ffi     PANIC {}: {}", f.name, msg);
                }
            }
        }
        let ffi_guard_ok = matches!(&ffi_guard, Ok(Ok(())));
        if !ffi_guard_ok {
            if !failed.iter().any(|n| n == &f.name) {
                failed.push(f.name.clone());
            }
            emit_bail_ffi(&mut module, *ffi, &ffi_sig);
        }
    }

    if module.finalize_definitions().is_err() {
        return failed;
    }
    let mut ffi_ptrs: HashMap<usize, *const u8> = HashMap::new();
    for (index, fid) in &ffi_fids {
        let ptr = module.get_finalized_function(*fid) as *const u8;
        ffi_ptrs.insert(*index, ptr);
    }
    jit_cache::stash_module(program_id, module);
    let mut proto_index_of: HashMap<(usize, usize), u32> = HashMap::new();
    for (index, proto) in program.protos.iter().enumerate() {
        if let Some(span) = proto.fn_span {
            if debug_trace {
                eprintln!("TIRDBG proto[{}] fn_span={:?}", index, span);
            }
            proto_index_of.insert(span, index as u32);
        } else if debug_trace {
            eprintln!("TIRDBG proto[{}] has NO fn_span", index);
        }
    }
    // Monomorphized instances share the source function's proto span, but the
    // JIT cache is keyed by proto index. Registering more than one instance for
    // the same proto would make dispatch non-deterministic (HashMap overwrite),
    // so skip JIT registration for shared protos and let the VM interpret them.
    let mut proto_users: HashMap<u32, usize> = HashMap::new();
    for f in &tir.functions {
        if let Some(&proto_index) = proto_index_of.get(&f.proto_span) {
            *proto_users.entry(proto_index).or_default() += 1;
        }
    }
    for f in &tir.functions {
        if debug_trace {
            eprintln!("TIRDBG tirfn {} span={:?}", f.name, f.proto_span);
        }
        let Some(&ptr) = ffi_ptrs.get(&f.index) else {
            continue;
        };
        let Some(&proto_index) = proto_index_of.get(&f.proto_span) else {
            if debug_trace {
                eprintln!("TIRDBG SKIP proto=? {}: no matching proto for span {:?}", f.name, f.proto_span);
            }
            continue;
        };
        if proto_users.get(&proto_index).copied().unwrap_or(0) > 1 {
            if debug_trace {
                eprintln!("TIRDBG SKIP JIT proto={} {} (shared by {} instances)", proto_index, f.name, proto_users[&proto_index]);
            }
            continue;
        }
        let failed_here = failed.iter().any(|n| n == &f.name);
        if debug_trace {
            if failed_here {
                eprintln!("TIRDBG FAIL proto={} {} (interpreter will run it)", proto_index, f.name);
            } else {
                eprintln!("TIRDBG OK   proto={} {}", proto_index, f.name);
            }
        }
        let arg_kinds: Vec<ArgKind> = f.params.iter().copied().map(ArgKind::from_tir).collect();
        jit_cache::register_tir_entry(
            program_id,
            proto_index,
            JitEntry {
                ptr,
                arity: f.params.len() as u32,
                arg_kinds,
            },
        );
    }
    failed
}

fn emit_trap_stub(
    module: &mut ObjectModule,
    f: &TirFn,
    fid: cranelift_module::FuncId,
) -> Result<(), String> {
    let sig = internal_signature(module, f, NativeMode::Obj);
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

fn emit_internal_bail<M: Module>(
    module: &mut M,
    f: &TirFn,
    fid: cranelift_module::FuncId,
    sig: &cranelift_codegen::ir::Signature,
) -> Result<(), String> {
    let mut ctx = module.make_context();
    ctx.func.signature = sig.clone();
    ctx.func.name = UserFuncName::user(0, fid.as_u32());
    let mut bctx = FunctionBuilderContext::new();
    let mut b = FunctionBuilder::new(&mut ctx.func, &mut bctx);
    let blk = b.create_block();
    b.append_block_params_for_function_params(blk);
    b.switch_to_block(blk);
    let zero_i64 = b.ins().iconst(types::I64, 0);
    let zero_f64 = b.ins().f64const(0.0);
    let zero_i8 = b.ins().iconst(types::I8, 0);
    let one_i8 = b.ins().iconst(types::I8, 1);
    match &f.ret {
        Some(TirTy::Int) => {
            b.ins().return_(&[zero_i64, one_i8]);
        }
        Some(TirTy::Float) => {
            b.ins().return_(&[zero_f64, one_i8]);
        }
        Some(TirTy::Bool) => {
            b.ins().return_(&[zero_i8, one_i8]);
        }
        Some(TirTy::Bytes) | Some(TirTy::Str) | Some(TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_)) => {
            b.ins().return_(&[zero_i64, one_i8]);
        }
        None => {
            b.ins().return_(&[one_i8]);
        }
    }
    b.seal_all_blocks();
    b.finalize(module.target_config());
    module.define_function(fid, &mut ctx).map_err(|e| format!("internal bail: {e:?}"))
}

pub fn obj_compile(
    _program_id: u64,
    tir: &TirProgram,
    output: &Path,
    entry: Option<&str>,
) -> Result<EmitOutcome, String> {
    if tir.functions.is_empty() {
        return Err("no core functions found (nothing is eligible for native code)".to_string());
    }
    let entry_name = entry.unwrap_or("main");
    let entry_index = tir
        .functions
        .iter()
        .position(|f| f.name == entry_name)
        .ok_or_else(|| {
            format!("entry function '{entry_name}' is not in the compile-mode core subset")
        })?;

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

    let mut fids: HashMap<usize, cranelift_module::FuncId> = HashMap::new();
    let mut skipped: Vec<String> = Vec::new();
    for (i, f) in tir.functions.iter().enumerate() {
        let is_entry = i == entry_index;
        let symbol = if is_entry {
            "un_main".to_string()
        } else {
            format!("un_tir_{i}")
        };
        // Method pointers are referenced from the generated driver, so every
        // emitted function must be link-visible, not just the entry point.
        let linkage = Linkage::Export;
        let sig = internal_signature(&module, f, NativeMode::Obj);
        match module.declare_function(&symbol, linkage, &sig) {
            Ok(fid) => {
                fids.insert(i, fid);
            }
            Err(e) => skipped.push(format!("{}: declare {symbol}: {e:?}", f.name)),
        }
    }

    let error_free = compute_error_free(&tir.functions);
    let mut emitted: Vec<String> = Vec::new();
    for (i, f) in tir.functions.iter().enumerate() {
        let Some(&fid) = fids.get(&i) else {
            skipped.push(format!("{}: declaration failed", f.name));
            continue;
        };
        let sig = internal_signature(&module, f, NativeMode::Obj);
        let needs_frame = error_free.get(i).copied().map(|p| !p).unwrap_or(true);
        let guard = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            emit_internal(f, &mut module, &fids, fid, &sig, &tir.functions, NativeMode::Obj, needs_frame)
        }));
        match guard {
            Ok(Ok(())) => emitted.push(f.name.clone()),
            _ => {
                skipped.push(format!("{}: emission failed (trap stub emitted)", f.name));
                let _ = emit_trap_stub(&mut module, f, fid);
            }
        }
    }

    let entry_emitted = tir
        .functions
        .iter()
        .position(|f| f.name == entry_name)
        .map(|i| emitted.contains(&tir.functions[i].name))
        .unwrap_or(false);
    if !entry_emitted {
        let reasons = skipped.join("; ");
        return Err(format!(
            "entry function '{entry_name}' was rejected (check the emitted/skipped lists). {reasons}"
        ));
    }

    let product = module.finish();
    let bytes = product
        .emit()
        .map_err(|e| format!("object emission failed: {e}"))?;
    std::fs::write(output, bytes).map_err(|e| format!("cannot write {}: {e}", output.display()))?;

    let mut class_methods = Vec::new();
    for (class_id, class_name) in tir.class_names.iter().enumerate() {
        let Some(method_names) = tir.class_methods.get(class_id) else { continue };
        for method_name in method_names.keys() {
            let exact = tir.functions.iter().enumerate().find(|(_, f)| {
                &f.name == method_name
                    && f.params.first() == Some(&crate::tir::TirTy::Class(class_id as u16))
            });
            let by_name = tir.functions.iter().enumerate().find(|(_, f)| &f.name == method_name);
            if let Some((tir_i, f)) = exact.or(by_name) {
                let ret_kind = f.ret.map_or(3, abi_kind);
                class_methods.push((class_name.clone(), method_name.clone(), tir_i, f.params.len(), ret_kind));
            }
        }
    }
    Ok(EmitOutcome {
        emitted,
        skipped,
        output: output.to_path_buf(),
        class_methods,
    })
}

fn declare_import<M: Module>(
    module: &mut M,
    name: &str,
    params: Vec<types::Type>,
    ret: Option<types::Type>,
) -> Result<cranelift_module::FuncId, ()> {
    let mut sig = module.make_signature();
    for t in params {
        sig.params.push(AbiParam::new(t));
    }
    if let Some(r) = ret {
        sig.returns.push(AbiParam::new(r));
    }
    module.declare_function(name, Linkage::Import, &sig).map_err(|_| ())
}

fn internal_signature<M: Module>(module: &M, f: &TirFn, mode: NativeMode) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    for t in &f.params {
        sig.params.push(AbiParam::new(match t {
            TirTy::Int => types::I64,
            TirTy::Float => types::F64,
            TirTy::Bool => types::I8,
            TirTy::Bytes | TirTy::Str | TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) => types::I64,
        }));
    }
    match &f.ret {
        Some(TirTy::Int) => sig.returns.push(AbiParam::new(types::I64)),
        Some(TirTy::Float) => sig.returns.push(AbiParam::new(types::F64)),
        Some(TirTy::Bool) => sig.returns.push(AbiParam::new(types::I8)),
        Some(TirTy::Bytes) | Some(TirTy::Str) => sig.returns.push(AbiParam::new(types::I64)),
        Some(TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_)) => sig.returns.push(AbiParam::new(types::I64)),
        None => {
            if mode == NativeMode::Obj {
                sig.returns.push(AbiParam::new(types::I64));
            }
        }
    }
    if mode == NativeMode::Vm {
        sig.returns.push(AbiParam::new(types::I8));
    }
    sig
}

fn ffi_signature<M: Module>(module: &M, f: &TirFn) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));    sig.params.push(AbiParam::new(types::I64));    for _ in 0..f.params.len() {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig
}




fn depth_sig<M: Module>(module: &M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn emit_exit_depth<M: Module>(
    builder: &mut FunctionBuilder,
    module: &mut M,
    fid_exit: cranelift_module::FuncId,
    acquired: cranelift_codegen::ir::Value,
) {
    let one = builder.ins().iconst(types::I8, 1);
    let is_one = builder.ins().icmp(IntCC::Equal, acquired, one);
    let do_blk = builder.create_block();
    let skip_blk = builder.create_block();
    builder.ins().brif(is_one, do_blk, &[], skip_blk, &[]);
    builder.switch_to_block(do_blk);
    let exit_ref = module.declare_func_in_func(fid_exit, builder.func);
    builder.ins().call(exit_ref, &[]);
    builder.ins().jump(skip_blk, &[]);
    builder.seal_block(do_blk);
    builder.seal_block(skip_blk);
    builder.switch_to_block(skip_blk);
}

struct FnEmit<'a> {
    internal_fids: &'a HashMap<usize, cranelift_module::FuncId>,
    fns: &'a [TirFn],
    vars: &'a [Variable],
    local_tys: &'a [TirTy],
    fid_exit: Option<cranelift_module::FuncId>,
    acquired: Variable,
    bail_blk: Block,
    loop_breaks: Vec<Block>,
    loop_skips: Vec<Block>,
    terminated: bool,
    mode: NativeMode,
    needs_frame: bool,
    active_handlers: Vec<cranelift_codegen::ir::Block>,
    current_span: Option<(usize, usize)>,
    unrt_str_new: Option<cranelift_module::FuncId>,
    unrt_str_cat: Option<cranelift_module::FuncId>,
    unrt_call_native: Option<cranelift_module::FuncId>,
    unrt_none: Option<cranelift_module::FuncId>,
    unrt_obj_int: Option<cranelift_module::FuncId>,
    unrt_obj_bool: Option<cranelift_module::FuncId>,
    unrt_obj_float: Option<cranelift_module::FuncId>,
    unrt_list_len: Option<cranelift_module::FuncId>,
    unrt_list_get: Option<cranelift_module::FuncId>,
    unrt_list_get_int: Option<cranelift_module::FuncId>,
    unrt_list_from_ints: Option<cranelift_module::FuncId>,
    unrt_list_from_floats: Option<cranelift_module::FuncId>,
    unrt_list_from_elems: Option<cranelift_module::FuncId>,
    unrt_list_repeat_int: Option<cranelift_module::FuncId>,
    unrt_list_repeat_float: Option<cranelift_module::FuncId>,
    unrt_int_from_obj: Option<cranelift_module::FuncId>,
    unrt_float_from_obj: Option<cranelift_module::FuncId>,
    unrt_dict_from_pairs: Option<cranelift_module::FuncId>,
    unrt_dict_get: Option<cranelift_module::FuncId>,
    unrt_dict_get_int: Option<cranelift_module::FuncId>,
    unrt_dict_set: Option<cranelift_module::FuncId>,
    unrt_dict_set_int: Option<cranelift_module::FuncId>,
    unrt_dict_keys: Option<cranelift_module::FuncId>,
    unrt_dict_values: Option<cranelift_module::FuncId>,
    unrt_dict_clear: Option<cranelift_module::FuncId>,
    unrt_list_set: Option<cranelift_module::FuncId>,
    unrt_list_set_int: Option<cranelift_module::FuncId>,
    unrt_list_push: Option<cranelift_module::FuncId>,
    unrt_write_error: Option<cranelift_module::FuncId>,
    nrt_set_error_site: Option<cranelift_module::FuncId>,
    nrt_push_frame: Option<cranelift_module::FuncId>,
    nrt_pop_frame: Option<cranelift_module::FuncId>,
    unrt_obj_closure: Option<cranelift_module::FuncId>,
    unrt_call_obj: Option<cranelift_module::FuncId>,
    unrt_global_get: Option<cranelift_module::FuncId>,
    unrt_global_set: Option<cranelift_module::FuncId>,
    unrt_take_error: Option<cranelift_module::FuncId>,
    unrt_dump_uncaught: Option<cranelift_module::FuncId>,
    unrt_make_int_div_zero: Option<cranelift_module::FuncId>,
    unrt_make_mod_zero_panic: Option<cranelift_module::FuncId>,
    unrt_make_math_expected_pos: Option<cranelift_module::FuncId>,
    unrt_make_math_sqrt_neg: Option<cranelift_module::FuncId>,
    unrt_class_new: Option<cranelift_module::FuncId>,
    unrt_member_get: Option<cranelift_module::FuncId>,
    unrt_member_set: Option<cranelift_module::FuncId>,
    unrt_method_call: Option<cranelift_module::FuncId>,
    unrt_bool_from_obj: Option<cranelift_module::FuncId>,
    unrt_list_pop: Option<cranelift_module::FuncId>,
    unrt_list_pop_at: Option<cranelift_module::FuncId>,
    unrt_list_sort: Option<cranelift_module::FuncId>,
    unrt_list_reverse: Option<cranelift_module::FuncId>,
    unrt_list_insert: Option<cranelift_module::FuncId>,
    unrt_list_index_of: Option<cranelift_module::FuncId>,
    unrt_list_contains: Option<cranelift_module::FuncId>,
    unrt_obj_is_err: Option<cranelift_module::FuncId>,
    unrt_obj_is_panic: Option<cranelift_module::FuncId>,
    unrt_obj_eq: Option<cranelift_module::FuncId>,
    unrt_len: Option<cranelift_module::FuncId>,
    /// Dynamic (tagged) dispatch helpers. Only present in Obj mode; Vm-mode
    /// functions that touch `TirTy::Obj` are bailed to the interpreter.
    unrt_dyn_bin: Option<cranelift_module::FuncId>,
    unrt_dyn_un: Option<cranelift_module::FuncId>,
    str_literals: HashMap<Vec<u8>, cranelift_module::DataId>,
    str_literal_count: u32,
    fn_index: u32,
    math_ffi_fids: std::collections::HashMap<String, cranelift_module::FuncId>,
}

fn embed_str_bytes<M: Module>(
    emitter: &mut FnEmit,
    module: &mut M,
    builder: &mut FunctionBuilder,
    bytes: &[u8],
) -> cranelift_codegen::ir::GlobalValue {
    let data_id = match emitter.str_literals.get(bytes) {
        Some(id) => *id,
        None => {
            let name = format!("un_str_{}_{}", emitter.fn_index, emitter.str_literal_count);
            emitter.str_literal_count += 1;
            let data_id = module
                .declare_data(&name, Linkage::Local, false, false)
                .expect("declare embedded bytes data");
            let mut desc = cranelift_module::DataDescription::new();
            desc.define(bytes.to_vec().into_boxed_slice());
            desc.set_align(1);
            module
                .define_data(data_id, &desc)
                .expect("define embedded bytes data");
            emitter.str_literals.insert(bytes.to_vec(), data_id);
            data_id
        }
    };
    module.declare_data_in_func(data_id, builder.func)
}

fn emit_internal<M: Module>(
    f: &TirFn,
    module: &mut M,
    internal_fids: &HashMap<usize, cranelift_module::FuncId>,
    func_id: cranelift_module::FuncId,
    sig: &cranelift_codegen::ir::Signature,
    fns: &[TirFn],
    mode: NativeMode,
    needs_frame: bool,
) -> Result<(), ()> {
    let fid_enter = if mode == NativeMode::Vm {
        Some(
            module
                .declare_function("un_jit_depth_enter", Linkage::Import, &depth_sig(module))
                .map_err(|_| ())?,
        )
    } else {
        None
    };
    let fid_exit = if mode == NativeMode::Vm {
        Some(
            module
                .declare_function("un_jit_depth_exit", Linkage::Import, &depth_sig(module))
                .map_err(|_| ())?,
        )
    } else {
        None
    };
    let mut func_ctx = module.make_context();
    let mut builder_ctx = FunctionBuilderContext::new();
    func_ctx.func.signature = sig.clone();
    func_ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    let mut builder = FunctionBuilder::new(&mut func_ctx.func, &mut builder_ctx);

    let unrt_none = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_obj_none", vec![], Some(types::I64))?)
    } else {
        None
    };
    let unrt_str_new = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_str_from_bytes",
            vec![types::I64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_str_cat = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_str_concat",
            vec![types::I64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_obj_int = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_obj_int", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_obj_bool = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_obj_bool", vec![types::I8], Some(types::I64))?)
    } else {
        None
    };
    let unrt_obj_float = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_obj_float", vec![types::F64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_call_native = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_call_native",
            vec![types::I64, types::I64, types::I64, types::I64, types::I64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_list_len = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_len", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_get = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_get", vec![types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_get_int = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_get_int", vec![types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_from_ints = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_from_ints", vec![types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_from_floats = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_from_floats", vec![types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_from_elems = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_list_from_elems",
            vec![types::I64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_list_repeat_int = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_list_repeat_int",
            vec![types::I64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_list_repeat_float = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_list_repeat_float",
            vec![types::F64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_int_from_obj = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_int_from_obj", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_float_from_obj = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_float_from_obj", vec![types::I64], Some(types::F64))?)
    } else {
        None
    };
    let unrt_dict_from_pairs = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_dict_from_pairs", vec![types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_dict_get = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_dict_get", vec![types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_dict_get_int = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_dict_get_int",
            vec![types::I64, types::I64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_obj_is_err = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_obj_is_err", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_obj_is_panic = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_obj_is_panic", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_dict_set = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_dict_set", vec![types::I64, types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_dict_set_int = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_dict_set_int", vec![types::I64, types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_dict_keys = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_dict_keys", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_dict_values = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_dict_values", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_dict_clear = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_dict_clear", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_set = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_set", vec![types::I64, types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_set_int = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_set_int", vec![types::I64, types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_push = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_push", vec![types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_write_error = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_write_error", vec![types::I64], None)?)
    } else {
        None
    };
    let nrt_set_error_site = if mode == NativeMode::Obj {
        Some(declare_import(module, "nrt_set_error_site", vec![types::I64, types::I64], None)?)
    } else { None };
    let nrt_push_frame = if mode == NativeMode::Obj {
        Some(declare_import(module, "nrt_push_frame", vec![types::I64, types::I64, types::I64, types::I64], None)?)
    } else { None };
    let nrt_pop_frame = if mode == NativeMode::Obj {
        Some(declare_import(module, "nrt_pop_frame", vec![], None)?)
    } else { None };
    let unrt_obj_closure = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_obj_closure",
            vec![types::I64, types::I64, types::I64],
            Some(types::I64),
        )?)
    } else { None };
    let unrt_call_obj = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_call_obj",
            vec![types::I64, types::I64, types::I64],
            Some(types::I64),
        )?)
    } else { None };
    let unrt_global_get = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_global_get", vec![types::I64], Some(types::I64))?)
    } else { None };
    let unrt_global_set = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_global_set", vec![types::I64, types::I64], Some(types::I64))?)
    } else { None };
    let unrt_take_error = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_take_error", vec![], Some(types::I64))?)
    } else {
        None
    };
    let unrt_dump_uncaught = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_dump_uncaught", vec![], None)?)
    } else {
        None
    };
    let unrt_make_int_div_zero = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_make_int_div_zero", vec![], Some(types::I64))?)
    } else {
        None
    };
    let unrt_make_mod_zero_panic = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_make_mod_zero_panic", vec![], Some(types::I64))?)
    } else {
        None
    };
    let unrt_make_math_expected_pos = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_make_math_expected_pos",
            vec![],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_make_math_sqrt_neg = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_make_math_sqrt_neg",
            vec![],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_class_new = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_class_new",
            vec![types::I64, types::I64, types::I64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_member_get = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_member_get",
            vec![types::I64, types::I64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_member_set = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_member_set",
            vec![types::I64, types::I64, types::I64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_method_call = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_method_call",
            vec![types::I64, types::I64, types::I64, types::I64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_bool_from_obj = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_bool_from_obj", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_pop = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_pop", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_pop_at = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_pop_at", vec![types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_sort = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_sort", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_reverse = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_reverse", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_insert = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_insert", vec![types::I64, types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_index_of = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_index_of", vec![types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_contains = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_contains", vec![types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_len = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_len", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_obj_eq = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_obj_eq", vec![types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_dyn_bin = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_dyn_bin",
            vec![types::I64, types::I64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };
    let unrt_dyn_un = if mode == NativeMode::Obj {
        Some(declare_import(
            module,
            "unrt_dyn_un",
            vec![types::I64, types::I64],
            Some(types::I64),
        )?)
    } else {
        None
    };

    let vars: Vec<Variable> = f
        .locals
        .iter()
        .map(|t| {
            builder.declare_var(match t {
                TirTy::Int => types::I64,
                TirTy::Float => types::F64,
                TirTy::Bool => types::I8,
                TirTy::Bytes | TirTy::Str | TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) => types::I64,
            })
        })
        .collect();
    let acquired = builder.declare_var(types::I8);

    let entry = builder.create_block(); 
    builder.append_block_params_for_function_params(entry);
    let incoming = builder.block_params(entry).to_vec();
    builder.switch_to_block(entry);

    for (i, var) in vars.iter().enumerate().take(f.params.len()) {
        builder.def_var(*var, incoming[i]);
    }
    for (var, ty) in vars.iter().skip(f.params.len()).zip(&f.locals[f.params.len()..]) {
        match ty {
            TirTy::Int => {
                let zero = builder.ins().iconst(types::I64, 0);
                builder.def_var(*var, zero);
            }
            TirTy::Float => {
                let zero = builder.ins().f64const(0.0);
                builder.def_var(*var, zero);
            }
            TirTy::Bool => {
                let zero = builder.ins().iconst(types::I8, 0);
                builder.def_var(*var, zero);
            }
            TirTy::Bytes | TirTy::Str => {
                let v = match unrt_none {
                    Some(id) => {
                        let none_ref = module.declare_func_in_func(id, builder.func);
                        let res = builder.ins().call(none_ref, &[]);
                        builder.inst_results(res)[0]
                    }
                    None => builder.ins().iconst(types::I64, 0),
                };
                builder.def_var(*var, v);
            }
            TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) => {
                let v = match unrt_none {
                    Some(id) => {
                        let none_ref = module.declare_func_in_func(id, builder.func);
                        let res = builder.ins().call(none_ref, &[]);
                        builder.inst_results(res)[0]
                    }
                    None => builder.ins().iconst(types::I64, 0),
                };
                builder.def_var(*var, v);
            }
        }
    }
    let (body_entry, bail_entry) = if let Some(fid_enter) = fid_enter {
        let enter_ref = module.declare_func_in_func(fid_enter, builder.func);
        let enter_res = builder.ins().call(enter_ref, &[]);
        let too_deep_i64 = builder.inst_results(enter_res)[0];
        let too_deep = builder.ins().ireduce(types::I8, too_deep_i64);
        builder.def_var(acquired, too_deep);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        let entered = builder.ins().icmp(IntCC::Equal, too_deep, zero_i8);
        let body_entry = builder.create_block();
        let bail_entry = builder.create_block();
        builder.ins().brif(entered, body_entry, &[], bail_entry, &[]);
        (body_entry, Some(bail_entry))
    } else {
        let zero = builder.ins().iconst(types::I8, 0);
        builder.def_var(acquired, zero);
        (entry, None)
    };
    if let Some(bail_entry) = bail_entry {
        builder.switch_to_block(bail_entry);
        emit_internal_return(
            f,
            &mut builder,
            module,
            fid_exit,
            acquired,
            None,
            true,
            mode,
        );
        builder.seal_block(bail_entry);
    }

    let bail_blk = builder.create_block();

    builder.switch_to_block(body_entry);
    let mut emitter = FnEmit {
        internal_fids,
        fns,
        vars: &vars,
        local_tys: &f.locals,
        fid_exit,
        acquired,
        bail_blk,
        loop_breaks: Vec::new(),
        loop_skips: Vec::new(),
        terminated: false,
        mode,
        needs_frame,
        active_handlers: Vec::new(),
        current_span: None,
        unrt_str_new,
        unrt_str_cat,
        unrt_call_native,
        unrt_none,
        unrt_obj_int,
        unrt_obj_bool,
        unrt_obj_float,
        unrt_list_len,
        unrt_list_get,
        unrt_list_get_int,
        unrt_list_from_ints,
        unrt_list_from_floats,
        unrt_list_from_elems,
        unrt_list_repeat_int,
        unrt_list_repeat_float,
        unrt_int_from_obj,
        unrt_float_from_obj,
        unrt_dict_from_pairs,
        unrt_dict_get,
        unrt_dict_get_int,
        unrt_dict_set,
        unrt_dict_set_int,
        unrt_dict_keys,
        unrt_dict_values,
        unrt_dict_clear,
        unrt_list_set,
        unrt_list_set_int,
        unrt_list_push,
        unrt_write_error,
        nrt_set_error_site,
        nrt_push_frame,
        nrt_pop_frame,
        unrt_obj_closure,
        unrt_call_obj,
        unrt_global_get,
        unrt_global_set,
        unrt_take_error,
        unrt_dump_uncaught,
        unrt_make_int_div_zero,
        unrt_make_math_expected_pos,
        unrt_make_math_sqrt_neg,
        unrt_make_mod_zero_panic,
        unrt_class_new,
        unrt_member_get,
        unrt_member_set,
        unrt_method_call,
        unrt_bool_from_obj,
        unrt_list_pop,
        unrt_list_pop_at,
        unrt_list_sort,
        unrt_list_reverse,
        unrt_list_insert,
        unrt_list_index_of,
        unrt_list_contains,
        unrt_obj_is_err,
        unrt_obj_is_panic,
        unrt_obj_eq,
        unrt_len,
        unrt_dyn_bin,
        unrt_dyn_un,
        str_literals: HashMap::new(),
        str_literal_count: 0,
        fn_index: f.index as u32,
        math_ffi_fids: std::collections::HashMap::new(),
    };
    for stmt in &f.body {
        if emitter.terminated {
            continue;
        }
        emit_stmt(stmt, &mut emitter, &mut builder, module, f);
    }
    if !emitter.terminated {
        match f.ret {
            None => emit_internal_return(
                f,
                &mut builder,
                module,
                fid_exit,
                acquired,
                None,
                false,
                mode,
            ),
            Some(_) => emit_internal_return(
                f,
                &mut builder,
                module,
                fid_exit,
                acquired,
                None,
                true,
                mode,
            ),
        }
    }
    builder.seal_block(body_entry);

    builder.switch_to_block(bail_blk);
    emit_internal_return(
        f,
        &mut builder,
        module,
        fid_exit,
        acquired,
        None,
        true,
        mode,
    );
    builder.seal_block(bail_blk);

    builder.seal_all_blocks();
    builder.finalize(module.target_config());
    match module.define_function(func_id, &mut func_ctx) {
        Ok(()) => Ok(()),
        Err(error) => {
            if std::env::var_os("UV_TIR_DEBUG").is_some() {
                eprintln!("TIRDBG define_function failed for '{}': {error:?}", f.name);
            }
            Err(())
        }
    }
}

fn emit_internal_return<M: Module>(
    f: &TirFn,
    builder: &mut FunctionBuilder,
    module: &mut M,
    fid_exit: Option<cranelift_module::FuncId>,
    acquired: Variable,
    result: Option<cranelift_codegen::ir::Value>,
    bail: bool,
    mode: NativeMode,
) {
    if let Some(fid_exit) = fid_exit {
        let acquired_v = builder.use_var(acquired);
        emit_exit_depth(builder, module, fid_exit, acquired_v);
    }
    let mut returns = Vec::new();
    match (&f.ret, result) {
        (Some(TirTy::Int), Some(v)) => returns.push(v),
        (Some(TirTy::Float), Some(v)) => returns.push(v),
        (Some(TirTy::Bool), Some(v)) => returns.push(v),
        (Some(TirTy::Bytes), Some(v)) | (Some(TirTy::Str), Some(v)) => returns.push(v),
        (Some(TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_)), Some(v)) => returns.push(v),
        (Some(TirTy::Int), None) => {
            returns.push(builder.ins().iconst(types::I64, 0));
        }
        (Some(TirTy::Float), None) => {
            returns.push(builder.ins().f64const(0.0));
        }
        (Some(TirTy::Bytes), None) | (Some(TirTy::Str), None) => {
            returns.push(builder.ins().iconst(types::I64, 0));
        }
        (Some(TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_)), None) => {
            returns.push(builder.ins().iconst(types::I64, 0));
        }
        (Some(TirTy::Bool), None) => {
            returns.push(builder.ins().iconst(types::I8, 0));
        }
        (None, _) => {
            if mode == NativeMode::Obj {
                returns.push(builder.ins().iconst(types::I64, 0));
            }
        }
    }
    if mode == NativeMode::Vm {
        let bail_v = if bail {
            builder.ins().iconst(types::I8, 1)
        } else {
            builder.ins().iconst(types::I8, 0)
        };
        returns.push(bail_v);
    }
    builder.ins().return_(&returns);
}

fn emit_block<M: Module>(
    body: &[TirStmt],
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    f: &TirFn,
) -> bool {
    for stmt in body {
        if emitter.terminated {
            return true;
        }
        if emit_stmt(stmt, emitter, builder, module, f) {
            return true;
        }
    }
    false
}

fn emit_stmt<M: Module>(
    stmt: &TirStmt,
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    f: &TirFn,
) -> bool {
    match stmt {
        TirStmt::Let(slot, expr) | TirStmt::Set(slot, expr) => {
            let var = emitter.vars[*slot as usize];
            let (value, value_ty) = emit_expr(expr, emitter, builder, module, f);
            let declared = emitter.local_tys[*slot as usize];
            let value = coerce_value(value, value_ty, declared, emitter, builder, module);
            builder.def_var(var, value);
            false
        }
        TirStmt::GlobalSet(index, expr) => {
            let (value, ty) = emit_expr(expr, emitter, builder, module, f);
            let value = wrap_as_obj(value, ty, emitter, builder, module);
            let id = emitter.unrt_global_set.expect("global import required");
            let r = module.declare_func_in_func(id, builder.func);
            let idx = builder.ins().iconst(types::I64, *index as i64);
            let _ = builder.ins().call(r, &[idx, value]);
            false
        }
        TirStmt::If { cond, then_body, else_body } => {
            let (cond_v, _) = emit_expr(cond, emitter, builder, module, f);
            let then_blk = builder.create_block();
            let else_blk = builder.create_block();
            let merge_blk = builder.create_block();
            builder.ins().brif(cond_v, then_blk, &[], else_blk, &[]);

            builder.switch_to_block(then_blk);
            emitter.terminated = false;            let then_terminated = emit_block(then_body, emitter, builder, module, f);
            if !then_terminated {
                builder.ins().jump(merge_blk, &[]);
            }
            builder.seal_block(then_blk);

            builder.switch_to_block(else_blk);
            emitter.terminated = false;            let else_terminated = emit_block(else_body, emitter, builder, module, f);
            if !else_terminated {
                builder.ins().jump(merge_blk, &[]);
            }
            builder.seal_block(else_blk);

            builder.switch_to_block(merge_blk);
            if then_terminated && else_terminated {
                builder.ins().jump(emitter.bail_blk, &[]);
                builder.seal_block(merge_blk);
                emitter.terminated = true;
                return true;
            }
            emitter.terminated = false;
            builder.seal_block(merge_blk);
            false
        }
        TirStmt::While { cond, body } => {
            let header = builder.create_block();
            let body_blk = builder.create_block();
            let exit_blk = builder.create_block();
            emitter.loop_breaks.push(exit_blk);
            emitter.loop_skips.push(header);
            builder.ins().jump(header, &[]);
            builder.switch_to_block(header);
            let (cond_v, _) = emit_expr(cond, emitter, builder, module, f);
            builder.ins().brif(cond_v, body_blk, &[], exit_blk, &[]);
            builder.switch_to_block(body_blk);
            emitter.terminated = false;            for s in body {
                if emitter.terminated {
                    break;
                }
                emit_stmt(s, emitter, builder, module, f);
            }
            if !emitter.terminated {
                builder.ins().jump(header, &[]);
            }
            builder.seal_block(body_blk);
            builder.seal_block(header);
            emitter.loop_breaks.pop();
            emitter.loop_skips.pop();
            emitter.terminated = false;
            builder.switch_to_block(exit_blk);
            builder.seal_block(exit_blk);
            false
        }
        TirStmt::ForRange { var, start, end, step, body } => {
            let start_v = emit_expr(start, emitter, builder, module, f).0;
            let end_v = emit_expr(end, emitter, builder, module, f).0;
            let step_v = emit_expr(step, emitter, builder, module, f).0;
            let step_sign = const_int_sign(step);
            let var_var = emitter.vars[*var as usize];
            builder.def_var(var_var, start_v);
            let header = builder.create_block();
            let body_blk = builder.create_block();
            let skip_blk = builder.create_block();
            let exit_blk = builder.create_block();
            emitter.loop_breaks.push(exit_blk);
            emitter.loop_skips.push(skip_blk);
            builder.ins().jump(header, &[]);
            builder.switch_to_block(header);
            let var_i = builder.use_var(var_var);
            let continue_ = if step_sign > 0 {
                builder.ins().icmp(IntCC::SignedLessThan, var_i, end_v)
            } else {
                builder.ins().icmp(IntCC::SignedGreaterThan, var_i, end_v)
            };
            builder.ins().brif(continue_, body_blk, &[], exit_blk, &[]);
            builder.switch_to_block(body_blk);
            emitter.terminated = false;            for s in body {
                if emitter.terminated {
                    break;
                }
                emit_stmt(s, emitter, builder, module, f);
            }
            if !emitter.terminated {
                builder.ins().jump(skip_blk, &[]);
            }
            builder.seal_block(body_blk);
            builder.switch_to_block(skip_blk);
            let var_i2 = builder.use_var(var_var);
            let next = builder.ins().iadd(var_i2, step_v);
            builder.def_var(var_var, next);
            builder.ins().jump(header, &[]);
            builder.seal_block(skip_blk);
            builder.seal_block(header);
            emitter.loop_breaks.pop();
            emitter.loop_skips.pop();
            emitter.terminated = false;
            builder.switch_to_block(exit_blk);
            builder.seal_block(exit_blk);
            false
        }
        TirStmt::ForEach { var, collection, body } => {
            let (col_v, col_ty) = emit_expr(collection, emitter, builder, module, f);
            let var_var = emitter.vars[*var as usize];
            let elem_ty = emitter.local_tys[*var as usize];
            let idx_var = builder.declare_var(types::I64);
            let zero = builder.ins().iconst(types::I64, 0);
            builder.def_var(idx_var, zero);
            // The trip count only needs one FFI call when the body cannot pop
            // elements from a list. Otherwise keep the per-iteration reload.
            let hoist_len = body_preserves_list_len(body);
            let hoisted_len = if hoist_len {
                let len_id = emitter.unrt_list_len.expect("unrt import required for lists");
                let len_ref = module.declare_func_in_func(len_id, builder.func);
                let len_res = builder.ins().call(len_ref, &[col_v]);
                Some(builder.inst_results(len_res)[0])
            } else {
                None
            };
            let header = builder.create_block();
            let body_blk = builder.create_block();
            let exit_blk = builder.create_block();
            emitter.loop_breaks.push(exit_blk);
            builder.ins().jump(header, &[]);
            builder.switch_to_block(header);
            let idx = builder.use_var(idx_var);
            let len_v = match hoisted_len {
                Some(v) => v,
                None => {
                    let len_id = emitter.unrt_list_len.expect("unrt import required for lists");
                    let len_ref = module.declare_func_in_func(len_id, builder.func);
                    let len_res = builder.ins().call(len_ref, &[col_v]);
                    builder.inst_results(len_res)[0]
                }
            };
            let cont = builder.ins().icmp(IntCC::SignedLessThan, idx, len_v);
            builder.ins().brif(cont, body_blk, &[], exit_blk, &[]);
            builder.switch_to_block(body_blk);
            let idx2 = builder.use_var(idx_var);
            let elem_val = if col_ty == TirTy::ListInt {
                let id = emitter.unrt_list_get_int.expect("unrt int-list import required");
                let f_ref = module.declare_func_in_func(id, builder.func);
                let res = builder.ins().call(f_ref, &[col_v, idx2]);
                builder.inst_results(res)[0]
            } else {
                let get_id = emitter.unrt_list_get.expect("unrt import required for lists");
                let get_ref = module.declare_func_in_func(get_id, builder.func);
                let elem_res = builder.ins().call(get_ref, &[col_v, idx2]);
                let elem_v = builder.inst_results(elem_res)[0];
                unbox_obj(elem_v, elem_ty, emitter, builder, module)
            };
            builder.def_var(var_var, elem_val);
            emitter.terminated = false;            for s in body {
                if emitter.terminated {
                    break;
                }
                emit_stmt(s, emitter, builder, module, f);
            }
            if !emitter.terminated {
                let i = builder.use_var(idx_var);
                let next = builder.ins().iadd_imm_s(i, 1);
                builder.def_var(idx_var, next);
                builder.ins().jump(header, &[]);
            }
            builder.seal_block(body_blk);
            builder.switch_to_block(exit_blk);
            emitter.loop_breaks.pop();
            emitter.loop_skips.pop();
            emitter.terminated = false;            builder.seal_block(exit_blk);
            builder.seal_block(header);
            false
        }
        TirStmt::Return(Some(expr)) => {
            let (value, value_ty) = emit_expr(expr, emitter, builder, module, f);
            let expected = f.ret.unwrap_or(TirTy::Bool);
            let value = coerce_value(value, value_ty, expected, emitter, builder, module);
            emit_internal_return(
                f,
                builder,
                module,
                emitter.fid_exit,
                emitter.acquired,
                Some(value),
                false,
                emitter.mode,
            );
            emitter.terminated = true;
            true
        }
        TirStmt::Return(None) => {
            emit_internal_return(
                f,
                builder,
                module,
                emitter.fid_exit,
                emitter.acquired,
                None,
                false,
                emitter.mode,
            );
            emitter.terminated = true;
            true
        }
        TirStmt::Break => {
            match emitter.loop_breaks.last().copied() {
                Some(exit_blk) => {
                    builder.ins().jump(exit_blk, &[]);
                }
                None => {
                    builder.ins().jump(emitter.bail_blk, &[]);
                }
            }
            emitter.terminated = true;
            true
        }
        TirStmt::Skip => {
            match emitter.loop_skips.last().copied() {
                Some(skip_blk) => {
                    builder.ins().jump(skip_blk, &[]);
                }
                None => {
                    builder.ins().jump(emitter.bail_blk, &[]);
                }
            }
            emitter.terminated = true;
            true
        }
        TirStmt::ExprStmt(expr) => {
            let _ = emit_expr(expr, emitter, builder, module, f);
            false
        }
        TirStmt::NativeCall { module: module_name, member, args } => {
            let id = emitter
                .unrt_call_native
                .expect("unrt import required for native calls");
            let mod_gv = embed_str_bytes(emitter, module, builder, module_name.as_bytes());
            let member_gv = embed_str_bytes(emitter, module, builder, member.as_bytes());
            let arg_count = args.len();
            let slot_data = cranelift_codegen::ir::StackSlotData::new(
                cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                8 * arg_count as u32,
                8,
            );
            let slot = builder.create_sized_stack_slot(slot_data);
            for (i, arg) in args.iter().enumerate() {
                let (v, vty) = emit_expr(arg, emitter, builder, module, f);
                let wrapped = match vty {
                    TirTy::Bytes | TirTy::Str => v,
                    TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) => v,
                    TirTy::Int => {
                        let id = emitter.unrt_obj_int.expect("unrt import required for native calls");
                        let int_ref = module.declare_func_in_func(id, builder.func);
                        let res = builder.ins().call(int_ref, &[v]);
                        builder.inst_results(res)[0]
                    }
                    TirTy::Bool => {
                        let id = emitter.unrt_obj_bool.expect("unrt import required for native calls");
                        let bool_ref = module.declare_func_in_func(id, builder.func);
                        let res = builder.ins().call(bool_ref, &[v]);
                        builder.inst_results(res)[0]
                    }
                    TirTy::Float => {
                        let id = emitter.unrt_obj_float.expect("unrt import required for native calls");
                        let float_ref = module.declare_func_in_func(id, builder.func);
                        let res = builder.ins().call(float_ref, &[v]);
                        builder.inst_results(res)[0]
                    }
                };
                let addr = builder.ins().stack_addr(types::I64, slot, 8 * i as i32);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), wrapped, addr, 0);
            }
            let sp = builder.ins().stack_addr(types::I64, slot, 0);
            let mod_ptr = builder.ins().symbol_value(types::I64, mod_gv);
            let mod_len = builder.ins().iconst(types::I64, module_name.as_bytes().len() as i64);
            let member_ptr = builder.ins().symbol_value(types::I64, member_gv);
            let member_len = builder.ins().iconst(types::I64, member.as_bytes().len() as i64);
            let argc = builder.ins().iconst(types::I64, arg_count as i64);
            let call_ref = module.declare_func_in_func(id, builder.func);
            let _ = builder.ins().call(call_ref, &[mod_ptr, mod_len, member_ptr, member_len, sp, argc]);
            false
        }
        TirStmt::ListSet { list, index, value } => {
            let (list_v, lty) = emit_expr(list, emitter, builder, module, f);
            let (idx_v, ity) = emit_expr(index, emitter, builder, module, f);
            if ity != TirTy::Int {
                panic!("list index must be int");
            }
            let (v_v, v_ty) = emit_expr(value, emitter, builder, module, f);
            // For `list[int]` with an i64 value the fused NRT ABI takes the raw
            // value, skipping the per-write boxing FFI. The result contract is
            // unchanged (returns `list` on success), so the existing
            // self-return gate still preserves errors, catch and traceback.
            let (set_id, written) = if lty == TirTy::ListInt && v_ty == TirTy::Int {
                (
                    emitter.unrt_list_set_int.expect("unrt int-list import required"),
                    v_v,
                )
            } else {
                (
                    emitter.unrt_list_set.expect("unrt import required for list assignment"),
                    wrap_as_obj(v_v, v_ty, emitter, builder, module),
                )
            };
            let set_ref = module.declare_func_in_func(set_id, builder.func);
            let res = builder.ins().call(set_ref, &[list_v, idx_v, written]);
            let raw = builder.inst_results(res)[0];
            // NRT returns `list` on success; only an out-of-range write yields
            // a different (error) handle, so no FFI tag checks are needed.
            emit_self_return_gate(emitter, builder, module, raw, list_v);
            false
        }
        TirStmt::ListAdd { receiver, arg } => {
            let (list_v, _) = emit_expr(receiver, emitter, builder, module, f);
            let (arg_v, arg_ty) = emit_expr(arg, emitter, builder, module, f);
            let obj = wrap_as_obj(arg_v, arg_ty, emitter, builder, module);
            let id = emitter.unrt_list_push.expect("unrt import required for list.add");
            let push_ref = module.declare_func_in_func(id, builder.func);
            let _ = builder.ins().call(push_ref, &[list_v, obj]);
            false
        }
        TirStmt::ListInsert { receiver, arg, index } => {
            let (list_v, _) = emit_expr(receiver, emitter, builder, module, f);
            let (arg_v, arg_ty) = emit_expr(arg, emitter, builder, module, f);
            let obj = wrap_as_obj(arg_v, arg_ty, emitter, builder, module);
            let (idx_v, ity) = emit_expr(index, emitter, builder, module, f);
            if ity != TirTy::Int {
                panic!("list insert index must be int");
            }
            let id = emitter.unrt_list_insert.expect("unrt import required for list.insert");
            let insert_ref = module.declare_func_in_func(id, builder.func);
            let _ = builder.ins().call(insert_ref, &[list_v, obj, idx_v]);
            false
        }
        TirStmt::ListReverse { receiver } => {
            let (list_v, _) = emit_expr(receiver, emitter, builder, module, f);
            let id = emitter.unrt_list_reverse.expect("unrt import required for list.reverse");
            let reverse_ref = module.declare_func_in_func(id, builder.func);
            let _ = builder.ins().call(reverse_ref, &[list_v]);
            false
        }
        TirStmt::ListSort { receiver } => {
            let (list_v, _) = emit_expr(receiver, emitter, builder, module, f);
            let id = emitter.unrt_list_sort.expect("unrt import required for list.sort");
            let sort_ref = module.declare_func_in_func(id, builder.func);
            let _ = builder.ins().call(sort_ref, &[list_v]);
            false
        }
        TirStmt::ListClear { receiver } => {
            let (list_v, _) = emit_expr(receiver, emitter, builder, module, f);
            // `unrt_list_pop` always succeeds while `len > 0`, so the loop
            // needs no error gate: the VM's `list.clear()` never raises.
            let len_id = emitter.unrt_list_len.expect("unrt list-len import required");
            let len_ref = module.declare_func_in_func(len_id, builder.func);
            let pop_id = emitter.unrt_list_pop.expect("unrt list-pop import required");
            let pop_ref = module.declare_func_in_func(pop_id, builder.func);
            let loop_blk = builder.create_block();
            let body_blk = builder.create_block();
            let done_blk = builder.create_block();
            builder.ins().jump(loop_blk, &[]);
            builder.switch_to_block(loop_blk);
            let len_res = builder.ins().call(len_ref, &[list_v]);
            let len = builder.inst_results(len_res)[0];
            let zero = builder.ins().iconst(types::I64, 0);
            let more = builder.ins().icmp(IntCC::SignedGreaterThan, len, zero);
            builder.ins().brif(more, body_blk, &[], done_blk, &[]);
            builder.switch_to_block(body_blk);
            let _ = builder.ins().call(pop_ref, &[list_v]);
            builder.ins().jump(loop_blk, &[]);
            builder.seal_block(body_blk);
            builder.seal_block(loop_blk);
            builder.switch_to_block(done_blk);
            builder.seal_block(done_blk);
            false
        }
        TirStmt::DictClear { receiver } => {
            let (dict_v, _) = emit_expr(receiver, emitter, builder, module, f);
            let id = emitter.unrt_dict_clear.expect("unrt import required for dict.clear");
            let clear_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(clear_ref, &[dict_v]);
            let raw = builder.inst_results(res)[0];
            emit_self_return_gate(emitter, builder, module, raw, dict_v);
            false
        }
        TirStmt::MemberSet { receiver, member, value } => {
            let (recv, _) = emit_expr(receiver, emitter, builder, module, f);
            let (v, v_ty) = emit_expr(value, emitter, builder, module, f);
            let v = wrap_as_obj(v, v_ty, emitter, builder, module);
            let gv = embed_str_bytes(emitter, module, builder, member.as_bytes());
            let nptr = builder.ins().symbol_value(types::I64, gv);
            let nlen = builder.ins().iconst(types::I64, member.as_bytes().len() as i64);
            let id = emitter.unrt_member_set.expect("unrt import required for classes");
            let set_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(set_ref, &[recv, nptr, nlen, v]);
            let obj = builder.inst_results(res)[0];
            // NRT returns the receiver on success; a mismatch is the only
            // possible failure (unknown field / bad receiver).
            emit_self_return_gate(emitter, builder, module, obj, recv);
            false
        }
        TirStmt::Throw(expr) => {
            let prev = emitter.current_span;
            if let TirExpr::Located { span, inner } = expr {
                emitter.current_span = Some(*span);
                let (value, ty) = emit_expr(inner, emitter, builder, module, f);
                let obj = wrap_as_obj(value, ty, emitter, builder, module);
                emit_set_error_site(emitter, builder, module);
                emitter.current_span = prev;
                let wid = emitter.unrt_write_error.expect("unrt import required");
                let wref = module.declare_func_in_func(wid, builder.func);
                let _ = builder.ins().call(wref, &[obj]);
                if let Some(&handler) = emitter.active_handlers.last() {
                    builder.ins().jump(handler, &[]);
                } else {
                    let d_id = emitter.unrt_dump_uncaught.expect("unrt import required");
                    let d_ref = module.declare_func_in_func(d_id, builder.func);
                    let _ = builder.ins().call(d_ref, &[]);
                    builder.ins().trap(cranelift_codegen::ir::TrapCode::unwrap_user(3));
                }
                emitter.terminated = true;
                return true;
            }
            let (value, ty) = emit_expr(expr, emitter, builder, module, f);
            let obj = wrap_as_obj(value, ty, emitter, builder, module);
            emit_set_error_site(emitter, builder, module);
            let wid = emitter.unrt_write_error.expect("unrt import required");
            let wref = module.declare_func_in_func(wid, builder.func);
            let _ = builder.ins().call(wref, &[obj]);
            if let Some(&handler) = emitter.active_handlers.last() {
                builder.ins().jump(handler, &[]);
            } else {
                let d_id = emitter.unrt_dump_uncaught.expect("unrt import required");
                let d_ref = module.declare_func_in_func(d_id, builder.func);
                let _ = builder.ins().call(d_ref, &[]);
                builder.ins().trap(cranelift_codegen::ir::TrapCode::unwrap_user(3));
            }
            emitter.terminated = true;
            true
        }
        TirStmt::Try { body, var, catch_body, finally_body } => {
            let success_blk = builder.create_block();
            let handler_blk = builder.create_block();
            let merge_blk = builder.create_block();
            emitter.active_handlers.push(handler_blk);
            emitter.terminated = false;
            let body_terminated = emit_block(body, emitter, builder, module, f);
            emitter.active_handlers.pop();
            if !body_terminated {
                builder.ins().jump(success_blk, &[]);
            }
            builder.switch_to_block(handler_blk);
            emitter.terminated = false;
            if let Some(slot) = var {
                let tid = emitter.unrt_take_error.expect("unrt import required");
                let snap_ref = module.declare_func_in_func(tid, builder.func);
                let res = builder.ins().call(snap_ref, &[]);
                let raw = builder.inst_results(res)[0];
                let var_var = emitter.vars[*slot as usize];
                builder.def_var(var_var, raw);
            } else {
                let tid = emitter.unrt_take_error.expect("unrt import required");
                let snap_ref = module.declare_func_in_func(tid, builder.func);
                let _ = builder.ins().call(snap_ref, &[]);
            }
            let catch_terminated = emit_block(catch_body, emitter, builder, module, f);
            if !catch_terminated {
                builder.ins().jump(success_blk, &[]);
            }
            builder.seal_block(handler_blk);
            builder.switch_to_block(success_blk);
            emitter.terminated = false;
            builder.seal_block(success_blk);
            if let Some(finally_body) = finally_body {
                let fin_terminated = emit_block(finally_body, emitter, builder, module, f);
                if fin_terminated {
                    emitter.terminated = true;
                    return true;
                }
                builder.ins().jump(merge_blk, &[]);
            } else {
                builder.ins().jump(merge_blk, &[]);
            }
            builder.switch_to_block(merge_blk);
            emitter.terminated = false;
            builder.seal_block(merge_blk);
            false
        }
        TirStmt::DictSet { dict, key, value } => {
            let (dict_v, dtype) = emit_expr(dict, emitter, builder, module, f);
            let (key_v, kty) = emit_expr(key, emitter, builder, module, f);
            if kty != TirTy::Str && kty != TirTy::Int {
                panic!("dict key must be str or int");
            }
            let (v_v, v_ty) = emit_expr(value, emitter, builder, module, f);
            if dtype == TirTy::DictIntInt && kty == TirTy::Int && v_ty == TirTy::Int {
                // The fused `unrt_dict_set_int` ABI always returns the dict
                // handle (it never produces an error object), so the two FFI
                // tag checks in `emit_error_gate` would be dead code here.
                let id = emitter.unrt_dict_set_int.expect("unrt int-dict import required");
                let f_ref = module.declare_func_in_func(id, builder.func);
                let _ = builder.ins().call(f_ref, &[dict_v, key_v, v_v]);
                return false;
            }
            let key_v = wrap_as_obj(key_v, kty, emitter, builder, module);
            let wrapped = wrap_as_obj(v_v, v_ty, emitter, builder, module);
            let set_id = emitter.unrt_dict_set.expect("unrt import required for dict assignment");
            let set_ref = module.declare_func_in_func(set_id, builder.func);
            let res = builder.ins().call(set_ref, &[dict_v, key_v, wrapped]);
            let raw = builder.inst_results(res)[0];
            // NRT returns the dict handle on success.
            emit_self_return_gate(emitter, builder, module, raw, dict_v);
            false
        }
    }
}

/// True when evaluating `e` cannot remove elements from a list. Used to decide
/// whether a `for x in list` bound can be computed once instead of every
/// iteration (the standalone NRT keeps a live list, so a `ListPop` in the body
/// could otherwise change the trip count).
fn expr_preserves_list_len(e: &TirExpr) -> bool {
    match e {
        TirExpr::ListPop(_) | TirExpr::ListPopAt(_, _) => false,
        TirExpr::Int(_) | TirExpr::Float(_) | TirExpr::Bool(_) | TirExpr::Str(_)
        | TirExpr::Local(_) | TirExpr::NoneValue | TirExpr::GlobalGet(_) => true,
        TirExpr::Promote(x) | TirExpr::CastInt(x) | TirExpr::CastFloat(x) | TirExpr::Un(_, x)
        | TirExpr::Len(x) | TirExpr::MemberGet { receiver: x, .. }
        | TirExpr::DictKeys(x) | TirExpr::DictValues(x)
        | TirExpr::Located { inner: x, .. } => expr_preserves_list_len(x),
        TirExpr::ListContains(a, b) | TirExpr::ListIndexOf(a, b) => {
            expr_preserves_list_len(a) && expr_preserves_list_len(b)
        }
        // `list.remove` shrinks its receiver.
        TirExpr::ListRemove(..) => false,
        TirExpr::Select { cond, then_expr, else_expr } => {
            expr_preserves_list_len(cond)
                && expr_preserves_list_len(then_expr)
                && expr_preserves_list_len(else_expr)
        }
        TirExpr::Bin(_, a, b) | TirExpr::Cmp(_, a, b) | TirExpr::DictGet(a, b)
        | TirExpr::ListGet(a, b) => expr_preserves_list_len(a) && expr_preserves_list_len(b),
        TirExpr::List(items) => items.iter().all(expr_preserves_list_len),
        TirExpr::Dict(pairs) => pairs.iter().all(|(k, v)| {
            expr_preserves_list_len(k) && expr_preserves_list_len(v)
        }),
        TirExpr::ListRepeat { value, len, .. } => {
            expr_preserves_list_len(value) && expr_preserves_list_len(len)
        }
        // A call may mutate any list reachable from its arguments (or a
        // captured/global one), so never assume it preserves list length.
        TirExpr::NativeCallExpr { .. } | TirExpr::ClassNew { .. }
        | TirExpr::Math(..) | TirExpr::Call(..)
        | TirExpr::CallObj { .. } | TirExpr::MethodCall { .. } => false,
        TirExpr::MakeClosure { .. } => true,
    }
}

/// True when none of the statements in `body` can change a list's length.
fn body_preserves_list_len(body: &[TirStmt]) -> bool {
    body.iter().all(|s| match s {
        // The only statements that grow or shrink a list.
        TirStmt::ListAdd { .. } | TirStmt::ListInsert { .. } | TirStmt::ListClear { .. } => false,
        TirStmt::Let(_, e) | TirStmt::Set(_, e) | TirStmt::ExprStmt(e) | TirStmt::Throw(e)
        | TirStmt::Return(Some(e)) => expr_preserves_list_len(e),
        // Indexed writes, member writes and reordering keep the length intact.
        TirStmt::Return(None) | TirStmt::Break | TirStmt::Skip | TirStmt::ListSet { .. }
        | TirStmt::DictSet { .. } | TirStmt::DictClear { .. } | TirStmt::MemberSet { .. }
        | TirStmt::GlobalSet(_, _) | TirStmt::ListReverse { .. } | TirStmt::ListSort { .. } => true,
        TirStmt::If { cond, then_body, else_body } => {
            expr_preserves_list_len(cond)
                && body_preserves_list_len(then_body)
                && body_preserves_list_len(else_body)
        }
        TirStmt::While { cond, body } => {
            expr_preserves_list_len(cond) && body_preserves_list_len(body)
        }
        TirStmt::ForRange { start, end, step, body, .. } => {
            expr_preserves_list_len(start)
                && expr_preserves_list_len(end)
                && expr_preserves_list_len(step)
                && body_preserves_list_len(body)
        }
        TirStmt::ForEach { collection, body, .. } => {
            expr_preserves_list_len(collection) && body_preserves_list_len(body)
        }
        TirStmt::NativeCall { args, .. } => args.iter().all(expr_preserves_list_len),
        TirStmt::Try { body, catch_body, finally_body, .. } => {
            body_preserves_list_len(body)
                && body_preserves_list_len(catch_body)
                && finally_body.as_ref().map_or(true, |b| body_preserves_list_len(b))
        }
    })
}

fn const_int_sign(step: &TirExpr) -> i64 {
    match step {
        TirExpr::Int(v) => {
            if *v > 0 {
                1
            } else if *v < 0 {
                -1
            } else {
                0
            }
        }
        _ => 1,
    }
}

fn list_elem_unbox<M: Module>(
    list_ty: TirTy,
    emitter: &FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    raw: cranelift_codegen::ir::Value,
) -> (cranelift_codegen::ir::Value, TirTy) {
    match list_ty {
        TirTy::ListInt => {
            let id = emitter.unrt_int_from_obj.expect("unrt import required for list element");
            let ub_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(ub_ref, &[raw]);
            (builder.inst_results(res)[0], TirTy::Int)
        }
        TirTy::ListFloat => {
            let id = emitter.unrt_float_from_obj.expect("unrt import required for list element");
            let ub_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(ub_ref, &[raw]);
            (builder.inst_results(res)[0], TirTy::Float)
        }
        TirTy::ListClass(id) => (raw, TirTy::Class(id)),
        TirTy::ListLambda => (raw, TirTy::Lambda(u32::MAX)),
        TirTy::ListGen(elem) => {
            let ty = tir_of_elem(elem);
            (unbox_obj(raw, ty, emitter, builder, module), ty)
        }
        _ => panic!("list method requires a list"),
    }
}

fn emit_set_error_site<M: Module>(
    emitter: &FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
) {
    if let Some(id) = emitter.nrt_set_error_site {
        let (start, len) = emitter.current_span.unwrap_or((0, 0));
        let s = builder.ins().iconst(types::I64, start as i64);
        let l = builder.ins().iconst(types::I64, len as i64);
        let r = module.declare_func_in_func(id, builder.func);
        let _ = builder.ins().call(r, &[s, l]);
    }
}

/// Fast error gate for NRT calls whose success contract is "return the input
/// handle unchanged". `unrt_list_set`, `unrt_dict_set` and `unrt_member_set`
/// all return their first argument on success and a freshly allocated error
/// object (tag 5) on failure; they never produce a panic (tag 10). Comparing
/// the result pointer against the input therefore detects every possible
/// failure with a single integer compare, replacing the two FFI tag checks in
/// `emit_error_gate`. The error path (source span, message, traceback, catch
/// dispatch) is byte-for-byte the same as the generic gate.
fn emit_self_return_gate<M: Module>(
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    result: cranelift_codegen::ir::Value,
    expected: cranelift_codegen::ir::Value,
) {
    let ok = builder.ins().icmp(IntCC::Equal, result, expected);
    let ok_blk = builder.create_block();
    let err_blk = builder.create_block();
    builder.ins().brif(ok, ok_blk, &[], err_blk, &[]);
    builder.seal_block(ok_blk);
    builder.switch_to_block(err_blk);
    emit_set_error_site(emitter, builder, module);
    let wid = emitter.unrt_write_error.expect("unrt import required");
    let wref = module.declare_func_in_func(wid, builder.func);
    let _ = builder.ins().call(wref, &[result]);
    match emitter.active_handlers.last().copied() {
        Some(handler) => {
            builder.ins().jump(handler, &[]);
        }
        None => {
            let d_id = emitter.unrt_dump_uncaught.expect("unrt import required");
            let d_ref = module.declare_func_in_func(d_id, builder.func);
            let _ = builder.ins().call(d_ref, &[]);
            builder.ins().trap(cranelift_codegen::ir::TrapCode::unwrap_user(3));
        }
    }
    builder.seal_block(err_blk);
    builder.switch_to_block(ok_blk);
}

/// Error gate for NRT calls whose contract only ever yields a regular error
/// (tag 5) on failure, never a panic (tag 10). Skips the `unrt_obj_is_panic`
/// FFI while keeping the exact same error path (span, message, traceback).
fn emit_error_gate_regular<M: Module>(
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    obj: cranelift_codegen::ir::Value,
) {
    let is_err_id = match emitter.unrt_obj_is_err {
        Some(id) => id,
        None => return,
    };
    let is_err_ref = module.declare_func_in_func(is_err_id, builder.func);
    let res = builder.ins().call(is_err_ref, &[obj]);
    let raw = builder.inst_results(res)[0];
    let is_err = builder.ins().ireduce(types::I8, raw);
    let err_blk = builder.create_block();
    let ok_blk = builder.create_block();
    builder.ins().brif(is_err, err_blk, &[], ok_blk, &[]);
    builder.seal_block(ok_blk);
    builder.switch_to_block(err_blk);
    emit_set_error_site(emitter, builder, module);
    let wid = emitter.unrt_write_error.expect("unrt import required");
    let wref = module.declare_func_in_func(wid, builder.func);
    let _ = builder.ins().call(wref, &[obj]);
    match emitter.active_handlers.last().copied() {
        Some(handler) => {
            builder.ins().jump(handler, &[]);
        }
        None => {
            let d_id = emitter.unrt_dump_uncaught.expect("unrt import required");
            let d_ref = module.declare_func_in_func(d_id, builder.func);
            let _ = builder.ins().call(d_ref, &[]);
            builder.ins().trap(cranelift_codegen::ir::TrapCode::unwrap_user(3));
        }
    }
    builder.seal_block(err_blk);
    builder.switch_to_block(ok_blk);
}

fn emit_error_gate<M: Module>(
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    obj: cranelift_codegen::ir::Value,
) {
    // Panics bypass catch handlers and terminate, like the VM.
    if let Some(id) = emitter.unrt_obj_is_panic {
        let r = module.declare_func_in_func(id, builder.func);
        let res = builder.ins().call(r, &[obj]);
        let raw_flag = builder.inst_results(res)[0];
        let flag = builder.ins().ireduce(types::I8, raw_flag);
        let panic_blk = builder.create_block();
        let cont_blk = builder.create_block();
        builder.ins().brif(flag, panic_blk, &[], cont_blk, &[]);
        builder.seal_block(panic_blk);
        builder.switch_to_block(panic_blk);
        if let Some(w) = emitter.unrt_write_error {
            let wref = module.declare_func_in_func(w, builder.func);
            let _ = builder.ins().call(wref, &[obj]);
        }
        let d_id = emitter.unrt_dump_uncaught.expect("unrt import required");
        let d_ref = module.declare_func_in_func(d_id, builder.func);
        let _ = builder.ins().call(d_ref, &[]);
        builder.ins().trap(cranelift_codegen::ir::TrapCode::unwrap_user(3));
        builder.seal_block(cont_blk);
        builder.switch_to_block(cont_blk);
    }
    let is_err_id = match emitter.unrt_obj_is_err {
        Some(id) => id,
        None => return,
    };
    let is_err_ref = module.declare_func_in_func(is_err_id, builder.func);
    let res = builder.ins().call(is_err_ref, &[obj]);
    let raw = builder.inst_results(res)[0];
    let is_err = builder.ins().ireduce(types::I8, raw);
    let err_blk = builder.create_block();
    let ok_blk = builder.create_block();
    builder.ins().brif(is_err, err_blk, &[], ok_blk, &[]);
    builder.seal_block(ok_blk);
    builder.switch_to_block(err_blk);
    match emitter.active_handlers.last().copied() {
        Some(handler) => {
            emit_set_error_site(emitter, builder, module);
            let wid = emitter.unrt_write_error.expect("unrt import required");
            let wref = module.declare_func_in_func(wid, builder.func);
            let _ = builder.ins().call(wref, &[obj]);
            builder.ins().jump(handler, &[]);
        }
        None => {
            emit_set_error_site(emitter, builder, module);
            let wid = emitter.unrt_write_error.expect("unrt import required");
            let wref = module.declare_func_in_func(wid, builder.func);
            let _ = builder.ins().call(wref, &[obj]);
            let d_id = emitter.unrt_dump_uncaught.expect("unrt import required");
            let d_ref = module.declare_func_in_func(d_id, builder.func);
            let _ = builder.ins().call(d_ref, &[]);
            builder.ins().trap(cranelift_codegen::ir::TrapCode::unwrap_user(3));
        }
    }
    builder.seal_block(err_blk);
    builder.switch_to_block(ok_blk);
}

fn emit_int_zero_guard<M: Module>(
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    divisor: cranelift_codegen::ir::Value,
) {
    let zero = builder.ins().iconst(types::I64, 0);
    let is_zero = builder.ins().icmp(IntCC::Equal, divisor, zero);
    let ok_blk = builder.create_block();
    if emitter.mode == NativeMode::Obj {
        let fail_blk = builder.create_block();
        builder.ins().brif(is_zero, fail_blk, &[], ok_blk, &[]);
        builder.seal_block(ok_blk);
        builder.switch_to_block(fail_blk);
        let mid = emitter.unrt_make_int_div_zero.expect("unrt import required");
        let mref = module.declare_func_in_func(mid, builder.func);
        let res = builder.ins().call(mref, &[]);
        let err = builder.inst_results(res)[0];
        emit_set_error_site(emitter, builder, module);
            let wid = emitter.unrt_write_error.expect("unrt import required");
        let wref = module.declare_func_in_func(wid, builder.func);
        let _ = builder.ins().call(wref, &[err]);
        if let Some(&handler) = emitter.active_handlers.last() {
            builder.ins().jump(handler, &[]);
        } else {
            let d_id = emitter.unrt_dump_uncaught.expect("unrt import required");
            let d_ref = module.declare_func_in_func(d_id, builder.func);
            let _ = builder.ins().call(d_ref, &[]);
            builder.ins().trap(cranelift_codegen::ir::TrapCode::unwrap_user(3));
        }
        builder.seal_block(fail_blk);
        builder.switch_to_block(ok_blk);
    } else {
        let bail_blk = emitter.bail_blk;
        builder.ins().brif(is_zero, bail_blk, &[], ok_blk, &[]);
        builder.seal_block(ok_blk);
        builder.switch_to_block(ok_blk);
    }
}

fn emit_mod_zero_guard<M: Module>(
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    divisor: cranelift_codegen::ir::Value,
) {
    let zero = builder.ins().iconst(types::I64, 0);
    let is_zero = builder.ins().icmp(IntCC::Equal, divisor, zero);
    let ok_blk = builder.create_block();
    if emitter.mode == NativeMode::Obj {
        let fail_blk = builder.create_block();
        builder.ins().brif(is_zero, fail_blk, &[], ok_blk, &[]);
        builder.seal_block(ok_blk);
        builder.switch_to_block(fail_blk);
        let mid = emitter.unrt_make_mod_zero_panic.expect("unrt import required");
        let mref = module.declare_func_in_func(mid, builder.func);
        let res = builder.ins().call(mref, &[]);
        let err = builder.inst_results(res)[0];
        emit_set_error_site(emitter, builder, module);
            let wid = emitter.unrt_write_error.expect("unrt import required");
        let wref = module.declare_func_in_func(wid, builder.func);
        let _ = builder.ins().call(wref, &[err]);
        let d_id = emitter.unrt_dump_uncaught.expect("unrt import required");
        let d_ref = module.declare_func_in_func(d_id, builder.func);
        let _ = builder.ins().call(d_ref, &[]);
        builder.ins().trap(cranelift_codegen::ir::TrapCode::unwrap_user(3));
        builder.seal_block(fail_blk);
        builder.switch_to_block(ok_blk);
    } else {
        let bail_blk = emitter.bail_blk;
        builder.ins().brif(is_zero, bail_blk, &[], ok_blk, &[]);
        builder.seal_block(ok_blk);
        builder.switch_to_block(ok_blk);
    }
}

fn emit_math_sqrt_guard<M: Module>(
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    x: cranelift_codegen::ir::Value,
) {
    let zero = builder.ins().f64const(0.0);
    let cond = builder.ins().fcmp(FloatCC::LessThan, x, zero);
    let ok_blk = builder.create_block();
    let fail_blk = builder.create_block();
    builder.ins().brif(cond, fail_blk, &[], ok_blk, &[]);
    builder.seal_block(ok_blk);
    builder.switch_to_block(fail_blk);
    if emitter.mode == NativeMode::Obj {
        let mid = emitter.unrt_make_math_sqrt_neg.expect("unrt import required");
        let mref = module.declare_func_in_func(mid, builder.func);
        let res = builder.ins().call(mref, &[]);
        let err = builder.inst_results(res)[0];
        emit_set_error_site(emitter, builder, module);
            let wid = emitter.unrt_write_error.expect("unrt import required");
        let wref = module.declare_func_in_func(wid, builder.func);
        let _ = builder.ins().call(wref, &[err]);
        if let Some(&handler) = emitter.active_handlers.last() {
            builder.ins().jump(handler, &[]);
        } else {
            let d_id = emitter.unrt_dump_uncaught.expect("unrt import required");
            let d_ref = module.declare_func_in_func(d_id, builder.func);
            let _ = builder.ins().call(d_ref, &[]);
            builder.ins().trap(cranelift_codegen::ir::TrapCode::unwrap_user(3));
        }
    } else {
        let bail_blk = emitter.bail_blk;
        builder.ins().jump(bail_blk, &[]);
    }
    builder.seal_block(fail_blk);
    builder.switch_to_block(ok_blk);
}

fn emit_math_log_guard<M: Module>(
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    x: cranelift_codegen::ir::Value,
) {
    let zero = builder.ins().f64const(0.0);
    let cond = builder.ins().fcmp(FloatCC::LessThanOrEqual, x, zero);
    let ok_blk = builder.create_block();
    let fail_blk = builder.create_block();
    builder.ins().brif(cond, fail_blk, &[], ok_blk, &[]);
    builder.seal_block(ok_blk);
    builder.switch_to_block(fail_blk);
    if emitter.mode == NativeMode::Obj {
        let mid = emitter.unrt_make_math_expected_pos.expect("unrt import required");
        let mref = module.declare_func_in_func(mid, builder.func);
        let res = builder.ins().call(mref, &[]);
        let err = builder.inst_results(res)[0];
        emit_set_error_site(emitter, builder, module);
            let wid = emitter.unrt_write_error.expect("unrt import required");
        let wref = module.declare_func_in_func(wid, builder.func);
        let _ = builder.ins().call(wref, &[err]);
        if let Some(&handler) = emitter.active_handlers.last() {
            builder.ins().jump(handler, &[]);
        } else {
            let d_id = emitter.unrt_dump_uncaught.expect("unrt import required");
            let d_ref = module.declare_func_in_func(d_id, builder.func);
            let _ = builder.ins().call(d_ref, &[]);
            builder.ins().trap(cranelift_codegen::ir::TrapCode::unwrap_user(3));
        }
    } else {
        let bail_blk = emitter.bail_blk;
        builder.ins().jump(bail_blk, &[]);
    }
    builder.seal_block(fail_blk);
    builder.switch_to_block(ok_blk);
}

fn emit_math_inline<M: Module>(
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    mf: crate::tir::MathFn,
    args: &[cranelift_codegen::ir::Value],
) -> cranelift_codegen::ir::Value {
    use crate::tir::MathFn::*;
    match mf {
        Sqrt => emit_math_sqrt_guard(emitter, builder, module, args[0]),
        Ln => emit_math_log_guard(emitter, builder, module, args[0]),
        Log => emit_math_log_guard(emitter, builder, module, args[0]),
        Log2 => emit_math_log_guard(emitter, builder, module, args[0]),
        Log10 => emit_math_log_guard(emitter, builder, module, args[0]),
        _ => {}
    }
    let fname = match mf {
        // In the standalone object build libm is already linked, so call
        // `sin` directly instead of going through NRT's one-line wrapper.
        // The Vm/JIT path keeps `un_math_sin` because JIT symbol resolution
        // only registers the `un_math_*` names.
        Sin if emitter.mode == NativeMode::Obj => "sin",
        Sin => "un_math_sin",
        Cos => "un_math_cos",
        Tan => "un_math_tan",
        Asin => "un_math_asin",
        Acos => "un_math_acos",
        Atan => "un_math_atan",
        Sqrt => "un_math_sqrt",
        Cbrt => "un_math_cbrt",
        Exp => "un_math_exp",
        Ln => "un_math_ln",
        Log => "un_math_ln",
        Log2 => "un_math_log2",
        Log10 => "un_math_log10",
        Floor => "un_math_floor",
        Ceil => "un_math_ceil",
        Trunc => "un_math_trunc",
        Abs => "un_math_abs",
        Pow => "un_math_powf",
    };
    match mf {
        Pow => emit_math_ffi_call(emitter, builder, module, "un_math_powf", args),
        Log => {
            if args.len() == 1 {
                emit_math_ffi_call(emitter, builder, module, "un_math_ln", args)
            } else {
                emit_math_ffi_call(emitter, builder, module, "un_math_log", args)
            }
        }
        _ => emit_math_ffi_call(emitter, builder, module, fname, args),
    }
}

fn emit_math_ffi_call<M: Module>(
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    symbol: &str,
    args: &[cranelift_codegen::ir::Value],
) -> cranelift_codegen::ir::Value {
    let n_params = args.len();
    let fid = match emitter.math_ffi_fids.get(symbol) {
        Some(&fid) => fid,
        None => {
            let mut sig = module.make_signature();
            for _ in 0..n_params {
                sig.params.push(AbiParam::new(types::F64));
            }
            sig.returns.push(AbiParam::new(types::F64));
            let fid = module
                .declare_function(symbol, cranelift_module::Linkage::Import, &sig)
                .expect("declare un_math FFI import");
            emitter.math_ffi_fids.insert(symbol.to_string(), fid);
            fid
        }
    };
    let callee_ref = module.declare_func_in_func(fid, builder.func);
    let res = builder.ins().call(callee_ref, args);
    builder.inst_results(res)[0]
}

fn wrap_as_obj<M: Module>(
    v: cranelift_codegen::ir::Value,
    ty: TirTy,
    emitter: &FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
) -> cranelift_codegen::ir::Value {
    match ty {
        TirTy::Int => {
            let id = emitter.unrt_obj_int.expect("unrt import required for collections");
            let int_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(int_ref, &[v]);
            builder.inst_results(res)[0]
        }
        TirTy::Float => {
            let id = emitter.unrt_obj_float.expect("unrt import required for collections");
            let float_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(float_ref, &[v]);
            builder.inst_results(res)[0]
        }
        TirTy::Bool => {
            let id = emitter.unrt_obj_bool.expect("unrt import required for collections");
            let bool_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(bool_ref, &[v]);
            builder.inst_results(res)[0]
        }
        TirTy::Bytes | TirTy::Str | TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat
        | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) => v,
    }
}

fn unbox_obj<M: Module>(
    obj: cranelift_codegen::ir::Value,
    ty: TirTy,
    emitter: &FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
) -> cranelift_codegen::ir::Value {
    match ty {
        TirTy::Int => {
            let id = emitter.unrt_int_from_obj.expect("unrt import required for int unbox");
            let int_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(int_ref, &[obj]);
            builder.inst_results(res)[0]
        }
        TirTy::Float => {
            let id = emitter.unrt_float_from_obj.expect("unrt import required for float unbox");
            let float_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(float_ref, &[obj]);
            builder.inst_results(res)[0]
        }
        TirTy::Bool => {
            let id = emitter.unrt_bool_from_obj.expect("unrt import required for bool unbox");
            let bool_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(bool_ref, &[obj]);
            let raw = builder.inst_results(res)[0];
            builder.ins().ireduce(types::I8, raw)
        }
        _ => obj,
    }
}

fn emit_expr<M: Module>(
    expr: &TirExpr,
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    f: &TirFn,
) -> (cranelift_codegen::ir::Value, TirTy) {
    match expr {
        TirExpr::NoneValue => {
            let id = emitter.unrt_none.expect("unrt import required for None");
            let r = module.declare_func_in_func(id, builder.func);
            let call = builder.ins().call(r, &[]);
            (builder.inst_results(call)[0], TirTy::Str)
        }
        TirExpr::GlobalGet(index) => {
            let id = emitter.unrt_global_get.expect("global import required");
            let r = module.declare_func_in_func(id, builder.func);
            let idx = builder.ins().iconst(types::I64, *index as i64);
            let call = builder.ins().call(r, &[idx]);
            let obj = builder.inst_results(call)[0];
            (unbox_obj(obj, TirTy::Int, emitter, builder, module), TirTy::Int)
        }
        TirExpr::Located { span, inner } => {
            let prev = emitter.current_span;
            emitter.current_span = Some(*span);
            let result = emit_expr(inner, emitter, builder, module, f);
            emitter.current_span = prev;
            result
        }
        TirExpr::MakeClosure { func_index, captures, .. } => {
            let index = builder.ins().iconst(types::I64, *func_index as i64);
            let slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                (8 * captures.len()) as u32,
                8,
            ));
            let capture_tys = emitter.fns.get(*func_index as usize)
                .map(|f| f.params[..captures.len().min(f.params.len())].to_vec())
                .unwrap_or_default();
            for (i, slot_idx) in captures.iter().enumerate() {
                let v = builder.use_var(emitter.vars[*slot_idx as usize]);
                let ty = capture_tys.get(i).copied().unwrap_or(TirTy::Int);
                let v = wrap_as_obj(v, ty, emitter, builder, module);
                let addr = builder.ins().stack_addr(types::I64, slot, (8 * i) as i32);
                builder.ins().store(MemFlagsData::new(), v, addr, 0);
            }
            let cap_ptr = builder.ins().stack_addr(types::I64, slot, 0);
            let cap_len = builder.ins().iconst(types::I64, captures.len() as i64);
            let id = emitter.unrt_obj_closure.expect("closure import required");
            let r = module.declare_func_in_func(id, builder.func);
            let call = builder.ins().call(r, &[index, cap_ptr, cap_len]);
            (builder.inst_results(call)[0], TirTy::Lambda(*func_index))
        }
        TirExpr::CallObj { callee, args, ret } => {
            let (cv, _) = emit_expr(callee, emitter, builder, module, f);
            if emitter.mode == NativeMode::Obj && emitter.needs_frame {
                if let Some(push) = emitter.nrt_push_frame {
                    let gv = embed_str_bytes(emitter, module, builder, f.name.as_bytes());
                    let np = builder.ins().symbol_value(types::I64, gv);
                    let nl = builder.ins().iconst(types::I64, f.name.as_bytes().len() as i64);
                    let (start, len) = emitter.current_span.unwrap_or((0, 0));
                    let s = builder.ins().iconst(types::I64, start as i64);
                    let l = builder.ins().iconst(types::I64, len as i64);
                    let push_ref = module.declare_func_in_func(push, builder.func);
                    let _ = builder.ins().call(push_ref, &[np, nl, s, l]);
                }
            }
            let mut vals = Vec::with_capacity(args.len());
            for a in args {
                let (v, ty) = emit_expr(a, emitter, builder, module, f);
                vals.push(wrap_as_obj(v, ty, emitter, builder, module));
            }
            let slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                (8 * vals.len()) as u32,
                8,
            ));
            for (i, v) in vals.iter().enumerate() {
                let addr = builder.ins().stack_addr(types::I64, slot, (8 * i) as i32);
                builder.ins().store(MemFlagsData::new(), *v, addr, 0);
            }
            let ap = builder.ins().stack_addr(types::I64, slot, 0);
            let ac = builder.ins().iconst(types::I64, vals.len() as i64);
            let id = emitter.unrt_call_obj.expect("closure call import required");
            let r = module.declare_func_in_func(id, builder.func);
            let call = builder.ins().call(r, &[cv, ap, ac]);
            if emitter.mode == NativeMode::Obj && emitter.needs_frame {
                if let Some(pop) = emitter.nrt_pop_frame {
                    let pop_ref = module.declare_func_in_func(pop, builder.func);
                    let _ = builder.ins().call(pop_ref, &[]);
                }
            }
            let obj = builder.inst_results(call)[0];
            emit_error_gate(emitter, builder, module, obj);
            (unbox_obj(obj, *ret, emitter, builder, module), *ret)
        }
        TirExpr::Int(v) => (builder.ins().iconst(types::I64, *v), TirTy::Int),
        TirExpr::Str(s) => {
            let id = emitter.unrt_str_new.expect("unrt import required for Str");
            let gv = embed_str_bytes(emitter, module, builder, s.as_bytes());
            let ptr = builder.ins().symbol_value(types::I64, gv);
            let len = builder.ins().iconst(types::I64, s.as_bytes().len() as i64);
            let str_new_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(str_new_ref, &[ptr, len]);
            let obj = builder.inst_results(res)[0];
            (obj, TirTy::Str)
        }
        TirExpr::Float(v) => (builder.ins().f64const(*v), TirTy::Float),
        TirExpr::List(elements) => {
            if elements.is_empty() {
                panic!("empty list literal rejected by validator");
            }
            let slot_t = builder.create_sized_stack_slot(
                cranelift_codegen::ir::StackSlotData::new(
                    cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                    (8 * elements.len()) as u32,
                    8,
                ),
            );
            let mut elem_ty = None;
            let mut vals = Vec::with_capacity(elements.len());
            for element in elements.iter() {
                let (v, ty) = emit_expr(element, emitter, builder, module, f);
                elem_ty = Some(match elem_ty {
                    None => ty,
                    Some(prev) if prev == ty => prev,
                    // Mixed `list[any]`: every element must be a boxed handle.
                    Some(prev) if prev == TirTy::Obj || ty == TirTy::Obj => TirTy::Obj,
                    // Heterogeneous scalar lists are boxed as `any` elements.
                    Some(_) => TirTy::Obj,
                });
                vals.push((v, ty));
            }
            let elem_ty = elem_ty.expect("non-empty list");
            for (i, (v, ty)) in vals.into_iter().enumerate() {
                // Scalars box at the dynamic collection boundary; for a typed
                // `list[int]`/`list[float]` the raw value is stored directly.
                let v = if matches!(ty, TirTy::Int | TirTy::Float) && elem_ty != TirTy::Obj {
                    v
                } else {
                    wrap_as_obj(v, ty, emitter, builder, module)
                };
                let addr = builder.ins().stack_addr(types::I64, slot_t, (8 * i) as i32);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), v, addr, 0);
            }
            let ptr = builder.ins().stack_addr(types::I64, slot_t, 0);
            let len = builder.ins().iconst(types::I64, elements.len() as i64);
            let (from_fid, ret_ty) = match elem_ty {
                TirTy::Int => (
                    emitter.unrt_list_from_ints.expect("unrt import required for lists"),
                    TirTy::ListInt,
                ),
                TirTy::Float => (
                    emitter.unrt_list_from_floats.expect("unrt import required for lists"),
                    TirTy::ListFloat,
                ),
                TirTy::Class(id) => (
                    emitter.unrt_list_from_elems.expect("unrt import required for class lists"),
                    TirTy::ListClass(id),
                ),
                TirTy::Lambda(_) => (
                    emitter.unrt_list_from_elems.expect("unrt import required for lambda lists"),
                    TirTy::ListLambda,
                ),
                other => (
                    emitter.unrt_list_from_elems.expect("unrt import required for generic lists"),
                    TirTy::ListGen(elem_of_tir(other)),
                ),
            };
            let from_ref = module.declare_func_in_func(from_fid, builder.func);
            let res = builder.ins().call(from_ref, &[ptr, len]);
            let obj = builder.inst_results(res)[0];
            (obj, ret_ty)
        }
        TirExpr::ClassNew { name, args, ret } => {
            let arg_vals: Vec<_> = args
                .iter()
                .map(|a| emit_expr(a, emitter, builder, module, f))
                .collect();
            let slot = builder.create_sized_stack_slot(
                cranelift_codegen::ir::StackSlotData::new(
                    cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                    (8 * arg_vals.len()) as u32,
                    8,
                ),
            );
            for (i, (v, ty)) in arg_vals.iter().enumerate() {
                let v = wrap_as_obj(*v, *ty, emitter, builder, module);
                let addr = builder.ins().stack_addr(types::I64, slot, (8 * i) as i32);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), v, addr, 0);
            }
            let gv = embed_str_bytes(emitter, module, builder, name.as_bytes());
            let nptr = builder.ins().symbol_value(types::I64, gv);
            let nlen = builder.ins().iconst(types::I64, name.as_bytes().len() as i64);
            let ptr = builder.ins().stack_addr(types::I64, slot, 0);
            let argc = builder.ins().iconst(types::I64, args.len() as i64);
            let id = emitter.unrt_class_new.expect("unrt import required for classes");
            let new_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(new_ref, &[nptr, nlen, ptr, argc]);
            let obj = builder.inst_results(res)[0];
            emit_error_gate_regular(emitter, builder, module, obj);
            (obj, *ret)
        }
        TirExpr::MemberGet { receiver, member, ret } => {
            let (recv, _) = emit_expr(receiver, emitter, builder, module, f);
            let gv = embed_str_bytes(emitter, module, builder, member.as_bytes());
            let nptr = builder.ins().symbol_value(types::I64, gv);
            let nlen = builder.ins().iconst(types::I64, member.as_bytes().len() as i64);
            let id = emitter.unrt_member_get.expect("unrt import required for classes");
            let get_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(get_ref, &[recv, nptr, nlen]);
            let obj = builder.inst_results(res)[0];
            emit_error_gate_regular(emitter, builder, module, obj);
            let v = unbox_obj(obj, *ret, emitter, builder, module);
            (v, *ret)
        }
        TirExpr::MethodCall { receiver, member, args, ret } => {
            let (recv, recv_ty) = emit_expr(receiver, emitter, builder, module, f);
            // When the receiver's class and the method name are both known at
            // compile time, call the emitted native function directly. This
            // skips the `unrt_method_call` FFI, the registry name lookup and
            // the box/unbox round trip for arguments and the return value.
            // Only exact (class, name, arity) matches on scalar-returning
            // methods are eligible; anything else keeps the generic path.
            let direct = if emitter.mode == NativeMode::Obj {
                match recv_ty {
                    TirTy::Class(class_id) => emitter.fns.iter().enumerate().find_map(|(i, callee)| {
                        let first_ok = callee.params.first() == Some(&TirTy::Class(class_id));
                        let arity_ok = callee.params.len() == args.len() + 1;
                        let ret_ok = matches!(
                            callee.ret,
                            Some(TirTy::Int | TirTy::Float | TirTy::Bool)
                        );
                        if &callee.name == member.as_ref() && first_ok && arity_ok && ret_ok {
                            Some(i)
                        } else {
                            None
                        }
                    }),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(index) = direct.and_then(|i| emitter.internal_fids.get(&i).copied()) {
                let mut vals = Vec::with_capacity(args.len() + 1);
                vals.push(recv);
                for a in args {
                    let (v, _) = emit_expr(a, emitter, builder, module, f);
                    vals.push(v);
                }
                let mut call_result = None;
                if emitter.needs_frame {
                    if let (Some(push), Some(pop)) = (emitter.nrt_push_frame, emitter.nrt_pop_frame) {
                        let gv = embed_str_bytes(emitter, module, builder, f.name.as_bytes());
                        let np = builder.ins().symbol_value(types::I64, gv);
                        let nl = builder.ins().iconst(types::I64, f.name.as_bytes().len() as i64);
                        let (start, len) = emitter.current_span.unwrap_or((0, 0));
                        let s = builder.ins().iconst(types::I64, start as i64);
                        let l = builder.ins().iconst(types::I64, len as i64);
                        let push_ref = module.declare_func_in_func(push, builder.func);
                        let _ = builder.ins().call(push_ref, &[np, nl, s, l]);
                        let callee_ref = module.declare_func_in_func(index, builder.func);
                        let call = builder.ins().call(callee_ref, &vals);
                        call_result = Some(builder.inst_results(call)[0]);
                        let pop_ref = module.declare_func_in_func(pop, builder.func);
                        let _ = builder.ins().call(pop_ref, &[]);
                    }
                }
                let result = match call_result {
                    Some(v) => v,
                    None => {
                        let callee_ref = module.declare_func_in_func(index, builder.func);
                        let call = builder.ins().call(callee_ref, &vals);
                        builder.inst_results(call)[0]
                    }
                };
                return (result, *ret);
            }
            let arg_vals: Vec<_> = args
                .iter()
                .map(|a| emit_expr(a, emitter, builder, module, f))
                .collect();
            let slot = builder.create_sized_stack_slot(
                cranelift_codegen::ir::StackSlotData::new(
                    cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                    (8 * arg_vals.len()) as u32,
                    8,
                ),
            );
            for (i, (v, ty)) in arg_vals.iter().enumerate() {
                let v = wrap_as_obj(*v, *ty, emitter, builder, module);
                let addr = builder.ins().stack_addr(types::I64, slot, (8 * i) as i32);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), v, addr, 0);
            }
            let gv = embed_str_bytes(emitter, module, builder, member.as_bytes());
            let nptr = builder.ins().symbol_value(types::I64, gv);
            let nlen = builder.ins().iconst(types::I64, member.as_bytes().len() as i64);
            let ptr = builder.ins().stack_addr(types::I64, slot, 0);
            let argc = builder.ins().iconst(types::I64, args.len() as i64);
            let id = emitter.unrt_method_call.expect("unrt import required for classes");
            let call_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(call_ref, &[recv, nptr, nlen, ptr, argc]);
            let obj = builder.inst_results(res)[0];
            emit_error_gate_regular(emitter, builder, module, obj);
            let v = unbox_obj(obj, *ret, emitter, builder, module);
            (v, *ret)
        }
        TirExpr::Dict(entries) => {
            if entries.is_empty() {
                panic!("empty dict literal rejected by validator");
            }
            let slot_t = builder.create_sized_stack_slot(
                cranelift_codegen::ir::StackSlotData::new(
                    cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                    (16 * entries.len()) as u32,
                    8,
                ),
            );
            let mut key_ty = None;
            let mut elem_ty = None;
            for (i, (key, value)) in entries.iter().enumerate() {
                let (k_v, k_ty) = emit_expr(key, emitter, builder, module, f);
                if key_ty.is_none() {
                    key_ty = Some(k_ty);
                }
                let k_v = wrap_as_obj(k_v, k_ty, emitter, builder, module);
                let (v_v, v_ty) = emit_expr(value, emitter, builder, module, f);
                elem_ty = Some(match elem_ty {
                    None => v_ty,
                    Some(prev) if prev == v_ty => prev,
                    // Heterogeneous `dict[str, any]` stores boxed values.
                    Some(_) => TirTy::Obj,
                });
                let v_v = wrap_as_obj(v_v, v_ty, emitter, builder, module);
                let base = (16 * i) as i32;
                let addr = builder.ins().stack_addr(types::I64, slot_t, base);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), k_v, addr, 0);
                let addr = builder.ins().stack_addr(types::I64, slot_t, base + 8);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), v_v, addr, 0);
            }
            let key_ty = key_ty.expect("non-empty dict");
            let elem_ty = elem_ty.expect("non-empty dict");
            let ret_ty = match (key_ty, elem_ty) {
                (TirTy::Str, TirTy::Int) => TirTy::DictStrInt,
                (TirTy::Str, TirTy::Float) => TirTy::DictStrFloat,
                (TirTy::Int, TirTy::Int) => TirTy::DictIntInt,
                (k @ (TirTy::Str | TirTy::Int), v) => {
                    TirTy::DictGen(elem_of_tir(k), elem_of_tir(v))
                }
                _ => panic!("core dicts require str/int keys"),
            };
            let ptr = builder.ins().stack_addr(types::I64, slot_t, 0);
            let len = builder.ins().iconst(types::I64, entries.len() as i64);
            let from_id = emitter.unrt_dict_from_pairs.expect("unrt import required for dicts");
            let from_ref = module.declare_func_in_func(from_id, builder.func);
            let res = builder.ins().call(from_ref, &[ptr, len]);
            let obj = builder.inst_results(res)[0];
            (obj, ret_ty)
        }
        TirExpr::CastInt(inner) => {
            let (v, ty) = emit_expr(inner, emitter, builder, module, f);
            match ty {
                TirTy::Int => (v, TirTy::Int),
                TirTy::Float => (
                    builder.ins().fcvt_to_sint(types::I64, v),
                    TirTy::Int,
                ),
                _ => unreachable!(),
            }
        }
        TirExpr::CastFloat(inner) => {
            let (v, ty) = emit_expr(inner, emitter, builder, module, f);
            match ty {
                TirTy::Int => (
                    builder.ins().fcvt_from_sint(types::F64, v),
                    TirTy::Float,
                ),
                TirTy::Float => (v, TirTy::Float),
                _ => unreachable!(),
            }
        }
        TirExpr::Math(mf, args) => {
            let arg_vals: Vec<_> = args
                .iter()
                .map(|a| emit_expr(a, emitter, builder, module, f).0)
                .collect();
            let result = emit_math_inline(emitter, builder, module, *mf, &arg_vals);
            (result, TirTy::Float)
        }
        TirExpr::ListRepeat { value, len, ty } => {
            let (v, vty) = emit_expr(value, emitter, builder, module, f);
            let (n, nty) = emit_expr(len, emitter, builder, module, f);
            if nty != TirTy::Int {
                panic!("iter.array size must be int");
            }
            match ty {
                TirTy::ListInt => {
                    if vty != TirTy::Int {
                        panic!("iter.array int result with non-int value");
                    }
                    let id = emitter.unrt_list_repeat_int.expect("unrt import required for iter.array");
                    let fid = module.declare_func_in_func(id, builder.func);
                    let res = builder.ins().call(fid, &[v, n]);
                    (builder.inst_results(res)[0], TirTy::ListInt)
                }
                TirTy::ListFloat => {
                    if vty != TirTy::Float {
                        panic!("iter.array float result with non-float value");
                    }
                    let id = emitter.unrt_list_repeat_float.expect("unrt import required for iter.array");
                    let fid = module.declare_func_in_func(id, builder.func);
                    let res = builder.ins().call(fid, &[v, n]);
                    (builder.inst_results(res)[0], TirTy::ListFloat)
                }
                _ => unreachable!("iter.array element type"),
            }
        }
        TirExpr::NativeCallExpr { module: mod_name, member: member_name, args, ret } => {
            if emitter.mode == NativeMode::Obj && emitter.needs_frame {
                if let Some(push) = emitter.nrt_push_frame {
                    let gv = embed_str_bytes(emitter, module, builder, f.name.as_bytes());
                    let np = builder.ins().symbol_value(types::I64, gv);
                    let nl = builder.ins().iconst(types::I64, f.name.as_bytes().len() as i64);
                    let (start, len) = emitter.current_span.unwrap_or((0, 0));
                    let s = builder.ins().iconst(types::I64, start as i64);
                    let l = builder.ins().iconst(types::I64, len as i64);
                    let pr = module.declare_func_in_func(push, builder.func);
                    let _ = builder.ins().call(pr, &[np, nl, s, l]);
                }
            }
            let id = emitter
                .unrt_call_native
                .expect("unrt import required for native calls");
            let mod_gv = embed_str_bytes(emitter, module, builder, mod_name.as_bytes());
            let member_gv = embed_str_bytes(emitter, module, builder, member_name.as_bytes());
            let arg_count = args.len();
            let slot_data = cranelift_codegen::ir::StackSlotData::new(
                cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                (8 * arg_count) as u32,
                8,
            );
            let slot = builder.create_sized_stack_slot(slot_data);
            for (i, arg) in args.iter().enumerate() {
                let (v, vty) = emit_expr(arg, emitter, builder, module, f);
                let wrapped = wrap_as_obj(v, vty, emitter, builder, module);
                let addr = builder.ins().stack_addr(types::I64, slot, 8 * i as i32);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), wrapped, addr, 0);
            }
            let sp = builder.ins().stack_addr(types::I64, slot, 0);
            let mod_ptr = builder.ins().symbol_value(types::I64, mod_gv);
            let mod_len = builder.ins().iconst(types::I64, mod_name.as_bytes().len() as i64);
            let member_ptr = builder.ins().symbol_value(types::I64, member_gv);
            let member_len = builder.ins().iconst(types::I64, member_name.as_bytes().len() as i64);
            let argc = builder.ins().iconst(types::I64, arg_count as i64);
            let call_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(call_ref, &[mod_ptr, mod_len, member_ptr, member_len, sp, argc]);
            let raw = builder.inst_results(res)[0];
            emit_error_gate(emitter, builder, module, raw);
            if emitter.mode == NativeMode::Obj && emitter.needs_frame {
                if let Some(pop) = emitter.nrt_pop_frame {
                    let pr = module.declare_func_in_func(pop, builder.func);
                    let _ = builder.ins().call(pr, &[]);
                }
            }
            match ret {
                TirTy::Int => {
                    let ub = emitter.unrt_int_from_obj.expect("unrt import required to unbox ints");
                    let ub_ref = module.declare_func_in_func(ub, builder.func);
                    let r = builder.ins().call(ub_ref, &[raw]);
                    (builder.inst_results(r)[0], TirTy::Int)
                }
                TirTy::Float => {
                    let ub = emitter.unrt_float_from_obj.expect("unrt import required to unbox floats");
                    let ub_ref = module.declare_func_in_func(ub, builder.func);
                    let r = builder.ins().call(ub_ref, &[raw]);
                    (builder.inst_results(r)[0], TirTy::Float)
                }
                TirTy::Bool => {
                    let ub = emitter.unrt_bool_from_obj.expect("unrt import required to unbox bools");
                    let ub_ref = module.declare_func_in_func(ub, builder.func);
                    let r = builder.ins().call(ub_ref, &[raw]);
                    let ub_val = builder.inst_results(r)[0];
                    let narrowed = builder.ins().ireduce(types::I8, ub_val);
                    (narrowed, TirTy::Bool)
                }
                TirTy::Str => (raw, TirTy::Str),
                TirTy::Bytes => (raw, TirTy::Bytes),
                TirTy::Obj | TirTy::Class(_) => (raw, TirTy::Obj),
                _ => unreachable!("unsupported bridged native call result"),
            }
        }
        TirExpr::Len(arg) => {
            let (arg_v, arg_ty) = emit_expr(arg, emitter, builder, module, f);
            // `unrt_len` always boxes the result as an object, which forces an
            // error gate + unbox round trip at every call site. For a
            // statically typed list `unrt_list_len` returns the raw i64 length
            // directly; the handle is a list by construction, so no error gate
            // is needed. Both the standalone NRT and the `--embed` VM bridge
            // export this symbol with the same signature.
            if matches!(
                arg_ty,
                TirTy::ListInt
                    | TirTy::ListFloat
                    | TirTy::ListClass(_)
                    | TirTy::ListLambda
                    | TirTy::ListGen(_)
            ) {
                let id = emitter.unrt_list_len.expect("unrt list-len import required");
                let len_ref = module.declare_func_in_func(id, builder.func);
                let res = builder.ins().call(len_ref, &[arg_v]);
                return (builder.inst_results(res)[0], TirTy::Int);
            }
            let id = emitter.unrt_len.expect("unrt import required for len()");
            let len_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(len_ref, &[arg_v]);
            let raw = builder.inst_results(res)[0];
            emit_error_gate_regular(emitter, builder, module, raw);
            let ub_ref = module.declare_func_in_func(emitter.unrt_int_from_obj.expect("unrt import required for int unbox"), builder.func);
            let res = builder.ins().call(ub_ref, &[raw]);
            (builder.inst_results(res)[0], TirTy::Int)
        }
        TirExpr::ListPop(receiver) => {
            let (list_v, lty) = emit_expr(receiver, emitter, builder, module, f);
            let _ = lty;
            let id = emitter.unrt_list_pop.expect("unrt import required for list.pop");
            let pop_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(pop_ref, &[list_v]);
            let raw = builder.inst_results(res)[0];
            emit_error_gate_regular(emitter, builder, module, raw);
            list_elem_unbox(lty, emitter, builder, module, raw)
        }
        TirExpr::ListPopAt(receiver, index) => {
            let (list_v, lty) = emit_expr(receiver, emitter, builder, module, f);
            let (idx_v, ity) = emit_expr(index, emitter, builder, module, f);
            if ity != TirTy::Int {
                panic!("list index must be int");
            }
            let id = emitter.unrt_list_pop_at.expect("unrt import required for list.pop");
            let pop_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(pop_ref, &[list_v, idx_v]);
            let raw = builder.inst_results(res)[0];
            emit_error_gate_regular(emitter, builder, module, raw);
            list_elem_unbox(lty, emitter, builder, module, raw)
        }
        TirExpr::ListContains(receiver, item) => {
            let (list_v, _) = emit_expr(receiver, emitter, builder, module, f);
            let (arg_v, arg_ty) = emit_expr(item, emitter, builder, module, f);
            let obj = wrap_as_obj(arg_v, arg_ty, emitter, builder, module);
            let id = emitter.unrt_list_contains.expect("unrt import required for list.contains");
            let contains_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(contains_ref, &[list_v, obj]);
            let raw = builder.inst_results(res)[0];
            let raw8 = builder.ins().ireduce(types::I8, raw);
            (raw8, TirTy::Bool)
        }
        TirExpr::ListIndexOf(receiver, item) => {
            let (list_v, _) = emit_expr(receiver, emitter, builder, module, f);
            let (arg_v, arg_ty) = emit_expr(item, emitter, builder, module, f);
            let obj = wrap_as_obj(arg_v, arg_ty, emitter, builder, module);
            let id = emitter.unrt_list_index_of.expect("unrt import required for list.index_of");
            let index_of_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(index_of_ref, &[list_v, obj]);
            let raw = builder.inst_results(res)[0];
            (raw, TirTy::Int)
        }
        TirExpr::ListRemove(receiver, item) => {
            let (list_v, _) = emit_expr(receiver, emitter, builder, module, f);
            let (arg_v, arg_ty) = emit_expr(item, emitter, builder, module, f);
            let obj = wrap_as_obj(arg_v, arg_ty, emitter, builder, module);
            // `unrt_list_index_of` + `unrt_list_pop_at` reproduce the VM's
            // `list.remove(value)`: first equal element removed, Bool returned.
            let index_id = emitter.unrt_list_index_of.expect("unrt import required for list.remove");
            let index_ref = module.declare_func_in_func(index_id, builder.func);
            let idx_res = builder.ins().call(index_ref, &[list_v, obj]);
            let idx = builder.inst_results(idx_res)[0];
            let minus_one = builder.ins().iconst(types::I64, -1);
            let found = builder.ins().icmp(IntCC::NotEqual, idx, minus_one);
            let pop_blk = builder.create_block();
            let skip_blk = builder.create_block();
            let merge_blk = builder.create_block();
            let result = builder.append_block_param(merge_blk, types::I8);
            builder.ins().brif(found, pop_blk, &[], skip_blk, &[]);
            builder.switch_to_block(pop_blk);
            let pop_id = emitter.unrt_list_pop_at.expect("unrt import required for list.remove");
            let pop_ref = module.declare_func_in_func(pop_id, builder.func);
            let _ = builder.ins().call(pop_ref, &[list_v, idx]);
            let one = builder.ins().iconst(types::I8, 1);
            builder.ins().jump(merge_blk, &[one.into()]);
            builder.seal_block(pop_blk);
            builder.switch_to_block(skip_blk);
            let zero = builder.ins().iconst(types::I8, 0);
            builder.ins().jump(merge_blk, &[zero.into()]);
            builder.seal_block(skip_blk);
            builder.seal_block(merge_blk);
            builder.switch_to_block(merge_blk);
            (result, TirTy::Bool)
        }
        TirExpr::DictKeys(receiver) | TirExpr::DictValues(receiver) => {
            let (dict_v, dty) = emit_expr(receiver, emitter, builder, module, f);
            let keys = matches!(expr, TirExpr::DictKeys(_));
            let elem = match dty {
                TirTy::DictStrInt | TirTy::DictStrFloat => crate::tir::ObjElem::Str,
                TirTy::DictIntInt => crate::tir::ObjElem::Int,
                TirTy::DictGen(k, v) => if keys { k } else { v },
                _ => panic!("dict.keys/values requires a dict"),
            };
            let id = if keys { emitter.unrt_dict_keys } else { emitter.unrt_dict_values }
                .expect("unrt import required for dict.keys/values");
            let call_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(call_ref, &[dict_v]);
            let raw = builder.inst_results(res)[0];
            emit_error_gate_regular(emitter, builder, module, raw);
            (raw, TirTy::ListGen(elem))
        }
        TirExpr::DictGet(target, key) => {
            let (dict_v, dty) = emit_expr(target, emitter, builder, module, f);
            let (key_v, kty) = emit_expr(key, emitter, builder, module, f);
            if kty != TirTy::Str && kty != TirTy::Int {
                panic!("dict key must be str or int");
            }
            // Fast path for `dict[int, int]` with an int key: the new NRT ABI
            // returns the raw i64 value and reports failures through an
            // out-parameter, avoiding key boxing + result unboxing + the
            // generic getter FFI (3 calls saved on the success path).
            if dty == TirTy::DictIntInt && kty == TirTy::Int {
                if let Some(get_id) = emitter.unrt_dict_get_int {
                    let slot = builder.create_sized_stack_slot(
                        cranelift_codegen::ir::StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            8,
                            8,
                        ),
                    );
                    let zero = builder.ins().iconst(types::I64, 0);
                    let err_addr = builder.ins().stack_addr(types::I64, slot, 0);
                    builder.ins().store(MemFlagsData::new(), zero, err_addr, 0);
                    let get_ref = module.declare_func_in_func(get_id, builder.func);
                    let res = builder.ins().call(get_ref, &[dict_v, key_v, err_addr]);
                    let value = builder.inst_results(res)[0];
                    let err_val = builder.ins().load(types::I64, MemFlagsData::new(), err_addr, 0);
                    let nonzero = builder.ins().icmp(IntCC::NotEqual, err_val, zero);
                    let err_blk = builder.create_block();
                    let ok_blk = builder.create_block();
                    builder.ins().brif(nonzero, err_blk, &[], ok_blk, &[]);
                    builder.switch_to_block(err_blk);
                    emit_error_gate_regular(emitter, builder, module, err_val);
                    // `emit_error_gate_regular` either jumps to a catch handler
                    // or traps; if it somehow falls through, merge into `ok_blk`.
                    builder.ins().jump(ok_blk, &[]);
                    builder.seal_block(err_blk);
                    builder.seal_block(ok_blk);
                    builder.switch_to_block(ok_blk);
                    return (value, TirTy::Int);
                }
            }
            let key_v = wrap_as_obj(key_v, kty, emitter, builder, module);
            let get_id = emitter.unrt_dict_get.expect("unrt import required for dict indexing");
            let get_ref = module.declare_func_in_func(get_id, builder.func);
            let res = builder.ins().call(get_ref, &[dict_v, key_v]);
            let raw = builder.inst_results(res)[0];
            emit_error_gate_regular(emitter, builder, module, raw);
            match dty {
                TirTy::DictStrInt | TirTy::DictIntInt => {
                    let id = emitter.unrt_int_from_obj.expect("unrt import required for int unbox");
                    let ub_ref = module.declare_func_in_func(id, builder.func);
                    let res = builder.ins().call(ub_ref, &[raw]);
                    (builder.inst_results(res)[0], TirTy::Int)
                }
                TirTy::DictStrFloat => {
                    let id = emitter.unrt_float_from_obj.expect("unrt import required for float unbox");
                    let ub_ref = module.declare_func_in_func(id, builder.func);
                    let res = builder.ins().call(ub_ref, &[raw]);
                    (builder.inst_results(res)[0], TirTy::Float)
                }
                TirTy::DictGen(_, elem) => {
                    let ty = tir_of_elem(elem);
                    (unbox_obj(raw, ty, emitter, builder, module), ty)
                }
                _ => panic!("indexing requires a dict with str/int keys"),
            }
        }
        TirExpr::ListGet(target, index) => {
            let (list_v, lty) = emit_expr(target, emitter, builder, module, f);
            let (idx_v, ity) = emit_expr(index, emitter, builder, module, f);
            if ity != TirTy::Int {
                panic!("list index must be int");
            }
            if lty == TirTy::ListInt {
                // `unrt_list_get_int` signals out-of-range through a pending
                // error (not a return handle), so a raw call would let a plain
                // exe print 0 where `un run` errors. Keep the raw fast path
                // only when bounds are statically checked; otherwise use the
                // boxed `unrt_list_get` + error gate so span/traceback stay
                // byte-identical with `un run`.
                let len_id = emitter.unrt_list_len.expect("unrt list-len import required");
                let len_ref = module.declare_func_in_func(len_id, builder.func);
                let len_res = builder.ins().call(len_ref, &[list_v]);
                let len = builder.inst_results(len_res)[0];
                let neg_len = builder.ins().ineg(len);
                let ge_neg = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, idx_v, neg_len);
                let lt_len = builder.ins().icmp(IntCC::SignedLessThan, idx_v, len);
                let in_bounds = builder.ins().band(ge_neg, lt_len);
                let fast_blk = builder.create_block();
                let oob_blk = builder.create_block();
                let merge_blk = builder.create_block();
                let merge_val = builder.append_block_param(merge_blk, types::I64);
                builder.ins().brif(in_bounds, fast_blk, &[], oob_blk, &[]);
                // In-bounds: raw i64 read (no pending error possible).
                builder.switch_to_block(fast_blk);
                let id = emitter.unrt_list_get_int.expect("unrt int-list import required");
                let f_ref = module.declare_func_in_func(id, builder.func);
                let res = builder.ins().call(f_ref, &[list_v, idx_v]);
                let fast_val = builder.inst_results(res)[0];
                builder.ins().jump(merge_blk, &[fast_val.into()]);
                builder.seal_block(fast_blk);
                // Out-of-range: boxed read returns an error handle that the
                // regular gate routes through the same span/catch/trace code.
                builder.switch_to_block(oob_blk);
                let get_id = emitter.unrt_list_get.expect("unrt import required for list indexing");
                let get_ref = module.declare_func_in_func(get_id, builder.func);
                let oob_res = builder.ins().call(get_ref, &[list_v, idx_v]);
                let err_obj = builder.inst_results(oob_res)[0];
                emit_error_gate_regular(emitter, builder, module, err_obj);
                // The gate either traps or jumps to a handler; this jump only
                // keeps the CFG well-formed for the (unreachable) fall-through.
                let zero = builder.ins().iconst(types::I64, 0);
                builder.ins().jump(merge_blk, &[zero.into()]);
                builder.seal_block(oob_blk);
                builder.switch_to_block(merge_blk);
                return (merge_val, TirTy::Int);
            }
            let get_id = emitter.unrt_list_get.expect("unrt import required for list indexing");
            let get_ref = module.declare_func_in_func(get_id, builder.func);
            let raw = builder.ins().call(get_ref, &[list_v, idx_v]);
            let raw = builder.inst_results(raw)[0];
            emit_error_gate_regular(emitter, builder, module, raw);
            match lty {
                TirTy::ListInt => {
                    let id = emitter.unrt_int_from_obj.expect("unrt import required for int unbox");
                    let ub_ref = module.declare_func_in_func(id, builder.func);
                    let res = builder.ins().call(ub_ref, &[raw]);
                    (builder.inst_results(res)[0], TirTy::Int)
                }
                TirTy::ListFloat => {
                    let id = emitter.unrt_float_from_obj.expect("unrt import required for float unbox");
                    let ub_ref = module.declare_func_in_func(id, builder.func);
                    let res = builder.ins().call(ub_ref, &[raw]);
                    (builder.inst_results(res)[0], TirTy::Float)
                }
                TirTy::ListClass(id) => (raw, TirTy::Class(id)),
                TirTy::ListLambda => (raw, TirTy::Lambda(u32::MAX)),
                TirTy::ListGen(elem) => {
                    let ty = tir_of_elem(elem);
                    (unbox_obj(raw, ty, emitter, builder, module), ty)
                }
                _ => panic!("indexing requires a list"),
            }
        }
        TirExpr::Bool(v) => (builder.ins().iconst(types::I8, *v as i64), TirTy::Bool),
        TirExpr::Local(slot) => {
            let var = emitter.vars[*slot as usize];
            (builder.use_var(var), emitter.local_tys[*slot as usize])
        }
        TirExpr::Promote(inner) => {
            let (v, inner_ty) = emit_expr(inner, emitter, builder, module, f);
            debug_assert_eq!(inner_ty, TirTy::Int);
            (builder.ins().fcvt_from_sint(types::F64, v), TirTy::Float)
        }
        TirExpr::Select { cond, then_expr, else_expr } => {
            let (cond_v, _) = emit_expr(cond, emitter, builder, module, f);
            let (t_v, t_ty) = emit_expr(then_expr, emitter, builder, module, f);
            let (e_v, _) = emit_expr(else_expr, emitter, builder, module, f);
            (builder.ins().select(cond_v, t_v, e_v), t_ty)
        }
        TirExpr::Un(op, inner) => {
            let (value, ty) = emit_expr(inner, emitter, builder, module, f);
            match op {
                TirUnOp::Neg if ty == TirTy::Obj => {
                    let id = emitter.unrt_dyn_un.expect("dynamic dispatch import required");
                    let fref = module.declare_func_in_func(id, builder.func);
                    let code = builder.ins().iconst(types::I64, 0);
                    let res = builder.ins().call(fref, &[code, value]);
                    let obj = builder.inst_results(res)[0];
                    emit_error_gate(emitter, builder, module, obj);
                    (obj, TirTy::Obj)
                }
                TirUnOp::Neg => match ty {
                    TirTy::Int => (builder.ins().ineg(value), TirTy::Int),
                    TirTy::Float => (builder.ins().fneg(value), TirTy::Float),
                    TirTy::Str => unreachable!("validator forbids neg of str"),
                    TirTy::Bytes => unreachable!("validator forbids neg of bytes"),
                    TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) => {
                        unreachable!("validator forbids neg of collection")
                    }
                    TirTy::Bool => unreachable!("validator forbids neg of bool"),
                },
                TirUnOp::Not => match ty {
                    TirTy::Bool | TirTy::Int => {
                        let one = match ty {
                            TirTy::Bool => builder.ins().iconst(types::I8, 1),
                            _ => builder.ins().iconst(types::I64, 1),
                        };
                        (builder.ins().bxor(value, one), ty)
                    }
                    TirTy::Float => unreachable!("validator forbids not of float"),
                    TirTy::Str => unreachable!("validator forbids not of str"),
                    TirTy::Bytes => unreachable!("validator forbids not of bytes"),
                    TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) => {
                        unreachable!("validator forbids not of collection")
                    }
                },
            }
        }
        TirExpr::Bin(op, lhs, rhs) => {
            // A field read has no side effects, so an identical `self.x` on
            // both sides of an operator only needs one NRT lookup. This is
            // deliberately restricted to structurally equal `MemberGet` pairs
            // emitted back-to-back, where no mutation can happen in between.
            if let (
                TirExpr::MemberGet { receiver: r1, member: m1, .. },
                TirExpr::MemberGet { receiver: r2, member: m2, .. },
            ) = (&**lhs, &**rhs)
            {
                if m1 == m2 && format!("{r1:?}") == format!("{r2:?}") {
                    let (a, a_ty) = emit_expr(lhs, emitter, builder, module, f);
                    return emit_binop(*op, a, a_ty, a, a_ty, emitter, builder, module);
                }
            }
            let (a, a_ty) = emit_expr(lhs, emitter, builder, module, f);
            let (b, b_ty) = emit_expr(rhs, emitter, builder, module, f);
            emit_binop(*op, a, a_ty, b, b_ty, emitter, builder, module)
        }
        TirExpr::Cmp(op, lhs, rhs) => {
            let (a, a_ty) = emit_expr(lhs, emitter, builder, module, f);
            let (b, b_ty) = emit_expr(rhs, emitter, builder, module, f);
            emit_cmp(*op, a, a_ty, b, b_ty, emitter, builder, module)
        }
        TirExpr::Call(index, args) => {
            let calleefid = emitter.internal_fids[&(*index as usize)];
            let callee_ret = emitter.fns[*index as usize].ret;
            let param_tys = emitter.fns[*index as usize].params.clone();
            let mut arg_vals = Vec::with_capacity(args.len());
            for (arg, param_ty) in args.iter().zip(param_tys.iter()) {
                let (v, ty) = emit_expr(arg, emitter, builder, module, f);
                arg_vals.push(coerce_value(v, ty, *param_ty, emitter, builder, module));
            }
            // Record the call site so a deeper error can print a trace frame.
            // Error-free functions never contribute an observable frame, so
            // skip the two FFI round trips entirely for them.
            if emitter.mode == NativeMode::Obj && emitter.needs_frame {
                if let (Some(push), Some(pop)) = (emitter.nrt_push_frame, emitter.nrt_pop_frame) {
                    let gv = embed_str_bytes(emitter, module, builder, f.name.as_bytes());
                    let np = builder.ins().symbol_value(types::I64, gv);
                    let nl = builder.ins().iconst(types::I64, f.name.as_bytes().len() as i64);
                    let (start, len) = emitter.current_span.unwrap_or((0, 0));
                    let s = builder.ins().iconst(types::I64, start as i64);
                    let l = builder.ins().iconst(types::I64, len as i64);
                    let push_ref = module.declare_func_in_func(push, builder.func);
                    let _ = builder.ins().call(push_ref, &[np, nl, s, l]);
                    let call = {
                        let callee_ref = module.declare_func_in_func(calleefid, builder.func);
                        builder.ins().call(callee_ref, &arg_vals)
                    };
                    let pop_ref = module.declare_func_in_func(pop, builder.func);
                    let _ = builder.ins().call(pop_ref, &[]);
                    let results = builder.inst_results(call).to_vec();
                    if emitter.mode == NativeMode::Obj {
                        return match callee_ret {
                            Some(t) => (results[0], t),
                            None => (builder.ins().iconst(types::I8, 0), TirTy::Bool),
                        };
                    }
                    unreachable!()
                }
            }
            let callee_ref = module.declare_func_in_func(calleefid, builder.func);
            let call = builder.ins().call(callee_ref, &arg_vals);
            let results = builder.inst_results(call).to_vec();
            if emitter.mode == NativeMode::Obj {
                return match callee_ret {
                    Some(t) => (results[0], t),
                    None => {
                        let zero = builder.ins().iconst(types::I8, 0);
                        (zero, TirTy::Bool)
                    }
                };
            }
            let bail = results[results.len() - 1];
            let zero = builder.ins().iconst(types::I8, 0);
            let not_bailed = builder.ins().icmp(IntCC::Equal, bail, zero);
            let cont_blk = builder.create_block();
            let bail_blk = emitter.bail_blk;
            builder.ins().brif(not_bailed, cont_blk, &[], bail_blk, &[]);
            builder.seal_block(cont_blk);
            builder.switch_to_block(cont_blk);
            match callee_ret {
                Some(t) => (results[0], t),
                None => (zero, TirTy::Bool),
            }
        }
    }
}

/// Op codes for `unrt_dyn_bin`. Keep in lockstep with `nrt/src/dynops.rs`.
fn dyn_bin_code(op: TirBinOp) -> i64 {
    match op {
        TirBinOp::Add => 0,
        TirBinOp::Sub => 1,
        TirBinOp::Mul => 2,
        TirBinOp::Div => 3,
        TirBinOp::IDiv => 4,
        TirBinOp::Mod => 5,
        TirBinOp::BAnd => 6,
        TirBinOp::BOr => 7,
        TirBinOp::BXor => 8,
        TirBinOp::Shl => 9,
        TirBinOp::Shr => 10,
    }
}

fn dyn_cmp_code(op: TirCmp) -> i64 {
    match op {
        TirCmp::Lt => 20,
        TirCmp::Le => 21,
        TirCmp::Gt => 22,
        TirCmp::Ge => 23,
        TirCmp::Eq => 24,
        TirCmp::Ne => 25,
    }
}

/// Box scalar operands, dispatch through NRT, and leave the tagged result boxed.
fn emit_dyn_bin<M: Module>(
    code: i64,
    a: cranelift_codegen::ir::Value,
    a_ty: TirTy,
    b: cranelift_codegen::ir::Value,
    b_ty: TirTy,
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
) -> cranelift_codegen::ir::Value {
    let id = emitter.unrt_dyn_bin.expect("dynamic dispatch import required");
    let fref = module.declare_func_in_func(id, builder.func);
    let op = builder.ins().iconst(types::I64, code);
    let a = wrap_as_obj(a, a_ty, emitter, builder, module);
    let b = wrap_as_obj(b, b_ty, emitter, builder, module);
    let res = builder.ins().call(fref, &[op, a, b]);
    let obj = builder.inst_results(res)[0];
    emit_error_gate(emitter, builder, module, obj);
    obj
}

/// Coerce a value across the scalar/dynamic boundary. `TirTy::Obj` values are
/// already boxed NObj handles, so only Int/Float/Bool need boxing/unboxing.
fn coerce_value<M: Module>(
    v: cranelift_codegen::ir::Value,
    from: TirTy,
    to: TirTy,
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
) -> cranelift_codegen::ir::Value {
    if from == to {
        return v;
    }
    if to == TirTy::Obj && matches!(from, TirTy::Int | TirTy::Float | TirTy::Bool) {
        wrap_as_obj(v, from, emitter, builder, module)
    } else if from == TirTy::Obj && matches!(to, TirTy::Int | TirTy::Float | TirTy::Bool) {
        unbox_obj(v, to, emitter, builder, module)
    } else {
        v
    }
}

fn emit_binop<M: Module>(
    op: TirBinOp,
    a: cranelift_codegen::ir::Value,
    a_ty: TirTy,
    b: cranelift_codegen::ir::Value,
    b_ty: TirTy,
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
) -> (cranelift_codegen::ir::Value, TirTy) {
    // Dynamic values must be checked before Str: a Str operand paired with an
    // `any` operand still goes through tag-based dispatch.
    if a_ty == TirTy::Obj || b_ty == TirTy::Obj {
        let obj = emit_dyn_bin(dyn_bin_code(op), a, a_ty, b, b_ty, emitter, builder, module);
        return (obj, TirTy::Obj);
    }
    match op {
        TirBinOp::Add if a_ty == TirTy::Str && b_ty == TirTy::Str => {
            let id = emitter.unrt_str_cat.expect("unrt import required for Str concat");
            let cat_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(cat_ref, &[a, b]);
            let obj = builder.inst_results(res)[0];
            (obj, TirTy::Str)
        }
        TirBinOp::Add => match (a_ty, b_ty) {
            (TirTy::Int, TirTy::Int) => (builder.ins().iadd(a, b), TirTy::Int),
            (TirTy::Float, TirTy::Float) => (builder.ins().fadd(a, b), TirTy::Float),
            (TirTy::Int, TirTy::Float) | (TirTy::Float, TirTy::Int) => {
                let a = ensure_float(a, a_ty, builder);
                let b = ensure_float(b, b_ty, builder);
                (builder.ins().fadd(a, b), TirTy::Float)
            }
            (TirTy::Str, _) | (_, TirTy::Str) => unreachable!("mixed Str arithmetic rejected by validator"),
            _ => unreachable!("bool add rejected by validator"),
        },
        TirBinOp::Sub => match (a_ty, b_ty) {
            (TirTy::Int, TirTy::Int) => (builder.ins().isub(a, b), TirTy::Int),
            (TirTy::Float, TirTy::Float) => (builder.ins().fsub(a, b), TirTy::Float),
            (TirTy::Int, TirTy::Float) | (TirTy::Float, TirTy::Int) => {
                let a = ensure_float(a, a_ty, builder);
                let b = ensure_float(b, b_ty, builder);
                (builder.ins().fsub(a, b), TirTy::Float)
            }
            _ => unreachable!(),
        },
        TirBinOp::Mul => match (a_ty, b_ty) {
            (TirTy::Int, TirTy::Int) => (builder.ins().imul(a, b), TirTy::Int),
            (TirTy::Float, TirTy::Float) => (builder.ins().fmul(a, b), TirTy::Float),
            (TirTy::Int, TirTy::Float) | (TirTy::Float, TirTy::Int) => {
                let a = ensure_float(a, a_ty, builder);
                let b = ensure_float(b, b_ty, builder);
                (builder.ins().fmul(a, b), TirTy::Float)
            }
            _ => unreachable!(),
        },
        TirBinOp::Div => {
            if a_ty == TirTy::Int && b_ty == TirTy::Int {
                emit_int_zero_guard(emitter, builder, module, b);
                let a = builder.ins().fcvt_from_sint(types::F64, a);
                let b = builder.ins().fcvt_from_sint(types::F64, b);
                return (builder.ins().fdiv(a, b), TirTy::Float);
            }
            let a = ensure_float(a, a_ty, builder);
            let b = ensure_float(b, b_ty, builder);
            (builder.ins().fdiv(a, b), TirTy::Float)
        }
        TirBinOp::IDiv => {
            emit_int_zero_guard(emitter, builder, module, b);
            let minus_one = builder.ins().iconst(types::I64, -1);
            let is_m1 = builder.ins().icmp(IntCC::Equal, b, minus_one);
            let one = builder.ins().iconst(types::I64, 1);
            let int_min = builder.ins().iconst(types::I64, i64::MIN);
            let b_safe = builder.ins().select(is_m1, one, b);
            let q = builder.ins().sdiv(a, b_safe);
            let a_int_min = int_min;
            let a_is_min = builder.ins().icmp(IntCC::Equal, a, a_int_min);
            let neg_a = builder.ins().ineg(a);            let m1_res = builder.ins().select(a_is_min, int_min, neg_a);
            let wrapped = builder.ins().select(is_m1, m1_res, q);
            return (wrapped, TirTy::Int);
        }
        TirBinOp::Mod => {
            emit_mod_zero_guard(emitter, builder, module, b);
            let minus_one = builder.ins().iconst(types::I64, -1);
            let is_m1 = builder.ins().icmp(IntCC::Equal, b, minus_one);
            let one = builder.ins().iconst(types::I64, 1);
            let b_safe = builder.ins().select(is_m1, one, b);
            let zero_v = builder.ins().iconst(types::I64, 0);
            let rem = builder.ins().srem(a, b_safe);
            let result = builder.ins().select(is_m1, zero_v, rem);
            (result, TirTy::Int)
        }
        TirBinOp::BAnd => match (a_ty, b_ty) {
            (TirTy::Int, TirTy::Int) => (builder.ins().band(a, b), TirTy::Int),
            (TirTy::Bool, TirTy::Bool) => (builder.ins().band(a, b), TirTy::Bool),
            _ => unreachable!(),
        },
        TirBinOp::BOr => match (a_ty, b_ty) {
            (TirTy::Int, TirTy::Int) => (builder.ins().bor(a, b), TirTy::Int),
            (TirTy::Bool, TirTy::Bool) => (builder.ins().bor(a, b), TirTy::Bool),
            _ => unreachable!(),
        },
        TirBinOp::BXor => match (a_ty, b_ty) {
            (TirTy::Int, TirTy::Int) => (builder.ins().bxor(a, b), TirTy::Int),
            (TirTy::Bool, TirTy::Bool) => (builder.ins().bxor(a, b), TirTy::Bool),
            _ => unreachable!(),
        },
        TirBinOp::Shl => {
            let mask = builder.ins().iconst(types::I64, 63);
            let b = builder.ins().band(b, mask);
            (builder.ins().ishl(a, b), TirTy::Int)
        }
        TirBinOp::Shr => {
            let mask = builder.ins().iconst(types::I64, 63);
            let b = builder.ins().band(b, mask);
            (builder.ins().sshr(a, b), TirTy::Int)
        }
    }
}

fn ensure_float(
    v: cranelift_codegen::ir::Value,
    ty: TirTy,
    builder: &mut FunctionBuilder,
) -> cranelift_codegen::ir::Value {
    match ty {
        TirTy::Float => v,
        TirTy::Int => builder.ins().fcvt_from_sint(types::F64, v),
        TirTy::Str => unreachable!("validator forbids str arithmetic"),
        TirTy::Bytes => unreachable!("validator forbids bytes arithmetic"),
        TirTy::Bool => unreachable!("validator forbids bool arithmetic"),
        TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) => unreachable!("validator forbids collection-class arithmetic"),
    }
}

fn emit_cmp<M: Module>(
    op: TirCmp,
    a: cranelift_codegen::ir::Value,
    a_ty: TirTy,
    b: cranelift_codegen::ir::Value,
    b_ty: TirTy,
    emitter: &mut FnEmit<'_>,
    builder: &mut FunctionBuilder,
    module: &mut M,
) -> (cranelift_codegen::ir::Value, TirTy) {
    if a_ty == TirTy::Obj || b_ty == TirTy::Obj {
        let obj = emit_dyn_bin(dyn_cmp_code(op), a, a_ty, b, b_ty, emitter, builder, module);
        let raw = unbox_obj(obj, TirTy::Bool, emitter, builder, module);
        return (raw, TirTy::Bool);
    }
    let numeric_pair = matches!((a_ty, b_ty), (TirTy::Int, TirTy::Int) | (TirTy::Int, TirTy::Float) | (TirTy::Float, TirTy::Int) | (TirTy::Float, TirTy::Float));
    if numeric_pair {
        if a_ty == TirTy::Int && b_ty == TirTy::Int {
            let cc = int_cmp(op);
            return (builder.ins().icmp(cc, a, b), TirTy::Bool);
        }
        let a = ensure_float(a, a_ty, builder);
        let b = ensure_float(b, b_ty, builder);
        if op == TirCmp::Ne {
            let eq = builder.ins().fcmp(FloatCC::Equal, a, b);
            let one = builder.ins().iconst(types::I8, 1);
            return (builder.ins().bxor(eq, one), TirTy::Bool);
        }
        let cc = float_cmp(op);
        return (builder.ins().fcmp(cc, a, b), TirTy::Bool);
    }
    if tir_is_ref(a_ty) && tir_is_ref(b_ty) {
        let id = match emitter.unrt_obj_eq {
            Some(id) => id,
            None => {
                builder.ins().jump(emitter.bail_blk, &[]);
                let zero = builder.ins().iconst(types::I64, 0);
                return (builder.ins().ireduce(types::I8, zero), TirTy::Bool);
            }
        };
        let eq_ref = module.declare_func_in_func(id, builder.func);
        let res = builder.ins().call(eq_ref, &[a, b]);
        let raw = builder.inst_results(res)[0];
        let raw8 = builder.ins().ireduce(types::I8, raw);
        if op == TirCmp::Eq {
            return (raw8, TirTy::Bool);
        }
        let one = builder.ins().iconst(types::I8, 1);
        return (builder.ins().bxor(raw8, one), TirTy::Bool);
    }
    if a_ty == TirTy::Bool && b_ty == TirTy::Bool {
        let cc = int_cmp(op);
        return (builder.ins().icmp(cc, a, b), TirTy::Bool);
    }
    unreachable!("validator forbids incomparable types")
}

fn int_cmp(op: TirCmp) -> IntCC {
    match op {
        TirCmp::Lt => IntCC::SignedLessThan,
        TirCmp::Le => IntCC::SignedLessThanOrEqual,
        TirCmp::Gt => IntCC::SignedGreaterThan,
        TirCmp::Ge => IntCC::SignedGreaterThanOrEqual,
        TirCmp::Eq => IntCC::Equal,
        TirCmp::Ne => IntCC::NotEqual,
    }
}

fn float_cmp(op: TirCmp) -> FloatCC {
    match op {
        TirCmp::Lt => FloatCC::LessThan,
        TirCmp::Le => FloatCC::LessThanOrEqual,
        TirCmp::Gt => FloatCC::GreaterThan,
        TirCmp::Ge => FloatCC::GreaterThanOrEqual,
        TirCmp::Eq => FloatCC::Equal,
        TirCmp::Ne => FloatCC::NotEqual,
    }
}

fn emit_ffi(
    f: &TirFn,
    module: &mut JITModule,
    internal_fid: cranelift_module::FuncId,
    ffi_fid: cranelift_module::FuncId,
    ffi_sig: &cranelift_codegen::ir::Signature,
) -> Result<(), ()> {
    let mut func_ctx = module.make_context();
    let mut builder_ctx = FunctionBuilderContext::new();
    func_ctx.func.signature = ffi_sig.clone();
    func_ctx.func.name = UserFuncName::user(0, ffi_fid.as_u32());
    let mut builder = FunctionBuilder::new(&mut func_ctx.func, &mut builder_ctx);

    let entry = builder.create_block(); 
    builder.append_block_params_for_function_params(entry);
    let incoming = builder.block_params(entry).to_vec();
    let hp = incoming[0];
    builder.switch_to_block(entry);

    let internal_ref = module.declare_func_in_func(internal_fid, builder.func);
    let args: Vec<cranelift_codegen::ir::Value> = f
        .params
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let v = incoming[2 + i];
            match t {
                TirTy::Int => v,
                TirTy::Float => builder.ins().bitcast(types::F64, MemFlagsData::new(), v),
                TirTy::Bool => builder.ins().ireduce(types::I8, v),
                TirTy::Bytes | TirTy::Str => v,
                TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_) => v,
            }
        })
        .collect();
    let call = builder.ins().call(internal_ref, &args);
    let results = builder.inst_results(call).to_vec();
    let bail = results[results.len() - 1];
    let zero_i8 = builder.ins().iconst(types::I8, 0);
    let not_bailed = builder.ins().icmp(IntCC::Equal, bail, zero_i8);
    let ok_blk = builder.create_block();
    let bail_blk = builder.create_block(); 
    builder.ins().brif(not_bailed, ok_blk, &[], bail_blk, &[]);
    builder.seal_block(bail_blk);
    builder.seal_block(ok_blk);

    builder.switch_to_block(bail_blk);
    let zero = builder.ins().iconst(types::I64, 0);
    let two = builder.ins().iconst(types::I64, TAG_BAIL);
    builder.ins().store(MemFlagsData::new(), zero, hp, 0);
    builder.ins().store(MemFlagsData::new(), two, hp, 8);
    builder.ins().return_(&[]);

    builder.switch_to_block(ok_blk);
    match &f.ret {
        Some(TirTy::Int) => {
            let bits = results[0];
            builder.ins().store(MemFlagsData::new(), bits, hp, 0);
            let tag_v = builder.ins().iconst(types::I64, TAG_INT);
            builder.ins().store(MemFlagsData::new(), tag_v, hp, 8);
        }
        Some(TirTy::Bytes) | Some(TirTy::Str) => {
            let zero = builder.ins().iconst(types::I64, 0);
            let two = builder.ins().iconst(types::I64, TAG_BAIL);
            builder.ins().store(MemFlagsData::new(), zero, hp, 0);
            builder.ins().store(MemFlagsData::new(), two, hp, 8);
        }
        Some(TirTy::ListInt | TirTy::ListFloat | TirTy::ListClass(_) | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::DictIntInt | TirTy::Class(_) | TirTy::ErrorObj | TirTy::Lambda(_) | TirTy::ListLambda | TirTy::Obj | TirTy::ListGen(_) | TirTy::DictGen(_, _) | TirTy::SetGen(_)) => {
            let zero = builder.ins().iconst(types::I64, 0);
            let two = builder.ins().iconst(types::I64, TAG_BAIL);
            builder.ins().store(MemFlagsData::new(), zero, hp, 0);
            builder.ins().store(MemFlagsData::new(), two, hp, 8);
        }
        Some(TirTy::Float) => {
            let bits = builder.ins().bitcast(types::I64, MemFlagsData::new(), results[0]);
            builder.ins().store(MemFlagsData::new(), bits, hp, 0);
            let tag_v = builder.ins().iconst(types::I64, TAG_FLOAT);
            builder.ins().store(MemFlagsData::new(), tag_v, hp, 8);
        }
        Some(TirTy::Bool) => {
            let bits = builder.ins().uextend(types::I64, results[0]);
            builder.ins().store(MemFlagsData::new(), bits, hp, 0);
            let tag_v = builder.ins().iconst(types::I64, TAG_BOOL);
            builder.ins().store(MemFlagsData::new(), tag_v, hp, 8);
        }
        None => {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().store(MemFlagsData::new(), zero, hp, 0);
            let tag_v = builder.ins().iconst(types::I64, TAG_NONE);
            builder.ins().store(MemFlagsData::new(), tag_v, hp, 8);
        }
    }
    builder.ins().return_(&[]);

    builder.seal_all_blocks();
    builder.finalize(module.target_config());
    module.define_function(ffi_fid, &mut func_ctx).map_err(|_| ())
}

fn emit_bail_ffi(
    module: &mut JITModule,
    ffi_fid: cranelift_module::FuncId,
    ffi_sig: &cranelift_codegen::ir::Signature,
) {
    let mut func_ctx = module.make_context();
    let mut builder_ctx = FunctionBuilderContext::new();
    func_ctx.func.signature = ffi_sig.clone();
    func_ctx.func.name = UserFuncName::user(0, ffi_fid.as_u32());
    let mut builder = FunctionBuilder::new(&mut func_ctx.func, &mut builder_ctx);
    let entry = builder.create_block(); 
    builder.append_block_params_for_function_params(entry);
    let incoming = builder.block_params(entry).to_vec();
    let hp = incoming[0];
    builder.switch_to_block(entry);
    let zero = builder.ins().iconst(types::I64, 0);
    let two = builder.ins().iconst(types::I64, TAG_BAIL);
    builder.ins().store(MemFlagsData::new(), zero, hp, 0);
    builder.ins().store(MemFlagsData::new(), two, hp, 8);
    builder.ins().return_(&[]);
    builder.seal_all_blocks();
    builder.finalize(module.target_config());
    let _ = module.define_function(ffi_fid, &mut func_ctx);
}





