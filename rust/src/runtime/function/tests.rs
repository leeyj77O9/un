//! Phase 7c function tests. the ground truth is `src/Core/Object/Function/{Fn,LFn,NFn,Arg}.cs`
//! and `src/Core/Object/Base/Property.cs`: `Bind` reports `missing required argument
//! '{name}'` / `too many positional arguments` / `multiple values for argument '{name}'`
//! / `unexpected keyword argument '{keyword.Keys.First()}'`, `self` / `super` / `field`
//! are bound as locals, `self` and `super` parameters are skipped, `*` / `**` parameters
//! swallow the leftovers, and the recursion limit is read from the closure's `CallDepth`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use super::*;
use crate::ast::{Node, NodeKind};
use crate::runtime::collections;
use crate::runtime::context::{self, Context};
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use crate::runtime::types::Types;
use crate::runtime::world;

fn message(o: &Obj) -> String {
    match obj::payload(o) {
        obj::Payload::Error { message, .. } => message,
        obj::Payload::Panic { name, message } => format!("{name}: {message}"),
        _ => obj::display(o),
    }
}

fn int_arg(name: &str) -> ArgData {
    ArgData {
        name: name.to_owned(),
        type_name: Types::single("int"),
        essential: true,
        optional: false,
        positional: false,
        keyword: false,
        default: Some(obj::null()),
    }
}

fn call(f: &Obj, values: Vec<Obj>) -> Obj {
    fn_call(f, &Args::positional(values))
}

fn call_named(f: &Obj, values: Vec<Obj>, names: Vec<&str>) -> Obj {
    fn_call(
        f,
        &Args::named(values, names.iter().map(|n| n.to_string()).collect()),
    )
}

/// a user function with a fixed body length and no parameters
fn body_fn(name: &str, body_len: usize) -> Obj {
    lfn(
        Some(name.to_owned()),
        Vec::new(),
        Types::single("any"),
        vec![Node::new(0, 1, NodeKind::Identifier); body_len],
        default_closure(),
    )
}

/// an executor hook that reports each body it runs and answers a scripted flow per call.
/// the first recorded entry is the function body, the following ones are the `defer`
/// drains, so the LIFO order of the drain is observable through `record`.
fn scripted_executor(record: Rc<RefCell<Vec<usize>>>, script: Vec<Flow>) -> world::Executor {
    let pending = Rc::new(RefCell::new(VecDeque::from(script)));
    let rec = record.clone();
    Rc::new(move |ctx: &Context, body: &[Node]| {
        rec.borrow_mut().push(body.len());
        let _ = ctx;
        pending.borrow_mut().pop_front().unwrap_or(Flow::Normal)
    })
}

#[test]
fn bind_reports_the_missing_required_argument() {
    world::reset();
    let f = lfn(
        Some("add".to_owned()),
        vec![int_arg("a"), int_arg("b")],
        Types::single("any"),
        Vec::new(),
        default_closure(),
    );
    let s = scope::new(None, true);

    let e = bind(&f, &s, &Args::positional(vec![obj::int(1)]));
    assert_eq!(message(&e), "missing required argument 'b'");
}

#[test]
fn bind_reports_too_many_positional_arguments() {
    world::reset();
    let f = lfn(
        Some("add".to_owned()),
        vec![int_arg("a")],
        Types::single("any"),
        Vec::new(),
        default_closure(),
    );
    let s = scope::new(None, true);

    let e = bind(&f, &s, &Args::positional(vec![obj::int(1), obj::int(2)]));
    assert_eq!(message(&e), "too many positional arguments");
}

#[test]
fn duplicate_keyword_values_are_reported_by_name() {
    world::reset();
    let f = lfn(
        Some("f".to_owned()),
        Vec::new(),
        Types::single("any"),
        Vec::new(),
        default_closure(),
    );
    let s = scope::new(None, true);

    let e = bind(
        &f,
        &s,
        &Args::named(
            vec![obj::int(1), obj::int(2)],
            vec!["x".to_owned(), "x".to_owned()],
        ),
    );
    assert_eq!(message(&e), "multiple values for argument 'x'");
}

