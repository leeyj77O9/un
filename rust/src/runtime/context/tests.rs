use super::*;
use crate::runtime::obj;
use crate::runtime::scope;
fn message(o: &Obj) -> String {
    match obj::payload(o) {
        obj::Payload::Error { message, .. } => message,
        obj::Payload::Panic { name, message } => format!("{name}: {message}"),
        _ => obj::display(o),
    }
}
fn frame(name: &str, source: Rc<Source>, start: usize) -> Frame {
    Frame {
        name: name.to_owned(),
        source,
        start,
        length: 1,
    }
}
fn source_with(code: &str) -> Rc<Source> {
    Rc::new(Source::new("probe.un", code))
}
fn pop_ok(ctx: &Context) -> Frame {
    match pop_frame(ctx) {
        Ok(f) => f,
        Err(o) => panic!("pop failed: {}", message(&o)),
    }
}
fn pop_err(ctx: &Context) -> Obj {
    match pop_frame(ctx) {
        Ok(f) => panic!("pop succeeded on frame {}", f.name),
        Err(o) => o,
    }
}
fn using_ok(ctx: &Context) -> Obj {
    match pop_using(ctx) {
        Some(o) => o,
        None => panic!("using stack is empty"),
    }
}
#[test]
fn popping_an_empty_frame_stack_is_the_reference_panic() {
    let ctx = new(scope::empty(), source_with("x = 1\n"), new_frames());
    let err = pop_err(&ctx);

    assert_eq!(message(&err), "panic: stack is empty");
    assert_eq!(frame_count(&ctx), 0);
}
#[test]
fn frames_pop_in_reverse_order() {
    let src = source_with("x = 1\ny = 2\n");
    let ctx = new(scope::empty(), src.clone(), new_frames());
    push_frame(&ctx, frame("a", src.clone(), 0));
    push_frame(&ctx, frame("b", src.clone(), 7));
    assert_eq!(frame_count(&ctx), 2);
    assert_eq!(pop_ok(&ctx).name, "b");
    assert_eq!(pop_ok(&ctx).name, "a");
    assert_eq!(frame_count(&ctx), 0);
}
#[test]
fn defers_drain_lifo() {
    let ctx = new(scope::empty(), source_with("x = 1\n"), new_frames());
    assert_eq!(defer_count(&ctx), 0);
    push_defer(
        &ctx,
        Defer::Node(Node::new(0, 1, crate::ast::NodeKind::Identifier)),
    );
    push_defer(
        &ctx,
        Defer::Node(Node::new(1, 1, crate::ast::NodeKind::Identifier)),
    );
    assert_eq!(defer_count(&ctx), 2);
    assert!(matches!(pop_defer(&ctx), Some(Defer::Node(node)) if node.start == 1));
    assert!(matches!(pop_defer(&ctx), Some(Defer::Node(node)) if node.start == 0));
    assert!(pop_defer(&ctx).is_none());
}
#[test]
fn usings_drain_lifo() {
    let ctx = new(scope::empty(), source_with("x = 1\n"), new_frames());
    push_using(&ctx, obj::int(1));
    push_using(&ctx, obj::int(2));
    assert_eq!(using_count(&ctx), 2);
    assert_eq!(obj::display(&using_ok(&ctx)), "2");
    assert_eq!(obj::display(&using_ok(&ctx)), "1");
    assert!(pop_using(&ctx).is_none());
}
#[test]
fn call_depth_is_shared_state_of_the_context_cell() {

    let ctx = new(scope::empty(), source_with("x = 1\n"), new_frames());
    let closure = ctx.clone();
    set_call_depth(&ctx, 4);
    assert_eq!(call_depth(&closure), 4);
    set_call_depth(&closure, 5);
    assert_eq!(call_depth(&ctx), 5);
}
#[test]
fn the_trace_skips_the_top_frame_and_prints_innermost_first() {
    let src = source_with("io.write(\"A\")\n    inner()\n");
    let ctx = new(scope::empty(), src.clone(), new_frames());
    push_frame(&ctx, frame("outer", src.clone(), 0));

    push_frame(&ctx, frame("inner", src.clone(), 14));
    let (lines, truncated) = trace_lines(&ctx);
    assert_eq!(truncated, 0);
    assert_eq!(lines, vec![" :[inner()] (inner)"]);
}
#[test]
fn the_trace_is_capped_at_ten_frames_with_a_truncation_note() {
    let src = source_with("x = 1\n");
    let ctx = new(scope::empty(), src.clone(), new_frames());
    for i in 0..12 {
        push_frame(&ctx, frame(&format!("f{i}"), src.clone(), 0));
    }
    let (lines, truncated) = trace_lines(&ctx);
    assert_eq!(lines.len(), 10);

    assert_eq!(lines.first().expect("first"), " :[x = 1] (f10)");
    assert_eq!(lines.last().expect("last"), " :[x = 1] (f1)");
    assert_eq!(truncated, 1);
}
#[test]
fn a_single_frame_prints_no_trace() {
    let src = source_with("x = 1\n");
    let ctx = new(scope::empty(), src.clone(), new_frames());
    push_frame(&ctx, frame("main", src, 0));
    let (lines, truncated) = trace_lines(&ctx);
    assert!(lines.is_empty());
    assert_eq!(truncated, 0);
}
#[test]
fn set_scope_replaces_the_context_scope() {
    let outer = scope::empty();
    let inner = scope::new(Some(outer.clone()), true);
    let ctx = new(outer.clone(), source_with("x = 1\n"), new_frames());
    assert_eq!(scope::slot_count(&scope(&ctx)), 0);
    set_scope(&ctx, inner.clone());
    assert!(std::rc::Rc::ptr_eq(&scope(&ctx), &inner));
    assert!(scope::is_function_scope(&scope(&ctx)));
}
