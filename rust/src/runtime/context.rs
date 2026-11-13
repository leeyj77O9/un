use std::cell::RefCell;
use std::rc::Rc;
use crate::ast::Node;
use crate::runtime::obj;
use crate::runtime::obj::Obj;
use crate::runtime::scope::Scope;
use crate::source::Source;
pub struct Frame {
    pub name: String,
    pub source: Rc<Source>,
    pub start: usize,
    pub length: usize,
}
pub type Frames = Rc<RefCell<Vec<Frame>>>;
pub fn new_frames() -> Frames {
    Rc::new(RefCell::new(Vec::new()))
}
#[derive(Clone)]
pub enum Defer {

    Node(Node),

    Proto { index: usize, program: Rc<crate::bytecode::Program> },
}
pub struct ContextData {
    pub scope: Scope,
    pub source: Rc<Source>,
    pub frames: Frames,

    pub defers: Vec<Defer>,

    pub usings: Vec<Obj>,
    pub call_depth: usize,
}
pub type Context = Rc<RefCell<ContextData>>;
pub fn new(scope: Scope, source: Rc<Source>, frames: Frames) -> Context {
    Rc::new(RefCell::new(ContextData {
        scope,
        source,
        frames,
        defers: Vec::new(),
        usings: Vec::new(),
        call_depth: 0,
    }))
}
pub fn scope(ctx: &Context) -> Scope {
    ctx.borrow().scope.clone()
}
pub fn set_scope(ctx: &Context, scope: Scope) {
    ctx.borrow_mut().scope = scope;
}
pub fn source(ctx: &Context) -> Rc<Source> {
    ctx.borrow().source.clone()
}
pub fn frames(ctx: &Context) -> Frames {
    ctx.borrow().frames.clone()
}
pub fn push_frame(ctx: &Context, frame: Frame) {
    frames(ctx).borrow_mut().push(frame);
}
pub fn pop_frame(ctx: &Context) -> Result<Frame, Obj> {
    let frames = frames(ctx);
    let mut stack = frames.borrow_mut();
    if stack.is_empty() {
        return Err(obj::panic("panic", "stack is empty"));
    }
    Ok(stack.pop().expect("checked"))
}
pub fn frame_count(ctx: &Context) -> usize {
    frames(ctx).borrow().len()
}
pub fn call_depth(ctx: &Context) -> usize {
    ctx.borrow().call_depth
}
pub fn set_call_depth(ctx: &Context, depth: usize) {
    ctx.borrow_mut().call_depth = depth;
}
pub fn push_defer(ctx: &Context, defer: Defer) {
    ctx.borrow_mut().defers.push(defer);
}
pub fn pop_defer(ctx: &Context) -> Option<Defer> {
    ctx.borrow_mut().defers.pop()
}
pub fn defer_count(ctx: &Context) -> usize {
    ctx.borrow().defers.len()
}
pub fn push_using(ctx: &Context, value: Obj) {
    ctx.borrow_mut().usings.push(value);
}
pub fn pop_using(ctx: &Context) -> Option<Obj> {
    ctx.borrow_mut().usings.pop()
}
pub fn using_count(ctx: &Context) -> usize {
    ctx.borrow().usings.len()
}
pub fn trace_lines(ctx: &Context) -> (Vec<String>, usize) {
    let frames = frames(ctx);
    let stack = frames.borrow();
    if stack.len() <= 1 {
        return (Vec::new(), 0);
    }
    let skipped = &stack[1..];
    let shown = if skipped.len() > 10 { &skipped[..10] } else { skipped };
    let lines = shown
        .iter()
        .rev()
        .map(|f| {
            format!(
                " :[{}] ({})",
                f.source.get_line_text(f.start).trim(),
                f.name
            )
        })
        .collect();
    (lines, stack.len().saturating_sub(11))
}
#[cfg(test)]
mod tests;
