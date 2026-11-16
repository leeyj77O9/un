//! Executor hooks.
//!
//! The VM registers closure executors here so tests and the WASM playground
//! can run a single proto's body without going through the whole evaluator
//! (`Flow::Normal` falls through when no executor is registered).

use super::WORLD;
use crate::ast::Node;
use crate::bytecode::Program;
use crate::runtime::context::Context;
use crate::runtime::function::Flow;
use std::rc::Rc;

pub type Executor = Rc<dyn Fn(&Context, &[Node]) -> Flow>;
pub type ProtoExecutor = Rc<dyn Fn(&Rc<Program>, &Context, usize) -> Flow>;

pub fn set_executor(executor: Option<Executor>) {
    WORLD.with(|w| w.borrow_mut().executor = executor);
}
pub fn execute(ctx: &Context, body: &[Node]) -> Flow {
    let executor = WORLD.with(|w| w.borrow().executor.clone());
    match executor {
        Some(f) => f(ctx, body),
        None => Flow::Normal,
    }
}

pub fn set_proto_executor(executor: Option<ProtoExecutor>) {
    WORLD.with(|w| w.borrow_mut().proto_executor = executor);
}
pub fn execute_proto(program: &Rc<Program>, ctx: &Context, proto_index: usize) -> Flow {
    let executor = WORLD.with(|w| w.borrow().proto_executor.clone());
    match executor {
        Some(f) => f(program, ctx, proto_index),
        None => Flow::Normal,
    }
}
