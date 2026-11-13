//! Translate an Un AST function to native `fn(i64) -> i64`.
//!
//! Supported subset (i64 only, zero objects):
//!   - one parameter typed at runtime as int
//!   - integer assignments (`target = Integer | Identifier | Binary`)
//!   - integer binary ops `+ - * / // %` and comparisons `< <= > >= == !=`
//!   - `for i in iter.range(min, max)` loops (lowers to an `i64` loop, `i < max`)
//!   - `return expr`
//!
//! Anything outside this subset returns `None` and the caller falls back to
//! the interpreter.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
use cranelift_codegen::ir::{AbiParam, InstBuilder, UserFuncName, types};
#[cfg(not(target_arch = "wasm32"))]
use cranelift_codegen::settings::{self, Configurable};
#[cfg(not(target_arch = "wasm32"))]
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
#[cfg(not(target_arch = "wasm32"))]
use cranelift_jit::{JITBuilder, JITModule};
#[cfg(not(target_arch = "wasm32"))]
use cranelift_module::{Linkage, Module, default_libcall_names};
#[cfg(not(target_arch = "wasm32"))]
use cranelift_native::builder as native_builder;

#[cfg(not(target_arch = "wasm32"))]
use crate::ast::{Const, Node, NodeKind};
#[cfg(target_arch = "wasm32")]
use crate::ast::Node;
use crate::source::Source;
#[cfg(not(target_arch = "wasm32"))]
use crate::token::TokenType;

/// Compile the Un function represented by `fn_node` to native code.
/// The compiled function follows the Un ABI `fn(i64) -> i64`.
/// Returns a pointer to the JIT-compiled function as an `extern "C"` fn pointer.
pub fn try_compile_int_fn(source: &Source, fn_node: &Node) -> Option<unsafe extern "C" fn(i64) -> i64> {
    let _ = (source, fn_node);

    #[cfg(target_arch = "wasm32")]
    {
        return None;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        jit_impl(source, fn_node)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn jit_impl(source: &Source, fn_node: &Node) -> Option<unsafe extern "C" fn(i64) -> i64> {
    // --- 1. detect param ---
    let mut param_name = None::<String>;
    let mut body_node: Option<&Node> = None;

    for child in &fn_node.children {
        match child.kind {
            NodeKind::Tuple => {
                for p in &child.children {
                    if p.kind == NodeKind::Parameter {
                        for q in &p.children {
                            if q.kind == NodeKind::Identifier {
                                param_name = Some(source.span(q.start, q.length));
                            }
                        }
                    }
                }
            }
            NodeKind::Block => {
                body_node = Some(child);
            }
            _ => {}
        }
    }

    let param_name = param_name?;
    let body = body_node?;

    let mut locals_set = std::collections::BTreeSet::<String>::new();
    collect_locals(source, &param_name, body, &mut locals_set);
    let locals: Vec<String> = locals_set.into_iter().collect();

    let native = native_builder().unwrap_or_else(|_| panic!("jit: native_builder failed"));
    let mut flag_builder = settings::builder();
    let _ = flag_builder.set("is_pic", "false");
    let _ = flag_builder.set("enable_alias_analysis", "true");
    let isa = native.finish(settings::Flags::new(flag_builder)).ok()?;

    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let mut ctx = module.make_context();
    let mut func_ctx = FunctionBuilderContext::new();

    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));

    let func_id = module
        .declare_function("un_int_fn", Linkage::Export, &sig)
        .ok()?;
    ctx.func.signature = sig;
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());

    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);

    let mut vars: HashMap<String, Variable> = HashMap::new();
    let param_var = builder.declare_var(types::I64);
    for name in &locals {
        let v = builder.declare_var(types::I64);
        vars.insert(name.clone(), v);
    }

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    let block_params = builder.block_params(entry).to_vec();
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let param_val = *block_params.first()?;
    builder.def_var(param_var, param_val);

    let ok = translate_block(&mut builder, body, source, &vars, param_var, &param_name);
    if !ok {
        return None;
    }

    builder.seal_all_blocks();
    builder.finalize(module.target_config());

    module.define_function(func_id, &mut ctx).ok()?;
    module.finalize_definitions().ok()?;

    let code = module.get_finalized_function(func_id);
    Some(unsafe { std::mem::transmute::<_, unsafe extern "C" fn(i64) -> i64>(code) })
}

