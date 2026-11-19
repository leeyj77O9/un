use std::cell::RefCell;
use std::rc::Rc;
use crate::ast::Node;
use crate::error::nl;
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
pub struct TraceEntry {
    pub source: Rc<Source>,
    pub start: usize,
    pub length: usize,
    pub name: String,
}
pub type Traces = Rc<RefCell<Vec<TraceEntry>>>;
pub fn new_trace() -> Traces {
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

    pub trace: Traces,
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
        trace: new_trace(),
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
pub fn traces(ctx: &Context) -> Traces {
    ctx.borrow().trace.clone()
}
pub fn push_trace(ctx: &Context, entry: TraceEntry) {
    traces(ctx).borrow_mut().push(entry);
}
pub fn take_trace(ctx: &Context) -> Vec<TraceEntry> {
    std::mem::take(&mut traces(ctx).borrow_mut())
}
pub fn clear_trace(ctx: &Context) {
    traces(ctx).borrow_mut().clear();
}

const COMPACT_FRAME_THRESHOLD: usize = 10;

enum TraceRow {
    Frame {
        name: String,
        file: String,
        line: usize,
        source_line: String,
    },
    Repeated(usize),
}

fn trace_frame_name(name: &str) -> &str {
    match name {
        "" => "<module>",
        "lambda" => "<lambda>",
        _ => name,
    }
}

pub fn trace_string(ctx: &Context, root: Option<(&Source, usize)>) -> Option<String> {
    let trace = take_trace(ctx);
    let mut rows: Vec<TraceRow> = Vec::new();
    if trace.is_empty() {
        if let Some((source, start)) = root {
            rows.push(TraceRow::Frame {
                name: "<module>".to_string(),
                file: format!("<{}>", source.name()),
                line: source.get_line(start),
                source_line: source.get_line_text(start).trim().to_string(),
            });
        }
    } else {
        let entries: Vec<_> = trace.into_iter().rev().collect();
        let mut i = 0;
        while i < entries.len() {
            let mut count = 1;
            while i + count < entries.len()
                && entries[i + count].source.path == entries[i].source.path
                && entries[i + count].start == entries[i].start
                && entries[i + count].name == entries[i].name
            {
                count += 1;
            }
            rows.push(TraceRow::Frame {
                name: trace_frame_name(&entries[i].name).to_string(),
                file: format!("<{}>", entries[i].source.name()),
                line: entries[i].source.get_line(entries[i].start),
                source_line: entries[i]
                    .source
                    .get_line_text(entries[i].start)
                    .trim()
                    .to_string(),
            });
            if count > 1 {
                rows.push(TraceRow::Repeated(count - 1));
            }
            i += count;
        }
    }
    if rows.len() <= 1 {
        return None;
    }
    let width = rows
        .iter()
        .map(|row| match row {
            TraceRow::Frame { name, .. } => name.chars().count(),
            TraceRow::Repeated(_) => 0,
        })
        .max()
        .unwrap_or(0);
    let frame_rows = rows
        .iter()
        .filter(|row| matches!(row, TraceRow::Frame { .. }))
        .count();
    let compact = frame_rows > COMPACT_FRAME_THRESHOLD;
    let mut out = String::new();
    out.push_str("un: error trace");
    out.push_str(nl());
    for row in &rows {
        match row {
            TraceRow::Frame {
                name,
                file,
                line,
                source_line,
            } => {
                out.push_str(&format!("    -> {name:<width$}  {file}, line [{line}]"));
                out.push_str(nl());
                if !compact && !source_line.is_empty() {
                    out.push_str("         ");
                    out.push_str(source_line);
                    out.push_str(nl());
                }
            }
            TraceRow::Repeated(count) => {
                out.push_str(&format!("    ... (repeated {count} more times)"));
                out.push_str(nl());
            }
        }
    }
    Some(out)
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
