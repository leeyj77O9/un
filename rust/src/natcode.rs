//! `natcode` ??compile-mode native backend: TIR ??Cranelift ??`jit_cache`.
//!
//! Every eligible core function is emitted twice:
//!
//! - `tir_core_{i}`: the internal, fully typed body. Static calling
//!   convention: `int` ??i64, `float` ??f64, `bool` ??i8. Calls to other
//!   functions in the same module go through this signature ??direct calls
//!   with no marshalling, no tag lists, no VM involvement.
//! - `tir_entry_{i}`: the boundary the VM (`jit_call`) can call: hidden
//!   result pointer (two i64 cells: `(value, tag)`), `param_tags` bitmap
//!   (bit p=IntVec handle, bit 32+p=Float bits, bit 40+p=Bool), then the
//!   raw i64 parameters.
//!
//! Calling convention for the bail flag: every function returns it as the
//! last `i8` return value (`0` = normal, `1` = bail). The FFI wrapper maps a
//! nonzero flag onto `TAG_BAIL`; internal callers propagate it outward until
//! the outermost entry sees it and the VM re-runs the whole call in the
//! interpreter ??never a wrong answer, never a silent miscompilation.

use crate::jit_cache::{self, ArgKind, JitEntry, TAG_BAIL, TAG_BOOL, TAG_FLOAT, TAG_INT, TAG_NONE};
use crate::bytecode::Program;
use crate::objify::EmitOutcome;
use crate::tir::{TirBinOp, TirCmp, TirExpr, TirFn, TirProgram, TirStmt, TirTy, TirUnOp};
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

/// Which backend the emitters target. `Vm` emits the in-process VM ABI
/// (hidden pointer, param_tags, i64 args, hidden i8 bail flag). `Obj` emits a
/// plain typed signature for the standalone object file — value returns only,
/// no bail flag, division failure traps instead of bailing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NativeMode {
    Vm,
    Obj,
}

/// Compile the TIR program, stash the native module in `jit_cache`, and
/// register every core entry there. Returns the names of functions that
/// failed to compile (their entries are always-bail stubs; the VM runs them
/// in the interpreter).
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
    let mut module = JITModule::new(builder);

    // Pass 1: declare internal + FFI entry for each function.
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

    // Pass 2: emit. A failing function is replaced by an always-bail FFI
    // stub; the interpreter picks up its semantics.
    for f in &tir.functions {
        // The VM JIT can't call the VM runtime for Str/heap-collection
        // operations, so those functions get an always-bail stub (the VM
        // runs them in the interpreter). Heap collections are the documented
        // fast path, not a failure — skip them silently so `un native`
        // stderr stays byte-identical to `un run`. Str functions keep the
        // pre-existing warning.
        let heap = crate::tir::fn_uses_heap(f);
        if crate::tir::fn_uses_str(f) || heap {
            if !heap {
                let name = f.name.clone();
                if !failed.iter().any(|n| n == &name) {
                    failed.push(name);
                }
            }
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
            emit_internal(f, &mut module, &internal_fids, *internal, &internal_sig, &tir.functions, NativeMode::Vm)
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
            // Internal compile failed: replace the FFI entry with a pure
            // bail stub so no symbol ever references the undefined internal.
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
    // Capture FFI pointers before stashing the module.
    let mut ffi_ptrs: HashMap<usize, *const u8> = HashMap::new();
    for (index, fid) in &ffi_fids {
        let ptr = module.get_finalized_function(*fid) as *const u8;
        ffi_ptrs.insert(*index, ptr);
    }
    jit_cache::stash_module(program_id, module);
    // Map proto spans to proto indices.
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
        let failed_here = failed.iter().any(|n| n == &f.name);
        if debug_trace {
            if failed_here {
                eprintln!("TIRDBG FAIL proto={} {} (interpreter will run it)", proto_index, f.name);
            } else {
                eprintln!("TIRDBG OK   proto={} {}", proto_index, f.name);
            }
        }
        let arg_kinds: Vec<ArgKind> = f
            .params
            .iter()
            .map(|t| match t {
                TirTy::Int => ArgKind::Int,
                TirTy::Float => ArgKind::Float,
                TirTy::Bool => ArgKind::Bool,
                // Str/List args are never passed to a VM-JIT candidate
                // (those are filtered out up front); reaching here is a
                // no-op mapping.
                TirTy::Str | TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj => ArgKind::Int,
            })
            .collect();
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

/// Emit an always-bail internal stub for a function the emitter cannot
/// translate. The caller (the FFI wrapper) converts this into a
/// bail-to-interpreter tag; the internal symbol still resolves.
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
        Some(TirTy::Str) | Some(TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj) => {
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

/// Compile the TIR program to a relocatable object file and write it to
/// `output`. Entry is the function named `entry` (default `main`), exported
/// as `un_main` with a C ABI (`() -> i64` for `int` returns). Functions that
/// fail to lower become trap stubs so the module stays linkable.
pub fn obj_compile(
    // Carried for symmetry with `native_compile`; unused (the TIR needs no
    // per-program id — no JIT cache is involved).
    _program_id: u64,
    tir: &TirProgram,
    output: &Path,
    entry: Option<&str>,
) -> Result<EmitOutcome, String> {
    if tir.functions.is_empty() {
        return Err("no core functions found (nothing is eligible for native code)".to_string());
    }
    if tir.functions.iter().any(crate::tir::fn_uses_classes) {
        return Err(
            "class-typed functions are not supported in a native executable yet; \
             rebuild with `un build --embed` (the embedded VM runs the whole program)"
                .to_string(),
        );
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

    // Pass 0: declare everything before any emission, so a caller never
    // references an undeclared callee.
    let mut fids: HashMap<usize, cranelift_module::FuncId> = HashMap::new();
    let mut skipped: Vec<String> = Vec::new();
    for (i, f) in tir.functions.iter().enumerate() {
        let is_entry = i == entry_index;
        let symbol = if is_entry {
            "un_main".to_string()
        } else {
            format!("un_tir_{i}")
        };
        let linkage = if is_entry { Linkage::Export } else { Linkage::Local };
        let sig = internal_signature(&module, f, NativeMode::Obj);
        match module.declare_function(&symbol, linkage, &sig) {
            Ok(fid) => {
                fids.insert(i, fid);
            }
            Err(e) => skipped.push(format!("{}: declare {symbol}: {e:?}", f.name)),
        }
    }

    // Pass 1: lower. Failure → trap stub so the module stays linkable.
    let mut emitted: Vec<String> = Vec::new();
    for (i, f) in tir.functions.iter().enumerate() {
        let Some(&fid) = fids.get(&i) else {
            skipped.push(format!("{}: declaration failed", f.name));
            continue;
        };
        let sig = internal_signature(&module, f, NativeMode::Obj);
        let guard = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            emit_internal(f, &mut module, &fids, fid, &sig, &tir.functions, NativeMode::Obj)
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

    Ok(EmitOutcome {
        emitted,
        skipped,
        output: output.to_path_buf(),
    })
}

/// Declare an imported C-ABI runtime function (used by Obj mode to call into
/// `unrt`). Vm mode never emits these calls.
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
            TirTy::Str | TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj => types::I64,
        }));
    }
    match &f.ret {
        Some(TirTy::Int) => sig.returns.push(AbiParam::new(types::I64)),
        Some(TirTy::Float) => sig.returns.push(AbiParam::new(types::F64)),
        Some(TirTy::Bool) => sig.returns.push(AbiParam::new(types::I8)),
        Some(TirTy::Str) => sig.returns.push(AbiParam::new(types::I64)),
        Some(TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj) => sig.returns.push(AbiParam::new(types::I64)),
        None => {}
    }
    if mode == NativeMode::Vm {
        // Hidden bail flag, returned as the last i8 always.
        sig.returns.push(AbiParam::new(types::I8));
    }
    sig
}

