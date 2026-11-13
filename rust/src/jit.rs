//! Cranelift JIT plumbing. Kept separate from the interpreter so it can
//! evolve into the `unvm run --jit=<file.un>` path / a full `.unbc` JIT.
//!
//! Phase A: i64-only probe ??proves Cranelift emits working code on this
//! Windows/GNU toolchain and that the vendored `cranelift-jit` runs.

use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::BlockArg;
use cranelift_frontend::FunctionBuilder;
use cranelift_frontend::FunctionBuilderContext;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_native::builder as native_builder;

/// Return a pointer to the JIT-compiled `nested_sum(size: i64) -> i64`
/// equivalent to the Un function:
///   fn nested_sum(size) total = 0 for i in iter.range(1, size + 1)
///       for j in iter.range(1, size + 1) total += i * j
///   -> total
///
/// This is Phase A-2 ??the i64 hot-loop benchmark path, compiled natively.
pub fn nested_sum_fn() -> Result<unsafe extern "C" fn(i64) -> i64, ()> {
    let native = native_builder().map_err(|_| ())?;
    let mut flag_builder = settings::builder();
    let _ = flag_builder.set("is_pic", "false");
    let _ = flag_builder.set("enable_alias_analysis", "true");

    let isa = native
        .finish(settings::Flags::new(flag_builder))
        .map_err(|_| ())?;
    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));

    let mut ctx = module.make_context();
    let mut func_ctx = FunctionBuilderContext::new();

    let mut sig_a = module.make_signature();
    sig_a.params.push(cranelift_codegen::ir::AbiParam::new(types::I64));
    sig_a.returns.push(cranelift_codegen::ir::AbiParam::new(types::I64));

    let func_a = module
        .declare_function("nested_sum_jit", Linkage::Export, &sig_a)
        .map_err(|_| ())?;
    ctx.func.signature = sig_a;
    ctx.func.name = cranelift_codegen::ir::UserFuncName::user(0, func_a.as_u32());

    {
        let mut bcx = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);

        let entry = bcx.create_block();
        let outer_header = bcx.create_block();
        let outer_body = bcx.create_block();
        let inner_header = bcx.create_block();
        let inner_body = bcx.create_block();
        let inner_exit = bcx.create_block();
        let outer_exit = bcx.create_block();

        bcx.append_block_params_for_function_params(entry);
        // entry(size)
        let size = bcx.block_params(entry)[0];

        bcx.switch_to_block(entry);
        bcx.seal_block(entry);
        let zero = bcx.ins().iconst(types::I64, 0);
        let one = bcx.ins().iconst(types::I64, 1);
        bcx.ins().jump(outer_header, &[BlockArg::Value(zero), BlockArg::Value(one)]);

        // outer_header(total, i)
        let total = bcx.append_block_param(outer_header, types::I64);
        let i = bcx.append_block_param(outer_header, types::I64);
        bcx.switch_to_block(outer_header);
        bcx.seal_block(outer_header);
        let cmp = bcx.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThan, i, size);
        bcx.ins().brif(cmp, outer_exit, &[BlockArg::Value(total)], outer_body, &[BlockArg::Value(total), BlockArg::Value(i)]);

        // outer_body(total, i) ??preheader for the inner loop
        let ob_total = bcx.append_block_param(outer_body, types::I64);
        let ob_i = bcx.append_block_param(outer_body, types::I64);
        bcx.switch_to_block(outer_body);
        bcx.seal_block(outer_body);
        let one2 = bcx.ins().iconst(types::I64, 1);
        bcx.ins().jump(inner_header, &[BlockArg::Value(ob_total), BlockArg::Value(ob_i), BlockArg::Value(one2)]);

        // inner_header(total, i, j)
        let ih_total = bcx.append_block_param(inner_header, types::I64);
        let ih_i = bcx.append_block_param(inner_header, types::I64);
        let ih_j = bcx.append_block_param(inner_header, types::I64);
        bcx.switch_to_block(inner_header);
        bcx.seal_block(inner_header);
        let cmp2 = bcx.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThan, ih_j, size);
        bcx.ins().brif(cmp2, inner_exit, &[BlockArg::Value(ih_total), BlockArg::Value(ih_i)], inner_body, &[BlockArg::Value(ih_total), BlockArg::Value(ih_i), BlockArg::Value(ih_j)]);

        // inner_body(total, i, j)
        let ib_total = bcx.append_block_param(inner_body, types::I64);
        let ib_i = bcx.append_block_param(inner_body, types::I64);
        let ib_j = bcx.append_block_param(inner_body, types::I64);
        bcx.switch_to_block(inner_body);
        bcx.seal_block(inner_body);
        let prod = bcx.ins().imul(ib_i, ib_j);
        let total_new = bcx.ins().iadd(ib_total, prod);
        let one3 = bcx.ins().iconst(types::I64, 1);
        let j_new = bcx.ins().iadd(ib_j, one3);
        bcx.ins().jump(inner_header, &[BlockArg::Value(total_new), BlockArg::Value(ib_i), BlockArg::Value(j_new)]);

        // inner_exit(total, i)
        let ie_total = bcx.append_block_param(inner_exit, types::I64);
        let ie_i = bcx.append_block_param(inner_exit, types::I64);
        bcx.switch_to_block(inner_exit);
        bcx.seal_block(inner_exit);
        let one4 = bcx.ins().iconst(types::I64, 1);
        let i_new = bcx.ins().iadd(ie_i, one4);
        bcx.ins().jump(outer_header, &[BlockArg::Value(ie_total), BlockArg::Value(i_new)]);

        // outer_exit(total)
        let oe_total = bcx.append_block_param(outer_exit, types::I64);
        bcx.switch_to_block(outer_exit);
        bcx.seal_block(outer_exit);
        bcx.ins().return_(&[oe_total]);

        bcx.seal_all_blocks();
        bcx.finalize(module.target_config());
    }
    module.define_function(func_a, &mut ctx).map_err(|_| ())?;

    module
        .finalize_definitions()
        .map_err(|_| ())?;
    let code_a = module.get_finalized_function(func_a);
    Ok(unsafe { std::mem::transmute::<_, unsafe extern "C" fn(i64) -> i64>(code_a) })
}

