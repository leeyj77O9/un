
use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::{AbiParam, BlockArg, InstBuilder, Value, types};
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
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
use crate::jit_cache::{
    PARAM_MASK_UNSAFE_SHIFT, RET_BOOL, RET_FLOAT, RET_INT, RET_INTLIST, RESULT_TAG_OFFSET,
    RESULT_VALUE_OFFSET,
};
use crate::vm::{Ins, ProtoPlan};

/// Maximum number of user arguments passed to a JIT'd proto. The value is
/// intentionally identical to `tir::MAX_CORE_ARITY` (Compile Mode), which is
/// private to that module; keep both at 8 so the two JIT paths agree.
pub const MAX_ARITY: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum MathFn {
    Sin, Cos, Tan, Asin, Acos, Atan,
    Sqrt, Cbrt, Exp,
    Log,    Log2, Log10,
    Floor, Ceil, Trunc, Sign,
    Pow, Hypot, Abs,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum MathFfi {
    Sin, Cos, Tan, Asin, Acos, Atan,
    Sqrt, Cbrt, Exp, Ln, Log2, Log10,
    Floor, Ceil, Abs,
    LogB, Hypot, Powf,
    Trunc, Sign,
}

impl MathFn {
    fn from_name(name: &str) -> Option<MathFn> {
        Some(match name {
            "sin" => MathFn::Sin, "cos" => MathFn::Cos, "tan" => MathFn::Tan,
            "asin" => MathFn::Asin, "acos" => MathFn::Acos, "atan" => MathFn::Atan,
            "sqrt" => MathFn::Sqrt, "cbrt" => MathFn::Cbrt, "exp" => MathFn::Exp,
            "log" => MathFn::Log, "log2" => MathFn::Log2, "log10" => MathFn::Log10,
            "floor" => MathFn::Floor, "ceil" => MathFn::Ceil, "trunc" => MathFn::Trunc,
            "sign" => MathFn::Sign,
            "pow" => MathFn::Pow, "hypot" => MathFn::Hypot, "abs" => MathFn::Abs,
            _ => return None,
        })
    }
}

const MATH_FNS: &[&str] = &[
    "sin", "cos", "tan", "asin", "acos", "atan",
    "sqrt", "cbrt", "exp", "log", "log2", "log10",
    "floor", "ceil", "trunc", "sign",
    "pow", "hypot", "abs",
];

fn program_has_intrinsic_rebind(program: &Program) -> bool {
    let mut has_math_use = false;
    let mut math_member_rebind = false;
    for proto in &program.protos {
        for ins in &proto.code {
            let Some(op) = ins.op_code() else { continue };
            if ins.operand < 0 {
                continue;
            }
            if op != Op::StoreName && op != Op::SetAttr && op != Op::MakeFunc && op != Op::LoadName {
                continue;
            }
            if op == Op::MakeFunc {
                if ins.flags & FLAG_FUNC_BIND != 0 {
                    let slot_of = ins.operand as usize;
                    if let Some(fn_proto) = program.protos.get(slot_of) {
                        if let Some(pname) = program.names.get(fn_proto.name) {
                            if matches!(pname.as_str(), "iter" | "range" | "len" | "math") {
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
            if op == Op::LoadName {
                if name == "math" {
                    has_math_use = true;
                }
                continue;
            }
            if name == "math" || matches!(name, "iter" | "range" | "len") {
                return true;
            }
            if op == Op::SetAttr && MATH_FNS.contains(&name) {
                math_member_rebind = true;
            }
        }
    }
    if has_math_use && math_member_rebind {
        return true;
    }
    false
}

pub fn register_eligible(program: &Program, program_id: u64) {
    if !crate::jit_cache::jit_enabled() {
        return;
    }
    // A panic in the setup phase (before per-proto emit has its own
    // catch_unwind) used to be swallowed here, silently disabling the whole
    // JIT even under UV_JIT=2. Per-proto failures are recorded as `failed`
    // and handled inside register_eligible_inner; only setup panics land here.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        register_eligible_inner(program, program_id)
    }));
    if result.is_err() && crate::jit_cache::jit_strict() != 0 {
        eprintln!("JITSTRICT: panic during JIT registration (program id {program_id})");
        std::process::exit(2);
    }
}
fn register_eligible_inner(program: &Program, program_id: u64) {
    let strict = crate::jit_cache::jit_strict();
    let mut failed: Vec<(usize, String)> = Vec::new();
    // Translate the bytecode once and share it between return-kind analysis and
    // every candidate emitter. Previously each candidate recompiled the whole
    // program.
    let plan = crate::vm::transfer::compile(program).ok();
    let ret_kinds = match &plan {
        Some(p) => analyze_return_kinds_with_plan(program, p),
        None => (0..program.protos.len()).map(|i| (i, 0)).collect(),
    };

    let candidates: Vec<usize> = (0..program.protos.len())
        .filter(|i| precheck_fn(program, *i).is_some())
        .collect();
    if candidates.is_empty() {
        return;
    }
    // Rebinding a JIT intrinsic (math/iter/range/len) disables JIT for the
    // whole program. It is outside the supported subset, but it also hid
    // every *other* eligible proto in the same program from UV_JIT=2.
    if program_has_intrinsic_rebind(program) {
        if crate::jit_cache::jit_debug() {
            eprintln!(
                "JITDBG SKIP program: intrinsic rebind ({} candidate proto(s))",
                candidates.len()
            );
        }
        if strict != 0 {
            eprintln!(
                "JITSTRICT: program-wide intrinsic rebind disables JIT for {} candidate proto(s)",
                candidates.len()
            );
            std::process::exit(2);
        }
        return;
    }
    let native = match native_builder() {
        Ok(n) => n,
        Err(_) => {
            if strict != 0 {
                eprintln!("JITSTRICT: native backend unavailable; refusing to run without the JIT");
                std::process::exit(2);
            }
            return;
        }
    };
    let mut flag_builder = settings::builder();
    let _ = flag_builder.set("is_pic", "false");
    let isa = match native.finish(settings::Flags::new(flag_builder)) {
        Ok(isa) => isa,
        Err(_) => {
            if strict != 0 {
                eprintln!("JITSTRICT: failed to build the ISA; refusing to run without the JIT");
                std::process::exit(2);
            }
            return;
        }
    };
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    builder.symbol("un_jit_depth_enter", crate::jit_cache::un_jit_depth_enter as *const u8);
    builder.symbol("un_jit_depth_exit", crate::jit_cache::un_jit_depth_exit as *const u8);
    builder.symbol("un_dict_int_empty", crate::runtime::int_dict::un_dict_int_empty as *const u8);
    builder.symbol("un_dict_int_set", crate::runtime::int_dict::un_dict_int_set as *const u8);
    builder.symbol("un_dict_int_get", crate::runtime::int_dict::un_dict_int_get as *const u8);
    builder.symbol("un_dict_int_has", crate::runtime::int_dict::un_dict_int_has as *const u8);
    builder.symbol("un_dict_int_free", crate::runtime::int_dict::un_dict_int_free as *const u8);
    builder.symbol("un_instance_int_get", crate::runtime::obj::un_instance_int_get as *const u8);
    builder.symbol("un_instance_int_ok", crate::runtime::obj::un_instance_int_ok as *const u8);
    builder.symbol("un_instance_int_ok_name", crate::runtime::obj::un_instance_int_ok_name as *const u8);
    builder.symbol("un_instance_int_set_name", crate::runtime::obj::un_instance_int_set_name as *const u8);
    builder.symbol("un_class_make_0", crate::runtime::int_class::un_class_make_0 as *const u8);
    builder.symbol("un_class_make_1", crate::runtime::int_class::un_class_make_1 as *const u8);
    builder.symbol("un_class_make_2", crate::runtime::int_class::un_class_make_2 as *const u8);
    builder.symbol("un_class_make_3", crate::runtime::int_class::un_class_make_3 as *const u8);
    builder.symbol("un_class_make_4", crate::runtime::int_class::un_class_make_4 as *const u8);
    builder.symbol("un_class_make_5", crate::runtime::int_class::un_class_make_5 as *const u8);
    builder.symbol("un_class_make_6", crate::runtime::int_class::un_class_make_6 as *const u8);
    builder.symbol("un_class_make_7", crate::runtime::int_class::un_class_make_7 as *const u8);
    builder.symbol("un_class_make_8", crate::runtime::int_class::un_class_make_8 as *const u8);
    builder.symbol("un_list_int_empty", crate::runtime::int_list::un_list_int_empty as *const u8);
    builder.symbol("un_list_int_push", crate::runtime::int_list::un_list_int_push as *const u8);
    builder.symbol("un_list_int_push_scalar", crate::runtime::int_list::un_list_int_push_scalar as *const u8);
    builder.symbol("un_list_int_get", crate::runtime::int_list::un_list_int_get as *const u8);
    builder.symbol("un_list_int_set", crate::runtime::int_list::un_list_int_set as *const u8);
    builder.symbol("un_list_int_set_scalar", crate::runtime::int_list::un_list_int_set_scalar as *const u8);
    builder.symbol("un_list_int_len", crate::runtime::int_list::un_list_int_len as *const u8);
    builder.symbol("un_list_int_slice", crate::runtime::int_list::un_list_int_slice as *const u8);
    builder.symbol("un_list_int_free", crate::runtime::int_list::un_list_int_free as *const u8);
    builder.symbol("un_list_int_is_handle", crate::runtime::int_list::un_list_int_is_handle as *const u8);
    builder.symbol("un_list_int_iter_new", crate::runtime::int_list::un_list_int_iter_new as *const u8);
    builder.symbol("un_list_int_iter_has_next", crate::runtime::int_list::un_list_int_iter_has_next as *const u8);
    builder.symbol("un_list_int_iter_next", crate::runtime::int_list::un_list_int_iter_next as *const u8);
    builder.symbol("un_list_int_iter_free", crate::runtime::int_list::un_list_int_iter_free as *const u8);
    let math_entries: &[(MathFfi, &str, *const u8, u8)] = &[
        (MathFfi::Sin, "un_math_sin", crate::native::math::un_math_sin as *const u8, 0),
        (MathFfi::Cos, "un_math_cos", crate::native::math::un_math_cos as *const u8, 0),
        (MathFfi::Tan, "un_math_tan", crate::native::math::un_math_tan as *const u8, 0),
        (MathFfi::Asin, "un_math_asin", crate::native::math::un_math_asin as *const u8, 0),
        (MathFfi::Acos, "un_math_acos", crate::native::math::un_math_acos as *const u8, 0),
        (MathFfi::Atan, "un_math_atan", crate::native::math::un_math_atan as *const u8, 0),
        (MathFfi::Sqrt, "un_math_sqrt", crate::native::math::un_math_sqrt as *const u8, 0),
        (MathFfi::Cbrt, "un_math_cbrt", crate::native::math::un_math_cbrt as *const u8, 0),
        (MathFfi::Exp, "un_math_exp", crate::native::math::un_math_exp as *const u8, 0),
        (MathFfi::Ln, "un_math_ln", crate::native::math::un_math_ln as *const u8, 0),
        (MathFfi::Log2, "un_math_log2", crate::native::math::un_math_log2 as *const u8, 0),
        (MathFfi::Log10, "un_math_log10", crate::native::math::un_math_log10 as *const u8, 0),
        (MathFfi::Floor, "un_math_floor", crate::native::math::un_math_floor as *const u8, 0),
        (MathFfi::Ceil, "un_math_ceil", crate::native::math::un_math_ceil as *const u8, 0),
        (MathFfi::Abs, "un_math_abs", crate::native::math::un_math_abs as *const u8, 0),
        (MathFfi::LogB, "un_math_log", crate::native::math::un_math_log as *const u8, 1),
        (MathFfi::Hypot, "un_math_hypot", crate::native::math::un_math_hypot as *const u8, 1),
        (MathFfi::Powf, "un_math_powf", crate::native::math::un_math_powf as *const u8, 1),
        (MathFfi::Trunc, "un_math_trunc_i64", crate::native::math::un_math_trunc_i64 as *const u8, 2),
        (MathFfi::Sign, "un_math_sign", crate::native::math::un_math_sign as *const u8, 2),
    ];
    for &(_, sym, ptr, _) in math_entries {
        builder.symbol(sym, ptr);
    }
    let mut module = JITModule::new(builder);

    // Every symbol declaration below is a prerequisite for emitting any
    // candidate in this program. Losing one silently disabled the whole JIT
    // (TD-51 class). Under UV_JIT=2 that must be a loud exit(2), never a
    // quiet interpreter fallback.
    macro_rules! jit_decl_failure {
        ($label:expr) => {{
            if crate::jit_cache::jit_debug() {
                eprintln!("JITDBG SKIP program: declaring {} failed", $label);
            }
            if strict != 0 {
                eprintln!("JITSTRICT: declaring {} for JIT failed", $label);
                std::process::exit(2);
            }
            return;
        }};
    }

    let mut sig_enter = module.make_signature();
    sig_enter.returns.push(AbiParam::new(types::I64));
    let sig_exit = module.make_signature();    let fid_enter = match module
        .declare_function("un_jit_depth_enter", Linkage::Import, &sig_enter)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_jit_depth_enter"),
    };
    let fid_exit = match module
        .declare_function("un_jit_depth_exit", Linkage::Import, &sig_exit)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_jit_depth_exit"),
    };
    let mut sig_empty = module.make_signature();
    sig_empty.returns.push(AbiParam::new(types::I64));
    let fid_dict_empty = match module
        .declare_function("un_dict_int_empty", Linkage::Import, &sig_empty)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_dict_int_empty"),
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
        Err(_) => jit_decl_failure!("un_dict_int_set"),
    };
    let mut sig_get = module.make_signature();
    sig_get.params.push(AbiParam::new(types::I64));
    sig_get.params.push(AbiParam::new(types::I64));
    sig_get.returns.push(AbiParam::new(types::I64));
    let fid_dict_get = match module
        .declare_function("un_dict_int_get", Linkage::Import, &sig_get)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_dict_int_get"),
    };
    let fid_dict_has = match module
        .declare_function("un_dict_int_has", Linkage::Import, &sig_get)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_dict_int_has"),
    };
    let mut sig_free = module.make_signature();
    sig_free.params.push(AbiParam::new(types::I64));
    let fid_dict_free = match module
        .declare_function("un_dict_int_free", Linkage::Import, &sig_free)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_dict_int_free"),
    };
    let mut sig_ig = module.make_signature();
    sig_ig.params.push(AbiParam::new(types::I64));
    sig_ig.params.push(AbiParam::new(types::I64));
    sig_ig.returns.push(AbiParam::new(types::I64));
    let fid_int_get = match module.declare_function("un_instance_int_get", Linkage::Import, &sig_ig) {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_instance_int_get"),
    };
    let mut sig_ionn = module.make_signature();
    sig_ionn.params.push(AbiParam::new(types::I64));
    sig_ionn.params.push(AbiParam::new(types::I64));
    sig_ionn.params.push(AbiParam::new(types::I64));
    sig_ionn.params.push(AbiParam::new(types::I64));
    sig_ionn.returns.push(AbiParam::new(types::I64));
    let fid_int_ok_name = match module.declare_function("un_instance_int_ok_name", Linkage::Import, &sig_ionn) {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_instance_int_ok_name"),
    };
    let mut sig_isn = module.make_signature();
    for _ in 0..5 {
        sig_isn.params.push(AbiParam::new(types::I64));
    }
    sig_isn.returns.push(AbiParam::new(types::I64));
    let fid_int_set_name = match module.declare_function("un_instance_int_set_name", Linkage::Import, &sig_isn) {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_instance_int_set_name"),
    };
    let mut sig_l_empty = module.make_signature();
    sig_l_empty.returns.push(AbiParam::new(types::I64));
    let fid_list_empty = match module
        .declare_function("un_list_int_empty", Linkage::Import, &sig_l_empty)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_empty"),
    };
    let mut sig_l_push = module.make_signature();
    for _ in 0..3 {
        sig_l_push.params.push(AbiParam::new(types::I64));
    }
    sig_l_push.returns.push(AbiParam::new(types::I64));
    let fid_list_push = match module
        .declare_function("un_list_int_push", Linkage::Import, &sig_l_push)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_push"),
    };
    let fid_list_push_scalar = match module
        .declare_function("un_list_int_push_scalar", Linkage::Import, &sig_l_push)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_push_scalar"),
    };
    let mut sig_l_get = module.make_signature();
    sig_l_get.params.push(AbiParam::new(types::I64));
    sig_l_get.params.push(AbiParam::new(types::I64));
    sig_l_get.returns.push(AbiParam::new(types::I64));
    let fid_list_get = match module
        .declare_function("un_list_int_get", Linkage::Import, &sig_l_get)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_get"),
    };
    let mut sig_l_set = module.make_signature();
    for _ in 0..4 {
        sig_l_set.params.push(AbiParam::new(types::I64));
    }
    sig_l_set.returns.push(AbiParam::new(types::I64));
    let fid_list_set = match module
        .declare_function("un_list_int_set", Linkage::Import, &sig_l_set)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_set"),
    };
    let fid_list_set_scalar = match module
        .declare_function("un_list_int_set_scalar", Linkage::Import, &sig_l_set)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_set_scalar"),
    };
    let mut sig_l_len = module.make_signature();
    sig_l_len.params.push(AbiParam::new(types::I64));
    sig_l_len.returns.push(AbiParam::new(types::I64));
    let fid_list_len = match module
        .declare_function("un_list_int_len", Linkage::Import, &sig_l_len)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_len"),
    };
    let mut sig_l_slice = module.make_signature();
    sig_l_slice.params.push(AbiParam::new(types::I64));
    sig_l_slice.params.push(AbiParam::new(types::I64));
    sig_l_slice.params.push(AbiParam::new(types::I64));
    sig_l_slice.returns.push(AbiParam::new(types::I64));
    let fid_list_slice = match module
        .declare_function("un_list_int_slice", Linkage::Import, &sig_l_slice)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_slice"),
    };
    let mut sig_l_free = module.make_signature();
    sig_l_free.params.push(AbiParam::new(types::I64));
    let fid_list_free = match module
        .declare_function("un_list_int_free", Linkage::Import, &sig_l_free)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_free"),
    };
    let mut sig_l_is = module.make_signature();
    sig_l_is.params.push(AbiParam::new(types::I64));
    sig_l_is.returns.push(AbiParam::new(types::I64));
    let fid_list_is_handle = match module
        .declare_function("un_list_int_is_handle", Linkage::Import, &sig_l_is)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_is_handle"),
    };
    let mut sig_iter_new = module.make_signature();
    sig_iter_new.params.push(AbiParam::new(types::I64));
    sig_iter_new.returns.push(AbiParam::new(types::I64));
    let fid_list_iter_new = match module
        .declare_function("un_list_int_iter_new", Linkage::Import, &sig_iter_new)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_iter_new"),
    };
    let mut sig_iter_has = module.make_signature();
    sig_iter_has.params.push(AbiParam::new(types::I64));
    sig_iter_has.returns.push(AbiParam::new(types::I64));
    let fid_list_iter_has = match module
        .declare_function("un_list_int_iter_has_next", Linkage::Import, &sig_iter_has)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_iter_has_next"),
    };
    let mut sig_iter_next = module.make_signature();
    sig_iter_next.params.push(AbiParam::new(types::I64));
    sig_iter_next.returns.push(AbiParam::new(types::I64));
    let fid_list_iter_next = match module
        .declare_function("un_list_int_iter_next", Linkage::Import, &sig_iter_next)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_iter_next"),
    };
    let mut sig_iter_free = module.make_signature();
    sig_iter_free.params.push(AbiParam::new(types::I64));
    let fid_list_iter_free = match module
        .declare_function("un_list_int_iter_free", Linkage::Import, &sig_iter_free)
    {
        Ok(fid) => fid,
        Err(_) => jit_decl_failure!("un_list_int_iter_free"),
    };
    let mut fid_class_make = Vec::with_capacity(9);
    for arity in 0..=8usize {
        let mut sig = module.make_signature();
        for _ in 0..=arity {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        match module.declare_function(
            &format!("un_class_make_{arity}"),
            Linkage::Import,
            &sig,
        ) {
            Ok(fid) => fid_class_make.push(fid),
            Err(_) => jit_decl_failure!("un_class_make"),
        }
    }

    let mut sig_f64_un = module.make_signature();
    sig_f64_un.params.push(AbiParam::new(types::F64));
    sig_f64_un.returns.push(AbiParam::new(types::F64));
    let mut sig_f64_bin = module.make_signature();
    sig_f64_bin.params.push(AbiParam::new(types::F64));
    sig_f64_bin.params.push(AbiParam::new(types::F64));
    sig_f64_bin.returns.push(AbiParam::new(types::F64));
    let mut sig_f64_to_i64 = module.make_signature();
    sig_f64_to_i64.params.push(AbiParam::new(types::F64));
    sig_f64_to_i64.returns.push(AbiParam::new(types::I64));

    let mut fid_math: HashMap<MathFfi, cranelift_module::FuncId> = HashMap::with_capacity(math_entries.len());
    for &(mfn, sym, _, sig_kind) in math_entries {
        let fid = match sig_kind {
            0 => module.declare_function(sym, Linkage::Import, &sig_f64_un),
            1 => module.declare_function(sym, Linkage::Import, &sig_f64_bin),
            _ => module.declare_function(sym, Linkage::Import, &sig_f64_to_i64),
        };
        match fid {
            Ok(f) => { fid_math.insert(mfn, f); }
            Err(_) => jit_decl_failure!("math FFI symbol"),
        }
    }

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
        sig.params.push(AbiParam::new(types::I64));
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
            Err(_) => {
                if strict != 0 {
                    let name = program.names.get(program.protos[idx].name).cloned().unwrap_or_else(|| "?".into());
                    eprintln!("JITSTRICT: declaring JIT function for proto {idx} ({name}) failed");
                    std::process::exit(2);
                }
                return;
            }
        }
    }
    if decls.is_empty() {
        return;
    }
    let verbose = crate::jit_cache::jit_debug();
    for &idx in &candidates {
        let func_id = match decls.get(&idx) {
            Some(fid) => *fid,
            None => continue,
        };
        let Some(shared_plan) = plan.as_ref() else { break };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            emit_fn_in_module(
                program,
                shared_plan,
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
                fid_dict_has,
                fid_dict_free,
                fid_int_get,
                fid_int_ok_name,
                fid_int_set_name,
                fid_list_empty,
                fid_list_push,
                fid_list_push_scalar,
                fid_list_get,
                fid_list_set,
                fid_list_set_scalar,
                fid_list_len,
                fid_list_slice,
                fid_list_free,
                fid_list_is_handle,
                fid_list_iter_new,
                fid_list_iter_has,
                fid_list_iter_next,
                fid_list_iter_free,
                &fid_class_make,
                &fid_math,
                &ret_kinds,
            )
        }));
        let result = match result {
            Ok(v) => v,
            Err(payload) => {
                if crate::jit_cache::jit_debug() {
                    let panic_msg = if let Some(s) = payload.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = payload.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "<non-string panic payload>".to_string()
                    };
                    eprintln!("JITDBG PANIC proto[{idx}]: {panic_msg}");
                }
                None
            }
        };
        if result.is_none() {
            let name = program.names.get(program.protos[idx].name).cloned().unwrap_or_else(|| "?".into());
            if crate::jit_cache::jit_debug() {
                eprintln!("JITREJECT proto[{idx}] {name} (JIT codegen failed, fallback stub)");
            }
            failed.push((idx, name));
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
    if strict != 0 && !failed.is_empty() {
        let names: Vec<String> = failed.iter().map(|&(i, ref n)| format!("{i}:{n}")).collect();
        eprintln!("JITSTRICT: {}/{} candidate(s) failed JIT codegen: {}", failed.len(), candidates.len(), names.join(", "));
        eprintln!("JITSTRICT: curate the workload or unset UV_JIT (0 = interpreter, 1/default = JIT with fallback)");
        std::process::exit(2);
    }
    if module.finalize_definitions().is_err() {
        if strict != 0 {
            eprintln!("JITSTRICT: finalizing the JIT module failed");
            std::process::exit(2);
        }
        return;
    }
    for &idx in &candidates {
        let func_id = decls[&idx];
        let code_ptr = module.get_finalized_function(func_id) as *const u8 as usize;
        let arity = match precheck_fn(program, idx) {
            Some(a) => a,
            None => continue,
        };
        crate::jit_cache::register_jit_entry(
            program_id,
            idx as u32,
            crate::jit_cache::JitEntry {
                ptr: code_ptr as *const u8,
                arity: arity as u32,
                arg_kinds: Vec::new(),
            },
        );
    }
    crate::jit_cache::stash_module(program_id, module);
}