#[test]
fn an_unmatched_keyword_is_reported_by_its_first_name() {
    world::reset();
    let f = lfn(
        Some("f".to_owned()),
        vec![int_arg("a")],
        Types::single("any"),
        Vec::new(),
        default_closure(),
    );
    let s = scope::new(None, true);

    let e = bind(
        &f,
        &s,
        &Args::named(vec![obj::int(1), obj::int(2)], vec!["a".to_owned(), "k".to_owned()]),
    );
    // the reference names `keyword.Keys.First()`, so the first leftover keyword wins.
    // the required parameter is satisfied positionally, so the keyword check is the one
    // that fires.
    assert_eq!(message(&e), "unexpected keyword argument 'k'");
}

#[test]
fn positional_values_bind_in_order_and_keyword_names_bind_by_name() {
    world::reset();
    let f = lfn(
        Some("f".to_owned()),
        vec![int_arg("a"), int_arg("b")],
        Types::single("any"),
        Vec::new(),
        default_closure(),
    );
    let s = scope::new(None, true);

    assert!(obj::is_none(&bind(
        &f,
        &s,
        &Args::named(vec![obj::int(1), obj::int(2)], vec!["b".to_owned(), "a".to_owned()])
    )));
    assert_eq!(obj::display(&scope::get(&s, "a")), "2");
    assert_eq!(obj::display(&scope::get(&s, "b")), "1");
}

#[test]
fn optional_parameters_get_their_default_and_essential_ones_are_skipped_in_the_fast_path() {
    world::reset();
    // `x = 5` is an optional parameter; `self` / `super` parameters are never bound from
    // arguments, they are the locals Bind installs first
    let mut x = int_arg("x");
    x.optional = true;
    // `Arg` sets `IsEssential = !optional && !positional && !keyword`, so an optional
    // parameter is never essential
    x.essential = false;
    x.default = Some(obj::int(5));
    let mut y = int_arg("y");
    y.essential = false;
    y.default = Some(obj::int(9));

    let f = lfn(
        Some("f".to_owned()),
        vec![x, y],
        Types::single("any"),
        Vec::new(),
        default_closure(),
    );
    let s = scope::new(None, true);

    assert!(obj::is_none(&bind(&f, &s, &Args::positional(Vec::new()))));
    assert_eq!(obj::display(&scope::get(&s, "x")), "5");
    assert_eq!(obj::display(&scope::get(&s, "y")), "9");
}

#[test]
fn star_parameters_swallow_the_remaining_positional_values() {
    world::reset();
    let mut rest = int_arg("rest");
    rest.positional = true;

    let f = lfn(
        Some("f".to_owned()),
        vec![int_arg("a"), rest],
        Types::single("any"),
        Vec::new(),
        default_closure(),
    );
    let s = scope::new(None, true);

    assert!(obj::is_none(&bind(
        &f,
        &s,
        &Args::positional(vec![obj::int(1), obj::int(2), obj::int(3)])
    )));
    assert_eq!(obj::display(&scope::get(&s, "a")), "1");
    assert_eq!(obj::display(&scope::get(&s, "rest")), "(2, 3)");
}

#[test]
fn double_star_parameters_swallow_the_remaining_keywords() {
    world::reset();
    let mut rest = int_arg("kw");
    rest.keyword = true;

    let f = lfn(
        Some("f".to_owned()),
        vec![int_arg("a"), rest],
        Types::single("any"),
        Vec::new(),
        default_closure(),
    );
    let s = scope::new(None, true);

    assert!(obj::is_none(&bind(
        &f,
        &s,
        &Args::named(
            vec![obj::int(1), obj::int(2), obj::int(3)],
            vec!["a".to_owned(), "p".to_owned(), "q".to_owned()]
        )
    )));
    assert_eq!(obj::display(&scope::get(&s, "kw")), "{p:2, q:3}");
}

#[test]
fn unpacking_a_spread_inherits_the_name_of_the_argument_it_came_from() {
    world::reset();
    let spread = collections::spread(vec![obj::int(1), obj::int(2)]);
    let (values, names) = unpack_args(&Args::named(vec![spread], vec!["a".to_owned()]));

    assert_eq!(values.len(), 2);
    assert_eq!(obj::display(&values[0]), "1");
    assert_eq!(obj::display(&values[1]), "2");
    assert_eq!(names, vec!["a".to_owned(), "a".to_owned()]);
}