/// Return a pointer to the JIT-compiled `add(a: i64, b: i64) -> i64`.
/// Uses Cranelift's `JITModule` machinery via the vendored raw-FFI crate.
pub fn add_fn() -> Result<unsafe extern "C" fn(i64, i64) -> i64, ()> {
    let native = native_builder().map_err(|_| ())?;
    let mut flag_builder = settings::builder();
    let _ = flag_builder.set("is_pic", "false");
    let _ = flag_builder.set("enable_alias_analysis", "true");

    let isa = native
        .finish(settings::Flags::new(flag_builder))
        .map_err(|_| ())?;
    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));

    let mut ctx = module.make_context();
    let mut func_ctx = FunctionBuilderContext::new();

    let mut sig_a = module.make_signature();
    sig_a.params.push(cranelift_codegen::ir::AbiParam::new(types::I64));
    sig_a.params.push(cranelift_codegen::ir::AbiParam::new(types::I64));
    sig_a.returns.push(cranelift_codegen::ir::AbiParam::new(types::I64));

    let func_a = module
        .declare_function("add", Linkage::Local, &sig_a)
        .map_err(|_| ())?;
    ctx.func.signature = sig_a;
    ctx.func.name = cranelift_codegen::ir::UserFuncName::user(0, func_a.as_u32());

    {
        let mut bcx = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
        let block = bcx.create_block();

        bcx.switch_to_block(block);
        bcx.append_block_params_for_function_params(block);
        let params = bcx.block_params(block).to_vec();
        let sum = bcx.ins().iadd(params[0], params[1]);
        bcx.ins().return_(&[sum]);
        bcx.seal_all_blocks();
        bcx.finalize(module.target_config());
    }
    module.define_function(func_a, &mut ctx).map_err(|_| ())?;

    module
        .finalize_definitions()
        .map_err(|_| ())?;
    let code_a = module.get_finalized_function(func_a);
    Ok(unsafe { std::mem::transmute::<_, unsafe extern "C" fn(i64, i64) -> i64>(code_a) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jit_add_runs() {
        let f = add_fn().expect("jit add");
        assert_eq!(unsafe { f(3, 4) }, 7);
        assert_eq!(unsafe { f(0, 0) }, 0);
        assert_eq!(unsafe { f(i64::MAX, 1) }, i64::MIN); // wrapping
    }

    #[test]
    fn jit_nested_sum_runs() {
        let f = nested_sum_fn().expect("jit nested_sum");
        assert_eq!(unsafe { f(0) }, 0);
        assert_eq!(unsafe { f(1) }, 1);
        assert_eq!(unsafe { f(2) }, 9);     // 1*1 + 1*2 + 2*1 + 2*2 = 9
        assert_eq!(unsafe { f(3) }, 36);    // (1+2+3)^2
        assert_eq!(unsafe { f(200) }, 404010000);
        let start = std::time::Instant::now();
        let sum: i64 = unsafe { f(200) };
        let dt = start.elapsed();
        eprintln!("jit_nested_sum(200) = {sum} in {dt:?}");
    }
}



