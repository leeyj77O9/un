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
//!   - unary `Neg` (`-x` on Int: two's-complement wrapping, like the interpreter's `wrapping_neg`; on Float: `fneg`)
//!   - integer `+ - *`; Float variants are emitted as `fadd`/`fsub`/`fmul`
//!     when any operand is Float (Un promotes Int+Float to Float)
//!   - `//` and `%` only with a non-zero integer constant divisor loaded into
//!     the right operand register by a preceding `LoadConst`
//!   - `< <= > >= == !=` producing Int 0/1 booleans for Int operands; Float
//!     operands are compared by `fcmp` (NaN compares the way IEEE says it
//!     compares) and the same 0/1 shape is produced
//!   - `if` / `while` via `JmpIf` (jump when the condition is falsy) and `Jmp`
//!   - `for i in iter.range(lo, hi)` loops over Int bounds
//!   - `FusedIntMulAdd` (the optimizer's accumulator shape)
//!   - user calls to other JIT-eligible protos of the same program (direct
//!     native calls; class constructors and Int-field methods lower to
//!     `un_class_make_<N>` / name-checked member intrinsics)
//!   - IntDict / IntVec intrinsics on known handles
//!   - `math.<fn>` Float intrinsics: sin/cos/tan/asin/acos/atan/sqrt/cbrt/exp/
//!     ln/log2/log10/log(base)/floor/ceil/trunc/sign/pow(mixed Float only)/
//!     hypot/abs(Float only), lowered to f64 FFI calls with domain guards
//!     (sqrt of a negative number / log of a non-positive number bail the
//!     frame to the interpreter, which raises the exact Un error)
//!   - `return expr`, either Int, or Int/Float promoted locals with per-path
//!     tagging
//!
//! Rejected (the whole function routes to the interpreter): spread/varargs,
//! enums, pattern matching, `go`/`wait`, mutable global reads, heterogeneous
//! union returns routed through anything other than a promoted local.
//!
//! Compiled functions follow the Un fast-call ABI: positional Int arguments
//! are passed as raw i64 values; results are returned as tagged raw i64
//! values (see above). The VM checks all argument payloads are `Int` before
//! using this path.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

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
use crate::vm::{Ins, ProtoPlan};

pub const MAX_ARITY: usize = 8;

/// The `math` module functions the Int-JIT can lower to f64 FFI calls.
/// Recognition is purely textual at the bytecode level
/// (`LoadName "math"` / `GetAttr <fn>` / `Call`), the same mechanism the
/// `len` marker and class-method markers use. The interpreter's error
/// semantics are preserved by bailing the whole frame to the interpreter
/// (tag=2) on any domain violation or type mismatch.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum MathFn {
    Sin, Cos, Tan, Asin, Acos, Atan,
    Sqrt, Cbrt, Exp,
    Log,   // `math.log(v)` or `math.log(v, base)`
    Log2, Log10,
    Floor, Ceil, Trunc, Sign,
    Pow, Hypot, Abs,
}

/// The `extern "C"` symbols registered for the math intrinsics (see
/// `native/math.rs`). `Log` is resolved at the call site by argc:
/// argc==1 → `un_math_ln`, argc==2 → `un_math_log` (base).
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

/// Exact-match list used by the rebind guard (`SetAttr` of a math module
/// member re-binds the shared module object at load time).
const MATH_FNS: &[&str] = &[
    "sin", "cos", "tan", "asin", "acos", "atan",
    "sqrt", "cbrt", "exp", "log", "log2", "log10",
    "floor", "ceil", "trunc", "sign",
    "pow", "hypot", "abs",
];

