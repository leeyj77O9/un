//! `Context` — the runtime state of `Un/Runner/Context.cs`: scope, source, call frames,
//! the `defer` / `using` stacks, and the call depth the recursion limit is read from.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::Node;
use crate::runtime::obj;
use crate::runtime::obj::Obj;
use crate::runtime::scope::Scope;
use crate::source::Source;

/// `Frame` — the record the reference pushes at call time. `PrintError` renders the
/// trace from these (` :[{line text}] ({name})`).
pub struct Frame {
    pub name: String,
    pub source: Rc<Source>,
    pub start: usize,
    pub length: usize,
}

/// the reference passes `Closure.Frames` into the new `Context` of a call, so the
/// frame stack is shared by every nested call and the trace sees the whole chain.
pub type Frames = Rc<RefCell<Vec<Frame>>>;

pub fn new_frames() -> Frames {
    Rc::new(RefCell::new(Vec::new()))
}

pub struct ContextData {
    pub scope: Scope,
    pub source: Rc<Source>,
    pub frames: Frames,
    /// `Stack<Node>` — the reference pops in LIFO order at exit.
    pub defers: Vec<Node>,
    /// `Stack<Obj>` — each entry is disposed by `Exit()` at exit.
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

/// `Context.PopFrame` — an empty frame stack is `Panic("stack is empty")`. the reference
/// throws; Rust has no unwinding, so the panic travels as a value.
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

pub fn push_defer(ctx: &Context, node: Node) {
    ctx.borrow_mut().defers.push(node);
}

pub fn pop_defer(ctx: &Context) -> Option<Node> {
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

/// `PrintError` trace: `Frames.Skip(1).Reverse()`, innermost first, at most 10 lines,
/// plus the truncation note the reference prints whenever `Frames.Count > 10`
/// (`truncated [{Frames.Count - 11}+]`).
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