/// Mismatch-path style used by [`Tr::emit_check_callee_result`].
/// The two call sites historically wrote `TAG_BAIL` through different (but
/// equivalent) helpers; keep the distinction so each site stays recognizable.
#[derive(Clone, Copy)]
enum CalleeBail {
    /// Write the `(0, TAG_BAIL)` pair via `emit_return_pair` (user-call site).
    ReturnPair,
    /// Write it via `emit_bail_exit` (class-method-call site).
    BailExit,
}

struct Tr<'a> {
    program: &'a Program,
    proto: &'a Proto,
    proto_index: usize,
    plan: &'a ProtoPlan,
    arena: Vec<String>,
    param_pos: HashMap<String, usize>,
    local_name_to_slot: HashMap<String, usize>,
    patches: HashMap<usize, RangePatch>,
    arity: usize,
    param_regs: HashSet<usize>,
    param_idx: HashMap<usize, usize>,
    local_param_pos: Vec<Option<usize>>,
    local_borrow_raw: Vec<bool>,
    locals_len: usize,
    field_locals: HashSet<usize>,
    vars: Vec<Variable>,
    f64_vars: Vec<Variable>,
    is_bool: Vec<bool>,
    is_none: Vec<bool>,
    is_float: Vec<bool>,
    local_is_bool: Vec<bool>,
    local_is_none: Vec<bool>,
    local_float: Vec<bool>,
    ret_slot: Vec<Option<usize>>,
    blocks: HashMap<usize, Block>,
    loop_inc: HashMap<usize, (Block, usize)>,
    poly_tag: Variable,
    hp_var: Variable,
    decls: HashMap<usize, cranelift_module::FuncId>,
    fid_exit: cranelift_module::FuncId,
    needs_depth: bool,
    ret_kinds: HashMap<usize, u8>,
    fid_dict_empty: cranelift_module::FuncId,
    fid_dict_set: cranelift_module::FuncId,
    fid_dict_get: cranelift_module::FuncId,
    fid_dict_has: cranelift_module::FuncId,
    fid_dict_free: cranelift_module::FuncId,
    fid_int_get: cranelift_module::FuncId,
    fid_int_ok_name: cranelift_module::FuncId,
    fid_int_set_name: cranelift_module::FuncId,
    fid_class_make: Vec<cranelift_module::FuncId>,
    fid_list_empty: cranelift_module::FuncId,
    fid_list_push: cranelift_module::FuncId,
    fid_list_push_scalar: cranelift_module::FuncId,
    fid_list_get: cranelift_module::FuncId,
    fid_list_set: cranelift_module::FuncId,
    fid_list_set_scalar: cranelift_module::FuncId,
    fid_list_len: cranelift_module::FuncId,
    fid_list_slice: cranelift_module::FuncId,
    fid_list_free: cranelift_module::FuncId,
    fid_list_is_handle: cranelift_module::FuncId,
    fid_list_iter_new: cranelift_module::FuncId,
    fid_list_iter_has: cranelift_module::FuncId,
    fid_list_iter_next: cranelift_module::FuncId,
    fid_list_iter_free: cranelift_module::FuncId,
    class_layouts: HashMap<usize, Vec<String>>,
    is_class_method: bool,
    self_regs: HashSet<usize>,
    intlist_regs: HashMap<usize, u8>,
    local_is_list: Vec<u8>,
    len_regs: HashSet<usize>,
    math_regs: HashSet<usize>,
    mathfn_regs: HashMap<usize, MathFn>,
    fid_math: HashMap<MathFfi, cranelift_module::FuncId>,
    list_add_regs: HashMap<usize, usize>,
    intlist_iter_regs: HashSet<usize>,
    class_ctor_regs: HashMap<usize, i64>,
    instance_regs: HashMap<usize, i64>,
    method_regs: HashMap<usize, (i64, usize, usize)>,
    class_methods: HashMap<i64, HashMap<String, usize>>,
    class_desc_ids: HashMap<String, i64>,
    int_dict_regs: HashSet<usize>,
    local_is_intdict: Vec<bool>,
    local_is_instance: Vec<Option<i64>>,
    callee_lookup: HashMap<String, usize>,
    call_candidate_regs: HashMap<usize, usize>,
    module: &'a mut JITModule,
    terminated: bool,
    cur_blk: Option<Block>,
}

impl<'a> Tr<'a> {
    fn goto(&mut self, builder: &mut FunctionBuilder, blk: Block) {
        if self.cur_blk.is_some() && !self.terminated && self.cur_blk != Some(blk) {
            builder.ins().jump(blk, &[]);
        }
        self.cur_blk = Some(blk);
        self.terminated = false;
        builder.switch_to_block(blk);
    }
    fn int_list_foreign_seen(&self, regs: &[usize]) -> bool {
        regs.iter().any(|&r| {
            self.int_dict_regs.contains(&r)
                || self.instance_regs.contains_key(&r)
                || self.class_ctor_regs.contains_key(&r)
                || self.method_regs.contains_key(&r)
                || self.len_regs.contains(&r)
                || self.intlist_iter_regs.contains(&r)
        })
    }
    /// Compile-time rejection: this proto runs in the interpreter instead.
    /// This is never a user-visible engine error, so the reason is only
    /// printed when `UV_JIT_DEBUG=1` is set.
    fn reject<T>(&self, reason: &str) -> Option<T> {
        if crate::jit_cache::jit_debug() {
            eprintln!("JITREJECT proto[{}] {}", self.proto_index, reason);
        }
        None
    }

    /// Clear every specialized-type annotation on a register. This must be
    /// called whenever an instruction overwrites a register with a plain
    /// value; otherwise object annotations from an earlier value that reused
    /// the same register survive and make arithmetic calls reject the proto.
    fn clear_specialized(&mut self, reg: usize) {
        self.int_dict_regs.remove(&reg);
        self.instance_regs.remove(&reg);
        self.class_ctor_regs.remove(&reg);
        self.method_regs.remove(&reg);
        self.intlist_regs.remove(&reg);
        self.len_regs.remove(&reg);
        self.intlist_iter_regs.remove(&reg);
        self.math_regs.remove(&reg);
        self.mathfn_regs.remove(&reg);
        self.self_regs.remove(&reg);
        self.call_candidate_regs.remove(&reg);
        self.param_regs.remove(&reg);
        self.param_idx.remove(&reg);
    }

    fn int_dict_seen(&self, regs: &[usize]) -> bool {
        regs.iter().any(|&r| {
            self.int_dict_regs.contains(&r)
                || self.instance_regs.contains_key(&r)
                || self.class_ctor_regs.contains_key(&r)
                || self.method_regs.contains_key(&r)
                || self.intlist_regs.contains_key(&r)
                || self.len_regs.contains(&r)
                || self.intlist_iter_regs.contains(&r)
        })
    }

    fn emit_user_call(
        &mut self,
        builder: &mut FunctionBuilder,
        ins: Ins,
        argc: usize,
        callee_reg: usize,
        proto_index: usize,
    ) -> Option<()> {
        if self.is_none[callee_reg] || self.is_float[callee_reg] || self.is_bool[callee_reg] {
            if crate::jit_cache::jit_debug() {
                eprintln!("JITREJECT emit_user_call callee_reg={callee_reg} is_none={} is_float={} is_bool={}", self.is_none[callee_reg], self.is_float[callee_reg], self.is_bool[callee_reg]);
            }
            return None;
        }
        if self.int_dict_seen(&[callee_reg]) {
            if crate::jit_cache::jit_debug() {
                eprintln!("JITREJECT emit_user_call callee_reg={callee_reg} in int_dict_seen");
            }
            return None;
        }
        for k in 0..argc {
            let r = callee_reg + 1 + k;
            if self.is_none[r] || self.is_float[r] || self.is_bool[r] {
                if crate::jit_cache::jit_debug() {
                    eprintln!("JITREJECT emit_user_call arg{k}=={r} is_none={} is_float={} is_bool={}", self.is_none[r], self.is_float[r], self.is_bool[r]);
                }
                return None;
            }
            if self.int_list_foreign_seen(&[r]) {
                if crate::jit_cache::jit_debug() {
                    eprintln!("JITREJECT emit_user_call arg{k}=={r} foreign handle");
                }
                return None;
            }
        }
        if argc > MAX_ARITY {
            if crate::jit_cache::jit_debug() {
                eprintln!(
                    "JITREJECT proto[{}] emit_user_call: argc={argc} exceeds MAX_ARITY={MAX_ARITY}",
                    self.proto_index
                );
            }
            return None;
        }
        let hp = builder.use_var(self.hp_var);
        let mut tags = builder.ins().iconst(types::I64, 0);
        for k in 0..argc {
            let r = callee_reg + 1 + k;
            if self.intlist_regs.contains_key(&r) || self.param_idx.contains_key(&r) {
                let bit = builder.ins().iconst(types::I64, 1i64 << k);
                tags = builder.ins().bor(tags, bit);
            }
        }
        let mut arg_vals: Vec<Value> = Vec::with_capacity(argc + 2);
        arg_vals.push(hp);
        arg_vals.push(tags);
        for k in 0..argc {
            let r = callee_reg + 1 + k;
            arg_vals.push(builder.use_var(self.rv_i64(r)));
        }
        let func_id = *self.decls.get(&proto_index)?;
        let callee_kind = self.ret_kinds.get(&proto_index).copied().unwrap_or(RET_INT);
        let func_ref = self.module.declare_func_in_func(func_id, builder.func);
        builder.ins().call(func_ref, &arg_vals);
        let pure_kind = callee_kind & (RET_INT | RET_FLOAT | RET_BOOL | RET_INTLIST);
        let val = self.emit_check_callee_result(builder, hp, callee_kind, CalleeBail::ReturnPair)?;
        builder.def_var(self.rv_i64(ins.a as usize), val);
        self.ret_slot[ins.a as usize] = None;
        self.class_ctor_regs.remove(&(ins.a as usize));
        self.instance_regs.remove(&(ins.a as usize));
        self.method_regs.remove(&(ins.a as usize));
        self.intlist_regs.remove(&(ins.a as usize));
        self.len_regs.remove(&(ins.a as usize));
        match pure_kind {
            k if k == RET_INT | RET_INTLIST => {
                self.intlist_regs.insert(ins.a as usize, INTLIST_UNION);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                Some(())
            }
            RET_INTLIST => {
                self.intlist_regs.insert(ins.a as usize, 7);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                Some(())
            }
            RET_INT => {
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                Some(())
            }
            RET_FLOAT => {
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = true;
                Some(())
            }
            RET_BOOL => {
                self.is_bool[ins.a as usize] = true;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                Some(())
            }
            _ => None,
        }
    }