#[cfg(not(target_arch = "wasm32"))]
fn collect_locals(source: &Source, name: &str, node: &Node, out: &mut std::collections::BTreeSet<String>) {
    if node.kind == NodeKind::Assign {
        if let Some(first) = node.children.first() {
            if first.kind == NodeKind::Identifier {
                let id = source.span(first.start, first.length);
                if id != name {
                    out.insert(id);
                }
            }
        }
    }
    if node.kind == NodeKind::For {
        if let Some(first) = node.children.first() {
            if first.kind == NodeKind::Identifier {
                let id = source.span(first.start, first.length);
                if id != name {
                    out.insert(id);
                }
            }
        }
    }
    for child in &node.children {
        collect_locals(source, name, child, out);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn translate_block(
    builder: &mut FunctionBuilder,
    block: &Node,
    source: &Source,
    vars: &HashMap<String, Variable>,
    param_var: Variable,
    param_name: &str,
) -> bool {
    for stmt in &block.children {
        let ok = translate_stmt(builder, stmt, source, vars, param_var, param_name);
        if !ok {
            return false;
        }
    }
    true
}

#[cfg(not(target_arch = "wasm32"))]
fn translate_stmt(
    builder: &mut FunctionBuilder,
    stmt: &Node,
    source: &Source,
    vars: &HashMap<String, Variable>,
    param_var: Variable,
    param_name: &str,
) -> bool {
    match stmt.kind {
        NodeKind::Assign => {
            let target = match stmt.children.first() {
                Some(n) if n.kind == NodeKind::Identifier => source.span(n.start, n.length),
                _ => return false,
            };
            let rhs = match stmt.children.get(1) {
                Some(n) => n,
                None => return false,
            };
            let value = match translate_expr(builder, rhs, source, vars, param_var, param_name) {
                Some(v) => v,
                None => return false,
            };
            let var = match vars.get(&target) {
                Some(v) => *v,
                None => param_var,
            };
            builder.def_var(var, value);
            true
        }
        NodeKind::Return => {
            let expr = stmt.children.first();
            let value = match expr {
                Some(n) => match translate_expr(builder, n, source, vars, param_var, param_name) {
                    Some(v) => v,
                    None => return false,
                },
                None => builder.ins().iconst(types::I64, 0),
            };
            builder.ins().return_(&[value]);
            true
        }
        NodeKind::For => translate_for(builder, stmt, source, vars, param_var, param_name),
        NodeKind::Block => translate_block(builder, stmt, source, vars, param_var, param_name),
        _ => false,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn translate_for(
    builder: &mut FunctionBuilder,
    for_node: &Node,
    source: &Source,
    vars: &HashMap<String, Variable>,
    param_var: Variable,
    param_name: &str,
) -> bool {
    let target_node = match for_node.children.first() {
        Some(n) if n.kind == NodeKind::Identifier => n,
        _ => return false,
    };
    let target = match vars.get(&source.span(target_node.start, target_node.length)) {
        Some(v) => *v,
        None => param_var,
    };
    let iterable = match for_node.children.get(1) {
        Some(n) => n,
        None => return false,
    };
    // Expected: Call(Property(Identifier("iter"), Identifier("range")), Tuple(min, max))
    if iterable.kind != NodeKind::Call || iterable.children.len() != 2 {
        return false;
    }
    let callee = &iterable.children[0];
    if callee.kind != NodeKind::Property {
        return false;
    }
    let prop_name = callee
        .children
        .get(1)
        .map(|n| source.span(n.start, n.length));
    if prop_name.as_deref() != Some("range") {
        return false;
    }
    let args_node = match iterable.children.get(1) {
        Some(n) if n.kind == NodeKind::Tuple && n.children.len() == 2 => &n.children,
        _ => return false,
    };
    let min_v = match translate_expr(builder, &args_node[0], source, vars, param_var, param_name) {
        Some(v) => v,
        None => return false,
    };
    let max_v = match translate_expr(builder, &args_node[1], source, vars, param_var, param_name) {
        Some(v) => v,
        None => return false,
    };

    let header = builder.create_block();
    let body = builder.create_block();
    let exit = builder.create_block();

    builder.def_var(target, min_v);
    builder.ins().jump(header, &[]);

    builder.switch_to_block(header);
    let target_value = builder.use_var(target);
    let cond = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::SignedLessThan,
        target_value,
        max_v,
    );
    builder.ins().brif(cond, body, &[], exit, &[]);

    builder.switch_to_block(body);
    builder.seal_block(body);
    let body_node = match for_node.children.get(2) {
        Some(n) if n.kind == NodeKind::Block => n,
        _ => return false,
    };
    let ok = translate_block(builder, body_node, source, vars, param_var, param_name);
    if !ok {
        return false;
    }
    let one = builder.ins().iconst(types::I64, 1);
    let cur = builder.use_var(target);
    let next = builder.ins().iadd(cur, one);
    builder.def_var(target, next);
    builder.ins().jump(header, &[]);

    // header's predecessors are now fully known (entry and body) — seal it.
    builder.seal_block(header);

    builder.switch_to_block(exit);
    builder.seal_block(exit);
    true
}

#[cfg(not(target_arch = "wasm32"))]
fn translate_expr(
    builder: &mut FunctionBuilder,
    expr: &Node,
    source: &Source,
    vars: &HashMap<String, Variable>,
    param_var: Variable,
    param_name: &str,
) -> Option<cranelift_codegen::ir::Value> {
    match expr.kind {
        NodeKind::Integer => {
            let n: i64 = match expr.value.as_ref() {
                Some(Const::Int(v)) => *v,
                _ => return None,
            };
            Some(builder.ins().iconst(types::I64, n))
        }
        NodeKind::Identifier => {
            let name = source.span(expr.start, expr.length);
            let var = if name == param_name {
                param_var
            } else {
                *vars.get(&name)?
            };
            Some(builder.use_var(var))
        }
        NodeKind::Binary => {
            let lhs = translate_expr(builder, &expr.children[0], source, vars, param_var, param_name)?;
            let rhs = translate_expr(builder, &expr.children[1], source, vars, param_var, param_name)?;
            let op = expr.op.unwrap_or(TokenType::Error);
            let value = match op {
                TokenType::Plus => builder.ins().iadd(lhs, rhs),
                TokenType::Minus => builder.ins().isub(lhs, rhs),
                TokenType::Asterisk => builder.ins().imul(lhs, rhs),
                TokenType::Slash | TokenType::DoubleSlash => builder.ins().sdiv(lhs, rhs),
                TokenType::Percent => builder.ins().srem(lhs, rhs),
                TokenType::LessThan => builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedLessThan, lhs, rhs),
                TokenType::LessOrEqual => builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedLessThanOrEqual, lhs, rhs),
                TokenType::GreaterThan => builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThan, lhs, rhs),
                TokenType::GreaterOrEqual => builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThanOrEqual, lhs, rhs),
                TokenType::Equal => builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, lhs, rhs),
                TokenType::Unequal => builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::NotEqual, lhs, rhs),
                _ => return None,
            };
            Some(value)
        }
        _ => None,
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::desugar::desugar;
    use crate::lexer::lex;
    use crate::optimize::optimize;
    use crate::parser::parse;

    fn compile_first_fn(source: &str) -> (std::rc::Rc<Source>, Node) {
        let mut source = Source::new("<test>", &source);
        let tokens = lex(&mut source).expect("lex");
        let source = std::rc::Rc::new(source);
        let node = parse(tokens, source.clone()).expect("parse");
        let root = optimize(&desugar(&node)).expect("optimize").root;
        (source, root)
    }

    fn first_fn(root: &Node) -> &Node {
        root.children
            .iter()
            .find(|c| c.kind == NodeKind::Function)
            .expect("first fn")
    }

    fn jit(source: &str) -> unsafe extern "C" fn(i64) -> i64 {
        let (source, root) = compile_first_fn(source);
        try_compile_int_fn(&source, first_fn(&root)).expect("jit translation")
    }

    #[test]
    fn returns_constant() {
        let f = jit("fn f(x)\n    -> 42");
        assert_eq!(unsafe { f(0) }, 42);
    }

    #[test]
    fn add_add() {
        let f = jit("fn f(a)\n    b = a + 1\n    -> b * 2");
        assert_eq!(unsafe { f(3) }, 8);
    }

    #[test]
    fn for_loop() {
        let f = jit("fn f(n)\n    total = 0\n    for i in iter.range(1, n + 1)\n        total = total + i * i\n    -> total");
        assert_eq!(unsafe { f(3) }, 14);
    }

    #[test]
    fn nested_sum() {
        let text = include_str!("../../bench/loop.un");
        let (source, root) = compile_first_fn(text);
        let func = first_fn(&root);
        let f = try_compile_int_fn(&source, func).expect("jit translation");
        assert_eq!(unsafe { f(6) }, 441);
        assert_eq!(unsafe { f(200) }, 404010000);
    }
}
