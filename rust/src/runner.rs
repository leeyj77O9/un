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
    compiler::compile(&source, &root).map_err(RunError::Panic)
}