fn ffi_signature<M: Module>(module: &M, f: &TirFn) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64)); // hidden result pointer
    sig.params.push(AbiParam::new(types::I64)); // param_tags
    for _ in 0..f.params.len() {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig
}

// ---------------------------------------------------------------------------
// Emitters: internal (typed, static) functions and FFI wrappers.
// ---------------------------------------------------------------------------



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
    /// `Some` in Vm mode only (the VM recursion depth guard).
    fid_exit: Option<cranelift_module::FuncId>,
    acquired: Variable,
    bail_blk: Block,
    loop_breaks: Vec<Block>,
    loop_skips: Vec<Block>,
    terminated: bool,
    mode: NativeMode,
    // No trap_blk: Obj-mode paths bail through `emit_error_gate`; Vm-mode
    // paths bail through `bail_blk`.
    /// Every fallible unrt call that returns an error object jumps to the
    /// handler at the top of the stack instead of trapping.
    active_handlers: Vec<cranelift_codegen::ir::Block>,
    /// Import declarations for the `unrt` runtime bridge (Obj mode only).
    unrt_str_new: Option<cranelift_module::FuncId>,
    unrt_str_cat: Option<cranelift_module::FuncId>,
    unrt_call_native: Option<cranelift_module::FuncId>,
    unrt_obj_int: Option<cranelift_module::FuncId>,
    unrt_obj_bool: Option<cranelift_module::FuncId>,
    unrt_obj_float: Option<cranelift_module::FuncId>,
    unrt_list_len: Option<cranelift_module::FuncId>,
    unrt_list_get: Option<cranelift_module::FuncId>,
    unrt_list_from_ints: Option<cranelift_module::FuncId>,
    unrt_list_from_floats: Option<cranelift_module::FuncId>,
    unrt_int_from_obj: Option<cranelift_module::FuncId>,
    unrt_float_from_obj: Option<cranelift_module::FuncId>,
    unrt_dict_from_pairs: Option<cranelift_module::FuncId>,
    unrt_dict_get: Option<cranelift_module::FuncId>,
    unrt_dict_set: Option<cranelift_module::FuncId>,
    unrt_list_set: Option<cranelift_module::FuncId>,
    unrt_list_push: Option<cranelift_module::FuncId>,
    unrt_write_error: Option<cranelift_module::FuncId>,
    unrt_take_error: Option<cranelift_module::FuncId>,
    unrt_dump_uncaught: Option<cranelift_module::FuncId>,
    unrt_make_int_div_zero: Option<cranelift_module::FuncId>,
    unrt_make_mod_zero_panic: Option<cranelift_module::FuncId>,
    unrt_list_pop: Option<cranelift_module::FuncId>,
    unrt_list_pop_at: Option<cranelift_module::FuncId>,
    unrt_list_sort: Option<cranelift_module::FuncId>,
    unrt_list_reverse: Option<cranelift_module::FuncId>,
    unrt_list_insert: Option<cranelift_module::FuncId>,
    unrt_list_index_of: Option<cranelift_module::FuncId>,
    unrt_list_contains: Option<cranelift_module::FuncId>,
    unrt_obj_is_err: Option<cranelift_module::FuncId>,
    unrt_obj_eq: Option<cranelift_module::FuncId>,
    unrt_len: Option<cranelift_module::FuncId>,
    /// String literals discovered during lowering, interned into the object's
    /// data section (Obj mode only).
    str_literals: HashMap<Vec<u8>, cranelift_module::DataId>,
    str_literal_count: u32,
    /// TirProgram index of this function: namespaces data object names so
    /// identical literals in different functions don't collide.
    fn_index: u32,
}