    fn emit_class_make(
        &mut self,
        builder: &mut FunctionBuilder,
        ins: Ins,
        argc: usize,
        callee_reg: usize,
        desc_id: i64,
    ) -> Option<()> {
        if argc > 8 {
            return None;
        }
        for k in 0..argc {
            let r = callee_reg + 1 + k;
            if self.is_none[r] || self.is_float[r] || self.is_bool[r] {
                return None;
            }
            if self.int_dict_seen(&[r]) {
                return None;
            }
        }
        let func_id = *self.fid_class_make.get(argc)?;
        let func_ref = self.module.declare_func_in_func(func_id, builder.func);
        let mut arg_vals: Vec<Value> = Vec::with_capacity(argc + 1);
        arg_vals.push(builder.ins().iconst(types::I64, desc_id));
        for k in 0..argc {
            arg_vals.push(builder.use_var(self.rv_i64(callee_reg + 1 + k)));
        }
        let call = builder.ins().call(func_ref, &arg_vals);
        let v = builder.inst_results(call)[0];
        let zero_c = builder.ins().iconst(types::I64, 0);
        let is_miss = builder.ins().icmp(IntCC::Equal, v, zero_c);
        let bail_blk = builder.create_block();
        let cont_blk = builder.create_block();
        builder.ins().brif(is_miss, bail_blk, &[], cont_blk, &[]);
         self.terminated = true;
        builder.seal_block(bail_blk);
        builder.seal_block(cont_blk);
        self.goto(builder, bail_blk);
        self.emit_bail_exit(builder);
        self.goto(builder, cont_blk);
        builder.def_var(self.rv_i64(ins.a as usize), v);
        self.ret_slot[ins.a as usize] = None;
        self.int_dict_regs.remove(&(ins.a as usize));
        self.class_ctor_regs.remove(&(ins.a as usize));
        self.method_regs.remove(&(ins.a as usize));
        self.intlist_regs.remove(&(ins.a as usize));
        self.len_regs.remove(&(ins.a as usize));
        self.instance_regs.insert(ins.a as usize, desc_id);
        self.is_bool[ins.a as usize] = false;
        self.is_none[ins.a as usize] = false;
        self.is_float[ins.a as usize] = false;
        Some(())
    }

    fn emit_class_method_call(
        &mut self,
        builder: &mut FunctionBuilder,
        ins: Ins,
        argc: usize,
        proto_index: usize,
        recv_reg: usize,
    ) -> Option<()> {
        if argc > MAX_ARITY {
            return None;
        }
        if !self.instance_regs.contains_key(&recv_reg) {
            return None;
        }
        for k in 0..argc {
            let r = ins.b as usize + 1 + k;
            if self.is_none[r] || self.is_float[r] || self.is_bool[r] {
                return None;
            }
            if self.int_dict_seen(&[r]) {
                return None;
            }
        }
        let func_id = *self.decls.get(&proto_index)?;
        let callee_kind = self.ret_kinds.get(&proto_index).copied().unwrap_or(RET_INT);
        let func_ref = self.module.declare_func_in_func(func_id, builder.func);
        let hp = builder.use_var(self.hp_var);
        let recv = builder.use_var(self.rv_i64(recv_reg));
        let mut tags = builder.ins().iconst(types::I64, 0);
        for k in 0..argc {
            let r = ins.b as usize + 1 + k;
            if self.intlist_regs.contains_key(&r) || self.param_idx.contains_key(&r) {
                // Class method: slot 0 is the receiver, so user arg `k` shifts up.
                let bit = builder.ins().iconst(
                    types::I64,
                    1i64 << (k + PARAM_MASK_UNSAFE_SHIFT),
                );
                tags = builder.ins().bor(tags, bit);
            }
        }
        let mut arg_vals: Vec<Value> = Vec::with_capacity(argc + 3);
        arg_vals.push(hp);
        arg_vals.push(tags);
        arg_vals.push(recv);
        for k in 0..argc {
            arg_vals.push(builder.use_var(self.rv_i64(ins.b as usize + 1 + k)));
        }
        builder.ins().call(func_ref, &arg_vals);
        let pure_kind = callee_kind & (RET_INT | RET_FLOAT | RET_BOOL | RET_INTLIST);
        let val = self.emit_check_callee_result(builder, hp, callee_kind, CalleeBail::BailExit)?;
        builder.def_var(self.rv_i64(ins.a as usize), val);
        self.ret_slot[ins.a as usize] = None;
        self.int_dict_regs.remove(&(ins.a as usize));
        self.class_ctor_regs.remove(&(ins.a as usize));
        self.instance_regs.remove(&(ins.a as usize));
        self.method_regs.remove(&(ins.a as usize));
        self.intlist_regs.remove(&(ins.a as usize));
        self.len_regs.remove(&(ins.a as usize));
        match pure_kind {
            k if k == RET_INT | RET_INTLIST => {
                self.intlist_regs.insert(ins.a as usize, INTLIST_UNION);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                Some(())
            }
            RET_INTLIST => {
                self.intlist_regs.insert(ins.a as usize, 7);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                Some(())
            }
            RET_INT => {
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                Some(())
            }
            RET_FLOAT => {
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = true;
                Some(())
            }
            RET_BOOL => {
                self.is_bool[ins.a as usize] = true;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                Some(())
            }
            _ => None,
        }
    }

    fn emit_intlist_target(
        &mut self,
        builder: &mut FunctionBuilder,
        target_reg: usize,
    ) -> Option<Value> {
        if self.intlist_regs.contains_key(&target_reg)
            && self.intlist_regs.get(&target_reg).copied() != Some(INTLIST_UNION)
        {
            return Some(builder.use_var(self.rv_i64(target_reg)));
        }
        let fid = self.fid_list_is_handle;
        let f_ref = self.module.declare_func_in_func(fid, builder.func);
        let v = builder.use_var(self.rv_i64(target_reg));
        let call = builder.ins().call(f_ref, &[v]);
        let ok = builder.inst_results(call)[0];
        let zero_v = builder.ins().iconst(types::I64, 0);
        let is_ok = builder.ins().icmp(IntCC::NotEqual, ok, zero_v);
        let cont_blk = builder.create_block();
        let bail_blk = builder.create_block();
        builder.ins().brif(is_ok, cont_blk, &[], bail_blk, &[]);
         self.terminated = true;
        builder.seal_block(bail_blk);
        builder.seal_block(cont_blk);
        self.goto(builder, bail_blk);
        self.emit_bail_exit(builder);
        self.goto(builder, cont_blk);
        let borrowed = self.param_regs.contains(&target_reg);
        self.intlist_regs.insert(target_reg, if borrowed { INTLIST_BORROWED } else { 7 });
        Some(v)
    }

    fn emit_int_cleanup(&mut self, builder: &mut FunctionBuilder<'_>, keep_all_intlist: bool) {
        let dict_slots: Vec<usize> = self
            .local_is_intdict
            .iter()
            .enumerate()
            .filter_map(|(i, &live)| live.then_some(i))
            .collect();
        let dict_regs: Vec<usize> = self.int_dict_regs.iter().copied().collect();
        if !dict_slots.is_empty() || !dict_regs.is_empty() {
            let fid = self.fid_dict_free;
            let f_free = self.module.declare_func_in_func(fid, builder.func);
            for slot in dict_slots {
                let id = builder.use_var(self.lv_i64(slot));
                builder.ins().call(f_free, &[id]);
            }
            for reg in dict_regs {
                let id = builder.use_var(self.rv_i64(reg));
                builder.ins().call(f_free, &[id]);
            }
        }
        let iter_regs: Vec<usize> = self.intlist_iter_regs.iter().copied().collect();
        if !iter_regs.is_empty() {
            let fid = self.fid_list_iter_free;
            let f_free = self.module.declare_func_in_func(fid, builder.func);
            for reg in iter_regs {
                let id = builder.use_var(self.rv_i64(reg));
                builder.ins().call(f_free, &[id]);
            }
        }
        if keep_all_intlist {
            return;
        }
        let list_slots: Vec<usize> = self
            .local_is_list
            .iter()
            .enumerate()
            .filter_map(|(i, &live)| (live != 0 && live != INTLIST_BORROWED).then_some(i))
            .collect();
        let list_regs: Vec<usize> = self
            .intlist_regs
            .iter()
            .filter(|&(_, &kind)| kind != INTLIST_BORROWED)
            .map(|(&reg, _)| reg)
            .collect();
        if !list_slots.is_empty() || !list_regs.is_empty() {
            let fid = self.fid_list_free;
            let f_free = self.module.declare_func_in_func(fid, builder.func);
            for slot in list_slots {
                let id = builder.use_var(self.lv_i64(slot));
                builder.ins().call(f_free, &[id]);
            }
            for reg in list_regs {
                let id = builder.use_var(self.rv_i64(reg));
                builder.ins().call(f_free, &[id]);
            }
        }
    }

    fn emit_int_dict_cleanup(&mut self, builder: &mut FunctionBuilder<'_>) {
        self.emit_int_cleanup(builder, false);
    }

    fn finish_math_call(&mut self, ins: Ins, callee_reg: usize, dst_is_float: bool) {
        let dst = ins.a as usize;
        self.is_bool[dst] = false;
        self.is_none[dst] = false;
        self.is_float[dst] = dst_is_float;
        self.ret_slot[dst] = None;
        self.mathfn_regs.remove(&callee_reg);
        self.mathfn_regs.remove(&dst);
    }

    fn emit_f64_arg(
        &self,
        builder: &mut FunctionBuilder<'_>,
        idx: usize,
    ) -> cranelift_codegen::ir::Value {
        if self.is_float[idx] {
            builder.use_var(self.rv_f64(idx))
        } else {
            let i = builder.use_var(self.rv_i64(idx));
            builder.ins().fcvt_from_sint(types::F64, i)
        }
    }

    fn emit_math_domain_bail(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        x: cranelift_codegen::ir::Value,
        less_equal_zero: bool,
    ) {
        let zero_f64 = builder.ins().f64const(0.0);
        let bad = builder.ins().fcmp(
            if less_equal_zero {
                cranelift_codegen::ir::condcodes::FloatCC::LessThanOrEqual
            } else {
                cranelift_codegen::ir::condcodes::FloatCC::LessThan
            },
            x,
            zero_f64,
        );
        let bail_blk = builder.create_block();
        let cont_blk = builder.create_block();
        builder.ins().brif(bad, bail_blk, &[], cont_blk, &[]);
        self.terminated = true;
        builder.seal_block(bail_blk);
        builder.seal_block(cont_blk);
        self.goto(builder, bail_blk);
        self.emit_bail_exit(builder);
        self.goto(builder, cont_blk);
    }