/// Translate the eligible protos of `program` into the JIT cache keyed by
/// `program_ptr`. Called once per program when the VM first plans it.
/// Registry entry for the pre-JIT compile guard: disables the entire
/// program's JIT path when a global binding is re-bound.
/// Names used by the JIT translation could be re-bound at module-load time
/// (`iter.range = …`, `len = …`), which a compile-time decision made
/// once per program could never reconcile. Rules:
///  * a store/definition of the bare name `math` vetoes unconditionally;
///  * a `SetAttr` whose attr name is one of the lowered `math` functions
///    vetoes only when the program also reads the `math` global somewhere
///    (`LoadName "math"`) — instance code that uses `math` elsewhere pays
///    for it, math-free programs never do.
/// Any violation disqualifies the entire program from the JIT
/// (exact interpreter semantics are preserved).
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
                // `fn math = ??-style definitions with FUNC_BIND (and the
                // decorated-export seed). Would clobber cross-function
                // inlining of the same name; block the risky programs.
                if ins.flags & FLAG_FUNC_BIND != 0 {
                    let slot_of = ins.operand as usize;
                    // operand is the proto-slot index, not name index
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
    // Hard safety net: any panic inside the Cranelift compile/declare/
    // finalize pipeline (a rare frontend mismatch seen on some launches ??
    // `declared type of variable N doesn't match type of value M`) must not
    // kill the process. Skip all JIT registration instead; the whole program
    // falls back to the interpreter, byte-identical to UV_JIT=0.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        register_eligible_inner(program, program_id)
    }));
}
fn register_eligible_inner(program: &Program, program_id: u64) {
    let strict = crate::jit_cache::jit_strict();
    // Per-candidate compile failures land here; a `UV_JIT=2` canary below
    // turns a whole-program fallback into a loud exit instead of a silent
    // interpreter world.
    let mut failed: Vec<(usize, String)> = Vec::new();
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
        Err(_) => {
            // Not reached by the canary: an env/backend failure here means
            // the JIT is entirely unavailable, which is observably the same
            // as `UV_JIT=0`. Still noisy under strict mode.
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
    let fid_dict_has = match module
        .declare_function("un_dict_int_has", Linkage::Import, &sig_get)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_free = module.make_signature();
    sig_free.params.push(AbiParam::new(types::I64));
    let fid_dict_free = match module
        .declare_function("un_dict_int_free", Linkage::Import, &sig_free)
    {
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
    // Name-checked variant: verifies the member at the given field index is
    // an Int and has exactly the expected name. Used by the class-method
    // intrinsic; a miss bails to the interpreter (tag=2).
    let mut sig_ionn = module.make_signature();
    sig_ionn.params.push(AbiParam::new(types::I64));
    sig_ionn.params.push(AbiParam::new(types::I64));
    sig_ionn.params.push(AbiParam::new(types::I64));
    sig_ionn.params.push(AbiParam::new(types::I64));
    sig_ionn.returns.push(AbiParam::new(types::I64));
    let fid_int_ok_name = match module.declare_function("un_instance_int_ok_name", Linkage::Import, &sig_ionn) {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_isn = module.make_signature();
    for _ in 0..5 {
        sig_isn.params.push(AbiParam::new(types::I64));
    }
    sig_isn.returns.push(AbiParam::new(types::I64));
    let fid_int_set_name = match module.declare_function("un_instance_int_set_name", Linkage::Import, &sig_isn) {
        Ok(fid) => fid,
        Err(_) => return,
    };
    // IntVec (list) intrinsics for the Int-list fast path.
    let mut sig_l_empty = module.make_signature();
    sig_l_empty.returns.push(AbiParam::new(types::I64));
    let fid_list_empty = match module
        .declare_function("un_list_int_empty", Linkage::Import, &sig_l_empty)
    {
        Ok(fid) => fid,
        Err(_) => return,
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
        Err(_) => return,
    };
    let fid_list_push_scalar = match module
        .declare_function("un_list_int_push_scalar", Linkage::Import, &sig_l_push)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_l_get = module.make_signature();
    sig_l_get.params.push(AbiParam::new(types::I64));
    sig_l_get.params.push(AbiParam::new(types::I64));
    sig_l_get.returns.push(AbiParam::new(types::I64));
    let fid_list_get = match module
        .declare_function("un_list_int_get", Linkage::Import, &sig_l_get)
    {
        Ok(fid) => fid,
        Err(_) => return,
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
        Err(_) => return,
    };
    let fid_list_set_scalar = match module
        .declare_function("un_list_int_set_scalar", Linkage::Import, &sig_l_set)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_l_len = module.make_signature();
    sig_l_len.params.push(AbiParam::new(types::I64));
    sig_l_len.returns.push(AbiParam::new(types::I64));
    let fid_list_len = match module
        .declare_function("un_list_int_len", Linkage::Import, &sig_l_len)
    {
        Ok(fid) => fid,
        Err(_) => return,
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
        Err(_) => return,
    };
    let mut sig_l_free = module.make_signature();
    sig_l_free.params.push(AbiParam::new(types::I64));
    let fid_list_free = match module
        .declare_function("un_list_int_free", Linkage::Import, &sig_l_free)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_l_is = module.make_signature();
    sig_l_is.params.push(AbiParam::new(types::I64));
    sig_l_is.returns.push(AbiParam::new(types::I64));
    let fid_list_is_handle = match module
        .declare_function("un_list_int_is_handle", Linkage::Import, &sig_l_is)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_iter_new = module.make_signature();
    sig_iter_new.params.push(AbiParam::new(types::I64));
    sig_iter_new.returns.push(AbiParam::new(types::I64));
    let fid_list_iter_new = match module
        .declare_function("un_list_int_iter_new", Linkage::Import, &sig_iter_new)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_iter_has = module.make_signature();
    sig_iter_has.params.push(AbiParam::new(types::I64));
    sig_iter_has.returns.push(AbiParam::new(types::I64));
    let fid_list_iter_has = match module
        .declare_function("un_list_int_iter_has_next", Linkage::Import, &sig_iter_has)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_iter_next = module.make_signature();
    sig_iter_next.params.push(AbiParam::new(types::I64));
    sig_iter_next.returns.push(AbiParam::new(types::I64));
    let fid_list_iter_next = match module
        .declare_function("un_list_int_iter_next", Linkage::Import, &sig_iter_next)
    {
        Ok(fid) => fid,
        Err(_) => return,
    };
    let mut sig_iter_free = module.make_signature();
    sig_iter_free.params.push(AbiParam::new(types::I64));
    let fid_list_iter_free = match module
        .declare_function("un_list_int_iter_free", Linkage::Import, &sig_iter_free)
    {
        Ok(fid) => fid,
        Err(_) => return,
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
            Err(_) => return,
        }
    }

    // math Float intrinsics (sqrt/sin/floor/…): three shared signatures.
    // Every shim is a total, side-effect-free Rust `f64` op; the JIT emits
    // guard branches for domain checks before calling, and a guard miss
    // bails the frame before the shim is reached.
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
            Err(_) => return,
        }
    }

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
        // Hidden argument-tag bitmap: bit p marks argument p as a borrowed
        // IntVec arena handle (not a plain Int).
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
        // Contain a backend panic (e.g. SSA type mismatch in
        // `f64_vars`-promoted loops) so it doesn't take the whole module
        // down with it: this candidate becomes an all-bail stub and the
        // rest of the module stays usable.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            emit_fn_in_module(
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
                if std::env::var_os("UV_JIT_DEBUG").is_some() {
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
            if std::env::var_os("UV_JIT_DEBUG").is_some() {
                eprintln!("JITREJECT proto[{idx}] {name} (panic in JIT codegen, fallback stub)");
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
    // Strict-mode canary: `UV_JIT=2` makes any candidate compile failure a
    // hard process exit instead of a silent fallback stub.
    // `std::process::exit` bypasses the `catch_unwind` in
    // `register_eligible` on purpose: this is a hard failure by operator
    // request, not an error to absorb.
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
    // Keep the module in the per-program registry so `jit_cache::clear()` /
    // `clear_program()` can free the native code on demand. The vendored
    // backend leaks the allocations otherwise (by design); stashing
    // preserves hook visibility instead of losing them to a `mem::forget`
    // inside the destructor.
    crate::jit_cache::stash_module(program_id, module);
}

/// Translator state. All per-function data is gathered under one borrow.
struct Tr<'a> {
    program: &'a Program,
    proto: &'a Proto,
    proto_index: usize,
    plan: &'a ProtoPlan,
    arena: Vec<String>,
    param_pos: HashMap<String, usize>,
    /// Frame-local slot for every declared local (proto.locals order). The
    /// binding index `name -> slot`. When a local and a parameter share a
    /// name (e.g. `fn f(n) -> ... n = n - 1 ...`), the VM merges both names
    /// onto the one local slot at bind time, and `LoadName` resolves there
    /// — never to the ABI parameter. Reading via `param_pos` instead would
    /// see the stale FFI argument register on every iteration (the JIT looped
    /// forever). LoadName must consult this map first.
    local_name_to_slot: HashMap<String, usize>,
    patches: HashMap<usize, RangePatch>,
    arity: usize,
    // Registers whose value is (still) one of the function's raw argument
    // values. Used by the list-handle probe to mark argument handles as
    // borrowed: the callee never frees a borrowed handle, because the caller
    // (jit_fast) owns it and copies its contents back into the caller's Un
    // object after the call returns. Best-effort: a miss degrades to a
    // skipped free at worst, never to a wrong result.
    param_regs: HashSet<usize>,
    /// Register -> the parameter position (0-based, in the FFI argument
    /// order) this register is (still) a raw parameter derived from.
    param_idx: HashMap<usize, usize>,
    /// Local slot -> parameter position, for locals that received a raw
    /// parameter store (`let a = n`).
    local_param_pos: Vec<Option<usize>>,
    // `local_borrow_raw[slot]` — the slot currently holds an (unprobed) raw
    // argument value. Kept so `LoadLocal` can restore the register-level
    // provenance on the way back out.
    local_borrow_raw: Vec<bool>,
    locals_len: usize,
    // Slots whose local name is `field`; those bindings hold the Un
    // `FieldRef` written by `accessor_bind` (property accessors). The JIT
    // must never touch them: a store would silently overwrite the FieldRef
    // instead of writing through to the property backing, and a load would
    // read it as an Int. Any such local disqualifies the whole proto.
    field_locals: HashSet<usize>,
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
    fid_dict_has: cranelift_module::FuncId,
    fid_dict_free: cranelift_module::FuncId,
    fid_int_get: cranelift_module::FuncId,
    fid_int_ok_name: cranelift_module::FuncId,
    fid_int_set_name: cranelift_module::FuncId,
    // Per-arity class constructor intrinsics (`un_class_make_<N>`); index is
    // the argument count.
    fid_class_make: Vec<cranelift_module::FuncId>,
    // IntVec intrinsics (`un_list_int_*`).
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
    // True when the proto is a class method (its first hidden argument is the
    // receiver, a raw `Rc<RefCell<ObjData>>` pointer). Local slot 0 is `self`.
    is_class_method: bool,
    // Registers that provably hold the receiver pointer (currently loaded
    // straight from `LoadLocal(0)` without being overwritten). Only those
    // may serve as the `self` operand of the direct attr-field intrinsic.
    self_regs: HashSet<usize>,
    // Register holding an IntVec handle (`un_list_int_*`). The i64 value in
    // the slot is the arena id; tag `1` = Int-uniform, `2` = Bool-uniform,
    // `0` = kind not yet known. Tracked like the other handle sets so no
    // arithmetic or return path ever observes it.
    intlist_regs: HashMap<usize, u8>,
    // Same for local slots.
    local_is_list: Vec<u8>,
    // Register marked as a bound call to the global `len` (LoadName "len").
    len_regs: HashSet<usize>,
    // Register marked as the global `math` module (LoadName "math"); the
    // bound intrinsic is resolved one instruction later at `GetAttr`.
    math_regs: HashSet<usize>,
    // Register holding a math-module member marker (`GetAttr` on a math
    // marker): `math.<fn>(...)` call sites lower to f64 FFI calls.
    mathfn_regs: HashMap<usize, MathFn>,
    // math Float intrinsic FuncIds keyed by shim identity.
    fid_math: HashMap<MathFfi, cranelift_module::FuncId>,
    // Register marked as a pending `IntVec.add(v)` call whose receiver is
    // recorded (`dst marker -> receiver register`).
    list_add_regs: HashMap<usize, usize>,
    // Register holding an iterator handle for the IntVec for-in path.
    intlist_iter_regs: HashSet<usize>,
    // Register holding the class token of a program-declared constructor
    // (`Point`), emitted by `LoadName`. The value is a compile-time
    // descriptor id; `Op::Call` on it lowers to `un_class_make_<N>`.
    class_ctor_regs: HashMap<usize, i64>,
    // Register holding a raw `Rc<RefCell<ObjData>>` pointer to a freshly
    // constructed instance of a known class (`un_class_make_<N>`). The i64
    // value is the descriptor id. Tracked like `int_dict_regs` so no
    // arithmetic, return or spill ever observes it, and so `.norm()` /
    // `SetAttr`/`StoreLocal` can specialise on it.
    instance_regs: HashMap<usize, i64>,
    // Register holding a "bound class-method" marker from `GetAttr` on an
    // instance register. Never materialised as a value; `Op::Call` on it
    // lowers to a direct native call into the method proto with the
    // instance as hidden `self` argument 0. Carries the receiver register.
    method_regs: HashMap<usize, (i64, usize, usize)>,
    // Descriptor id -> method name -> proto index for every JIT-eligible
    // method of every program-declared class. Built once per compiled
    // function from the compile-time program view.
    class_methods: HashMap<i64, HashMap<String, usize>>,
    // Class name -> descriptor id (for program-declared classes only).
    class_desc_ids: HashMap<String, i64>,
    // Slots that hold IntDict references (opaque i64 ids used by the JIT).
    int_dict_regs: HashSet<usize>,
    local_is_intdict: Vec<bool>,
    local_is_instance: Vec<Option<i64>>,
    // Proto indices by global function name (exact match against arena).
    callee_lookup: HashMap<String, usize>,
    // Register -> proto index for registers holding a callable function
    // object, tracked from LoadName emissions.
    call_candidate_regs: HashMap<usize, usize>,
    module: &'a mut JITModule,
    // True when the currently open block of the emission already carries a
    // terminator instruction. Cranelift's frontend requires the current
    // block to be terminated before it is switched away from, so every
    // `goto` call site relies on this flag (or on being switched-to from
    // outside emission), and every terminator-emitting site sets it.
    terminated: bool,
    cur_blk: Option<Block>,
}

impl<'a> Tr<'a> {
    // Close the current block and open `blk`. If the current block is
    // still open we close it with an unconditional jump to `blk`; arms that
    // already ended the block with a terminator set `terminated = true`,
    // in which case the switch is free. Switching onto the block that is
    // already open never inserts anything (the block is simply re-entered
    // with new instructions appended, matching the emission order).
    fn goto(&mut self, builder: &mut FunctionBuilder, blk: Block) {
        if self.cur_blk.is_some() && !self.terminated && self.cur_blk != Some(blk) {
            builder.ins().jump(blk, &[]);
        }
        self.cur_blk = Some(blk);
        self.terminated = false;
        builder.switch_to_block(blk);
    }
    /// True if any register refers to a non-Int opaque handle (an IntDict
    /// id, a class token, a constructed instance pointer or a bound-method
    /// marker) rather than an ordinary Int. Handles must never flow into
    /// values the interpreter can observe (a handle returned as a tag=0 Int
    /// would masquerade as a number, and leaked handles would corrupt scope
    /// or callee arguments).
    /// A handle that is NOT an IntVec handle. IntVec intrinsics (e.g.
    /// `xs.add(v)` / `xs[i] = v`) fully understand list handles (the row
    /// path), so the generic `int_dict_seen` check would defeat the row
    /// mechanism; anything else (dict ids, class instances, methods, iters)
    /// cannot be stored/passed and must bail to the interpreter.
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
        // All args must be number-like Ints and not none/bool (the Int JIT
        // ABI does not carry them). Bool == Int 1/0 in our Cranelift IR, so
        // Bool would *seem* safe; reject anything unknown anyway.
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
            // List handles are allowed: the callee prologue guards its own
            // params (unsafe_param_mask), so a list arg that would be read
            // as an Int bails to the interpreter there — the caller never
            // needs an Int ABI violation to reach the callee.
            if self.int_list_foreign_seen(&[r]) {
                if crate::jit_cache::jit_debug() {
                    eprintln!("JITREJECT emit_user_call arg{k}=={r} foreign handle");
                }
                return None;
            }
        }
        if argc > MAX_ARITY {
            return None;
        }
        let hp = builder.use_var(self.hp_var);
        // Compute the callee's `param_tags` bitmap from the staged
        // argument registers: a bit per argument that is currently marked
        // as an (un)tracked arena handle. Bits are static; the runtime
        // value is read by the callee's entry guard and intrinsics.
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
        let int_or_vec = pure_kind == RET_INT | RET_INTLIST;
        let expected_tag = match pure_kind {
            RET_INT => 0i64,
            RET_FLOAT => 1i64,
            RET_BOOL => 3i64,
            RET_INTLIST => 6i64,
            // Heterogeneous Int lane: the callee's Return emission probes
            // the actual register at run time and tags Int (0) vs IntVec
            // (6) accordingly. Accept either; a plain Int >= ID_BASE could
            // alias a live arena handle, so it falls into the bail arm.
            k if k == RET_INT | RET_INTLIST => 6i64,
            // Holed Float|Int, Bool|numeric and friends cannot be bound at
            // this site; leave those to the interpreter.
            _k => {
                if crate::jit_cache::jit_debug() {
                    eprintln!("JITREJECT emit_user_call callee_kind={callee_kind:#x} (heterogeneous)");
                }
                return None;
            }
        };
        // Load (value, tag) out of the shared hidden slot
        let val = builder.ins().load(types::I64, cranelift_codegen::ir::MemFlagsData::new(), hp, 0);
        let tag = builder.ins().load(types::I64, cranelift_codegen::ir::MemFlagsData::new(), hp, 8);
        // The callee may only produce `expected_tag` for this call site ??any
        // other value (including an unexpected tag from a mis-analysed
        // callee) bails the frame to the interpreter, which is always
        // correct.
        let is_expected = if int_or_vec {
            let zero_v = builder.ins().iconst(types::I64, 0);
            let six_v = builder.ins().iconst(types::I64, 6);
            let id_base_v = builder.ins().iconst(types::I64, crate::runtime::int_list::id_base());
            let is_int = builder.ins().icmp(IntCC::Equal, tag, zero_v);
            let is_list = builder.ins().icmp(IntCC::Equal, tag, six_v);
            // A plain Int >= ID_BASE could alias a live arena handle.
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
        let zero = builder.ins().iconst(types::I64, 0);
        let bail_tag = builder.ins().iconst(types::I64, 2);
        self.emit_return_pair(builder, zero, bail_tag);
        self.goto(builder, cont_blk);
        // The register now holds the callee's return value. If the callee
        // was inferred `RET_INTLIST` at plan time (see
        // `analyze_return_kinds`), the caller types it as an IntList
        // handle; otherwise as the numeric kind it was assigned.
        builder.def_var(self.rv_i64(ins.a as usize), val);
        self.ret_slot[ins.a as usize] = None;
        self.class_ctor_regs.remove(&(ins.a as usize));
        self.instance_regs.remove(&(ins.a as usize));
        self.method_regs.remove(&(ins.a as usize));
        self.intlist_regs.remove(&(ins.a as usize));
        self.len_regs.remove(&(ins.a as usize));
        match pure_kind {
            k if k == RET_INT | RET_INTLIST => {
                // Runtime value already validated as Int < ID_BASE or an
                // IntVec handle. Every IntVec intrinsic probes the arena id
                // at run time, so both arms run the same code; Int-consuming
                // ops reject the register via int_dict_seen. Marked UNION
                // (not owned 7) so Return re-tags by probing instead of
                // unconditionally emitting TAG_INTLIST.
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
                // Int slot contains Float bits ??downstream already treats
                // the register as Float-typed when is_float=true elsewhere.
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
            // Unreachable (filtered above); keep as None for safety.
            _ => None,
        }
    }

    /// Free every IntDict this function created (locals and live int-dict
    /// registers). IntDict ids cannot escape a compilable function ??Return
    /// with an id register declines at translation, so the interpreter never
    /// sees one ??making the tables dead at any exit. `un_dict_int_free`
    /// tombstones slots, so an id reachable through both a local and a
    /// register (e.g. created, stored, loaded again) is freed exactly once
    /// at runtime.
    /// Constructor fast path: emit `un_class_make_<argc>(desc_id, args...)`,
    /// parking the allocated instance in the thread-local arena (the raw
    /// pointer is valid for the duration of this native frame). Returns
    /// `None`, making the whole proto bail, when any class- or call-site
    /// condition is violated.
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
        // The class token itself is a pointer-width constant we control;
        // its type flags are all false by construction.
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
        // A zero result means the runtime declined (unknown class,
        // constructor threw); bail the whole frame to the interpreter.
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
        // Drop the constructor token and any stale marker; the register
        // now holds a live instance pointer only.
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

    /// Bound-class-method call on an instance register created by
    /// `GetAttr`'s method-marker path. Emits a direct native call into the
    /// method proto with `[hidden result ptr, raw self pointer, ...args]`,
    /// mirroring `emit_user_call`'s hidden-slot and tag=2 bail handling.
    /// The callee's `tag=2` (runtime name-check miss) bails the whole
    /// frame; anything else re-types the destination register.
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
        // The receiver must be a known instance register; anything else is
        // rejected so the marker can never be fed a raw handle it doesn't
        // own.
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
        // Borrowed-handle bitmap for the class method's own arguments
        // (position 0 is `self`, which is never a list; bits start at the
        // callee parameter positions).
        let mut tags = builder.ins().iconst(types::I64, 0);
        for k in 0..argc {
            let r = ins.b as usize + 1 + k;
            if self.intlist_regs.contains_key(&r) || self.param_idx.contains_key(&r) {
                let bit = builder.ins().iconst(types::I64, 1i64 << (k + 1));
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
        let int_or_vec = pure_kind == RET_INT | RET_INTLIST;
        let expected_tag = match pure_kind {
            RET_INT => 0i64,
            RET_FLOAT => 1i64,
            RET_BOOL => 3i64,
            RET_INTLIST => 6i64,
            // Heterogeneous Int lane: the callee's Return emission probes
            // the actual register at run time and tags Int (0) vs IntVec
            // (6) accordingly. Accept either; a plain Int >= ID_BASE could
            // alias a live arena handle, so it falls into the bail arm.
            k if k == RET_INT | RET_INTLIST => 6i64,
            // Holed Float|Int, Bool|numeric and friends cannot be bound at
            // this site; leave those to the interpreter.
            _k => {
                if crate::jit_cache::jit_debug() {
                    eprintln!("JITREJECT emit_user_call callee_kind={callee_kind:#x} (heterogeneous)");
                }
                return None;
            }
        };
        let val = builder.ins().load(types::I64, cranelift_codegen::ir::MemFlagsData::new(), hp, 0);
        let tag = builder.ins().load(types::I64, cranelift_codegen::ir::MemFlagsData::new(), hp, 8);
        // The callee may only produce `expected_tag` for this call site ??any
        // other value (including an unexpected tag from a mis-analysed
        // callee) bails the frame to the interpreter, which is always
        // correct.
        let is_expected = if int_or_vec {
            let zero_v = builder.ins().iconst(types::I64, 0);
            let six_v = builder.ins().iconst(types::I64, 6);
            let id_base_v = builder.ins().iconst(types::I64, crate::runtime::int_list::id_base());
            let is_int = builder.ins().icmp(IntCC::Equal, tag, zero_v);
            let is_list = builder.ins().icmp(IntCC::Equal, tag, six_v);
            // A plain Int >= ID_BASE could alias a live arena handle.
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
        self.emit_bail_exit(builder);
        self.goto(builder, cont_blk);
        builder.def_var(self.rv_i64(ins.a as usize), val);
        self.ret_slot[ins.a as usize] = None;
        self.int_dict_regs.remove(&(ins.a as usize));
        // The register now holds the method's return value; stale markers
        // must not survive onto it.
        self.class_ctor_regs.remove(&(ins.a as usize));
        self.instance_regs.remove(&(ins.a as usize));
        self.method_regs.remove(&(ins.a as usize));
        self.intlist_regs.remove(&(ins.a as usize));
        self.len_regs.remove(&(ins.a as usize));
        match pure_kind {
            k if k == RET_INT | RET_INTLIST => {
                // Runtime value already validated as Int < ID_BASE or an
                // IntVec handle. Every IntVec intrinsic probes the arena id
                // at run time, so both arms run the same code; Int-consuming
                // ops reject the register via int_dict_seen. Marked UNION
                // (not owned 7) so Return re-tags by probing instead of
                // unconditionally emitting TAG_INTLIST.
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
            // Unreachable (filtered above); keep as None for safety.
            _ => None,
        }
    }

    /// Emit a runtime `un_list_int_is_handle(reg)` check. Returns `Some(value)`
    /// on the handle path (the register may hold a bare Int whose handle only
    /// materialised through the FFI), `None` on the bail path (the whole
    /// function flips to interpreter, tag=2). This is how FFI-passed lists
    /// become visible to the intrinsics ??the callee cannot trust an unmarked
    /// Int argument to be a handle without residue analysis, so it checks.
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
        // The receiver is now known-good; remember it statically for the
        // rest of the emit pass. Argument handles are the caller's
        // property: they are marked borrowed so the callee's teardown
        // does not free them (jit_fast copies contents back afterwards).
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

    /// Marker cleanup shared by every math-intrinsic arm: consume the
    /// callee marker and mark the destination register's kind.
    fn finish_math_call(&mut self, ins: Ins, callee_reg: usize, dst_is_float: bool) {
        let dst = ins.a as usize;
        self.is_bool[dst] = false;
        self.is_none[dst] = false;
        self.is_float[dst] = dst_is_float;
        self.ret_slot[dst] = None;
        self.mathfn_regs.remove(&callee_reg);
        self.mathfn_regs.remove(&dst);
    }

    /// An argument read as f64: Float registers pass through, Int registers
    /// are promoted (`fcvt_from_sint`), mirroring `Int + Float` promotion in
    /// the arithmetic arms. `self.is_none`/`is_bool` are checked by the
    /// caller before this is used.
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

    /// Emit the guard for `sqrt` (`x < 0`) / `ln`/`log2`/`log10`/`log`
    /// (`x <= 0`). A miss bails the whole frame to the interpreter, which
    /// re-runs the proto and raises the exact Un domain error.
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

    /// Emit `math.<fn>(args...)` as an f64 FFI call. `callee_reg` holds the
    /// member marker from the preceding `GetAttr`; the arguments sit in
    /// consecutive registers after it (`callee_reg + 1`, `+2`), and `ins.d`
    /// is argc.
    ///
    /// Domain violations and tag/type mismatches never surface inside the
    /// FFI: either the guard fires (tag=2 bail, the interpreter re-runs the
    /// frame and raises the exact Un error) or the proto is declined up
    /// front (`None`, an all-bail stub, and the interpreter runs everything).
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
        // Bool / none payloads are never f64 numbers; bail to the
        // interpreter, which raises the exact type error.
        if self.is_bool[a0] || self.is_none[a0] {
            return None;
        }
        match mfn {
            MathFn::Floor | MathFn::Ceil => {
                if !self.is_float[a0] {
                    // Int in → identity Int (the interpreter returns the Obj untouched).
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
                // `obj::int(v.trunc() as i64)` in the interpreter also saturates.
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
                // sign is always an Int, NaN collapses to 0 — matching the interpreter.
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
                    // Both-Int: the interpreter owns the exact Int semantics
                    // (integral integral, finite, saturating i64 cast). No
                    // dynamic tag can be expressed for a nameless register, so
                    // the whole proto declines to the interpreter.
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
                // Int args could reach i64::MIN, which the interpreter would
                // wrap (release) or panic (debug); keep the interpreter's path.
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
            // Transcendentals have no domain check in the interpreter
            // (NaN passes through); the same Rust f64 methods run in FFI.
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

    /// Store `(0, TAG_BAIL)` through the hidden pointer and return ??the
    /// interpreter re-runs the frame exactly once. Every bail exit must take
    /// the depth-exit path too, or the depth counter leaks and every future
    /// JIT entry looks "too deep" (silent permanent interpreter fallback).
    fn emit_bail_exit(&mut self, builder: &mut FunctionBuilder<'_>) {
        if self.needs_depth {
            let exit_ref = self.module.declare_func_in_func(self.fid_exit, builder.func);
            builder.ins().call(exit_ref, &[]);
        }
        self.emit_int_dict_cleanup(builder);
        let hp = builder.use_var(self.hp_var);
        let zero = builder.ins().iconst(types::I64, 0);
        let two = builder.ins().iconst(types::I64, crate::jit_cache::TAG_BAIL);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), zero, hp, 0);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), two, hp, 8);
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
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), bits, hp, 0);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), tag, hp, 8);
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

    /// The shared body of `Op::LoadLocal` and the frame-local arm of
    /// `Op::LoadName`: seed the destination register from the slot's (typed)
    /// variable with the full marker bookkeeping (ret_slot, intlist/intdict/
    /// instance markers, borrowed params, class-method `self`).
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
        self.class_ctor_regs.remove(&dst_reg);
        self.instance_regs.remove(&dst_reg);
        self.method_regs.remove(&dst_reg);
        self.intlist_regs.remove(&dst_reg);
        self.len_regs.remove(&dst_reg);
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
                // Local slot 0 is `self`; a register loaded straight
                // from it holds the receiver pointer.
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
                ip = patch.body_start_ip;
                continue;
            }

            if self.emit_ins(builder, code, ip, ins).is_none() {
                if crate::jit_cache::jit_debug() {
                    eprintln!("JITREJECT emit_ins ip={ip} op={:?}", ins.op);
                }
                return None;
            }
            ip += 1;
        }
        if !self.terminated {
            // Reaching the end of the instruction stream means the proto
            // did not terminate (every compilable proto ends in Return).
            // Bail through the hidden pointer so the interpreter re-runs
            // the frame instead of the caller reading an uninitialised pair.
            // Must still release the depth guard, like every other bail exit.
            if self.needs_depth {
                let exit_ref = self.module.declare_func_in_func(self.fid_exit, builder.func);
                builder.ins().call(exit_ref, &[]);
            }
            self.emit_int_dict_cleanup(builder);
            let hp = builder.use_var(self.hp_var);
            let zero = builder.ins().iconst(types::I64, 0);
            let two = builder.ins().iconst(types::I64, crate::jit_cache::TAG_BAIL);
            builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), zero, hp, 0);
            builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), two, hp, 8);
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
        // unrelated def of the same register (e.g. a Float const) ??it says
        // nothing about the value stored below.
        if self.local_float[i_slot] {
            // range loops are Int-only; a Float iteration variable the
            // jit cannot describe ? fall back to the interpreter.
            return None;
        }
        let i_var = self.lv_i64(i_slot);

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
         self.terminated = true;

        // header: i < stop ? body : exit
        self.goto(builder, header_blk);
        let i_v = builder.block_params(header_blk)[0];
        builder.def_var(i_var, i_v);
        let cmp = builder.ins().icmp(IntCC::SignedLessThan, i_v, stop_v);
        builder.ins().brif(cmp, body_blk, &[BlockArg::Value(i_v)], exit_blk, &[]);
         self.terminated = true;

        // increment block: i = i + 1; jumps back to header
        self.goto(builder, inc_blk);
        let i_v = builder.block_params(inc_blk)[0];
        builder.def_var(i_var, i_v);
        let one = builder.ins().iconst(types::I64, 1);
        let inc = builder.ins().iadd(i_v, one);
        builder.ins().jump(header_blk, &[BlockArg::Value(inc)]);
         self.terminated = true;

        // continue emission in the body block
        self.goto(builder, body_blk);
        let i_v = builder.block_params(body_blk)[0];
        builder.def_var(i_var, i_v);
        // ForNext always yields an Int. The yield register's `is_float`
        // mark may hold a stale value from an earlier def of the same
        // register (e.g. `total` reloading into the same slot), so use the
        // i64 variable regardless and drop the stale flag; later Float ops
        // will re-derive it from operands.
        let yield_reg = patch.yield_reg as usize;
        let yield_var = self.rv_i64(yield_reg);
        builder.def_var(yield_var, i_v);
        self.is_float[yield_reg] = false;

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
        // `ins.a` is the destination register for most emitters; marking it
        // fresh keeps stale identities out of the marker maps. Marker
        // accounting is split between this flush (for value-defining
        // ops) and explicit re-derivation inside the identity ops
        // (`LoadLocal`, `Dup`, `Unwrap`) and the marker-producing ops
        // (`LoadName`, `GetAttr`, `Call`). Every marker-carrying register
        // is either rebound here or has had its markers cleared/re-set by
        // the arm that wrote it, so no stale marker can survive past an
        // instruction below the dataflow edge it belonged to.
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
                | Op::Slice
                | Op::PrepIter
                // Pure argument-framing markers: `No register effect in the
                // JIT` (see the ArgPos/ArgKw arm). Flushing `ins.a` here
                // strips the marker off the staged argument.
                | Op::ArgBegin
                | Op::ArgPos
                | Op::ArgKw
        );
        let r = self.emit_ins_body(builder, code, ip, ins);
        if !is_load_local {
            self.self_regs.remove(&(ins.a as usize));
        }
        if !preserve {
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
                // A name that shares a frame-local slot (declared locals,
                // including a parameter re-declared by `n = ...`) is bound
                // to that slot at run time by the frame's preseed merge —
                // never to the ABI parameter. Route through the local slot
                // or every iteration reads the stale call-time argument.
                if let Some(&slot) = self.local_name_to_slot.get(&name) {
                    return self.emit_load_local(builder, ins.a as usize, slot);
                }
                // NB: call_candidate_regs is populated by the match below
                // (it knows the name's param/global identity).
                self.call_candidate_regs.remove(&(ins.a as usize));
                self.class_ctor_regs.remove(&(ins.a as usize));
                self.instance_regs.remove(&(ins.a as usize));
                self.method_regs.remove(&(ins.a as usize));
                self.intlist_regs.remove(&(ins.a as usize));
                self.len_regs.remove(&(ins.a as usize));
                self.list_add_regs.remove(&(ins.a as usize));
                self.param_regs.remove(&(ins.a as usize));
                // The builtin `len` name lowers to a bare marker: the call
                // site can special-case an IntVec argument. Values for it
                // never materialise at the FFI boundary.
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
                        // A list-typed argument arrives as a raw arena
                        // handle, but its runtime identity is unknown at
                        // compile time. The list intrinsics probe the
                        // handle lazily at their use sites (see
                        // `emit_intlist_target`), so the register itself
                        // stays unmarked and cannot poison the arithmetic
                        // bail checks used by the compare/mul arms. `-> xs`
                        // returns go through the `fid_list_is_handle`
                        // probe in the `Op::Return` arm.
                        builder.def_var(self.rv_i64(ins.a as usize), v);
                        self.is_bool[ins.a as usize] = false;
                        self.is_none[ins.a as usize] = false;
                        self.is_float[ins.a as usize] = false;
                        self.ret_slot[ins.a as usize] = None;
                        Some(())
                    }
                    None => {
                        // The global `math` module lowers to a bare marker
                        // (a param named `math` never reaches this arm).
                        // `GetAttr <fn>` binds the member intrinsic marker,
                        // and `Op::Call` lowers the call to an f64 FFI.
                        // Values for it never materialise at the FFI boundary.
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
                        // Program-declared class constructor: emit a
                        // compile-time constructor token. `Op::Call` on
                        // this register lowers to `un_class_make_<N>`.
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
                // scope/frame bookkeeping markers ? no register effect in Int JIT
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
                // math intrinsic: `math.<fn>` on the `math` marker binds the
                // callee marker that `Op::Call` lowers to an f64 FFI call.
                // The receiver must be exactly the `math` module marker from
                // `LoadName "math"`; anything else falls through to the
                // class/method paths below.
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
                        // `math.<unknown>` - never matches the Call arm's
                        // math branch, and never falls through to the
                        // class-field path (receiver isn't a class self).
                        // The only safe outcome is interpreter.
                        None => return None,
                    }
                }
                // The destination is rebound here: stale identities are
                // dropped before any branch re-marks it. Capture the
                // receiver marker first ??`q = p.norm()` keeps `p`'s
                // identity; `p = p.norm()` overwrites it and re-marks the
                // destination inside the branch.
                let inst_recv = self.instance_regs.get(&(ins.b as usize)).copied();
                // IntVec method: `xs.add(v)` lowers to a direct marker; the
                // `Call` arm turns it into `un_list_int_push`.
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
                        // Keep the receiver pointer live in the destination
                        // variable: `p.norm()` hands it to the direct call
                        // as hidden self, and it must round-trip through
                        // `Unwrap` etc. unchanged.
                        let ptr = builder.use_var(self.rv_i64(ins.b as usize));
                        builder.def_var(self.rv_i64(ins.a as usize), ptr);
                        self.instance_regs.insert(ins.a as usize, desc_id);
                        // The raw pointer moves with the value, so the bound
                        // callable sits on `ins.a` itself.
                        self.method_regs.insert(ins.a as usize, (desc_id, proto_idx, ins.a as usize));
                        self.is_bool[ins.a as usize] = false;
                        self.is_none[ins.a as usize] = false;
                        self.is_float[ins.a as usize] = false;
                        self.ret_slot[ins.a as usize] = None;
                        return Some(());
                    }
                    return None;
                }
                // Class-layout-backed direct field access ??only for class
                // methods with a name-checked receiver (see `self_regs`). The
                // receiver is verified to be the live instance of this proto's
                // class before the layout index is used; anything else means
                // the safest correct behaviour is to bail to the interpreter.
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
                // Map the field-name-at-index-d to a field index into `layout`.
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
                // The receiver pointer comes from the register that loaded
                // `self`; the member table is name-checked AT THE GIVEN INDEX
                // before any read happens. A mismatch means the layout and
                // the live member order disagree ??bail instead of reading.
                let f_ok = self.module.declare_func_in_func(self.fid_int_ok_name, builder.func);
                let self_ptr = builder.use_var(self.rv_i64(ins.b as usize));
                let e_idx = builder.ins().iconst(types::I64, field_ix as i64);
                // The native code holds this pointer past the end of the
                // translation; leak the string so it outlives the module.
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
                // Value must be Int: the FFI only carries i64 payloads, so a
                // Float/Bool/None source can't be stored through the
                // name-checked field intrinsic; bail to the interpreter.
                if self.is_float[ins.c as usize] || self.is_bool[ins.c as usize] || self.is_none[ins.c as usize] {
                    return None;
                }
                // An IntDict id stored into a field would be silently stored
                // as a raw int ??reject.
                if self.int_dict_seen(&[ins.c as usize]) {
                    return None;
                }
                // The write verifies the member name at the field index and
                // its Int-ness (or appends at the layout-seeded append
                // position); any mismatch bails to the interpreter instead
                // of silently overwriting a wrong field. One intrinsic call,
                // one branch: the append-time order is exactly the seeded
                // layout order, so the push maintains it byte-for-byte.
                let f_ok = self.module.declare_func_in_func(self.fid_int_set_name, builder.func);
                // The native code holds this pointer past the end of the
                // translation; leak the string so it outlives the module.
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
                    return None;
                }
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
                // Int-only, exact two's-complement i64 semantics (mirrors
                // `bit()` in runtime/obj.rs for `int & int`). Bool/None/Float
                // operands bail to the interpreter, which raises
                // "unsupported operand type(s)" for those shapes ??computing
                // anything here would diverge from the interpreter.
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
                // The destination register is rebound here: stale markers
                // are dropped before any branch re-marks it. When the
                // callee sits in the same register, that marker is the
                // *input* (e.g. the class token) and is consumed by the
                // branches below instead.
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
                // Class constructor (`Point(i, i + 1)`): lower to the
                // un_class_make intrinsic.
                if let Some(&desc_id) = self.class_ctor_regs.get(&callee_reg) {
                    return self.emit_class_make(builder, ins, argc, callee_reg, desc_id);
                }
                // Global `len(xs)` on an IntVec handle.
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
                    // `-1` means "not a live handle"; bail the frame to the
                    // interpreter rather than bind a bogus length.
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
                // math intrinsic: `math.<fn>(...)` on the member marker.
                if let Some(&mfn) = self.mathfn_regs.get(&callee_reg) {
                    return self.emit_math_call(builder, ins, argc, callee_reg, mfn);
                }
                // `xs.add(v)` marker from a preceding GetAttr.
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
                        // Plain Int: never let a user Int >= ID_BASE alias an
                        // arena handle and trigger the outer-row promotion.
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
                // Bound class-method of a known instance (`p.norm()`):
                // direct native call into the method proto.
                if let Some(&(_, proto_index, recv_reg)) = self.method_regs.get(&callee_reg) {
                    return self.emit_class_method_call(builder, ins, argc, proto_index, recv_reg);
                }
                // User JIT-eligible callee: call it.
                if let Some(&proto_index) = self.call_candidate_regs.get(&callee_reg) {
                    if self.decls.contains_key(&proto_index) {
                        return self.emit_user_call(builder, ins, argc, callee_reg, proto_index);
                    }
                }
                None
            }
            Op::ToBool => {
                if self.int_dict_seen(&[ins.b as usize]) {
                    return None;
                }
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
            Op::Neg => {
                let a = ins.a as usize;
                let b = ins.b as usize;
                // Bool / none payloads, and arena handles (list/dict/class
                // markers), are not numbers. Bail to the interpreter, which
                // raises the exact error ("unsupported operand type(s) for -").
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
                // Int: wrapping negation (matches the interpreter's `wrapping_neg`).
                let v = builder.use_var(self.rv_i64(b));
                let n = builder.ins().ineg(v);
                builder.def_var(self.rv_i64(a), n);
                self.is_bool[a] = false;
                self.is_none[a] = false;
                self.is_float[a] = false;
                self.ret_slot[a] = None;
                Some(())
            }
            Op::ArgBegin => Some(()), // frame marker: no register effect
            Op::ArgPos | Op::ArgKw => {
                // Marks an argument as positional/keyword in the arg frame.
                // No register effect in the JIT.
                Some(())
            }
            Op::Idiv | Op::Mod => {
                if self.int_dict_seen(&[ins.b as usize]) {
                    return None;
                }
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
                if self.int_dict_seen(&[ins.b as usize, ins.c as usize]) {
                    return None;
                }
                let b_float = self.is_float[ins.b as usize];
                let c_float = self.is_float[ins.c as usize];
                if b_float || c_float {
                    // Float compare: Int-only operand is promoted with fcvt;
                    // NaN semantics match Rust (ordered predicates, false for <, <=).
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
                    return None;
                }
                if self.local_is_intdict[dst] || self.local_is_intdict[x] || self.local_is_intdict[y] {
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
                // Int-Int dictionary kernel: inline `un_dict_int_empty()` and,
                // for a literal dict being built right here, one
                // `un_dict_int_set` per [key, value] pair. String/Float keys
                // decline into the interpreter (a pure IntDict can't key them).
                let n = ins.d as usize;
                // An out-of-range entry count signals a corrupt plan; bail.
                if n > 256 {
                    return None;
                }
                let a = ins.a as usize;
                // All key AND Int-only value operands must be Int-typed with no
                // Bool/Float/None markers in the register-kind map.
                for i in 0..2 * n {
                    let r = a + i;
                    if self.is_float[r] || self.is_bool[r] || self.is_none[r] {
                        return None;
                    }
                    if self.int_dict_regs.contains(&r) {
                        return None;
                    }
                }
                // Call un_dict_int_empty() -> i64 (id)
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
                // IntVec fast path: known IntVec receiver, Int key and an
                // Int/Bool value. Anything else bails to the interpreter.
                let value = ins.a as usize;
                let target = ins.b as usize;
                let key = ins.c as usize;
                // Int-Int dict fast path: target (b) must be a known IntDict id,
                // key (c) and value (a) must be Int. The IntDict index is loaded
                // directly with `rv_i64`.
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
                    // An IntDict id as value or key would be stored as a raw int ??
                    // reject; the id must not leak into the table.
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
                        // Plain Int: never let a user Int >= ID_BASE alias an
                        // arena handle and trigger the outer-row promotion.
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
                    // Stored value flows out as the result; the list takes
                    // care of kind bookkeeping at the arena level.
                    self.is_float[value] = false;
                    self.is_none[value] = false;
                    self.ret_slot[value] = None;
                    return Some(());
                }
                None
            }
            Op::BuildList => {
                // IntVec fast path: lower `xs = []` to a fresh arena handle.
                // Non-empty literals fall back to the interpreter (they need
                // the real `Obj::List` value anyway).
                if ins.d != 0 {
                    if crate::jit_cache::jit_debug() {
                        eprintln!("JITREJECT buildlist n={}", ins.d);
                    }
                    return None;
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
                // IntVec sublist `xs[start:end]` (clamped, like Python). No
                // strided slices in IntVec yet.
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
                // Real Un list slice ??interpreter handles it.
                None
            }
            Op::PrepIter => {
                // IntVec iterator handle. Receiver must be a live IntVec
                // (known statically, or FFI-materialised via the runtime
                // `is_handle` check). The iterator is released in
                // `emit_int_cleanup`.
                let Some(d_i) = self.emit_intlist_target(builder, ins.a as usize) else {
                    return None;
                };
                let fid = self.fid_list_iter_new;
                let f_ref = self.module.declare_func_in_func(fid, builder.func);
                let call = builder.ins().call(f_ref, &[d_i]);
                let handle = builder.inst_results(call)[0];
                // `0` means the receiver is not iterable by the Int-JIT
                // (e.g. a KIND_OUTER nested list); bail to the interpreter.
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
                // IntVec forward iterator. Receiver must be a handle marked
                // by the preceding `PrepIter` arm; other receivers (range
                // loops, general iters) still fall through to the fallback
                // arm or fail candidacy.
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
                // Int-Int dict fast path: target (b) must be a known IntDict id,
                // key (c) and value (a) must be Int. The IntDict index is loaded
                // directly with `rv_i64`.
                if self.int_dict_regs.contains(&target) && !self.is_float[key] && !self.is_bool[key] && !self.is_none[key] {
                    // Probe presence first: the interpreter raises
                    // `key not found` for a missing key, but the JIT cannot
                    // raise — bail to the interpreter so the exact error
                    // semantics are preserved (a `-1` fallback would be a
                    // silently wrong answer).
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
                    builder.def_var(self.rv_i64(dst), res);
                    self.is_bool[dst] = false;
                    self.is_none[dst] = false;
                    self.is_float[dst] = false;
                    self.ret_slot[dst] = None;
                    return Some(());
                }
                // IntVec fast path: target must be a known IntVec receiver
                // (a static marker or a fresh handle the runtime check just
                // verified); any out-of-range index bails to the interpreter.
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
                // The JIT ABI writes the (value, tag) pair through a hidden
                // pointer: tag 0 ??value is an Int result; tag 1 ??Float bits;
                // tag 2 ??runtime bail; tag 3 ??Bool (0/1). Anything else
                // (none, a class instance, an unexpected payload) bails to the
                // interpreter, which runs the function with full semantics.
                // A class instance / bound method / constructor token / self in
                // the return slot is never a valid raw Int ??decline (tag=2).
                if self.instance_regs.contains_key(&(ins.a as usize))
                    || self.method_regs.contains_key(&(ins.a as usize))
                    || self.class_ctor_regs.contains_key(&(ins.a as usize))
                    || self.self_regs.contains(&(ins.a as usize))
                {
                    return None;
                }
                if self.is_none[ins.a as usize] {
                    // An implicit-None return (class `__init__`s, void-ish
                    // functions). Emit `tag = TAG_NONE`; the value cell is
                    // ignored by the caller.
                    let zero = builder.ins().iconst(types::I64, 0);
                    let five = builder.ins().iconst(types::I64, crate::jit_cache::TAG_NONE);
                    self.emit_return_pair(builder, zero, five);
                    return Some(());
                }
                // A union INT|INTLIST register has no static arm: strip the
                // UNION marker so the runtime probe below re-tags the value
                // (Int < ID_BASE stays tag=0, an arena handle emits TAG_INTLIST).
                let was_union =
                    self.intlist_regs.get(&(ins.a as usize)).copied() == Some(INTLIST_UNION);
                if was_union {
                    self.intlist_regs.remove(&(ins.a as usize));
                }
                // An IntVec handle returned as a list ??the caller
                // materialises it back into an `Obj::List`.
                if self.intlist_regs.contains_key(&(ins.a as usize)) {
                    let bits = builder.use_var(self.rv_i64(ins.a as usize));
                    let six = builder.ins().iconst(types::I64, crate::jit_cache::TAG_INTLIST);
                    self.emit_return_pair_keep(builder, bits, six, true);
                    return Some(());
                }
                // The runtime tag probe decides Int vs IntVec. Only ever run
                // it for registers that carry the INT|INTLIST UNION marker
                // from a nested union call, or for a raw parameter register
                // (a borrowed handle re-passed unchanged): every other
                // register is a plain Int (or Float/Bool/none) and a user
                // Int >= ID_BASE must not masquerade as an arena handle.
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
                // An IntDict id returned as tag=0 Int would masquerade as a
                // number for the caller ??decline.
                if self.int_dict_seen(&[ins.a as usize]) {
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

/// Monotone Int??????????????????fixpoint over the register file. Determines which
/// locals hold promoted Float values on at least one execution path.
/// Registers are Float when any operand/source is Float (Un's promotion
/// rule).
// Ret tag bit flags used by analyze_return_kinds.
const RET_INT: u8 = 0x1;
const RET_FLOAT: u8 = 0x2;
const RET_INTLIST: u8 = 0x8;
const RET_BOOL: u8 = 0x4;
/// `intlist_regs`/`local_is_list` marker for an *argument* list handle.
/// The handle is owned by the caller (`jit_fast`): the callee must never
/// free it (the caller copies contents back into the caller's Un list and
/// frees it there). Owned arena handles are marker `7`; marker `8` is the
/// same handle seen through an argument.
const INTLIST_BORROWED: u8 = 8;
/// A heterogeneous `Int | IntVec handle` register (union call result).
/// Every intrinsic still probes the arena id at run time (see
/// `emit_intlist_target`), and `Return` re-tags the value by probing the
/// register instead of blindly emitting `TAG_INTLIST`.
const INTLIST_UNION: u8 = 9;

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
    // 0 = unknown. Starting from 0 (not RET_INT) keeps mutual-recursive
    // bool callees from being stuck at pass-0's stale Int assumption.
    let mut tags: HashMap<usize, u8> = (0..n).map(|i| (i, 0)).collect();
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
                // The tags map must only ever GROW: a callee whose dynamic
                // result any arm produced a handle is at least INTLIST, and a
                // growth step can never be undone by a later pass. Making the
                // step monotone makes the fixpoint terminate and makes the
                // result independent of pass order.
                let union = cur | tags[&proto_index];
                if union != tags[&proto_index] {
                    tags.insert(proto_index, union);
                        changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Final pass: with the fixpoint converged, recompute every proto's kinds
    // from the final tags so a callee whose return type was only resolved
    // in a later pass is not frozen at its stale pass-0 value.
    for proto_index in 0..n {
        let cur_tags = tags.clone();
        if let Some(cur) = analyze_one_proto(program, plan, proto_index, &callee_lookup, &cur_tags) {
            let union = cur | tags[&proto_index];
            tags.insert(proto_index, union);
        }
    }
    if std::env::var_os("UV_JIT_DEBUG").is_some() && std::env::var_os("UV_JIT_DEBUG_IF").is_some() {
        for (i, t) in &tags {
            eprintln!("JITDBG retkinds proto={i} kinds={t:#x}");
        }
    }
    tags
}

/// Bitmask of parameter positions whose values can reach an operation that
/// reads them as plain Ints (arithmetic, bitwise, compare, string
/// materialisation, error propagation, unary negation ...). A value in one
/// of those positions that was actually passed as a borrowed IntVec arena
/// handle (see the hidden `param_tags` slot) must bail the frame to the
/// interpreter; everything else is genuinely Int-safe.
///
/// This is a simple forwards flow over the register file (locals first,
/// then scratch slots); it does not model control flow exactly, but it is
/// monotone and free of false negatives relative to that walk. False
/// positives only cost one extra interpreter frame.
/// `param_mask_by_d` maps `LoadName` name indices (as they appear in
/// `ins.d`) to the parameter position they stage; built once per proto by
/// the caller.
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
                // The param staging arms of the JIT lower `LoadName <param>`
                // by reading the FFI argument variable; the position is
                // carried in `param_mask_by_d`.
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
                // The callee is an argument-naked name; the arguments
                // follow it in consecutive registers (b+1..b+d).
                let mut m = 0u64;
                for k in 0..ins.d.max(0) as usize {
                    if b + 1 + k < 256 {
                        m |= reg_taint[b + 1 + k];
                    }
                }
                reg_taint[a] = m;
            }
            Op::GetItem | Op::Slice | Op::GetAttr => {
                // Elements of a *flat* IntVec are plain Ints; outer lists
                // are out of the Int-JIT's supported shape and bail at the
                // intrinsic probes themselves.
                reg_taint[a] = 0;
            }
            Op::LoadUpv => {
                // Upvalues may capture anything outside this ABI; be strict
                // and mark them so downstream arithmetic bails.
                reg_taint[a] = u64::MAX;
            }
            _ => {
                // Potentially Int-consuming operation: if a parameter-derived
                // value reaches it, this frame must not run natively on a
                // handle.
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

/// Every operation that *reads a register as an Int* (arithmetic, bitwise,
/// compare, conversions, string materialisation, error propagation). Anything
/// not in `unsafe_param_mask`'s explicit-match arms above is conservative by
/// default here, but note the explicitly listed intrinsic-shaped arms are
/// free of Int assumptions (they probe the arena at run time).
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
    let n_regs = code.iter().map(|i| i.a as usize).max().unwrap_or(0) + 1;
    let n_locals = proto.locals.len();
    let mut reg_float = vec![false; n_regs];
    let mut reg_bool = vec![false; n_regs];
    let mut local_float = vec![false; n_locals];
    let mut local_bool = vec![false; n_locals];
    let mut reg_list = vec![false; n_regs];
    let mut local_list = vec![false; n_locals];
    let mut local_param = vec![false; n_locals];
    // Which local slot a register was loaded from (covers the canonical
    // `LoadLocal/Dup/StoreLocal` round-trips for list-typed locals). Used at
    // `Return` to attribute list-ness back onto the returned register.
    let mut reg_slot: Vec<Option<usize>> = vec![None; n_regs];
    // `param_regs[r]` = the register was populated by a `LoadName` of a
    // parameter (or is now a copy/read of such a register). Used at
    // `Op::Return` to attribute list-capability to parameter forwarding
    // (`-> xs`). A rebind clears the bit.
    let mut param_regs: Vec<bool> = vec![false; n_regs];
    // Which proto a register currently holds, resolved via LoadName;
    // consumed by Op::Call to propagate the callee's return kinds to the
    // call's destination register. Any op that rebinds the register clears
    // the entry (see the loop head below).
    let mut reg_call: Vec<Option<usize>> = vec![None; n_regs];
    // Local slot -> name for every slot the proto ever binds. Used at
    // `LoadName` to remember which slot the name resolves to, so `Return`
    // can attribute list-ness back through the slot (xs -> local 0).
    // Parameters are recorded on the same map with a sentinel `0usize - 1 - ix`
    // (never a real slot, because proto.locals.len() is much smaller); the
    // analyzer only distinguishes "name not found" from a real slot index,
    // so any consistent hashable key works.
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

    // Coarse-but-sound extrapolated claim: a proto whose body contains
    // BuildList produces lists at some point in its own execution, so the
    // outside world should be prepared for a list handle to come back. The
    // `Return` emission probes the actual register content at runtime when
    // the register carries no static marker, so claiming INTLIST
    // unconditionally is safe; surviving Ints/Floats take their usual
    // paths, and callers always see the tag anyway. The inner fixpoint
    // still wins for callee-tag visibility (mutual recursion bootstraps
    // this way).
    let claims_intlist = code.iter().any(|ins| ins.op == Op::BuildList);

    // Inner fixpoint: propagate register/local kinds to fixpoint in this proto.
    for _inner in 0..8 {
        let mut gchanged = false;
        for (_ip, ins) in code.iter().enumerate() {
            // Clear stale provenance identities from any register rebound by
            // a non-LoadName op (including intrinsic producers like
            // BuildList). `LoadName`/`Dup`/`StoreLocal`/`Call` maintain
            // these themselves in their arms. `param_regs` is set only at
            // `LoadName` (a parameter is referenced) and is preserved here,
            // so `Return` can see through the clears — that is what the
            // parameter-forwarding test relies on.
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
                            // List-typed register straight from a list
                            // producer. Anything else (including parameter
                            // forwards, whose runtime value the compiler
                            // cannot see) is typed INT at plan time and the
                            // Return emission probes it at run time.
                            kinds |= RET_INTLIST;
                        } else {
                            kinds |= RET_INT;
                        }
                        if std::env::var_os("UV_JIT_DEBUG_IF").is_some() {
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
                    // An empty list literal produces a fresh IntVec handle.
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
                    // A slice of a list-typed receiver is a list handle.
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
                        let tag = tags.get(&callee).copied().unwrap_or(0);
                        // Conservative: mark the destination as possibly a
                        // list handle whenever the callee can return one.
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
                    // If the name is a known callee in this program, borrow its kinds
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
                        let tag = tags.get(&callee).copied().unwrap_or(0);
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
    // Note: a parameter-forward-only return may still produce a list handle
    // at run time; the Return emission probes the register and tags=6 vs 0,
    // so a homogeneous RET_INT claim here is safe — a mismatched dynamic arm
    // costs one bail, never a wrong answer.
    if std::env::var_os("UV_JIT_DEBUG").is_some() && std::env::var_os("UV_JIT_DEBUG_IF").is_some() {
        eprintln!("JITDBG analyze_one proto={proto_index} kinds={kinds:#x}");
    }
    if std::env::var_os("UV_JIT_DEBUG_IF").is_some() {
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
                    // Track the `math` module marker; the math call lowering is
                    // typed in the arms below. A param named `math` compiles to
                    // a param path at emission and the proto is rejected there,
                    // so over-marking here only widens the poly window — safe.
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
                    // Register move with unchanged float-kind; nothing to track.
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
                        // Int-preserving for Int args, and the Int path is
                        // identity; Float args produce Float.
                        Some(MathFn::Floor | MathFn::Ceil | MathFn::Trunc) => reg_float[callee + 1],
                        // Int in → bail at emission time, so whatever is emitted
                        // stays Float *or* the proto is declined (caption needed
                        // nowhere else); sign is always Int.
                        Some(MathFn::Abs) => reg_float[callee + 1],
                        Some(MathFn::Sign) => false,
                        // sin/cos/tan/asin/acos/atan/sqrt/cbrt/exp/ln/log2/log10/log/hypot: always Float.
                        // pow: both-Int declines at emission (proto rejected), else Float.
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

/// Cheap pre-check that a proto is in the supported subset *shape* (params
/// only). Does not try to compile; used for up-front module declaration.
/// Class methods additionally require a hidden `self` Int arg at slot 0 ??
/// this bumps the returned arity by one.
pub fn precheck_fn(program: &Program, proto_index: usize) -> Option<usize> {
    let proto = program.protos.get(proto_index)?;
    if proto.flags & PROTO_FUNCTION == 0 {
        return None;
    }
    // Local slots named `field` hold Un `FieldRef`s inside property
    // accessors; the JIT cannot model them (a store would silently
    // overwrite the FieldRef instead of writing through to the property
    // backing). Disqualify the proto here, before any codegen is attempted.
    // `proto.locals` indices are proto-local name indices: resolve through
    // `proto.names` before hitting `program.names`.
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
    if proto.ext_type.is_some() {
        // Class methods carry the receiver as an extra hidden first argument
        // (arity = params.len() + 1) and bind local slot 0 to it; `self` reads
        // are `LoadLocal(0)`. The attr-field intrinsic verifies the member
        // name at the given field index against the expected name at call
        // time; a miss bails to the interpreter (tag = 2) instead of reading
        // or writing a wrong field. Non-Int field values are not admitted by
        // the JIT either, so Int `self.f` reads observe the interpreter's own
        // member order.
    }
    let arity = proto.params.len() + usize::from(proto.ext_type.is_some());
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

fn emit_fn_in_module(
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

    // ---------- pass 2c: Int-promotion fixpoint ----------
    let local_float = compute_local_float(code, proto, program);
    // If the function contains no user-call sites, there is no chance of
    // infinite recursion through the JIT, so skip the depth bookkeeping on
    // entry/exit. Otherwise take the safe path: a user-call site may recurse
    // (fib/sum), so guard against native stack overflow.
    let has_user_call = code.iter().any(|ins| ins.op == Op::Call);

    // ---------- pass 3: build the Cranelift context ----------
    let mut fun_ctx = module.make_context();
    let mut func_ctx = FunctionBuilderContext::new();

    fun_ctx.func.signature = sig;
    fun_ctx.func.name = cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32());

    let mut builder = FunctionBuilder::new(&mut fun_ctx.func, &mut func_ctx);
    let locals_len = proto.locals.len();
    // Local slots bound to a `field` name are FieldRef holders (see the
    // `field_locals` field on `Tr`). Compute once; `LoadLocal`/`StoreLocal`
    // bail on sight of one. `proto.locals` indices are proto-local name
    // indices: resolve through `proto.names` before `program.names`.
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

    // Build the per-proto unsafe-param mask (compile time only): parameter
    // positions that can reach an Int-consuming operation must bail the
    // frame when the caller passed a borrowed IntVec handle there.
    let param_mask_by_d: HashMap<usize, usize> = {
        let mut m = HashMap::new();
        // Map the proto-local name indices used as LoadName operands onto
        // the parameter positions. LoadName staging uses the same names as
        // the FFI argument order, so the pos values line up directly.
        for (name, pos) in param_pos.iter() {
            for (local_idx, &global_idx) in proto.names.iter().enumerate() {
                if program.names.get(global_idx).is_some_and(|n| n == name) {
                    m.insert(local_idx, *pos);
                }
            }
        }
        m
    };
    let unsafe_param_mask = unsafe_param_mask(&code, &param_mask_by_d);

    let prologue = builder.create_block();
    builder.append_block_params_for_function_params(prologue); // hidden result ptr, tags, args...
    let incoming = builder.block_params(prologue).to_vec();
    let hidden_ptr = incoming[0];
    let incoming_tags = incoming[1];
    // Cranelift's layout entry block (the one in position 0) must carry the
    // function signature's parameters, so the prologue is that block. The
    // body starts at `entry`, an empty parameterless block; the prologue
    // terminates with a jump into it (no arguments).
    let entry = builder.create_block();
    // The prologue writes into its own block and jumps into `entry`, so the
    // body block starts pristine. Cranelift's debug verifier requires the
    // current block to be empty/terminated/unreachable before a
    // switch_to_block; a partial prologue block would fail that check.
    builder.switch_to_block(prologue);
    // keep the hidden pointer in a dedicated variable so returns can store
    let hp_var = builder.declare_var(types::I64);
    builder.def_var(hp_var, hidden_ptr);
    let tags_var = builder.declare_var(types::I64);
    builder.def_var(tags_var, incoming_tags);
    for (i, v) in incoming.iter().enumerate().skip(2) {
        builder.def_var(vars[i - 2], *v);
    }
    // If any parameter position that can reach an Int-consuming operation
    // was passed a borrowed IntVec handle, bail the whole frame to the
    // interpreter. A single runtime check per call; zero cost for Int FFI.
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
        // bail path: tag the hidden pair and return (the interpreter
        // re-runs the frame from scratch).
        builder.switch_to_block(bail_blk);
        let zero2 = builder.ins().iconst(types::I64, 0);
        let two = builder.ins().iconst(types::I64, crate::jit_cache::TAG_BAIL);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), zero2, hidden_ptr, 0);
        builder.ins().store(cranelift_codegen::ir::MemFlagsData::new(), two, hidden_ptr, 8);
        builder.ins().return_(&[]);
        builder.switch_to_block(cont_blk);
    }
    let zero = builder.ins().iconst(types::I64, 0);
    for i in arity..vars.len() {
        // Class-method local slot 0 is `self`; pin it to the receiver.
        if is_class && i == arity {
            builder.def_var(vars[i], incoming[2]);
            continue;
        }
        builder.def_var(vars[i], zero);
    }
    // A local sharing its name with a parameter is the SAME binding slot in
    // the VM (the frame preseed merges param and local onto one slot), so its
    // variable must start with the FFI argument, not zero — otherwise a loop
    // whose carried variable is first written inside the body (e.g.
    // `while n > 0` on a parameter Cranelift reads as uninitialised) reads
    // Cranelift's undef/zero instead of the argument.
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
        // no depth guard inserted ??jump straight into the body block
        builder.ins().jump(entry, &[]);
        entry_blk = entry;
    }

    // Program-declared classes: one constructor descriptor per class, plus
    // the method targets for the bound-method fast path. Descriptors are
    // process-global (deduplicated by name) and live until
    // `jit_cache::clear()`.
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