#[test]
fn the_required_parameter_check_runs_before_the_keyword_leftover_check() {
    world::reset();
    let f = lfn(
        Some("f".to_owned()),
        vec![int_arg("a")],
        Types::single("any"),
        Vec::new(),
        default_closure(),
    );

    // the reference walks the parameter list first, so a missing required parameter is
    // reported even when keyword arguments are unmatched
    assert_eq!(
        message(&call_named(&f, vec![obj::int(1), obj::int(2)], vec!["k", "j"])),
        "missing required argument 'a'"
    );
}

#[test]
fn self_super_and_field_are_bound_as_locals() {
    world::reset();
    let prop = property("x");

    let f = lfn(
        Some("f".to_owned()),
        Vec::new(),
        Types::single("any"),
        Vec::new(),
        default_closure(),
    );
    set_property(&f, Some(prop.clone()));

    let owner = obj::obj_new("obj", Payload::Plain);
    bind_self(&f, &owner);

    let s = scope::new(None, true);
    assert!(obj::is_none(&bind(&f, &s, &Args::positional(Vec::new()))));

    // `self` is the receiver object itself, `super` is the receiver's super, and
    // `field` is the FieldRef over the property
    assert!(Rc::ptr_eq(&scope::get(&s, "self"), &owner));
    assert!(obj::is_none(&scope::get(&s, "super")));
    assert!(matches!(
        obj::payload(&scope::get(&s, "field")),
        Payload::FieldRef(..)
    ));
    assert!(!scope::contains_key_in_top(&s, "x"));
}

#[test]
fn the_recursion_limit_is_read_from_the_closure_depth() {
    world::reset();
    let f = body_fn("f", 1);
    let closure = { f.borrow().payload.clone() };
    let cell = match closure {
        Payload::Func(c) => c,
        _ => panic!("func payload"),
    };

    context::set_call_depth(&cell.borrow().closure, MAX_RECURSION_DEPTH);
    let e = call(&f, Vec::new());
    assert_eq!(message(&e), "maximum recursion depth exceeded");

    context::set_call_depth(&cell.borrow().closure, MAX_RECURSION_DEPTH - 1);
    let record = Rc::new(RefCell::new(Vec::new()));
    world::set_executor(Some(scripted_executor(
        record.clone(),
        vec![Flow::Return(obj::int(7))],
    )));
    assert_eq!(obj::display(&call(&f, Vec::new())), "7");
    assert_eq!(*record.borrow(), vec![1]);
    assert_eq!(context::call_depth(&cell.borrow().closure), MAX_RECURSION_DEPTH - 1);
}

#[test]
fn the_native_depth_counter_is_decremented_on_every_exit() {
    world::reset();
    let host: Rc<dyn Fn(&Scope) -> Obj> = Rc::new(|_s: &Scope| obj::int(3));
    let f = nfn("native", Vec::new(), host);
    let closure = { f.borrow().payload.clone() };
    let cell = match closure {
        Payload::Func(c) => c,
        _ => panic!("func payload"),
    };

    context::set_call_depth(&cell.borrow().closure, MAX_RECURSION_DEPTH);
    let e = call(&f, Vec::new());
    assert_eq!(message(&e), "maximum recursion depth exceeded");
    // `NFn.Call` increments, tests, then decrements on the failure path
    assert_eq!(context::call_depth(&cell.borrow().closure), MAX_RECURSION_DEPTH);

    context::set_call_depth(&cell.borrow().closure, MAX_RECURSION_DEPTH - 1);
    assert_eq!(obj::display(&call(&f, Vec::new())), "3");
    assert_eq!(context::call_depth(&cell.borrow().closure), MAX_RECURSION_DEPTH - 1);
}

#[test]
fn a_return_flow_value_is_the_call_result() {
    world::reset();
    let f = body_fn("f", 2);
    let record = Rc::new(RefCell::new(Vec::new()));
    world::set_executor(Some(scripted_executor(
        record.clone(),
        vec![Flow::Return(obj::int(42))],
    )));

    assert_eq!(obj::display(&call(&f, Vec::new())), "42");
    assert_eq!(*record.borrow(), vec![2]);
}