    fn emit_math_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ins: Ins,
        argc: usize,
        callee_reg: usize,
        mfn: MathFn,
    ) -> Option<()> {
        let dst = ins.a as usize;
        let argc_ok = match mfn {
            MathFn::Log => argc == 1 || argc == 2,
            MathFn::Pow | MathFn::Hypot => argc == 2,
            _ => argc == 1,
        };
        if !argc_ok {
            return None;
        }
        let a0 = callee_reg + 1;
        if self.is_bool[a0] || self.is_none[a0] {
            return None;
        }
        match mfn {
            MathFn::Floor | MathFn::Ceil => {
                if !self.is_float[a0] {
                    let v = builder.use_var(self.rv_i64(a0));
                    builder.def_var(self.rv_i64(dst), v);
                    self.finish_math_call(ins, callee_reg, false);
                    return Some(());
                }
                let x = builder.use_var(self.rv_f64(a0));
                let fid = self.fid_math[&(if mfn == MathFn::Floor {
                    MathFfi::Floor
                } else {
                    MathFfi::Ceil
                })];
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[x]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_f64(dst), res);
                self.finish_math_call(ins, callee_reg, true);
                Some(())
            }
            MathFn::Trunc => {
                if !self.is_float[a0] {
                    let v = builder.use_var(self.rv_i64(a0));
                    builder.def_var(self.rv_i64(dst), v);
                    self.finish_math_call(ins, callee_reg, false);
                    return Some(());
                }
                let x = builder.use_var(self.rv_f64(a0));
                let fid = self.fid_math[&MathFfi::Trunc];
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[x]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_i64(dst), res);
                self.finish_math_call(ins, callee_reg, false);
                Some(())
            }
            MathFn::Sign => {
                let x = self.emit_f64_arg(builder, a0);
                let fid = self.fid_math[&MathFfi::Sign];
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[x]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_i64(dst), res);
                self.finish_math_call(ins, callee_reg, false);
                Some(())
            }
            MathFn::Pow => {
                let a1 = callee_reg + 2;
                if self.is_bool[a1] || self.is_none[a1] {
                    return None;
                }
                if !self.is_float[a0] && !self.is_float[a1] {
                    return None;
                }
                let x = self.emit_f64_arg(builder, a0);
                let e = self.emit_f64_arg(builder, a1);
                let fid = self.fid_math[&MathFfi::Powf];
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[x, e]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_f64(dst), res);
                self.finish_math_call(ins, callee_reg, true);
                Some(())
            }
            MathFn::Hypot => {
                let a1 = callee_reg + 2;
                if self.is_bool[a1] || self.is_none[a1] {
                    return None;
                }
                let x = self.emit_f64_arg(builder, a0);
                let y = self.emit_f64_arg(builder, a1);
                let fid = self.fid_math[&MathFfi::Hypot];
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[x, y]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_f64(dst), res);
                self.finish_math_call(ins, callee_reg, true);
                Some(())
            }
            MathFn::Abs => {
                if !self.is_float[a0] {
                    return None;
                }
                let x = builder.use_var(self.rv_f64(a0));
                let fid = self.fid_math[&MathFfi::Abs];
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[x]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_f64(dst), res);
                self.finish_math_call(ins, callee_reg, true);
                Some(())
            }
            MathFn::Sqrt => {
                let x = self.emit_f64_arg(builder, a0);
                self.emit_math_domain_bail(builder, x, false);
                let fid = self.fid_math[&MathFfi::Sqrt];
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[x]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_f64(dst), res);
                self.finish_math_call(ins, callee_reg, true);
                Some(())
            }
            MathFn::Log | MathFn::Log2 | MathFn::Log10 => {
                let x = self.emit_f64_arg(builder, a0);
                self.emit_math_domain_bail(builder, x, true);
                if mfn == MathFn::Log && argc == 2 {
                    let b = callee_reg + 2;
                    if self.is_bool[b] || self.is_none[b] {
                        return None;
                    }
                    let b_f = self.emit_f64_arg(builder, b);
                    let fid = self.fid_math[&MathFfi::LogB];
                    let f_ref = self.module.declare_func_in_func(fid, builder.func);
                    let call = builder.ins().call(f_ref, &[x, b_f]);
                    let res = builder.inst_results(call)[0];
                    builder.def_var(self.rv_f64(dst), res);
                    self.finish_math_call(ins, callee_reg, true);
                    return Some(());
                }
                let fid = self.fid_math[&(match mfn {
                    MathFn::Log => MathFfi::Ln,
                    MathFn::Log2 => MathFfi::Log2,
                    _ => MathFfi::Log10,
                })];
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[x]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_f64(dst), res);
                self.finish_math_call(ins, callee_reg, true);
                Some(())
            }
            _ => {
                let x = self.emit_f64_arg(builder, a0);
                let mffi = match mfn {
                    MathFn::Sin => MathFfi::Sin,
                    MathFn::Cos => MathFfi::Cos,
                    MathFn::Tan => MathFfi::Tan,
                    MathFn::Asin => MathFfi::Asin,
                    MathFn::Acos => MathFfi::Acos,
                    MathFn::Atan => MathFfi::Atan,
                    MathFn::Cbrt => MathFfi::Cbrt,
                    MathFn::Exp => MathFfi::Exp,
                    _ => return None,
                };
                let fid = self.fid_math[&mffi];
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[x]);
                let res = builder.inst_results(call)[0];
                builder.def_var(self.rv_f64(dst), res);
                self.finish_math_call(ins, callee_reg, true);
                Some(())
            }
        }
    }

    /// Validate a callee's hidden-pointer result against its static `RET_*`
    /// kind, branch to a bail path on mismatch, and return the loaded value.
    ///
    /// This is the single home for the `.tag` match + hidden-pointer loads +
    /// bail branch formerly duplicated at the user-call and class-method-call
    /// sites. `callee_kind` is still consumed by each caller for post-call
    /// register metadata; only the *validation* is shared here.
    fn emit_check_callee_result(
        &mut self,
        builder: &mut FunctionBuilder,
        hp: cranelift_codegen::ir::Value,
        callee_kind: u8,
        bail: CalleeBail,
    ) -> Option<cranelift_codegen::ir::Value> {
        let pure_kind = callee_kind & (RET_INT | RET_FLOAT | RET_BOOL | RET_INTLIST);
        let int_or_vec = pure_kind == RET_INT | RET_INTLIST;
        let Some(expected_tag) = crate::jit_cache::ret_kind_to_tag(pure_kind) else {
            if crate::jit_cache::jit_debug() {
                eprintln!("JITREJECT emit_user_call callee_kind={callee_kind:#x} (heterogeneous)");
            }
            return None;
        };
        let val = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlagsData::new(),
            hp,
            RESULT_VALUE_OFFSET,
        );
        let tag = builder.ins().load(
            types::I64,
            cranelift_codegen::ir::MemFlagsData::new(),
            hp,
            RESULT_TAG_OFFSET,
        );
        let is_expected = if int_or_vec {
            let int_tag_v = builder.ins().iconst(types::I64, crate::jit_cache::TAG_INT);
            let list_tag_v = builder.ins().iconst(types::I64, crate::jit_cache::TAG_INTLIST);
            let id_base_v = builder.ins().iconst(types::I64, crate::runtime::int_list::id_base());
            let is_int = builder.ins().icmp(IntCC::Equal, tag, int_tag_v);
            let is_list = builder.ins().icmp(IntCC::Equal, tag, list_tag_v);
            let vint = builder.ins().icmp(IntCC::SignedLessThan, val, id_base_v);
            let int_ok = builder.ins().band(is_int, vint);
            builder.ins().bor(int_ok, is_list)
        } else {
            let expected_v = builder.ins().iconst(types::I64, expected_tag);
            builder.ins().icmp(IntCC::Equal, tag, expected_v)
        };
        let bail_blk = builder.create_block();
        let cont_blk = builder.create_block();
        builder.ins().brif(is_expected, cont_blk, &[], bail_blk, &[]);
        self.terminated = true;
        builder.seal_block(bail_blk);
        builder.seal_block(cont_blk);
        self.goto(builder, bail_blk);
        match bail {
            CalleeBail::ReturnPair => {
                let zero = builder.ins().iconst(types::I64, 0);
                let bail_tag = builder.ins().iconst(types::I64, crate::jit_cache::TAG_BAIL);
                self.emit_return_pair(builder, zero, bail_tag);
            }
            CalleeBail::BailExit => self.emit_bail_exit(builder),
        }
        self.goto(builder, cont_blk);
        Some(val)
    }

    fn emit_bail_exit(&mut self, builder: &mut FunctionBuilder<'_>) {
        if self.needs_depth {
            let exit_ref = self.module.declare_func_in_func(self.fid_exit, builder.func);
            builder.ins().call(exit_ref, &[]);
        }
        self.emit_int_dict_cleanup(builder);
        let hp = builder.use_var(self.hp_var);
        let zero = builder.ins().iconst(types::I64, 0);
        let two = builder.ins().iconst(types::I64, crate::jit_cache::TAG_BAIL);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), zero, hp, RESULT_VALUE_OFFSET);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), two, hp, RESULT_TAG_OFFSET);
        builder.ins().return_(&[]);
        self.terminated = true;
    }

    fn emit_return_pair(
        &mut self,
        builder: &mut FunctionBuilder,
        bits: cranelift_codegen::ir::Value,
        tag: cranelift_codegen::ir::Value,
    ) {
        self.emit_return_pair_keep(builder, bits, tag, false)
    }

    fn emit_return_pair_keep(
        &mut self,
        builder: &mut FunctionBuilder,
        bits: cranelift_codegen::ir::Value,
        tag: cranelift_codegen::ir::Value,
        keep_all_intlist: bool,
    ) {
        if self.needs_depth {
            let exit_ref = self.module.declare_func_in_func(
                self.fid_exit,
                builder.func,
            );
            builder.ins().call(exit_ref, &[]);
        }
        self.emit_int_cleanup(builder, keep_all_intlist);
        let hp = builder.use_var(self.hp_var);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), bits, hp, RESULT_VALUE_OFFSET);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), tag, hp, RESULT_TAG_OFFSET);
        builder.ins().return_(&[]);
         self.terminated = true;
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

    fn emit_load_local(
        &mut self,
        builder: &mut FunctionBuilder,
        dst_reg: usize,
        slot: usize,
    ) -> Option<()> {
        if slot >= self.locals_len {
            return None;
        }
        if self.field_locals.contains(&slot) {
            return None;
        }
        self.clear_specialized(dst_reg);
        if self.local_borrow_raw[slot] {
            self.param_regs.insert(dst_reg);
        } else {
            self.param_regs.remove(&dst_reg);
        }
        match self.local_param_pos[slot] {
            Some(p) => {
                self.param_idx.insert(dst_reg, p);
            }
            None => {
                self.param_idx.remove(&dst_reg);
            }
        }
        let float_slot = self.local_float[slot];
        if float_slot {
            let v = builder.use_var(self.lv_f64(slot));
            builder.def_var(self.rv_f64(dst_reg), v);
            self.is_bool[dst_reg] = false;
            self.is_none[dst_reg] = false;
            self.is_float[dst_reg] = true;
            self.ret_slot[dst_reg] = Some(slot);
        } else {
            let v = builder.use_var(self.lv_i64(slot));
            builder.def_var(self.rv_i64(dst_reg), v);
            self.is_bool[dst_reg] = self.local_is_bool[slot];
            self.is_none[dst_reg] = self.local_is_none[slot];
            self.is_float[dst_reg] = false;
            if self.local_is_intdict[slot] {
                self.int_dict_regs.insert(dst_reg);
            }
            if let Some(desc) = self.local_is_instance[slot] {
                self.instance_regs.insert(dst_reg, desc);
            }
            if self.local_is_list[slot] != 0 {
                self.intlist_regs.insert(dst_reg, self.local_is_list[slot]);
            }
            if self.is_class_method && slot == 0 {
                self.self_regs.insert(dst_reg);
            }
            self.ret_slot[dst_reg] = Some(slot);
        }
        Some(())
    }

    fn emit_stream(&mut self, builder: &mut FunctionBuilder) -> Option<()> {
        let code = &self.plan.code;
        let mut ip = 0usize;
        self.goto(builder, self.blocks[&0]);
        while ip < code.len() {
            let ins = code[ip];

            match self.blocks.get(&ip).copied() {
                Some(blk) => {
                    self.goto(builder, blk);
                }
                None => {
                    if self.terminated {
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
                ip = patch.body_start_ip;
                continue;
            }

            if self.emit_ins(builder, code, ip, ins).is_none() {
                if crate::jit_cache::jit_debug() {
                    eprintln!(
                        "JITREJECT emit_ins ip={ip} op={:?} (see the JITREJECT reason line above, if any)",
                        ins.op
                    );
                }
                return None;
            }
            ip += 1;
        }
        if !self.terminated {
            if self.needs_depth {
                let exit_ref = self.module.declare_func_in_func(self.fid_exit, builder.func);
                builder.ins().call(exit_ref, &[]);
            }
            self.emit_int_dict_cleanup(builder);
            let hp = builder.use_var(self.hp_var);
            let zero = builder.ins().iconst(types::I64, 0);
            let two = builder.ins().iconst(types::I64, crate::jit_cache::TAG_BAIL);
            builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), zero, hp, RESULT_VALUE_OFFSET);
            builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), two, hp, RESULT_TAG_OFFSET);
            builder.ins().return_(&[]);
            self.terminated = true;
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

        let store = code.get(patch.fornext_ip + 1)?;
        if store.op != Op::StoreLocal || store.a != patch.yield_reg {
            return None;
        }
        let i_slot = store.d as usize;
        if i_slot >= self.locals_len {
            return None;
        }
        if self.local_float[i_slot] {
            return None;
        }
        let i_var = self.lv_i64(i_slot);

        // The loop index is always an Int. The local slot may have held a
        // different value type earlier in this proto (register reuse), so
        // clear every tracked specialization before the loop body reads it.
        self.local_is_intdict[i_slot] = false;
        self.local_is_instance[i_slot] = None;
        self.local_is_list[i_slot] = 0;
        self.local_is_bool[i_slot] = false;
        self.local_is_none[i_slot] = false;
        self.local_borrow_raw[i_slot] = false;
        self.local_param_pos[i_slot] = None;

        let (inc_blk, _inc_i_var) = self.loop_inc.get(&patch.fornext_ip).copied()?;
        let header_blk = self.blocks.get(&patch.fornext_ip).copied()?;
        let body_blk = self.blocks.get(&patch.body_start_ip).copied()?;
        let exit_blk = self.blocks.get(&patch.exit_ip).copied()?;

        let _param0 = builder.append_block_param(header_blk, types::I64);
        let _param1 = builder.append_block_param(body_blk, types::I64);
        let _param2 = builder.append_block_param(inc_blk, types::I64);

        builder.def_var(i_var, min_v);
        let i0 = builder.use_var(i_var);
        builder.ins().jump(header_blk, &[BlockArg::Value(i0)]);
         self.terminated = true;

        self.goto(builder, header_blk);
        let i_v = builder.block_params(header_blk)[0];
        builder.def_var(i_var, i_v);
        let cmp = builder.ins().icmp(IntCC::SignedLessThan, i_v, stop_v);
        builder.ins().brif(cmp, body_blk, &[BlockArg::Value(i_v)], exit_blk, &[]);
         self.terminated = true;

        self.goto(builder, inc_blk);
        let i_v = builder.block_params(inc_blk)[0];
        builder.def_var(i_var, i_v);
        let one = builder.ins().iconst(types::I64, 1);
        let inc = builder.ins().iadd(i_v, one);
        builder.ins().jump(header_blk, &[BlockArg::Value(inc)]);
         self.terminated = true;

        self.goto(builder, body_blk);
        let i_v = builder.block_params(body_blk)[0];
        builder.def_var(i_var, i_v);
        let yield_reg = patch.yield_reg as usize;
        let yield_var = self.rv_i64(yield_reg);
        builder.def_var(yield_var, i_v);
        self.is_float[yield_reg] = false;
        self.is_bool[yield_reg] = false;
        self.is_none[yield_reg] = false;
        // The yield register may have held a dict (or other object) earlier;
        // after ForNext it is a plain Int and the following StoreLocal reads
        // these sets to infer the local's type.
        self.int_dict_regs.remove(&yield_reg);
        self.instance_regs.remove(&yield_reg);
        self.method_regs.remove(&yield_reg);
        self.intlist_regs.remove(&yield_reg);
        self.len_regs.remove(&yield_reg);
        self.math_regs.remove(&yield_reg);
        self.class_ctor_regs.remove(&yield_reg);

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
                    if slot >= self.locals_len || self.field_locals.contains(&slot) {
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
        let is_load_local = ins.op == Op::LoadLocal;
        let preserve = matches!(
            ins.op,
            Op::LoadLocal
                | Op::Dup
                | Op::Unwrap
                | Op::LoadName
                | Op::GetAttr
                | Op::Call
                | Op::BuildList
                | Op::BuildDict
                | Op::Slice
                | Op::PrepIter
                | Op::ArgBegin
                | Op::ArgPos
                | Op::ArgKw
        );
        let r = self.emit_ins_body(builder, code, ip, ins);
        if !is_load_local {
            self.self_regs.remove(&(ins.a as usize));
        }
        if !preserve {
            self.int_dict_regs.remove(&(ins.a as usize));
            self.call_candidate_regs.remove(&(ins.a as usize));
            self.class_ctor_regs.remove(&(ins.a as usize));
            self.instance_regs.remove(&(ins.a as usize));
            self.method_regs.remove(&(ins.a as usize));
            self.intlist_regs.remove(&(ins.a as usize));
            self.param_idx.remove(&(ins.a as usize));
            self.len_regs.remove(&(ins.a as usize));
            self.math_regs.remove(&(ins.a as usize));
            self.mathfn_regs.remove(&(ins.a as usize));
            self.list_add_regs.remove(&(ins.a as usize));
            self.intlist_iter_regs.remove(&(ins.a as usize));
            self.param_regs.remove(&(ins.a as usize));
        }
        r
    }

    fn emit_ins_body(
        &mut self,
        builder: &mut FunctionBuilder,
        code: &[Ins],
        ip: usize,
        ins: Ins,
    ) -> Option<()> {
        match ins.op {
            Op::LoadConst => {
                self.clear_specialized(ins.a as usize);
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
                            _ => {
                                return self.reject("LoadConst: constant kind is not Int/Float/Bool")
                            }
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
                self.clear_specialized(ins.a as usize);
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
                let v = builder.use_var(self.rv(ins.a as usize));
                builder.def_var(self.rv(ins.a as usize), v);
                Some(())
            }
            Op::LoadName => {
                let name = self.arena.get(ins.d as usize)?.clone();
                if let Some(&slot) = self.local_name_to_slot.get(&name) {
                    return self.emit_load_local(builder, ins.a as usize, slot);
                }
                self.call_candidate_regs.remove(&(ins.a as usize));
                self.class_ctor_regs.remove(&(ins.a as usize));
                self.instance_regs.remove(&(ins.a as usize));
                self.method_regs.remove(&(ins.a as usize));
                self.intlist_regs.remove(&(ins.a as usize));
                self.len_regs.remove(&(ins.a as usize));
                self.list_add_regs.remove(&(ins.a as usize));
                self.param_regs.remove(&(ins.a as usize));
                if name == "len" {
                    self.len_regs.insert(ins.a as usize);
                    let v = builder.ins().iconst(types::I64, 0);
                    builder.def_var(self.rv_i64(ins.a as usize), v);
                    self.is_bool[ins.a as usize] = false;
                    self.is_none[ins.a as usize] = false;
                    self.is_float[ins.a as usize] = false;
                    self.ret_slot[ins.a as usize] = None;
                    return Some(());
                }
                match self.param_pos.get(&name) {
                    Some(&pos) => {
                        let v = builder.use_var(self.vars[pos]);
                        self.param_regs.insert(ins.a as usize);
                        self.param_idx.insert(ins.a as usize, pos);
                        builder.def_var(self.rv_i64(ins.a as usize), v);
                        self.is_bool[ins.a as usize] = false;
                        self.is_none[ins.a as usize] = false;
                        self.is_float[ins.a as usize] = false;
                        self.ret_slot[ins.a as usize] = None;
                        Some(())
                    }
                    None => {
                        if name == "math" {
                            self.math_regs.insert(ins.a as usize);
                            let v = builder.ins().iconst(types::I64, 0);
                            builder.def_var(self.rv_i64(ins.a as usize), v);
                            self.is_bool[ins.a as usize] = false;
                            self.is_none[ins.a as usize] = false;
                            self.is_float[ins.a as usize] = false;
                            self.ret_slot[ins.a as usize] = None;
                            return Some(());
                        }
                        if let Some(&desc_id) = self.class_desc_ids.get(&name) {
                            self.class_ctor_regs.insert(ins.a as usize, desc_id);
                            let v = builder.ins().iconst(types::I64, desc_id);
                            builder.def_var(self.rv_i64(ins.a as usize), v);
                            self.is_bool[ins.a as usize] = false;
                            self.is_none[ins.a as usize] = false;
                            self.is_float[ins.a as usize] = false;
                            self.ret_slot[ins.a as usize] = None;
                            return Some(());
                        }
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
                        if crate::jit_cache::jit_debug() {
                            eprintln!(
                                "JITREJECT proto[{}] LoadName {name:?} is not a param/local/intrinsic/JIT callee",
                                self.proto_index
                            );
                        }
                        None
                    }
                }
            }
            Op::Dup => {
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
                } else {
                    self.int_dict_regs.remove(&(ins.a as usize));
                }
                if self.len_regs.contains(&(ins.b as usize)) {
                    self.len_regs.insert(ins.a as usize);
                } else {
                    self.len_regs.remove(&(ins.a as usize));
                }
                if self.math_regs.contains(&(ins.b as usize)) {
                    self.math_regs.insert(ins.a as usize);
                } else {
                    self.math_regs.remove(&(ins.a as usize));
                }
                if let Some(&k) = self.mathfn_regs.get(&(ins.b as usize)) {
                    self.mathfn_regs.insert(ins.a as usize, k);
                } else {
                    self.mathfn_regs.remove(&(ins.a as usize));
                }
                if let Some(&k) = self.intlist_regs.get(&(ins.b as usize)) {
                    self.intlist_regs.insert(ins.a as usize, k);
                } else {
                    self.intlist_regs.remove(&(ins.a as usize));
                }
                if self.param_regs.contains(&(ins.b as usize)) {
                    self.param_regs.insert(ins.a as usize);
                } else {
                    self.param_regs.remove(&(ins.a as usize));
                }
                if let Some(&p) = self.param_idx.get(&(ins.b as usize)) {
                    self.param_idx.insert(ins.a as usize, p);
                } else {
                    self.param_idx.remove(&(ins.a as usize));
                }
                if let Some(&desc) = self.instance_regs.get(&(ins.b as usize)) {
                    self.instance_regs.insert(ins.a as usize, desc);
                } else {
                    self.instance_regs.remove(&(ins.a as usize));
                }
                if let Some(&desc) = self.class_ctor_regs.get(&(ins.b as usize)) {
                    self.class_ctor_regs.insert(ins.a as usize, desc);
                } else {
                    self.class_ctor_regs.remove(&(ins.a as usize));
                }
                self.method_regs.remove(&(ins.a as usize));
                self.ret_slot[ins.a as usize] = self.ret_slot[ins.b as usize];
                Some(())
            }
            Op::Pop | Op::PopScope | Op::NewScope => {
                Some(())
            }
            Op::StoreLocal => {
                let slot = ins.d as usize;
                if slot >= self.locals_len || self.field_locals.contains(&slot) {
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
                self.local_is_intdict[slot] = self.int_dict_regs.contains(&(ins.a as usize));
                self.local_borrow_raw[slot] = self.param_regs.contains(&(ins.a as usize));
                self.local_is_instance[slot] = self.instance_regs.get(&(ins.a as usize)).copied();
                self.local_is_list[slot] = self.intlist_regs.get(&(ins.a as usize)).copied().unwrap_or(0);
                self.local_param_pos[slot] = self.param_idx.get(&(ins.a as usize)).copied();
                self.local_is_bool[slot] = self.is_bool[ins.a as usize];
                self.local_is_none[slot] = self.is_none[ins.a as usize];
                Some(())
            }
            Op::GetAttr => {
                if self.math_regs.contains(&(ins.b as usize)) {
                    let field_name = self.arena.get(ins.d as usize).cloned();
                    match field_name.as_deref().and_then(MathFn::from_name) {
                        Some(mfn) => {
                            self.mathfn_regs.insert(ins.a as usize, mfn);
                            let v = builder.ins().iconst(types::I64, 0);
                            builder.def_var(self.rv_i64(ins.a as usize), v);
                            self.is_bool[ins.a as usize] = false;
                            self.is_none[ins.a as usize] = false;
                            self.is_float[ins.a as usize] = false;
                            self.ret_slot[ins.a as usize] = None;
                            return Some(());
                        }
                        None => return None,
                    }
                }
                let inst_recv = self.instance_regs.get(&(ins.b as usize)).copied();
                if self.intlist_regs.contains_key(&(ins.b as usize)) {
                    let Some(method_name) = self.arena.get(ins.d as usize).cloned() else {
                        return None;
                    };
                    if method_name == "add" {
                        let recv = ins.b as usize;
                        self.intlist_regs.remove(&(ins.a as usize));
                        self.list_add_regs.insert(ins.a as usize, recv);
                        self.class_ctor_regs.remove(&(ins.a as usize));
                        self.instance_regs.remove(&(ins.a as usize));
                        self.method_regs.remove(&(ins.a as usize));
                        self.len_regs.remove(&(ins.a as usize));
                        self.param_regs.remove(&(ins.a as usize));
                        let ptr = builder.use_var(self.rv_i64(ins.b as usize));
                        builder.def_var(self.rv_i64(ins.a as usize), ptr);
                        self.is_bool[ins.a as usize] = false;
                        self.is_none[ins.a as usize] = false;
                        self.is_float[ins.a as usize] = false;
                        self.ret_slot[ins.a as usize] = None;
                        return Some(());
                    }
                    return None;
                }
                self.class_ctor_regs.remove(&(ins.a as usize));
                self.instance_regs.remove(&(ins.a as usize));
                self.method_regs.remove(&(ins.a as usize));
                self.param_regs.remove(&(ins.a as usize));
                if let Some(&desc_id) = inst_recv.as_ref() {
                    let Some(method_name) = self.arena.get(ins.d as usize).cloned() else {
                        return None;
                    };
                    if let Some(&proto_idx) = self
                        .class_methods
                        .get(&desc_id)
                        .and_then(|m| m.get(&method_name))
                    {
                        let ptr = builder.use_var(self.rv_i64(ins.b as usize));
                        builder.def_var(self.rv_i64(ins.a as usize), ptr);
                        self.instance_regs.insert(ins.a as usize, desc_id);
                        self.method_regs.insert(ins.a as usize, (desc_id, proto_idx, ins.a as usize));
                        self.is_bool[ins.a as usize] = false;
                        self.is_none[ins.a as usize] = false;
                        self.is_float[ins.a as usize] = false;
                        self.ret_slot[ins.a as usize] = None;
                        return Some(());
                    }
                    return None;
                }
                let Some(layout) = self.class_layouts.get(&self.proto_index) else {
                    if crate::jit_cache::jit_debug() {
                        eprintln!("JITREJECT getattr proto={} no_layout", self.proto_index);
                    }
                    return None;
                };
                if !self.self_regs.contains(&(ins.b as usize)) {
                    if crate::jit_cache::jit_debug() {
                        eprintln!("JITREJECT getattr ins.b={} not_self_regs proto={}", ins.b, self.proto_index);
                    }
                    return None;
                }
                let Some(field_ref) = self.arena.get(ins.d as usize) else {
                    if crate::jit_cache::jit_debug() {
                        eprintln!("JITREJECT getattr d={} arlen={}", ins.d, self.arena.len());
                    }
                    return None;
                };
                let Some(field_ix) = layout.iter().position(|f| f == field_ref) else {
                    if crate::jit_cache::jit_debug() {
                        eprintln!("JITREJECT getattr not_in_layout ref={}", field_ref);
                    }
                    return None;
                };
                let f_ok = self.module.declare_func_in_func(self.fid_int_ok_name, builder.func);
                let self_ptr = builder.use_var(self.rv_i64(ins.b as usize));
                let e_idx = builder.ins().iconst(types::I64, field_ix as i64);
                let leaked: &'static str = Box::leak(field_ref.clone().into_boxed_str());
                let name_ptr = builder.ins().iconst(types::I64, leaked.as_ptr() as i64);
                let name_len = builder.ins().iconst(types::I64, leaked.len() as i64);
                let ok_call = builder.ins().call(f_ok, &[self_ptr, e_idx, name_ptr, name_len]);
                let ok = builder.inst_results(ok_call)[0];
                let zero_i = builder.ins().iconst(types::I64, 0);
                let is_miss = builder.ins().icmp(IntCC::Equal, ok, zero_i);
                let bail_attr = builder.create_block();
                let cont_attr = builder.create_block();
                builder.ins().brif(is_miss, bail_attr, &[], cont_attr, &[]);
                 self.terminated = true;
                builder.seal_block(bail_attr);
                builder.seal_block(cont_attr);
                self.goto(builder, bail_attr);
                self.emit_bail_exit(builder);
                self.goto(builder, cont_attr);
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
                let layout = match self.class_layouts.get(&self.proto_index) {
                    Some(l) => l.clone(),
                    None => {
                        if crate::jit_cache::jit_debug() {
                            eprintln!("JITREJECT setattronatlayout proto_index={} layout_miss=", self.proto_index);
                        }
                        return None;
                    }
                };
                if !self.self_regs.contains(&(ins.b as usize)) {
                    if crate::jit_cache::jit_debug() {
                        eprintln!("JITREJECT setattronatslot ins.b={} pix={}", ins.b, self.proto_index);
                    }
                    return None;
                }
                let Some(field_ref) = self.arena.get(ins.d as usize) else {
                    return None;
                };
                let Some(field_ix) = layout.iter().position(|f| f == field_ref) else {
                    return None;
                };
                if self.is_float[ins.c as usize] || self.is_bool[ins.c as usize] || self.is_none[ins.c as usize] {
                    return None;
                }
                if self.int_dict_seen(&[ins.c as usize]) {
                    return None;
                }
                let f_ok = self.module.declare_func_in_func(self.fid_int_set_name, builder.func);
                let leaked: &'static str = Box::leak(field_ref.clone().into_boxed_str());
                let self_ptr = builder.use_var(self.rv_i64(ins.b as usize));
                let name_ptr = builder.ins().iconst(types::I64, leaked.as_ptr() as i64);
                let name_len = builder.ins().iconst(types::I64, leaked.len() as i64);
                let e_idx = builder.ins().iconst(types::I64, field_ix as i64);
                let val = builder.use_var(self.rv_i64(ins.c as usize));
                let ok_call = builder.ins().call(f_ok, &[self_ptr, e_idx, val, name_ptr, name_len]);
                let ok = builder.inst_results(ok_call)[0];
                let zero_i = builder.ins().iconst(types::I64, 0);
                let is_miss = builder.ins().icmp(IntCC::Equal, ok, zero_i);
                let bail_attr = builder.create_block();
                let cont_attr = builder.create_block();
                builder.ins().brif(is_miss, bail_attr, &[], cont_attr, &[]);
                 self.terminated = true;
                builder.seal_block(bail_attr);
                builder.seal_block(cont_attr);
                self.goto(builder, bail_attr);
                self.emit_bail_exit(builder);
                self.goto(builder, cont_attr);
                builder.def_var(self.rv_i64(ins.a as usize), val);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                self.int_dict_regs.remove(&(ins.a as usize));
                self.ret_slot[ins.a as usize] = None;
                Some(())
            }
            Op::LoadLocal => self.emit_load_local(builder, ins.a as usize, ins.d as usize),
            Op::Add | Op::Sub | Op::Mul => {
                if self.int_dict_seen(&[ins.b as usize, ins.c as usize]) {
                    if crate::jit_cache::jit_debug() {
                        eprintln!("JITREJECT arith ip={ip} b={} c={} b_dict={} c_dict={} b_iter={} c_iter={} b_len={} c_len={}",
                            ins.b, ins.c,
                            self.int_dict_regs.contains(&(ins.b as usize)), self.int_dict_regs.contains(&(ins.c as usize)),
                            self.intlist_iter_regs.contains(&(ins.b as usize)), self.intlist_iter_regs.contains(&(ins.c as usize)),
                            self.len_regs.contains(&(ins.b as usize)), self.len_regs.contains(&(ins.c as usize)));
                    }
                    return None;
                }
                // The result is a fresh numeric value; the destination may have
                // held an object earlier (register reuse).
                self.clear_specialized(ins.a as usize);
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
            Op::Band | Op::Bor | Op::Bxor => {
                if self.int_dict_seen(&[ins.b as usize, ins.c as usize]) {
                    return None;
                }
                if self.is_float[ins.b as usize]
                    || self.is_float[ins.c as usize]
                    || self.is_bool[ins.b as usize]
                    || self.is_bool[ins.c as usize]
                    || self.is_none[ins.b as usize]
                    || self.is_none[ins.c as usize]
                {
                    return None;
                }
                let lhs = builder.use_var(self.rv_i64(ins.b as usize));
                let rhs = builder.use_var(self.rv_i64(ins.c as usize));
                let v = match ins.op {
                    Op::Band => builder.ins().band(lhs, rhs),
                    Op::Bor => builder.ins().bor(lhs, rhs),
                    Op::Bxor => builder.ins().bxor(lhs, rhs),
                    _ => return None,
                };
                builder.def_var(self.rv_i64(ins.a as usize), v);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                self.ret_slot[ins.a as usize] = None;
                Some(())
            }
            Op::Call => {
                let argc = ins.d as usize;
                let callee_reg = ins.b as usize;
                let dst = ins.a as usize;
                if dst != callee_reg {
                    self.call_candidate_regs.remove(&dst);
                    self.class_ctor_regs.remove(&dst);
                    self.instance_regs.remove(&dst);
                    self.method_regs.remove(&dst);
                    self.intlist_regs.remove(&dst);
                    self.len_regs.remove(&dst);
                    self.math_regs.remove(&dst);
                    self.mathfn_regs.remove(&dst);
                    self.list_add_regs.remove(&dst);
                    self.intlist_iter_regs.remove(&dst);
                    self.param_regs.remove(&dst);
                }
                if let Some(&desc_id) = self.class_ctor_regs.get(&callee_reg) {
                    return self.emit_class_make(builder, ins, argc, callee_reg, desc_id);
                }
                if self.len_regs.contains(&callee_reg) {
                    if argc != 1 {
                        return None;
                    }
                    let arg_reg = callee_reg + 1;
                    if self.is_none[arg_reg] || self.is_float[arg_reg] {
                        return None;
                    }
                    let Some(d_i) = self.emit_intlist_target(builder, arg_reg) else {
                        return None;
                    };
                    let fid = self.fid_list_len;
                    let f_ref = self.module.declare_func_in_func(fid, builder.func);
                    let call = builder.ins().call(f_ref, &[d_i]);
                    let res = builder.inst_results(call)[0];
                    let one_v = builder.ins().iconst(types::I64, -1);
                    let is_dead = builder.ins().icmp(IntCC::Equal, res, one_v);
                    let bail_blk = builder.create_block();
                    let cont_blk = builder.create_block();
                    builder.ins().brif(is_dead, bail_blk, &[], cont_blk, &[]);
                    self.terminated = true;
                    builder.seal_block(bail_blk);
                    builder.seal_block(cont_blk);
                    self.goto(builder, bail_blk);
                    self.emit_bail_exit(builder);
                    self.goto(builder, cont_blk);
                    builder.def_var(self.rv_i64(ins.a as usize), res);
                    self.is_bool[ins.a as usize] = false;
                    self.is_none[ins.a as usize] = false;
                    self.is_float[ins.a as usize] = false;
                    self.ret_slot[ins.a as usize] = None;
                    self.len_regs.remove(&(ins.a as usize));
                    return Some(());
                }
                if let Some(&mfn) = self.mathfn_regs.get(&callee_reg) {
                    return self.emit_math_call(builder, ins, argc, callee_reg, mfn);
                }
                if let Some(&recv) = self.list_add_regs.get(&callee_reg) {
                    if argc != 1 {
                        return None;
                    }
                    let arg_reg = callee_reg + 1;
                    if self.is_float[arg_reg] || self.is_none[arg_reg] {
                        return None;
                    }
                    if self.int_list_foreign_seen(&[arg_reg]) {
                        return None;
                    }
                    let Some(d_i) = self.emit_intlist_target(builder, recv) else {
                        return None;
                    };
                    let is_bool = self.is_bool[arg_reg];
                    let fid = if self.intlist_regs.contains_key(&arg_reg) {
                        self.fid_list_push
                    } else {
                        self.fid_list_push_scalar
                    };
                    let f_ref = self.module.declare_func_in_func(fid, builder.func);
                    let v_i = builder.use_var(self.rv_i64(arg_reg));
                    let is_bool_v = builder.ins().iconst(types::I64, is_bool as i64);
                    let call = builder.ins().call(f_ref, &[d_i, v_i, is_bool_v]);
                    let res = builder.inst_results(call)[0];
                    builder.def_var(self.rv_i64(ins.a as usize), res);
                    self.is_bool[ins.a as usize] = false;
                    self.is_none[ins.a as usize] = false;
                    self.is_float[ins.a as usize] = false;
                    self.ret_slot[ins.a as usize] = None;
                    self.list_add_regs.remove(&(ins.a as usize));
                    return Some(());
                }
                if let Some(&(_, proto_index, recv_reg)) = self.method_regs.get(&callee_reg) {
                    return self.emit_class_method_call(builder, ins, argc, proto_index, recv_reg);
                }
                if let Some(&proto_index) = self.call_candidate_regs.get(&callee_reg) {
                    if self.decls.contains_key(&proto_index) {
                        return self.emit_user_call(builder, ins, argc, callee_reg, proto_index);
                    }
                }
                if crate::jit_cache::jit_debug() {
                    eprintln!(
                        "JITREJECT proto[{}] Call: callee is not a JIT intrinsic or a compiled JIT proto",
                        self.proto_index
                    );
                }
                None
            }
            Op::ToBool => {
                if self.int_dict_seen(&[ins.b as usize]) {
                    return None;
                }
                if self.is_float[ins.b as usize] {
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
            Op::Neg => {
                let a = ins.a as usize;
                let b = ins.b as usize;
                if self.is_bool[b] || self.is_none[b] {
                    return None;
                }
                if self.int_dict_seen(&[b])
                    || self.intlist_regs.contains_key(&b)
                    || self.intlist_iter_regs.contains(&b)
                    || self.instance_regs.contains_key(&b)
                    || self.method_regs.contains_key(&b)
                    || self.class_ctor_regs.contains_key(&b)
                {
                    return None;
                }
                if self.is_float[b] {
                    let v = builder.use_var(self.rv_f64(b));
                    let n = builder.ins().fneg(v);
                    builder.def_var(self.rv_f64(a), n);
                    self.is_bool[a] = false;
                    self.is_none[a] = false;
                    self.is_float[a] = true;
                    self.ret_slot[a] = None;
                    return Some(());
                }
                let v = builder.use_var(self.rv_i64(b));
                let n = builder.ins().ineg(v);
                builder.def_var(self.rv_i64(a), n);
                self.is_bool[a] = false;
                self.is_none[a] = false;
                self.is_float[a] = false;
                self.ret_slot[a] = None;
                Some(())
            }
            Op::ArgBegin => Some(()),            Op::ArgPos | Op::ArgKw => {
                Some(())
            }
            Op::Idiv | Op::Mod => {
                if self.int_dict_seen(&[ins.b as usize]) {
                    return None;
                }
                let divisor = match const_load_int_at(code, ip, ins.c as usize, self.proto, self.program) {
                    Some(n) if n != 0 => n,
                    _ => {
                        return self.reject("Idiv/Mod: divisor is not a non-zero Int constant")
                    }
                };
                if self.is_float[ins.b as usize] {
                    return self.reject("Idiv/Mod: dividend is a Float (Int only)");
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
                if self.int_dict_seen(&[ins.b as usize, ins.c as usize]) {
                    return None;
                }
                let b_float = self.is_float[ins.b as usize];
                let c_float = self.is_float[ins.c as usize];
                if b_float || c_float {
                    let lhs = if b_float {
                        builder.use_var(self.rv_f64(ins.b as usize))
                    } else {
                        let i = builder.use_var(self.rv_i64(ins.b as usize));
                        builder.ins().fcvt_from_sint(types::F64, i)
                    };
                    let rhs = if c_float {
                        builder.use_var(self.rv_f64(ins.c as usize))
                    } else {
                        let i = builder.use_var(self.rv_i64(ins.c as usize));
                        builder.ins().fcvt_from_sint(types::F64, i)
                    };
                    let cc = match ins.op {
                        Op::Lt => FloatCC::LessThan,
                        Op::Le => FloatCC::LessThanOrEqual,
                        Op::Gt => FloatCC::GreaterThan,
                        Op::Ge => FloatCC::GreaterThanOrEqual,
                        Op::Eq => FloatCC::Equal,
                        Op::Ne => FloatCC::NotEqual,
                        _ => return None,
                    };
                    let cmp = builder.ins().fcmp(cc, lhs, rhs);
                    let one = builder.ins().iconst(types::I64, 1);
                    let zero_v = builder.ins().iconst(types::I64, 0);
                    let v = builder.ins().select(cmp, one, zero_v);
                    builder.def_var(self.rv_i64(ins.a as usize), v);
                    self.is_bool[ins.a as usize] = true;
                    self.is_none[ins.a as usize] = false;
                    self.is_float[ins.a as usize] = false;
                    self.ret_slot[ins.a as usize] = None;
                    return Some(());
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
                    return self.reject("FusedIntMulAdd: local slot out of range");
                }
                if self.local_is_intdict[dst] || self.local_is_intdict[x] || self.local_is_intdict[y] {
                    return self.reject("FusedIntMulAdd: an operand is an IntDict handle");
                }
                if self.local_float[dst] || self.local_float[x] || self.local_float[y] {
                    return self.reject("FusedIntMulAdd: an operand is a Float local");
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
                if let Some(&(inc_blk, i_slot)) = self.loop_inc.get(&target) {
                    let i_v = builder.use_var(self.lv(i_slot));
                    builder.ins().jump(inc_blk, &[BlockArg::Value(i_v)]);
                     self.terminated = true;
                    return Some(());
                }
                let blk = match self.blocks.get(&target).copied() {
                    Some(b) => b,
                    None => return None,
                };
                builder.ins().jump(blk, &[]);
                 self.terminated = true;
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
                     self.terminated = true;
                } else {
                    builder.ins().brif(cond, else_blk, &[], then_blk, &[]);
                     self.terminated = true;
                }
                Some(())
            }
            Op::BuildDict => {
                let n = ins.d as usize;
                if n > 256 {
                    return None;
                }
                let a = ins.a as usize;
                for i in 0..2 * n {
                    let r = a + i;
                    if self.is_float[r] || self.is_bool[r] || self.is_none[r] {
                        return None;
                    }
                    if self.int_dict_regs.contains(&r) {
                        return None;
                    }
                }
                let fid_empty = self.fid_dict_empty;
                let f_empty = self.module.declare_func_in_func(fid_empty, builder.func);
                let empty_call = builder.ins().call(f_empty, &[]);
                let id = builder.inst_results(empty_call)[0];
                if n > 0 {
                    let fid_set = self.fid_dict_set;
                    let f_set = self.module.declare_func_in_func(fid_set, builder.func);
                    for i in 0..n {
                        let k = builder.use_var(self.rv_i64(a + 2 * i));
                        let v = builder.use_var(self.rv_i64(a + 2 * i + 1));
                        builder.ins().call(f_set, &[id, k, v]);
                    }
                }
                builder.def_var(self.rv_i64(a), id);
                self.int_dict_regs.insert(a);
                self.is_bool[a] = false;
                self.is_none[a] = false;
                self.is_float[a] = false;
                self.ret_slot[a] = None;
                Some(())
            }
            Op::SetItem => {
                let value = ins.a as usize;
                let target = ins.b as usize;
                let key = ins.c as usize;
                if self.int_dict_regs.contains(&target)
                    && !self.is_float[value]
                    && !self.is_bool[value]
                    && !self.is_none[value]
                    && !self.is_float[key]
                    && !self.is_bool[key]
                    && !self.is_none[key]
                    && !self.is_float[target]
                    && !self.is_bool[target]
                {
                    if self.int_dict_seen(&[value, key]) {
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
                    return Some(());
                }
                if let Some(d_i) = self.emit_intlist_target(builder, target) {
                    if self.is_float[value] || self.is_none[value] || self.is_float[key] || self.is_none[key] || self.is_bool[key] {
                        return None;
                    }
                    if self.int_list_foreign_seen(&[value]) || self.int_dict_seen(&[key]) {
                        return None;
                    }
                    let fid_len = self.fid_list_len;
                    let fid_set = if self.intlist_regs.contains_key(&value) {
                        self.fid_list_set
                    } else {
                        self.fid_list_set_scalar
                    };
                    let f_len = self.module.declare_func_in_func(fid_len, builder.func);
                    let f_set = self.module.declare_func_in_func(fid_set, builder.func);
                    let k_i = builder.use_var(self.rv_i64(key));
                    let v_i = builder.use_var(self.rv_i64(value));
                    let call_len = builder.ins().call(f_len, &[d_i]);
                    let len = builder.inst_results(call_len)[0];
                    let zero_i = builder.ins().iconst(types::I64, 0);
                    let is_neg = builder.ins().icmp(IntCC::SignedLessThan, k_i, zero_i);
                    let ge_len = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, k_i, len);
                    let bad = builder.ins().bor(is_neg, ge_len);
                    let bail_blk = builder.create_block();
                    let cont_blk = builder.create_block();
                    builder.ins().brif(bad, bail_blk, &[], cont_blk, &[]);
                     self.terminated = true;
                    builder.seal_block(bail_blk);
                    builder.seal_block(cont_blk);
                    self.goto(builder, bail_blk);
                    self.emit_bail_exit(builder);
                    self.goto(builder, cont_blk);
                    let is_bool = builder.ins().iconst(types::I64, self.is_bool[value] as i64);
                    let call = builder.ins().call(f_set, &[d_i, k_i, v_i, is_bool]);
                    let res = builder.inst_results(call)[0];
                    builder.def_var(self.rv_i64(value), res);
                    self.is_float[value] = false;
                    self.is_none[value] = false;
                    self.ret_slot[value] = None;
                    return Some(());
                }
                None
            }
            Op::BuildList => {
                if ins.d != 0 {
                    return self.reject(&format!(
                        "BuildList: only empty literal lists are JIT-supported (n={})",
                        ins.d
                    ));
                }
                self.intlist_regs.remove(&(ins.a as usize));
                let fid = self.fid_list_empty;
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[]);
                let handle = builder.inst_results(call)[0];
                builder.def_var(self.rv_i64(ins.a as usize), handle);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                self.ret_slot[ins.a as usize] = None;
                self.int_dict_regs.remove(&(ins.a as usize));
                self.intlist_regs.insert(ins.a as usize, 1);
                Some(())
            }
            Op::Slice => {
                let dst = ins.a as usize;
                let flags = ins.flags;
                let has_start = flags & 1 != 0;
                let has_end = flags & 2 != 0;
                let has_step = flags & 4 != 0;
                if has_step {
                    return None;
                }
                if let Some(d_i) = self.emit_intlist_target(builder, ins.b as usize) {
                    let mut reg = ins.b as usize + 1;
                    let read_bound = |r: usize, builder: &mut FunctionBuilder| -> Option<cranelift_codegen::ir::Value> {
                        if self.is_float[r] || self.is_none[r] || self.is_bool[r] {
                            return None;
                        }
                        if self.int_dict_seen(&[r]) {
                            return None;
                        }
                        Some(builder.use_var(self.rv_i64(r)))
                    };
                    let start = if has_start {
                        let r = reg;
                        reg += 1;
                        read_bound(r, builder)?
                    } else {
                        builder.ins().iconst(types::I64, 0)
                    };
                    let end = if has_end {
                        let r = reg;
                        let _ = reg;
                        read_bound(r, builder)?
                    } else {
                        builder.ins().iconst(types::I64, i64::MAX)
                    };
                    let fid = self.fid_list_slice;
                    let f_ref = self.module.declare_func_in_func(fid, builder.func);
                    let call = builder.ins().call(f_ref, &[d_i, start, end]);
                    let _ = call;
                    let handle = builder.inst_results(call)[0];
                    builder.def_var(self.rv_i64(dst), handle);
                    self.is_bool[dst] = false;
                    self.is_none[dst] = false;
                    self.is_float[dst] = false;
                    self.ret_slot[dst] = None;
                    self.int_dict_regs.remove(&(dst));
                    self.intlist_regs.insert(dst, 1);
                    return Some(());
                }
                None
            }
            Op::PrepIter => {
                let Some(d_i) = self.emit_intlist_target(builder, ins.a as usize) else {
                    return None;
                };
                let fid = self.fid_list_iter_new;
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[d_i]);
                let handle = builder.inst_results(call)[0];
                let zero_iv = builder.ins().iconst(types::I64, 0);
                let is_zero = builder.ins().icmp(IntCC::Equal, handle, zero_iv);
                let bail_blk = builder.create_block();
                let cont_blk = builder.create_block();
                builder.ins().brif(is_zero, bail_blk, &[], cont_blk, &[]);
                self.terminated = true;
                builder.seal_block(bail_blk);
                builder.seal_block(cont_blk);
                self.goto(builder, bail_blk);
                self.emit_bail_exit(builder);
                self.goto(builder, cont_blk);
                builder.def_var(self.rv_i64(ins.a as usize), handle);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                self.ret_slot[ins.a as usize] = None;
                self.intlist_iter_regs.insert(ins.a as usize);
                self.intlist_regs.remove(&(ins.a as usize));
                Some(())
            }
            Op::ForNext => {
                let iter = ins.b as usize;
                if !self.intlist_iter_regs.contains(&iter) {
                    return None;
                }
                let exit_ip = jump_target(ip, ins.d);
                let Some(&exit_blk) = self.blocks.get(&exit_ip) else {
                    return None;
                };
                let Some(&body_blk) = self.blocks.get(&(ip + 1)) else {
                    return None;
                };
                let fid_has = self.fid_list_iter_has;
                let fid_next = self.fid_list_iter_next;
                let f_has = self.module.declare_func_in_func(fid_has, builder.func);
                let f_next = self.module.declare_func_in_func(fid_next, builder.func);
                let id = builder.use_var(self.rv_i64(iter));
                let call_h = builder.ins().call(f_has, &[id]);
                let has = builder.inst_results(call_h)[0];
                let call_n = builder.ins().call(f_next, &[id]);
                let next = builder.inst_results(call_n)[0];
                builder.def_var(self.rv_i64(ins.a as usize), next);
                self.is_bool[ins.a as usize] = false;
                self.is_none[ins.a as usize] = false;
                self.is_float[ins.a as usize] = false;
                self.ret_slot[ins.a as usize] = None;
                self.intlist_regs.remove(&(ins.a as usize));
                let zero_i = builder.ins().iconst(types::I64, 0);
                let done = builder.ins().icmp(IntCC::Equal, has, zero_i);
                builder.ins().brif(done, exit_blk, &[], body_blk, &[]);
                 self.terminated = true;
                Some(())
            }
            Op::GetItem => {
                let dst = ins.a as usize;
                let target = ins.b as usize;
                let key = ins.c as usize;
                if self.int_dict_regs.contains(&target) && !self.is_float[key] && !self.is_bool[key] && !self.is_none[key] {
                    let fid_has = self.fid_dict_has;
                    let f_has = self.module.declare_func_in_func(fid_has, builder.func);
                    let d_i = builder.use_var(self.rv_i64(target));
                    let k_i = builder.use_var(self.rv_i64(key));
                    let call_has = builder.ins().call(f_has, &[d_i, k_i]);
                    let has = builder.inst_results(call_has)[0];
                    let zero_i = builder.ins().iconst(types::I64, 0);
                    let missing = builder.ins().icmp(IntCC::Equal, has, zero_i);
                    let bail_blk = builder.create_block();
                    let cont_blk = builder.create_block();
                    builder.ins().brif(missing, bail_blk, &[], cont_blk, &[]);
                    self.terminated = true;
                    builder.seal_block(bail_blk);
                    builder.seal_block(cont_blk);
                    self.goto(builder, bail_blk);
                    self.emit_bail_exit(builder);
                    self.goto(builder, cont_blk);
                    let fid = self.fid_dict_get;
                    let f_ref = self.module.declare_func_in_func(fid, builder.func);
                    let call = builder.ins().call(f_ref, &[d_i, k_i]);
                    let res = builder.inst_results(call)[0];
                    // The value read from an IntDict is an Int; the destination
                    // register may have held a dict earlier (register reuse).
                    self.clear_specialized(dst);
                    builder.def_var(self.rv_i64(dst), res);
                    self.is_bool[dst] = false;
                    self.is_none[dst] = false;
                    self.is_float[dst] = false;
                    self.ret_slot[dst] = None;
                    return Some(());
                }
                if let Some(d_i) = self.emit_intlist_target(builder, target) {
                    if self.is_float[key] || self.is_bool[key] || self.is_none[key] {
                        return None;
                    }
                    if self.int_dict_seen(&[key]) {
                        return None;
                    }
                    let fid_len = self.fid_list_len;
                    let fid_get = self.fid_list_get;
                    let f_len = self.module.declare_func_in_func(fid_len, builder.func);
                    let f_get = self.module.declare_func_in_func(fid_get, builder.func);
                    let k_i = builder.use_var(self.rv_i64(key));
                    let call_len = builder.ins().call(f_len, &[d_i]);
                    let len = builder.inst_results(call_len)[0];
                    let zero_i = builder.ins().iconst(types::I64, 0);
                    let is_neg = builder.ins().icmp(IntCC::SignedLessThan, k_i, zero_i);
                    let ge_len = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, k_i, len);
                    let bad = builder.ins().bor(is_neg, ge_len);
                    let bail_blk = builder.create_block();
                    let cont_blk = builder.create_block();
                    builder.ins().brif(bad, bail_blk, &[], cont_blk, &[]);
                    self.terminated = true;
                    builder.seal_block(bail_blk);
                    builder.seal_block(cont_blk);
                    self.goto(builder, bail_blk);
                    self.emit_bail_exit(builder);
                    self.goto(builder, cont_blk);
                    let call = builder.ins().call(f_get, &[d_i, k_i]);
                    let res = builder.inst_results(call)[0];
                    builder.def_var(self.rv_i64(dst), res);
                    let kind = self.intlist_regs.get(&target).copied().unwrap_or(0);
                    self.is_bool[dst] = kind == 2;
                    self.is_none[dst] = false;
                    self.is_float[dst] = false;
                    self.ret_slot[dst] = None;
                    self.int_dict_regs.remove(&dst);
                    return Some(());
                }
                None
            }
    Op::Return => {
                if self.instance_regs.contains_key(&(ins.a as usize))
                    || self.method_regs.contains_key(&(ins.a as usize))
                    || self.class_ctor_regs.contains_key(&(ins.a as usize))
                    || self.self_regs.contains(&(ins.a as usize))
                {
                    return None;
                }
                if self.is_none[ins.a as usize] {
                    let zero = builder.ins().iconst(types::I64, 0);
                    let five = builder.ins().iconst(types::I64, crate::jit_cache::TAG_NONE);
                    self.emit_return_pair(builder, zero, five);
                    return Some(());
                }
                let was_union =
                    self.intlist_regs.get(&(ins.a as usize)).copied() == Some(INTLIST_UNION);
                if was_union {
                    self.intlist_regs.remove(&(ins.a as usize));
                }
                if self.intlist_regs.contains_key(&(ins.a as usize)) {
                    let bits = builder.use_var(self.rv_i64(ins.a as usize));
                    let six = builder.ins().iconst(types::I64, crate::jit_cache::TAG_INTLIST);
                    self.emit_return_pair_keep(builder, bits, six, true);
                    return Some(());
                }
                let was_param = self.param_idx.contains_key(&(ins.a as usize));
                if (was_union || was_param)
                    && !self.is_none[ins.a as usize]
                    && !self.is_float[ins.a as usize]
                    && !self.is_bool[ins.a as usize]
                    && self.ret_slot[ins.a as usize].map_or(true, |slot| !self.local_float[slot])
                    && !self.int_dict_seen(&[ins.a as usize])
                {
                    let fid = self.fid_list_is_handle;
                    let f_ref = self.module.declare_func_in_func(fid, builder.func);
                    let v = builder.use_var(self.rv_i64(ins.a as usize));
                    let call = builder.ins().call(f_ref, &[v]);
                    let chk = builder.inst_results(call)[0];
                    let zero_v = builder.ins().iconst(types::I64, 0);
                    let ok = builder.ins().icmp(IntCC::NotEqual, chk, zero_v);
                    let list_blk = builder.create_block();
                    let int_blk = builder.create_block();
                    builder.ins().brif(ok, list_blk, &[], int_blk, &[]);
                    self.terminated = true;
                    builder.seal_block(list_blk);
                    builder.seal_block(int_blk);
                    self.goto(builder, list_blk);
                    let six = builder.ins().iconst(types::I64, crate::jit_cache::TAG_INTLIST);
                    self.emit_return_pair_keep(builder, v, six, true);
                    self.goto(builder, int_blk);
                }
                if self.int_dict_seen(&[ins.a as usize]) {
                    return None;
                }
                 if self.is_float[ins.a as usize]
                    && self.ret_slot[ins.a as usize].is_none()
                {
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
                            return None;
                        }
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
            _ => self.reject(&format!(
                "unsupported opcode {:?} (outside the JIT subset)",
                ins.op
            )),
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

const INTLIST_BORROWED: u8 = 8;
const INTLIST_UNION: u8 = 9;

pub fn analyze_return_kinds(program: &Program) -> HashMap<usize, u8> {
    let plan = match crate::vm::transfer::compile(program) {
        Ok(p) => p,
        Err(_) => return (0..program.protos.len()).map(|i| (i, 0)).collect(),
    };
    analyze_return_kinds_with_plan(program, &plan)
}

/// Same analysis as `analyze_return_kinds` but reuses a plan already computed
/// by the caller, so JIT registration translates the bytecode only once for
/// the whole program instead of once per candidate proto.
fn analyze_return_kinds_with_plan(program: &Program, plan: &crate::vm::Plan) -> HashMap<usize, u8> {
    let n = program.protos.len();
    // Proto indices are dense (0..n), so the fixed-point state is a Vec<u8>.
    // The old code cloned the whole HashMap once per proto per iteration; here
    // only the current proto's tag is copied into a scalar before the update.
    // The HashMap boundary stays at the return so callers are unaffected.
    let mut tags: Vec<u8> = vec![0; n];
    let plan = &plan.protos;

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
            let old = tags[proto_index];
            if let Some(cur) = analyze_one_proto(program, plan, proto_index, &callee_lookup, &tags) {
                let union = cur | old;
                if union != old {
                    tags[proto_index] = union;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    for proto_index in 0..n {
        let old = tags[proto_index];
        if let Some(cur) = analyze_one_proto(program, plan, proto_index, &callee_lookup, &tags) {
            tags[proto_index] = old | cur;
        }
    }
    let tags: HashMap<usize, u8> = tags.into_iter().enumerate().collect();
    if crate::jit_cache::jit_debug() && crate::jit_cache::jit_debug_if() {
        for (i, t) in &tags {
            eprintln!("JITDBG retkinds proto={i} kinds={t:#x}");
        }
    }
    tags
}

fn unsafe_param_mask(code: &[Ins], param_mask_by_d: &HashMap<usize, usize>) -> u64 {
    let mut reg_taint: Vec<u64> = vec![0; 256];
    let mut local_taint: Vec<u64> = vec![0; 256];
    let mut mask: u64 = 0;
    for ins in code {
        let a = ins.a as usize;
        let b = ins.b as usize;
        let c = ins.c as usize;
        match ins.op {
            Op::LoadName => {
                reg_taint[a] = param_mask_by_d
                    .get(&(ins.d as usize))
                    .map(|&pos| 1u64 << pos)
                    .unwrap_or(0);
            }
            Op::LoadLocal => {
                reg_taint[a] = local_taint[ins.d as usize];
            }
            Op::StoreLocal => {
                local_taint[ins.d as usize] = reg_taint[a];
            }
            Op::Dup | Op::Unwrap => {
                reg_taint[a] = reg_taint[b];
            }
            Op::Swap => {
                let t = reg_taint[a];
                reg_taint[a] |= reg_taint[b];
                reg_taint[b] |= t;
            }
            Op::Rot => {
                let t = reg_taint[a];
                reg_taint[a] |= reg_taint[b];
                reg_taint[b] |= reg_taint[c];
                reg_taint[c] = t;
            }
            Op::Pop | Op::PopN => {}
            Op::Call => {
                let mut m = 0u64;
                for k in 0..ins.d.max(0) as usize {
                    if b + 1 + k < 256 {
                        m |= reg_taint[b + 1 + k];
                    }
                }
                reg_taint[a] = m;
            }
            Op::GetItem | Op::Slice | Op::GetAttr => {
                reg_taint[a] = 0;
            }
            Op::LoadUpv => {
                reg_taint[a] = u64::MAX;
            }
            _ => {
                let reads_a = matches!(
                    ins.op,
                    Op::SetItem | Op::SetSlice | Op::Await | Op::Raise | Op::In | Op::Xor
                );
                let involved =
                    reg_taint[b] | reg_taint[c] | if reads_a { reg_taint[a] } else { 0 };
                if involved != 0 && is_unsafe_consumer(ins.op) {
                    mask |= involved;
                }
                reg_taint[a] = 0;
            }
        }
    }
    mask
}

fn is_unsafe_consumer(op: Op) -> bool {
    matches!(
        op,
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
            | Op::Not
            | Op::Pos
            | Op::Neg
            | Op::Bnot
            | Op::ToStr
            | Op::FString
            | Op::Propagate
            | Op::FusedIntMulAdd
            | Op::SetItem
            | Op::SetSlice
            | Op::Await
            | Op::Raise
    )
}

fn analyze_one_proto(
    program: &Program,
    plan: &[crate::vm::ProtoPlan],
    proto_index: usize,
    callee_lookup: &HashMap<String, usize>,
    tags: &[u8],
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
    let n_regs = code.iter().map(|i| i.a as usize).max().unwrap_or(0) + 1;
    let n_locals = proto.locals.len();
    let mut reg_float = vec![false; n_regs];
    let mut reg_bool = vec![false; n_regs];
    let mut local_float = vec![false; n_locals];
    let mut local_bool = vec![false; n_locals];
    let mut reg_list = vec![false; n_regs];
    let mut local_list = vec![false; n_locals];
    let mut local_param = vec![false; n_locals];
    let mut reg_slot: Vec<Option<usize>> = vec![None; n_regs];
    let mut param_regs: Vec<bool> = vec![false; n_regs];
    let mut reg_call: Vec<Option<usize>> = vec![None; n_regs];
    let mut name_to_slot: HashMap<String, usize> = HashMap::new();
    for (slot, &proto_ix) in proto.locals.iter().enumerate() {
        if let Some(name) = proto
            .names
            .get(proto_ix)
            .and_then(|&m| program.names.get(m))
        {
            name_to_slot.entry(name.clone()).or_insert(slot);
        }
    }
    for (ix, p) in proto.params.iter().enumerate() {
        let Some(name) = program.names.get(p.name).cloned().map(|s| s) else {
            continue;
        };
        let entry = usize::MAX - ix;
        name_to_slot.entry(name).or_insert(entry);
    }

    let mut kinds = 0u8;

    let claims_intlist = code.iter().any(|ins| ins.op == Op::BuildList);

    for _inner in 0..8 {
        let mut gchanged = false;
        for (_ip, ins) in code.iter().enumerate() {
            if !matches!(ins.op, Op::LoadName | Op::Dup | Op::Call | Op::StoreLocal | Op::LoadLocal | Op::Return) {
                let a = ins.a as usize;
                if a < n_regs {
                    if reg_call[a].is_some() {
                        reg_call[a] = None;
                        gchanged = true;
                    }
                    if reg_slot[a].is_some() {
                        reg_slot[a] = None;
                        gchanged = true;
                    }
                    if param_regs[a] {
                        param_regs[a] = false;
                        gchanged = true;
                    }
                }
            }
            match ins.op {
                Op::Return => {
                    let r = ins.a as usize;
                    if r < n_regs {
                        if reg_float[r] {
                            kinds |= RET_FLOAT;
                        } else if reg_bool[r] {
                            kinds |= RET_BOOL;
                        } else if reg_list[r] {
                            kinds |= RET_INTLIST;
                        } else {
                            kinds |= RET_INT;
                        }
                        if crate::jit_cache::jit_debug_if() {
                            eprintln!("JITDBG ret arm proto={proto_index} r={r} reg_list={} param_regs={} kinds={kinds:#x}", reg_list[r], param_regs[r]);
                        }
                        gchanged = true;
                    }
                }
                Op::LoadConst => {
                    let is_f = matches!(
                        proto
                            .consts
                            .get(ins.d as usize)
                            .and_then(|&ci| program.consts.get(ci)),
                        Some(Constant::Float(_))
                    );
                    if reg_float[ins.a as usize] != is_f {
                        reg_float[ins.a as usize] = is_f;
                        gchanged = true;
                    }
                    if reg_bool[ins.a as usize] {
                        reg_bool[ins.a as usize] = false;
                        gchanged = true;
                    }
                    if reg_list[ins.a as usize] {
                        reg_list[ins.a as usize] = false;
                        gchanged = true;
                    }
                    if reg_slot[ins.a as usize].is_some() {
                        reg_slot[ins.a as usize] = None;
                        gchanged = true;
                    }
                }
                Op::LoadTrue | Op::LoadFalse => {
                    if !reg_bool[ins.a as usize] {
                        reg_bool[ins.a as usize] = true;
                        gchanged = true;
                    }
                    if reg_float[ins.a as usize] {
                        reg_float[ins.a as usize] = false;
                        gchanged = true;
                    }
                    if reg_list[ins.a as usize] {
                        reg_list[ins.a as usize] = false;
                        gchanged = true;
                    }
                    if reg_slot[ins.a as usize].is_some() {
                        reg_slot[ins.a as usize] = None;
                        gchanged = true;
                    }
                }
                Op::LoadNone | Op::LoadNull => {
                    if reg_float[ins.a as usize] {
                        reg_float[ins.a as usize] = false;
                        gchanged = true;
                    }
                    if reg_bool[ins.a as usize] {
                        reg_bool[ins.a as usize] = false;
                        gchanged = true;
                    }
                    if reg_list[ins.a as usize] {
                        reg_list[ins.a as usize] = false;
                        gchanged = true;
                    }
                    if reg_slot[ins.a as usize].is_some() {
                        reg_slot[ins.a as usize] = None;
                        gchanged = true;
                    }
                }
                Op::BuildList => {
                    let a = ins.a as usize;
                    let want = ins.d == 0;
                    if reg_list[a] != want {
                        reg_list[a] = want;
                        gchanged = true;
                    }
                    if reg_float[a] {
                        reg_float[a] = false;
                        gchanged = true;
                    }
                    if reg_bool[a] {
                        reg_bool[a] = false;
                        gchanged = true;
                    }
                    param_regs[a] = false;
                }
                Op::Slice => {
                    let a = ins.a as usize;
                    let b = ins.b as usize;
                    let want = reg_list[b];
                    if reg_list[a] != want {
                        reg_list[a] = want;
                        gchanged = true;
                    }
                    if reg_float[a] {
                        reg_float[a] = false;
                        gchanged = true;
                    }
                    if reg_bool[a] {
                        reg_bool[a] = false;
                        gchanged = true;
                    }
                    param_regs[a] = false;
                }
                Op::Call => {
                    let a = ins.a as usize;
                    if let Some(callee) = reg_call[ins.b as usize] {
                        let tag = tags.get(callee).copied().unwrap_or(0);
                        let want = (tag & RET_INTLIST) != 0;
                        if reg_list[a] != want {
                            reg_list[a] = want;
                            gchanged = true;
                        }
                        let want_f = (tag & RET_FLOAT) != 0
                            && (tag & (RET_INT | RET_BOOL | RET_INTLIST)) == 0;
                        if reg_float[a] != want_f {
                            reg_float[a] = want_f;
                            gchanged = true;
                        }
                        let want_b = (tag & RET_BOOL) != 0
                            && (tag & (RET_INT | RET_FLOAT | RET_INTLIST)) == 0;
                        if reg_bool[a] != want_b {
                            reg_bool[a] = want_b;
                            gchanged = true;
                        }
                    } else {
                        if reg_list[a] {
                            reg_list[a] = false;
                            gchanged = true;
                        }
                        if reg_float[a] {
                            reg_float[a] = false;
                            gchanged = true;
                        }
                        if reg_bool[a] {
                            reg_bool[a] = false;
                            gchanged = true;
                        }
                    }
                    param_regs[a] = false;
                    reg_call[a] = None;
                }
                Op::LoadName => {
                    let name = arena.get(ins.d as usize)?.to_string();
                    let was_param = name_to_slot.get(&name).map(|&s| s >= n_locals).unwrap_or(false);
                    if let Some(&slot) = name_to_slot.get(&name) {
                        reg_slot[ins.a as usize] = Some(slot);
                    } else {
                        reg_slot[ins.a as usize] = None;
                    }
                    param_regs[ins.a as usize] = was_param;
                    if was_param {
                        gchanged = true;
                    }
                    if let Some(&callee) = callee_lookup.get(&name) {
                        reg_call[ins.a as usize] = Some(callee);
                        let tag = tags.get(callee).copied().unwrap_or(0);
                        let want_f = (tag & RET_FLOAT) != 0
                            && (tag & (RET_INT | RET_BOOL | RET_INTLIST)) == 0;
                        if reg_float[ins.a as usize] != want_f {
                            reg_float[ins.a as usize] = want_f;
                            gchanged = true;
                        }
                        let want_b = (tag & RET_BOOL) != 0
                            && (tag & (RET_INT | RET_FLOAT | RET_INTLIST)) == 0;
                        if reg_bool[ins.a as usize] != want_b {
                            reg_bool[ins.a as usize] = want_b;
                            gchanged = true;
                        }
                        let want_l = (tag & RET_INTLIST) != 0;
                        if reg_list[ins.a as usize] != want_l {
                            reg_list[ins.a as usize] = want_l;
                            gchanged = true;
                        }
                    } else {
                        reg_call[ins.a as usize] = None;
                        if reg_float[ins.a as usize] {
                            reg_float[ins.a as usize] = false;
                            gchanged = true;
                        }
                        if reg_bool[ins.a as usize] {
                            reg_bool[ins.a as usize] = false;
                            gchanged = true;
                        }
                        if reg_list[ins.a as usize] {
                            reg_list[ins.a as usize] = false;
                            gchanged = true;
                        }
                    }
                }
                Op::LoadLocal => {
                    let slot = ins.d as usize;
                    let a = ins.a as usize;
                    reg_slot[a] = Some(slot);
                    if slot < n_locals {
                        if reg_float[a] != local_float[slot] {
                            reg_float[a] = local_float[slot];
                            gchanged = true;
                        }
                        if reg_bool[a] != local_bool[slot] {
                            reg_bool[a] = local_bool[slot];
                            gchanged = true;
                        }
                        let want = local_list[slot];
                        if reg_list[a] != want {
                            reg_list[a] = want;
                            gchanged = true;
                        }
                        if param_regs[a] != local_param[slot] {
                            param_regs[a] = local_param[slot];
                            gchanged = true;
                        }
                    }
                }
                Op::Add | Op::Sub | Op::Mul => {
                    let a = ins.a as usize;
                    let b = ins.b as usize;
                    let c = ins.c as usize;
                    let want = reg_float[b] || reg_float[c];
                    if reg_float[a] != want {
                        reg_float[a] = want;
                        gchanged = true;
                    }
                    if reg_bool[a] {
                        reg_bool[a] = false;
                        gchanged = true;
                    }
                    if reg_list[a] {
                        reg_list[a] = false;
                        gchanged = true;
                    }
                    param_regs[a] = false;
                }
                Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Eq | Op::Ne => {
                    let a = ins.a as usize;
                    if !reg_bool[a] {
                        reg_bool[a] = true;
                        gchanged = true;
                    }
                    if reg_float[a] {
                        reg_float[a] = false;
                        gchanged = true;
                    }
                    if reg_list[a] {
                        reg_list[a] = false;
                        gchanged = true;
                    }
                }
                Op::ToBool => {
                    let a = ins.a as usize;
                    if !reg_bool[a] {
                        reg_bool[a] = true;
                        gchanged = true;
                    }
                    if reg_float[a] {
                        reg_float[a] = false;
                        gchanged = true;
                    }
                    if reg_list[a] {
                        reg_list[a] = false;
                        gchanged = true;
                    }
                }
                Op::Dup => {
                    let a = ins.a as usize;
                    let b = ins.b as usize;
                    reg_slot[a] = reg_slot[b];
                    param_regs[a] = param_regs[b];
                    if reg_float[a] != reg_float[b] {
                        reg_float[a] = reg_float[b];
                        gchanged = true;
                    }
                    if reg_bool[a] != reg_bool[b] {
                        reg_bool[a] = reg_bool[b];
                        gchanged = true;
                    }
                    let want = reg_list[b];
                    if reg_list[a] != want {
                        reg_list[a] = want;
                        gchanged = true;
                    }
                    reg_call[a] = reg_call[b];
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
                        if reg_list[a] && !local_list[slot] {
                            local_list[slot] = true;
                            gchanged = true;
                        }
                        if param_regs[a] && !local_param[slot] {
                            local_param[slot] = true;
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
    if kinds == 0 {
        kinds = RET_INT;
    }
    if claims_intlist {
        kinds |= RET_INTLIST;
    }
    if crate::jit_cache::jit_debug() && crate::jit_cache::jit_debug_if() {
        eprintln!("JITDBG analyze_one proto={proto_index} kinds={kinds:#x}");
    }
    if crate::jit_cache::jit_debug_if() {
        eprintln!("JITDBG analyze_one-full proto={proto_index} kinds={kinds:#x}");
    }
    Some(kinds)
}

fn compute_local_float(
    code: &[Ins],
    proto: &Proto,
    program: &Program,
) -> Vec<bool> {
    let n_regs = code.iter().map(|i| i.a as usize).max().unwrap_or(0) + 1;
    let n_locals = proto.locals.len();
    let mut local_float = vec![false; n_locals];
    let mut math_regs: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut mathfn_regs: std::collections::HashMap<usize, MathFn> = std::collections::HashMap::new();
    loop {
        let mut changed = false;
        let mut reg_float = vec![false; n_regs];
        for (_ip, ins) in code.iter().enumerate() {
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
                    math_regs.remove(&(ins.a as usize));
                    mathfn_regs.remove(&(ins.a as usize));
                }
                Op::LoadTrue | Op::LoadFalse | Op::LoadNone | Op::LoadNull | Op::LoadName => {
                    reg_float[ins.a as usize] = false;
                    if ins.op == Op::LoadName
                        && proto
                            .names
                            .get(ins.d as usize)
                            .and_then(|&i| program.names.get(i))
                            .map(|s| s.as_str() == "math")
                            .unwrap_or(false)
                    {
                        math_regs.insert(ins.a as usize);
                    } else {
                        math_regs.remove(&(ins.a as usize));
                    }
                    mathfn_regs.remove(&(ins.a as usize));
                }
                Op::LoadLocal => {
                    let slot = ins.d as usize;
                    reg_float[ins.a as usize] = slot < n_locals && local_float[slot];
                    math_regs.remove(&(ins.a as usize));
                    mathfn_regs.remove(&(ins.a as usize));
                }
                Op::Add | Op::Sub | Op::Mul => {
                    reg_float[ins.a as usize] =
                        reg_float[ins.b as usize] || reg_float[ins.c as usize];
                    math_regs.remove(&(ins.a as usize));
                    mathfn_regs.remove(&(ins.a as usize));
                }
                Op::Dup => {
                    reg_float[ins.a as usize] = reg_float[ins.b as usize];
                    if mathfn_regs.contains_key(&(ins.b as usize)) {
                        mathfn_regs.insert(ins.a as usize, mathfn_regs[&(ins.b as usize)]);
                    } else {
                        mathfn_regs.remove(&(ins.a as usize));
                    }
                    if math_regs.contains(&(ins.b as usize)) {
                        math_regs.insert(ins.a as usize);
                    } else {
                        math_regs.remove(&(ins.a as usize));
                    }
                }
                Op::Unwrap => {
                }
                Op::GetAttr => {
                    reg_float[ins.a as usize] = false;
                    if math_regs.contains(&(ins.b as usize))
                        && proto
                            .names
                            .get(ins.d as usize)
                            .and_then(|&i| program.names.get(i))
                            .and_then(|n| MathFn::from_name(n))
                            .is_some()
                    {
                        let mfn = proto
                            .names
                            .get(ins.d as usize)
                            .and_then(|&i| program.names.get(i))
                            .and_then(|n| MathFn::from_name(n))
                            .unwrap();
                        mathfn_regs.insert(ins.a as usize, mfn);
                    } else {
                        mathfn_regs.remove(&(ins.a as usize));
                    }
                }
                Op::Call => {
                    let dst = ins.a as usize;
                    let callee = ins.b as usize;
                    reg_float[dst] = match mathfn_regs.get(&callee).copied() {
                        Some(MathFn::Floor | MathFn::Ceil | MathFn::Trunc) => reg_float[callee + 1],
                        Some(MathFn::Abs) => reg_float[callee + 1],
                        Some(MathFn::Sign) => false,
                        Some(_) => true,
                        None => false,
                    };
                    mathfn_regs.remove(&dst);
                    mathfn_regs.remove(&callee);
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

pub fn precheck_fn(program: &Program, proto_index: usize) -> Option<usize> {
    let proto = program.protos.get(proto_index)?;
    if proto.flags & PROTO_FUNCTION == 0 {
        return None;
    }
    // Compiler-generated zero-argument wrappers are not user functions in the
    // Int subset. The old `arity == 0` rejection excluded them by accident;
    // now that real zero-argument functions are eligible, keep these out
    // explicitly (a stub here would also trip UV_JIT=2 in every class/defer/
    // go/lambda program).
    if let Some(name) = program.names.get(proto.name) {
        if matches!(name.as_str(), "<classes_init>" | "defer" | "go" | "lambda") {
            return None;
        }
    }
    let has_field_local = proto.locals.iter().any(|&proto_ix| {
        proto
            .names
            .get(proto_ix)
            .and_then(|&m| program.names.get(m))
            .is_some_and(|n| n == "field")
    });
    if has_field_local {
        return None;
    }
    let arity = proto.params.len() + usize::from(proto.ext_type.is_some());
    // Zero-argument functions are in the Int subset too: vm::jit_call has an
    // F0 signature for them. Rejecting them here silently sent otherwise
    // eligible protos to the interpreter (and UV_JIT=2 could not see it).
    if arity > MAX_ARITY {
        return None;
    }
    const ALLOWED: u16 = PARAM_ESSENTIAL | PARAM_OPTIONAL;
    if proto.params.iter().any(|p| p.flags & !ALLOWED != 0) {
        return None;
    }
    Some(arity)
}

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
    let two = builder.ins().iconst(types::I64, crate::jit_cache::TAG_BAIL);
    builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), zero, hp_in, RESULT_VALUE_OFFSET);
    builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), two, hp_in, RESULT_TAG_OFFSET);
    builder.ins().return_(&[]);
    builder.seal_all_blocks();
    builder.finalize(module.target_config());
    if let Err(e) = module.define_function(func_id, &mut fun_ctx) {
        let _ = e;
    }
}

fn emit_fn_in_module(
    program: &Program,
    plan: &crate::vm::Plan,
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
    fid_dict_has: cranelift_module::FuncId,
    fid_dict_free: cranelift_module::FuncId,
    fid_int_get: cranelift_module::FuncId,
    fid_int_ok_name: cranelift_module::FuncId,
    fid_int_set_name: cranelift_module::FuncId,
    fid_list_empty: cranelift_module::FuncId,
    fid_list_push: cranelift_module::FuncId,
    fid_list_push_scalar: cranelift_module::FuncId,
    fid_list_get: cranelift_module::FuncId,
    fid_list_set: cranelift_module::FuncId,
    fid_list_set_scalar: cranelift_module::FuncId,
    fid_list_len: cranelift_module::FuncId,
    fid_list_slice: cranelift_module::FuncId,
    fid_list_free: cranelift_module::FuncId,
    fid_list_is_handle: cranelift_module::FuncId,
    fid_list_iter_new: cranelift_module::FuncId,
    fid_list_iter_has: cranelift_module::FuncId,
    fid_list_iter_next: cranelift_module::FuncId,
    fid_list_iter_free: cranelift_module::FuncId,
    fid_class_make: &[cranelift_module::FuncId],
    fid_math: &HashMap<MathFfi, cranelift_module::FuncId>,
    ret_kinds: &HashMap<usize, u8>,
) -> Option<()> {
    let proto = program.protos.get(proto_index)?;
    if proto.flags & PROTO_FUNCTION == 0 {
        return None;
    }
    let is_class = proto.ext_type.is_some();
    let arity = proto.params.len() + usize::from(is_class);

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
        .map(|(i, p)| (program.names[p.name].clone(), i + usize::from(is_class)))
        .collect();
    let local_name_to_slot: HashMap<String, usize> = proto
        .locals
        .iter()
        .enumerate()
        .filter_map(|(slot, &proto_ix)| {
            proto
                .names
                .get(proto_ix)
                .and_then(|&g| program.names.get(g).cloned())
                .map(|n| (n, slot))
        })
        .collect();

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

    let mut patches: HashMap<usize, RangePatch> = HashMap::new();
    for ip in 0..code.len() {
        if let Some(p) = match_range_for_next(code, &arena, ip) {
            leaders.insert(p.body_start_ip);
            leaders.insert(p.exit_ip);
            patches.insert(ip, p);
        }
    }

    let local_float = compute_local_float(code, proto, program);
    let has_user_call = code.iter().any(|ins| ins.op == Op::Call);

    let mut fun_ctx = module.make_context();
    let mut func_ctx = FunctionBuilderContext::new();

    fun_ctx.func.signature = sig;
    fun_ctx.func.name = cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32());

    let mut builder = FunctionBuilder::new(&mut fun_ctx.func, &mut func_ctx);
    let locals_len = proto.locals.len();
    let field_locals: HashSet<usize> = proto
        .locals
        .iter()
        .enumerate()
        .filter_map(|(slot, &proto_ix)| {
            proto
                .names
                .get(proto_ix)
                .and_then(|&m| program.names.get(m))
                .and_then(|n| if n == "field" { Some(slot) } else { None })
        })
        .collect();
    let total_slots = arity + locals_len + plan.reg_count;
    let mut vars: Vec<Variable> = Vec::with_capacity(total_slots);
    let mut f64_vars: Vec<Variable> = Vec::with_capacity(total_slots);
    for _ in 0..total_slots {
        vars.push(builder.declare_var(types::I64));
        f64_vars.push(builder.declare_var(types::F64));
    }

    // Walk the per-proto name pool once and consult `param_pos` (already a
    // name -> param-position HashMap) for an O(1) lookup. The previous form
    // scanned all of `proto.names` for every parameter (O(params * names)).
    // Iterating names also preserves the original handling of duplicate names
    // in `proto.names`: every matching local slot is recorded.
    let param_mask_by_d: HashMap<usize, usize> = {
        let mut m = HashMap::new();
        for (local_idx, &global_idx) in proto.names.iter().enumerate() {
            if let Some(name) = program.names.get(global_idx) {
                if let Some(&pos) = param_pos.get(name.as_str()) {
                    m.insert(local_idx, pos);
                }
            }
        }
        m
    };
    let unsafe_param_mask = unsafe_param_mask(&code, &param_mask_by_d);

    let prologue = builder.create_block();
    builder.append_block_params_for_function_params(prologue);    let incoming = builder.block_params(prologue).to_vec();
    let hidden_ptr = incoming[0];
    let incoming_tags = incoming[1];
    let entry = builder.create_block();
    builder.switch_to_block(prologue);
    let hp_var = builder.declare_var(types::I64);
    builder.def_var(hp_var, hidden_ptr);
    let tags_var = builder.declare_var(types::I64);
    builder.def_var(tags_var, incoming_tags);
    for (i, v) in incoming.iter().enumerate().skip(2) {
        builder.def_var(vars[i - 2], *v);
    }
    if unsafe_param_mask != 0 {
        let t = builder.use_var(tags_var);
        let m = builder.ins().iconst(types::I64, unsafe_param_mask as i64);
        let r = builder.ins().band(t, m);
        let zero = builder.ins().iconst(types::I64, 0);
        let any = builder.ins().icmp(IntCC::NotEqual, r, zero);
        let bail_blk = builder.create_block();
        let cont_blk = builder.create_block();
        builder.ins().brif(any, bail_blk, &[], cont_blk, &[]);
        builder.seal_block(bail_blk);
        builder.seal_block(cont_blk);
        builder.switch_to_block(bail_blk);
        let zero2 = builder.ins().iconst(types::I64, 0);
        let two = builder.ins().iconst(types::I64, crate::jit_cache::TAG_BAIL);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), zero2, hidden_ptr, RESULT_VALUE_OFFSET);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), two, hidden_ptr, RESULT_TAG_OFFSET);
        builder.ins().return_(&[]);
        builder.switch_to_block(cont_blk);
    }
    let zero = builder.ins().iconst(types::I64, 0);
    for i in arity..vars.len() {
        if is_class && i == arity {
            builder.def_var(vars[i], incoming[2]);
            continue;
        }
        builder.def_var(vars[i], zero);
    }
    for (slot, &proto_ix) in proto.locals.iter().enumerate() {
        let Some(name) = proto
            .names
            .get(proto_ix)
            .and_then(|&g| program.names.get(g).cloned())
        else {
            continue;
        };
        let Some(&p) = param_pos.get(&name) else {
            continue;
        };
        if p < vars.len() {
            let src = builder.use_var(vars[p]);
            builder.def_var(vars[arity + slot], src);
        }
    }
    let zero_f = builder.ins().f64const(0.0);
    for i in 0..vars.len() {
        builder.def_var(f64_vars[i], zero_f);
    }
    let poly_tag = builder.declare_var(types::I64);
    builder.def_var(poly_tag, zero);

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
        let zero_e = builder.ins().iconst(types::I64, 0);
        let two_e = builder.ins().iconst(types::I64, crate::jit_cache::TAG_BAIL);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), zero_e, hidden_ptr, RESULT_VALUE_OFFSET);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), two_e, hidden_ptr, RESULT_TAG_OFFSET);
        builder.ins().return_(&[]);
        entry_blk = cont_entry_blk;
    } else {
        builder.ins().jump(entry, &[]);
        entry_blk = entry;
    }

    let (class_desc_ids, class_methods) = {
        let mut names: Vec<String> = Vec::new();
        for p in &program.protos {
            for instr in &p.code {
                if instr.op_code() != Some(Op::MakeClass) || instr.operand < 0 {
                    continue;
                }
                names.push(program.name(p, instr.operand as usize).to_string());
            }
        }
        names.sort();
        names.dedup();
        let mut ids: HashMap<String, i64> = HashMap::new();
        for name in &names {
            ids.insert(name.clone(), crate::runtime::int_class::register(name));
        }
        let mut methods: HashMap<i64, HashMap<String, usize>> = HashMap::new();
        for (i, p) in program.protos.iter().enumerate() {
            let Some(ext) = p.ext_type else { continue };
            let Some(class_name) = program.names.get(ext) else { continue };
            let Some(&id) = ids.get(class_name) else { continue };
            if precheck_fn(program, i).is_none() {
                continue;
            }
            let method_name = program.names.get(p.name).cloned().unwrap_or_default();
            methods.entry(id).or_default().insert(method_name, i);
        }
        (ids, methods)
    };

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
        local_name_to_slot,
        patches,
        f64_vars,
        arity,
        locals_len,
        field_locals,
        vars,
        is_bool: vec![false; plan.reg_count],
        is_none: vec![false; plan.reg_count],
        is_float: vec![false; plan.reg_count],
        local_is_bool: vec![false; locals_len],
        local_is_none: vec![false; locals_len],
        local_float,
        local_param_pos: vec![None; locals_len],
        param_idx: HashMap::new(),
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
        fid_dict_has,
        fid_dict_free,
        fid_int_get,
        fid_int_ok_name,
        fid_int_set_name,
        fid_list_empty,
        fid_list_push,
        fid_list_push_scalar,
        fid_list_get,
        fid_list_set,
        fid_list_set_scalar,
        fid_list_len,
        fid_list_slice,
        fid_list_free,
        fid_list_is_handle,
        fid_list_iter_new,
        fid_list_iter_has,
        fid_list_iter_next,
        fid_list_iter_free,
        fid_class_make: fid_class_make.to_vec(),
        fid_math: fid_math.clone(),
        class_ctor_regs: HashMap::new(),
        instance_regs: HashMap::new(),
        method_regs: HashMap::new(),
        class_methods,
        class_desc_ids,
        intlist_regs: HashMap::new(),
        local_is_list: vec![0u8; locals_len],
        param_regs: HashSet::new(),
        local_borrow_raw: vec![false; locals_len],
        len_regs: HashSet::new(),
        math_regs: HashSet::new(),
        mathfn_regs: HashMap::new(),
        list_add_regs: HashMap::new(),
        intlist_iter_regs: HashSet::new(),
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
        local_is_instance: vec![None; locals_len],
        call_candidate_regs: HashMap::new(),
        module,
        is_class_method: is_class,
        self_regs: HashSet::new(),
        terminated: false,
        cur_blk: None,
    };
    if tri.emit_stream(&mut builder).is_none() {
        return None;
    }
    builder.seal_all_blocks();
    builder.finalize(module.target_config());

    if let Err(err) = module.define_function(func_id, &mut fun_ctx) {
        if crate::jit_cache::jit_debug() {
            let name = program.names.get(proto.name).cloned().unwrap_or_default();
            eprintln!("JITREJECT define_function ip proto={name} err={err:?}");
        }
        return None;
    }
    Some(())
}