/// Embed raw bytes as a data object in the object module and return the
/// global value that loads their address. Shared by string literals and
/// native-call module/member names.
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

/// Build the internal (typed) function body.
fn emit_internal<M: Module>(
    f: &TirFn,
    module: &mut M,
    internal_fids: &HashMap<usize, cranelift_module::FuncId>,
    func_id: cranelift_module::FuncId,
    sig: &cranelift_codegen::ir::Signature,
    fns: &[TirFn],
    mode: NativeMode,
) -> Result<(), ()> {
    // VM mode imports the depth guard; Obj mode has no VM to re-enter.
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

    // Import the unrt runtime bridge for Obj mode (Str values and native
    // calls go through it). Vm mode never emits Str/native code.
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
    let unrt_obj_is_err = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_obj_is_err", vec![types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_dict_set = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_dict_set", vec![types::I64, types::I64, types::I64], Some(types::I64))?)
    } else {
        None
    };
    let unrt_list_set = if mode == NativeMode::Obj {
        Some(declare_import(module, "unrt_list_set", vec![types::I64, types::I64, types::I64], Some(types::I64))?)
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

    let vars: Vec<Variable> = f
        .locals
        .iter()
        .map(|t| {
            builder.declare_var(match t {
                TirTy::Int => types::I64,
                TirTy::Float => types::F64,
                TirTy::Bool => types::I8,
                TirTy::Str | TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj => types::I64,
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
            TirTy::Str => {
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
            TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj => {
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
    // Depth enter: acquire or immediately bail (Vm only); Obj mode skips.
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
    // bail_entry: bail without acquiring (Vm only).
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

    // The shared bail block for guard failures (division by zero, native
    let bail_blk = builder.create_block();

    builder.switch_to_block(body_entry);
    // Body statements.
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
        active_handlers: Vec::new(),
        unrt_str_new,
        unrt_str_cat,
        unrt_call_native,
        unrt_obj_int,
        unrt_obj_bool,
        unrt_obj_float,
        unrt_list_len,
        unrt_list_get,
        unrt_list_from_ints,
        unrt_list_from_floats,
        unrt_int_from_obj,
        unrt_float_from_obj,
        unrt_dict_from_pairs,
        unrt_dict_get,
        unrt_dict_set,
        unrt_list_set,
        unrt_list_push,
        unrt_write_error,
        unrt_take_error,
        unrt_dump_uncaught,
        unrt_make_int_div_zero,
        unrt_make_mod_zero_panic,
        unrt_list_pop,
        unrt_list_pop_at,
        unrt_list_sort,
        unrt_list_reverse,
        unrt_list_insert,
        unrt_list_index_of,
        unrt_list_contains,
        unrt_obj_is_err,
        unrt_obj_eq,
        unrt_len,
        str_literals: HashMap::new(),
        str_literal_count: 0,
        fn_index: f.index as u32,
    };
    for stmt in &f.body {
        if emitter.terminated {
            continue;
        }
        emit_stmt(stmt, &mut emitter, &mut builder, module, f);
    }
    // Falling off the end: ret-None returns none; ret-value bails.
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

    // bail_blk: acquired depth is held; drop it and return junk with the
    // bail flag. (`emit_internal_return` exits the depth counter exactly
    // once, when the frame holds it.)
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

/// Emit a return from the internal function: exit the depth counter (Vm),
/// return the value (if any) and the bail flag (Vm only).
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
        (Some(TirTy::Str), Some(v)) => returns.push(v),
        (Some(TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj), Some(v)) => returns.push(v),
        (Some(TirTy::Int), None) => {
            returns.push(builder.ins().iconst(types::I64, 0));
        }
        (Some(TirTy::Float), None) => {
            returns.push(builder.ins().f64const(0.0));
        }
        (Some(TirTy::Str), None) => {
            returns.push(builder.ins().iconst(types::I64, 0));
        }
        (Some(TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj), None) => {
            returns.push(builder.ins().iconst(types::I64, 0));
        }
        (Some(TirTy::Bool), None) => {
            returns.push(builder.ins().iconst(types::I8, 0));
        }
        (None, _) => {}
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

/// Emit a sequence of statements. Returns `true` if control flow
/// terminated before the end of the block (e.g. a `Return`), `false` if
/// the block reached its natural end and should be sealed with a jump.
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
            let (value, _) = emit_expr(expr, emitter, builder, module, f);
            builder.def_var(var, value);
            false
        }
        TirStmt::If { cond, then_body, else_body } => {
            let (cond_v, _) = emit_expr(cond, emitter, builder, module, f);
            let then_blk = builder.create_block();
            let else_blk = builder.create_block();
            let merge_blk = builder.create_block();
            builder.ins().brif(cond_v, then_blk, &[], else_blk, &[]);

            builder.switch_to_block(then_blk);
            emitter.terminated = false; // fresh reachable block
            let then_terminated = emit_block(then_body, emitter, builder, module, f);
            if !then_terminated {
                builder.ins().jump(merge_blk, &[]);
            }
            builder.seal_block(then_blk);

            builder.switch_to_block(else_blk);
            emitter.terminated = false; // fresh reachable block
            let else_terminated = emit_block(else_body, emitter, builder, module, f);
            if !else_terminated {
                builder.ins().jump(merge_blk, &[]);
            }
            builder.seal_block(else_blk);

            builder.switch_to_block(merge_blk);
            if then_terminated && else_terminated {
                // No path reaches merge; give it a terminator so the block
                // stays well-formed, and stop emitting dead statements.
                builder.ins().jump(emitter.bail_blk, &[]);
                builder.seal_block(merge_blk);
                emitter.terminated = true;
                return true;
            }
            // merge_blk is reached from whichever arm did not terminate;
            // reset the flag so trailing statements are still emitted.
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
            // Do NOT seal the header yet: the back edge from the body's
            // terminal jump may not be known. Sealing is SSA construction —
            // seal only once every predecessor is emitted.
            builder.switch_to_block(body_blk);
            emitter.terminated = false; // fresh reachable block
            for s in body {
                if emitter.terminated {
                    break;
                }
                emit_stmt(s, emitter, builder, module, f);
            }
            if !emitter.terminated {
                builder.ins().jump(header, &[]);
            }
            builder.seal_block(body_blk);
            // Header now has all its predecessors (entry jump + back edge).
            builder.seal_block(header);
            emitter.loop_breaks.pop();
            emitter.loop_skips.pop();
            // exit_blk is reached from the loop guard; reset the terminated
            // flag so following statements still get emitted.
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
            // Do NOT seal the header yet: the back edge arrives via skip_blk.
            builder.switch_to_block(body_blk);
            emitter.terminated = false; // fresh reachable block
            for s in body {
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
            // skip_blk and header now have all their predecessors.
            builder.seal_block(skip_blk);
            builder.seal_block(header);
            emitter.loop_breaks.pop();
            emitter.loop_skips.pop();
            // exit_blk is reached from the loop guard; reset the terminated
            // flag so following statements still get emitted.
            emitter.terminated = false;
            builder.switch_to_block(exit_blk);
            builder.seal_block(exit_blk);
            false
        }
        TirStmt::ForEach { var, collection, body } => {
            let col_v = emit_expr(collection, emitter, builder, module, f).0;
            let var_var = emitter.vars[*var as usize];
            let elem_ty = emitter.local_tys[*var as usize];
            // Loop counter variable.
            let idx_var = builder.declare_var(types::I64);
            let zero = builder.ins().iconst(types::I64, 0);
            builder.def_var(idx_var, zero);
            let header = builder.create_block();
            let body_blk = builder.create_block();
            let exit_blk = builder.create_block();
            emitter.loop_breaks.push(exit_blk);
            builder.ins().jump(header, &[]);
            builder.switch_to_block(header);
            let idx = builder.use_var(idx_var);
            let len_id = emitter.unrt_list_len.expect("unrt import required for lists");
            let len_ref = module.declare_func_in_func(len_id, builder.func);
            let len_res = builder.ins().call(len_ref, &[col_v]);
            let len_v = builder.inst_results(len_res)[0];
            let cont = builder.ins().icmp(IntCC::SignedLessThan, idx, len_v);
            builder.ins().brif(cont, body_blk, &[], exit_blk, &[]);
            builder.switch_to_block(body_blk);
            let get_id = emitter.unrt_list_get.expect("unrt import required for lists");
            let get_ref = module.declare_func_in_func(get_id, builder.func);
            let idx2 = builder.use_var(idx_var);
            let elem_res = builder.ins().call(get_ref, &[col_v, idx2]);
            let elem_v = builder.inst_results(elem_res)[0];
            let elem_val = match elem_ty {
                TirTy::Int => {
                    let unbox = emitter.unrt_int_from_obj.expect("unrt import required for ints");
                    let ub_ref = module.declare_func_in_func(unbox, builder.func);
                    let res = builder.ins().call(ub_ref, &[elem_v]);
                    builder.inst_results(res)[0]
                }
                TirTy::Float => {
                    let unbox = emitter.unrt_float_from_obj.expect("unrt import required for floats");
                    let ub_ref = module.declare_func_in_func(unbox, builder.func);
                    let res = builder.ins().call(ub_ref, &[elem_v]);
                    builder.inst_results(res)[0]
                }
                _ => panic!("list iteration requires an int/float element type"),
            };
            builder.def_var(var_var, elem_val);
            emitter.terminated = false; // fresh reachable block
            for s in body {
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
            emitter.terminated = false; // exit_blk reached from loop guard
            builder.seal_block(exit_blk);
            builder.seal_block(header);
            false
        }
        TirStmt::Return(Some(expr)) => {
            let (value, _) = emit_expr(expr, emitter, builder, module, f);
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
            // `mod.member(args)` via `unrt_call_native`: build the argument
            // pointer array on the stack and call the bridge function.
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
                // Native calls take Obj arguments — wrap raw values first.
                let wrapped = match vty {
                    TirTy::Str => v,
                    TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj => v,
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
            let (list_v, _) = emit_expr(list, emitter, builder, module, f);
            let (idx_v, ity) = emit_expr(index, emitter, builder, module, f);
            if ity != TirTy::Int {
                panic!("list index must be int");
            }
            let (v_v, v_ty) = emit_expr(value, emitter, builder, module, f);
            let wrapped = match v_ty {
                TirTy::Int => {
                    let id = emitter.unrt_obj_int.expect("unrt import required for list assignment");
                    let int_ref = module.declare_func_in_func(id, builder.func);
                    let res = builder.ins().call(int_ref, &[v_v]);
                    builder.inst_results(res)[0]
                }
                TirTy::Float => {
                    let id = emitter.unrt_obj_float.expect("unrt import required for list assignment");
                    let float_ref = module.declare_func_in_func(id, builder.func);
                    let res = builder.ins().call(float_ref, &[v_v]);
                    builder.inst_results(res)[0]
                }
                _ => panic!("list elements must be int or float"),
            };
            let set_id = emitter.unrt_list_set.expect("unrt import required for list assignment");
            let set_ref = module.declare_func_in_func(set_id, builder.func);
            let res = builder.ins().call(set_ref, &[list_v, idx_v, wrapped]);
            let raw = builder.inst_results(res)[0];
            emit_error_gate(emitter, builder, module, raw);
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
        TirStmt::MemberSet { .. } => {
            unreachable!("class functions are rejected before Obj-mode emission")
        }
        TirStmt::Throw(expr) => {
            let (value, ty) = emit_expr(expr, emitter, builder, module, f);
            let obj = wrap_as_obj(value, ty, emitter, builder, module);
            let wid = emitter.unrt_write_error.expect("unrt import required");
            let wref = module.declare_func_in_func(wid, builder.func);
            let _ = builder.ins().call(wref, &[obj]);
            if let Some(&handler) = emitter.active_handlers.last() {
                builder.ins().jump(handler, &[]);
            } else {
                // Uncaught error leaked to the top: report and kill.
                let d_id = emitter.unrt_dump_uncaught.expect("unrt import required");
                let d_ref = module.declare_func_in_func(d_id, builder.func);
                let _ = builder.ins().call(d_ref, &[]);
                builder.ins().trap(cranelift_codegen::ir::TrapCode::unwrap_user(3));
            }
            emitter.terminated = true;
            true
        }
        TirStmt::Try { body, var, catch_body, finally_body } => {
            // Flow sketch:
            //   body      --ok--> success_blk --finally-> merge_blk
            //   body      --err--> handler_blk --bind-> catch_body -> success_blk
            // `finally_body` runs unconditionally once the try completes.
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
            // Handler: take the parked error object and bind it if the user
            // asked for a name.
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
            // Success/merge path: run `finally_body` unconditionally.
            builder.switch_to_block(success_blk);
            emitter.terminated = false;
            builder.seal_block(success_blk);
            if let Some(finally_body) = finally_body {
                // `finally_body` ends in a return path or a terminator that
                // finishes the function; otherwise control falls through to
                // the merge block.
                let fin_terminated = emit_block(finally_body, emitter, builder, module, f);
                if fin_terminated {
                    emitter.terminated = true;
                    return true;
                }
                builder.ins().jump(merge_blk, &[]);
            } else {
                // Unconditional transfer to the merge block after the try
                // completes: required for Cranelift to close `success_blk`
                // with a terminator.
                builder.ins().jump(merge_blk, &[]);
            }
            builder.switch_to_block(merge_blk);
            emitter.terminated = false;
            builder.seal_block(merge_blk);
            false
        }
        TirStmt::DictSet { dict, key, value } => {
            let (dict_v, _) = emit_expr(dict, emitter, builder, module, f);
            let (key_v, kty) = emit_expr(key, emitter, builder, module, f);
            if kty != TirTy::Str {
                panic!("dict key must be str");
            }
            let (v_v, v_ty) = emit_expr(value, emitter, builder, module, f);
            let wrapped = match v_ty {
                TirTy::Int => {
                    let id = emitter.unrt_obj_int.expect("unrt import required for dict assignment");
                    let int_ref = module.declare_func_in_func(id, builder.func);
                    let res = builder.ins().call(int_ref, &[v_v]);
                    builder.inst_results(res)[0]
                }
                TirTy::Float => {
                    let id = emitter.unrt_obj_float.expect("unrt import required for dict assignment");
                    let float_ref = module.declare_func_in_func(id, builder.func);
                    let res = builder.ins().call(float_ref, &[v_v]);
                    builder.inst_results(res)[0]
                }
                _ => panic!("dict values must be int or float"),
            };
            let set_id = emitter.unrt_dict_set.expect("unrt import required for dict assignment");
            let set_ref = module.declare_func_in_func(set_id, builder.func);
            let res = builder.ins().call(set_ref, &[dict_v, key_v, wrapped]);
            let raw = builder.inst_results(res)[0];
            emit_error_gate(emitter, builder, module, raw);
            false
        }
    }
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

/// Unbox an Obj-element of a list into the matching scalar representation.
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
        _ => panic!("list method requires a list[int] or list[float]"),
    }
}

/// Route an error-object return from a fallible unrt call: branch to the
/// active handler if any, else abort through `unrt_dump_uncaught`.
/// On success, execution continues. Defines no values visible to callers.
/// Every fallible unrt call on the fast path becomes:
///   %res = call <unrt fn>(...)
///   unrt_error_gate(emitter, builder, module, %res)
fn emit_error_gate<M: Module>(
    emitter: &mut FnEmit,
    builder: &mut FunctionBuilder,
    module: &mut M,
    obj: cranelift_codegen::ir::Value,
) {
    let is_err_id = match emitter.unrt_obj_is_err {
        Some(id) => id,
        None => return, // VM mode: no bridge — never reached
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
            let wid = emitter.unrt_write_error.expect("unrt import required");
            let wref = module.declare_func_in_func(wid, builder.func);
            let _ = builder.ins().call(wref, &[obj]);
            builder.ins().jump(handler, &[]);
        }
        None => {
            // No enclosing handler: park the error and abort the process
            // with the interpreter's uncaught-error behaviour.
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

/// Guard against `a // b`, `a % b` and `a / b` with an Int divisor of zero.
/// The interpreter reports `int division by zero` as a catchable error;
/// the native emitters must agree with that behaviour.
/// In Obj mode, the error object is constructed, parked as the pending
/// error, and control branches either to the enclosing handler or to
/// `unrt_dump_uncaught`. In Vm mode, control bails to the interpreter.
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
        // Build the same VM error object the interpreter would.
        let mid = emitter.unrt_make_int_div_zero.expect("unrt import required");
        let mref = module.declare_func_in_func(mid, builder.func);
        let res = builder.ins().call(mref, &[]);
        let err = builder.inst_results(res)[0];
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

/// Guard against `a % b` with an Int divisor of zero. Unlike `//` and `/`,
/// the interpreter reports int `%`-by-zero as a `Panic`-payload Obj
/// (`divide_by_zero()` → `System.DivideByZeroException`), which `try` never
/// catches — the interpreter always prints the report and exits 1.
/// In Obj mode this code parks that exact panic object pending and calls
/// `unrt_dump_uncaught` unconditionally (active handlers are bypassed), so
/// the native exe prints the identical line and exits 1. In Vm mode, control
/// bails to the interpreter, which does the same thing.
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
        // Build the same VM panic object the interpreter would and dump it.
        let mid = emitter.unrt_make_mod_zero_panic.expect("unrt import required");
        let mref = module.declare_func_in_func(mid, builder.func);
        let res = builder.ins().call(mref, &[]);
        let err = builder.inst_results(res)[0];
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

/// Wrap raw ints/bools/floats as VM Obj pointers for the unrt bridge.
/// Already-Obj values pass through unchanged.
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
        TirTy::Str | TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat
        | TirTy::Class(_) | TirTy::ErrorObj => v,
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
        TirExpr::Int(v) => (builder.ins().iconst(types::I64, *v), TirTy::Int),
        TirExpr::Str(s) => {
            // String literal: embed the bytes in the object's data section
            // and reference it with a global value.
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
            for (i, element) in elements.iter().enumerate() {
                let (v, ty) = emit_expr(element, emitter, builder, module, f);
                if elem_ty.is_none() {
                    elem_ty = Some(ty);
                }
                let addr = builder.ins().stack_addr(types::I64, slot_t, (8 * i) as i32);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), v, addr, 0);
            }
            let elem_ty = elem_ty.expect("non-empty list");
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
                _ => panic!("core lists require int/float elements"),
            };
            let from_ref = module.declare_func_in_func(from_fid, builder.func);
            let res = builder.ins().call(from_ref, &[ptr, len]);
            let obj = builder.inst_results(res)[0];
            (obj, ret_ty)
        }
        TirExpr::ClassNew { .. } | TirExpr::MemberGet { .. } | TirExpr::MethodCall { .. } => {
            unreachable!("class functions are rejected before Obj-mode emission")
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
            let mut elem_ty = None;
            for (i, (key, value)) in entries.iter().enumerate() {
                let (k_v, _) = emit_expr(key, emitter, builder, module, f);
                let (v_v, v_ty) = emit_expr(value, emitter, builder, module, f);
                if elem_ty.is_none() {
                    elem_ty = Some(v_ty);
                }
                // Values cross the bridge as Obj pointers (`unrt_dict_from_pairs`
                // expects a heterogeneous-object table): wrap raw ints/floats.
                let v_v = match v_ty {
                    TirTy::Int => {
                        let id = emitter.unrt_obj_int.expect("unrt import required for dict values");
                        let int_ref = module.declare_func_in_func(id, builder.func);
                        let res = builder.ins().call(int_ref, &[v_v]);
                        builder.inst_results(res)[0]
                    }
                    TirTy::Float => {
                        let id = emitter.unrt_obj_float.expect("unrt import required for dict values");
                        let float_ref = module.declare_func_in_func(id, builder.func);
                        let res = builder.ins().call(float_ref, &[v_v]);
                        builder.inst_results(res)[0]
                    }
                    _ => v_v, // already an Obj pointer (e.g. a Str)
                };
                let base = (16 * i) as i32;
                let addr = builder.ins().stack_addr(types::I64, slot_t, base);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), k_v, addr, 0);
                let addr = builder.ins().stack_addr(types::I64, slot_t, base + 8);
                builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), v_v, addr, 0);
            }
            let elem_ty = elem_ty.expect("non-empty dict");
            let ret_ty = match elem_ty {
                TirTy::Int => TirTy::DictStrInt,
                TirTy::Float => TirTy::DictStrFloat,
                _ => panic!("core dicts require int/float values"),
            };
            let ptr = builder.ins().stack_addr(types::I64, slot_t, 0);
            let len = builder.ins().iconst(types::I64, entries.len() as i64);
            let from_id = emitter.unrt_dict_from_pairs.expect("unrt import required for dicts");
            let from_ref = module.declare_func_in_func(from_id, builder.func);
            let res = builder.ins().call(from_ref, &[ptr, len]);
            let obj = builder.inst_results(res)[0];
            (obj, ret_ty)
        }
        TirExpr::Len(arg) => {
            let (arg_v, _) = emit_expr(arg, emitter, builder, module, f);
            let id = emitter.unrt_len.expect("unrt import required for len()");
            let len_ref = module.declare_func_in_func(id, builder.func);
            let res = builder.ins().call(len_ref, &[arg_v]);
            let raw = builder.inst_results(res)[0];
            emit_error_gate(emitter, builder, module, raw);
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
            emit_error_gate(emitter, builder, module, raw);
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
            emit_error_gate(emitter, builder, module, raw);
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
        TirExpr::DictGet(target, key) => {
            let (dict_v, dty) = emit_expr(target, emitter, builder, module, f);
            let (key_v, kty) = emit_expr(key, emitter, builder, module, f);
            if kty != TirTy::Str {
                panic!("dict key must be str");
            }
            let get_id = emitter.unrt_dict_get.expect("unrt import required for dict indexing");
            let get_ref = module.declare_func_in_func(get_id, builder.func);
            let res = builder.ins().call(get_ref, &[dict_v, key_v]);
            let raw = builder.inst_results(res)[0];
            emit_error_gate(emitter, builder, module, raw);
            match dty {
                TirTy::DictStrInt => {
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
                _ => panic!("indexing requires a dict[str, int] or dict[str, float]"),
            }
        }
        TirExpr::ListGet(target, index) => {
            let (list_v, lty) = emit_expr(target, emitter, builder, module, f);
            let (idx_v, ity) = emit_expr(index, emitter, builder, module, f);
            if ity != TirTy::Int {
                panic!("list index must be int");
            }
            let get_id = emitter.unrt_list_get.expect("unrt import required for list indexing");
            let get_ref = module.declare_func_in_func(get_id, builder.func);
            let raw = builder.ins().call(get_ref, &[list_v, idx_v]);
            let raw = builder.inst_results(raw)[0];
            emit_error_gate(emitter, builder, module, raw);
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
                _ => panic!("indexing requires a list[int] or list[float]"),
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
                TirUnOp::Neg => match ty {
                    TirTy::Int => (builder.ins().ineg(value), TirTy::Int),
                    TirTy::Float => (builder.ins().fneg(value), TirTy::Float),
                    TirTy::Str => unreachable!("validator forbids neg of str"),
                    TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj => {
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
                    TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj => {
                        unreachable!("validator forbids not of collection")
                    }
                },
            }
        }
        TirExpr::Bin(op, lhs, rhs) => {
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
            let mut arg_vals = Vec::with_capacity(args.len());
            for arg in args {
                let (v, _) = emit_expr(arg, emitter, builder, module, f);
                arg_vals.push(v);
            }
            let callee_ref = module.declare_func_in_func(calleefid, builder.func);
            let call = builder.ins().call(callee_ref, &arg_vals);
            let results = builder.inst_results(call).to_vec();
            if emitter.mode == NativeMode::Obj {
                // Plain typed call: no hidden bail flag at the tail.
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
            // int/int keeps-the-Int operands so div-by-zero routes through
            // emit_int_zero_guard (same machinery as `//`; int `%` uses
            // emit_mod_zero_guard because the interpreter treats `%` zero as
            // an uncatchable Panic).
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
            // Division by zero routes to the enclosing catch (Obj) or bails
            // to the interpreter (Vm). INT_MIN // -1 wraps like the
            // interpreter's wrapping_div: execute `sdiv` with a clamped
            // divisor so the hardware never faults on the INT_MIN/-1 case.
            emit_int_zero_guard(emitter, builder, module, b);
            let minus_one = builder.ins().iconst(types::I64, -1);
            let is_m1 = builder.ins().icmp(IntCC::Equal, b, minus_one);
            let one = builder.ins().iconst(types::I64, 1);
            let int_min = builder.ins().iconst(types::I64, i64::MIN);
            let b_safe = builder.ins().select(is_m1, one, b);
            let q = builder.ins().sdiv(a, b_safe);
            let a_int_min = int_min;
            let a_is_min = builder.ins().icmp(IntCC::Equal, a, a_int_min);
            let neg_a = builder.ins().ineg(a); // wraps: INT_MIN -> INT_MIN
            let m1_res = builder.ins().select(a_is_min, int_min, neg_a);
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
        TirTy::Bool => unreachable!("validator forbids bool arithmetic"),
        TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj => unreachable!("validator forbids collection-class arithmetic"),
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
    let numeric_pair = matches!((a_ty, b_ty), (TirTy::Int, TirTy::Int) | (TirTy::Int, TirTy::Float) | (TirTy::Float, TirTy::Int) | (TirTy::Float, TirTy::Float));
    if numeric_pair {
        if a_ty == TirTy::Int && b_ty == TirTy::Int {
            let cc = int_cmp(op);
            return (builder.ins().icmp(cc, a, b), TirTy::Bool);
        }
        let a = ensure_float(a, a_ty, builder);
        let b = ensure_float(b, b_ty, builder);
        if op == TirCmp::Ne {
            // Rust `!=` is true on NaN pairs; OrderedNotEqual is false there.
            let eq = builder.ins().fcmp(FloatCC::Equal, a, b);
            let one = builder.ins().iconst(types::I8, 1);
            return (builder.ins().bxor(eq, one), TirTy::Bool);
        }
        let cc = float_cmp(op);
        return (builder.ins().fcmp(cc, a, b), TirTy::Bool);
    }
    if a_ty == TirTy::Str && b_ty == TirTy::Str {
        // String equality via the VM runtime (`obj::key_eq`). Vm mode never
        // reaches here (Str functions are pre-filtered to the interpreter).
        let id = match emitter.unrt_obj_eq {
            Some(id) => id,
            None => {
                // Defensive: bail so the interpreter can re-run the call.
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
        // != is the boolean negation of ==.
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

/// Emit the FFI wrapper: converts parameters to the internal convention,
/// calls the typed internal function, and reports the result through the
/// hidden `(value, tag)` pair.
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

    // Convert the parameters to the internal convention.
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
                TirTy::Str => v,
                TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj => v,
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

    // bail_blk: report TAG_BAIL and return.
    builder.switch_to_block(bail_blk);
    let zero = builder.ins().iconst(types::I64, 0);
    let two = builder.ins().iconst(types::I64, TAG_BAIL);
    builder.ins().store(MemFlagsData::new(), zero, hp, 0);
    builder.ins().store(MemFlagsData::new(), two, hp, 8);
    builder.ins().return_(&[]);

    // ok_blk: convert the value and report it with its tag.
    builder.switch_to_block(ok_blk);
    match &f.ret {
        Some(TirTy::Int) => {
            let bits = results[0];
            builder.ins().store(MemFlagsData::new(), bits, hp, 0);
            let tag_v = builder.ins().iconst(types::I64, TAG_INT);
            builder.ins().store(MemFlagsData::new(), tag_v, hp, 8);
        }
        Some(TirTy::Str) => {
            // Str returns travel through the Vm FFI only when a Vm function
            // bails to the interpreter — report TAG_BAIL so the VM re-runs
            // the call instead of trusting this value.
            let zero = builder.ins().iconst(types::I64, 0);
            let two = builder.ins().iconst(types::I64, TAG_BAIL);
            builder.ins().store(MemFlagsData::new(), zero, hp, 0);
            builder.ins().store(MemFlagsData::new(), two, hp, 8);
        }
        Some(TirTy::ListInt | TirTy::ListFloat | TirTy::DictStrInt | TirTy::DictStrFloat | TirTy::Class(_) | TirTy::ErrorObj) => {
            // Same conservatism: heap collections and classes never cross the Vm boundary natively.
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

/// Replace a failed function with an always-bail FFI stub.
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