#[test]
fn a_propagated_flow_value_is_the_call_result() {
    world::reset();
    let f = body_fn("f", 1);
    world::set_executor(Some(scripted_executor(
        Rc::new(RefCell::new(Vec::new())),
        vec![Flow::Propagate(obj::err("boom"))],
    )));

    assert_eq!(message(&call(&f, Vec::new())), "boom");
}

#[test]
fn defers_drain_in_reverse_order_after_the_body() {
    world::reset();
    let record = Rc::new(RefCell::new(Vec::new()));
    let rec = record.clone();
    let node = Node::new(0, 1, NodeKind::Identifier);
    let exec: world::Executor = Rc::new(move |ctx: &Context, body: &[Node]| {
        rec.borrow_mut().push(body.len());
        if body.len() == 3 {
            // the body pushes two defers; `finally` pops them LIFO
            context::push_defer(ctx, context::Defer::Node(node.clone()));
            context::push_defer(ctx, context::Defer::Node(node.clone()));
            Flow::Return(obj::int(1))
        } else {
            Flow::Return(obj::int(2))
        }
    });
    world::set_executor(Some(exec));

    let f = body_fn("f", 3);
    // the last drain that produces a flow wins, matching the reference's escaping `finally`
    assert_eq!(obj::display(&call(&f, Vec::new())), "2");
    assert_eq!(*record.borrow(), vec![3, 1, 1]);
}

#[test]
fn defers_and_usings_are_emptied_at_call_exit() {
    world::reset();
    let seen = Rc::new(RefCell::new(None));
    let captured = seen.clone();
    let exec: world::Executor = Rc::new(move |ctx: &Context, body: &[Node]| {
        if body.len() == 2 {
            *captured.borrow_mut() = Some(ctx.clone());
            context::push_defer(ctx, context::Defer::Node(Node::new(0, 1, NodeKind::Identifier)));
            context::push_using(ctx, obj::obj_new("obj", Payload::Plain));
        }
        Flow::Normal
    });
    world::set_executor(Some(exec));

    let f = body_fn("f", 2);
    assert!(obj::is_none(&call(&f, Vec::new())));

    let ctx = seen.borrow().clone().expect("body ran");
    assert_eq!(context::defer_count(&ctx), 0);
    assert_eq!(context::using_count(&ctx), 0);
}

#[test]
fn a_using_exit_runs_through_the_dunder_member() {
    world::reset();
    let flag = Rc::new(RefCell::new(false));
    let mark = flag.clone();
    let host: Rc<dyn Fn(&Scope) -> Obj> = Rc::new(move |_s: &Scope| {
        *mark.borrow_mut() = true;
        obj::none()
    });
    let exit_fn = nfn("__exit__", Vec::new(), host);

    let obj_cell = obj::obj_new("obj", Payload::Plain);
    obj::member_set(&obj_cell, "__exit__", &exit_fn);

    obj::exit(&obj_cell);
    assert!(flag.borrow().to_owned());
}

#[test]
fn func_display_is_the_to_str_form_and_repr_shows_the_name() {
    world::reset();
    let f = lfn(
        Some("add".to_owned()),
        vec![int_arg("a")],
        Types::single("any"),
        Vec::new(),
        default_closure(),
    );
    assert_eq!(obj::display(&obj::repr(&f)), "fn add(a: int) -> any");
    assert_eq!(obj::display(&f), "fn(int) -> any");
}

#[test]
fn cloning_a_function_shares_the_parameter_list_cell() {
    world::reset();
    let f = lfn(
        Some("add".to_owned()),
        vec![int_arg("a")],
        Types::single("any"),
        Vec::new(),
        default_closure(),
    );
    let c = obj::clone_obj(&f);

    let original = match obj::payload(&f) {
        Payload::Func(cell) => cell,
        _ => panic!("func payload"),
    };
    let cloned = match obj::payload(&c) {
        Payload::Func(cell) => cell,
        _ => panic!("func payload"),
    };

    // the reference assigns the same `List<Arg>` object, so the parameter cell is shared
    assert!(Rc::ptr_eq(
        &original.borrow().args,
        &cloned.borrow().args
    ));
    assert_eq!(cloned.borrow().name.as_ref().expect("name"), "add");
    // the closure cell is shared too, so the recursion limit is shared
    assert!(Rc::ptr_eq(
        &original.borrow().closure,
        &cloned.borrow().closure
    ));
    // `fn_clone` is a new cell, so the objects are not the same
    assert!(!Rc::ptr_eq(&f, &c));
}

