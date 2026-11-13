use std::rc::Rc;
use crate::bytecode::Program;
use crate::compiler;
use crate::desugar::desugar;
use crate::error::{LangError, Panic};
use crate::lexer::lex;
use crate::optimize::optimize;
use crate::parser::parse;
use crate::source::Source;
pub enum RunError {
    Parse(LangError),
    Panic(Panic),
}
impl RunError {
    pub fn message(&self) -> String {
        match self {
            RunError::Parse(error) => error.message.clone(),
            RunError::Panic(panic) => panic.message.clone(),
        }
    }
}
pub fn compile_source(source: &Source) -> Result<Program, RunError> {
    let mut source = source.clone();
    let tokens = lex(&mut source).map_err(RunError::Parse)?;
    let source = Rc::new(source);
    let node = parse(tokens, source.clone()).map_err(RunError::Parse)?;
    let desugared = desugar(&node);
    let root = optimize(&desugared).map_err(RunError::Panic)?.root;
    register_eligible_jit_fns(&source, &root);
    compiler::compile(&source, &root).map_err(RunError::Panic)
}

/// Translate every Un function whose body is a pure integer computation
/// into native code and register it under its Un name. This makes the
/// interpreter call path for those functions take a constant-time native
/// dispatch.
fn register_eligible_jit_fns(source: &Source, root: &crate::ast::Node) {
    if !crate::jit_cache::jit_enabled() {
        return;
    }
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind == crate::ast::NodeKind::Function {
            // name (first Identifier child of Function)
            let name = match node.children.first() {
                Some(n) if n.kind == crate::ast::NodeKind::Identifier => {
                    source.span(n.start, n.length)
                }
                _ => continue,
            };
            if let Some(f) = crate::jit_fn::try_compile_int_fn(source, node) {
                crate::jit_cache::register_jit_fn(&name, f);
            }
        }
        for child in &node.children {
            stack.push(child);
        }
    }
}