#[test]
fn a_property_materialises_its_backing_member_from_the_default() {
    world::reset();
    let prop = property("x");
    set_prop_default(&prop, obj::int(3));

    let instance = obj::obj_new("obj", Payload::Plain);
    assert_eq!(obj::display(&prop_read(&prop, &instance)), "3");
    assert!(obj::member(&instance, "$x").is_some());
    // the materialised value is the stored member from then on
    assert_eq!(obj::display(&obj::member(&instance, "$x").expect("backing")), "3");
    assert_eq!(obj::display(&prop_read(&prop, &instance)), "3");
    assert_eq!(obj::display(&obj::repr(&prop)), "<property x>");
    // `Property.Clone()` / `Copy()` are `this`, so the property object is shared
    assert!(Rc::ptr_eq(&obj::clone_obj(&prop), &prop));
    assert!(Rc::ptr_eq(&obj::copy_obj(&prop), &prop));
}

#[test]
fn writing_a_property_writes_the_backing_key() {
    world::reset();
    let prop = property("x");
    let instance = obj::obj_new("obj", Payload::Plain);

    let value = obj::int(8);
    prop_write(&prop, &instance, &value);
    assert_eq!(obj::display(&obj::member(&instance, "$x").expect("backing")), "8");
    assert_eq!(obj::display(&prop_read(&prop, &instance)), "8");
}

#[test]
fn a_property_accessor_runs_with_the_owner_bound() {
    world::reset();
    let prop = property("x");
    set_prop_default(&prop, obj::int(1));

    // `get` returns the field view, which reads the owner's backing member
    let host: Rc<dyn Fn(&Scope) -> Obj> = Rc::new(|s: &Scope| {
        let field = scope::get(s, "field");
        field_get(&field)
    });
    let getter = nfn("get", Vec::new(), host);
    set_prop_getter(&prop, getter);

    let instance = obj::obj_new("obj", Payload::Plain);
    assert_eq!(obj::display(&prop_get(&prop, &instance)), "1");

    // `set` writes through the same field view
    let host: Rc<dyn Fn(&Scope) -> Obj> = Rc::new(|s: &Scope| {
        let field = scope::get(s, "field");
        field_set(&field, &scope::get(s, "v"));
        obj::none()
    });
    let setter = nfn("set", vec![ArgData::essential("v")], host);
    set_prop_setter(&prop, setter);

    prop_set(&prop, &instance, obj::int(6));
    assert_eq!(obj::display(&obj::member(&instance, "$x").expect("backing")), "6");
    assert_eq!(obj::display(&prop_get(&prop, &instance)), "6");
}

#[test]
fn a_field_ref_view_writes_through_to_its_owner() {
    world::reset();
    let prop = property("x");
    set_prop_default(&prop, obj::int(2));

    let owner = obj::obj_new("obj", Payload::Plain);
    let field = field_ref(&owner, &prop);

    assert_eq!(obj::display(&field_get(&field)), "2");
    field_set(&field, &obj::int(5));
    assert_eq!(obj::display(&obj::member(&owner, "$x").expect("backing")), "5");
    assert_eq!(obj::display(&field_get(&field)), "5");
    // `FieldRef.Clone()` / `Copy()` are `this`
    assert!(Rc::ptr_eq(&obj::clone_obj(&field), &field));
    assert!(Rc::ptr_eq(&obj::copy_obj(&field), &field));
}

#[test]
fn calling_a_function_payload_runs_it_and_a_plain_object_is_not_callable() {
    world::reset();
    let f = body_fn("f", 1);
    world::set_executor(Some(scripted_executor(
        Rc::new(RefCell::new(Vec::new())),
        vec![Flow::Return(obj::int(4))],
    )));
    assert_eq!(obj::display(&obj::call(&f, &[])), "4");

    let n = obj::int(1);
    assert_eq!(
        message(&obj::call(&n, &[])),
        "unsupported operand type(s) for (): 'int'"
    );
}
