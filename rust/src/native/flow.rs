use std::cell::RefCell;
use std::rc::Rc;
use crate::runtime::collections;
use crate::runtime::context::Context;
use crate::runtime::function::{self, Args};
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use super::{arg, native};
#[derive(Default)]
pub struct FutureState {
    pub value: Option<Obj>,

    pub pending: Option<(usize, Rc<crate::bytecode::Program>, Scope)>,
}
#[derive(Default)]
pub struct PoolState {
    pub workers: i64,
    pub closed: bool,
}
#[derive(Default)]
pub struct LockState {
    pub held: bool,
}
pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "spawn".to_owned(),
            native(
                closure,
                "spawn",
                vec![arg("worker_count", false, true, false)],
                spawn_impl,
            ),
        ),
        (
            "lock".to_owned(),
            native(closure, "lock", Vec::new(), lock_impl),
        ),
    ]
}
pub fn future_of(value: Obj) -> Obj {
    obj::obj_new(
        "future",
        Payload::Future(Rc::new(RefCell::new(FutureState {
            value: Some(value),
            pending: None,
        }))),
    )
}
pub fn future_pending(
    index: usize,
    program: Rc<crate::bytecode::Program>,
    scope: Scope,
) -> Obj {
    obj::obj_new(
        "future",
        Payload::Future(Rc::new(RefCell::new(FutureState {
            value: None,
            pending: Some((index, program, scope)),
        }))),
    )
}
pub fn future_value(future: &Obj) -> Option<Obj> {
    match obj::payload(future) {
        Payload::Future(state) => state.borrow().value.clone(),
        _ => None,
    }
}
pub fn new_pool(workers: i64) -> Obj {
    obj::obj_new(
        "pool",
        Payload::Pool(Rc::new(RefCell::new(PoolState {
            workers,
            closed: false,
        }))),
    )
}
pub fn new_lock() -> Obj {
    obj::obj_new(
        "lock",
        Payload::Lock(Rc::new(RefCell::new(LockState::default()))),
    )
}
fn spawn_impl(scope: &Scope) -> Obj {
    let count = scope::get(scope, "worker_count");
    let workers = if obj::is_none(&count) {
        4
    } else {
        match obj::payload(&count) {
            Payload::Int(v) => v,
            _ => return obj::err("argument 'worker' must be of type int"),
        }
    };
    new_pool(workers)
}
fn lock_impl(_scope: &Scope) -> Obj {
    new_lock()
}
pub fn pool_members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "submit".to_owned(),
            native(
                closure,
                "submit",
                vec![arg("fn", true, false, false), arg("args", false, false, true)],
                pool_submit,
            ),
        ),
        (
            "map".to_owned(),
            native(
                closure,
                "map",
                vec![arg("fn", true, false, false), arg("vargs", false, false, true)],
                pool_map,
            ),
        ),
        (
            "close".to_owned(),
            native(closure, "close", Vec::new(), pool_close),
        ),
    ]
}
pub fn lock_members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "acquire".to_owned(),
            native(closure, "acquire", Vec::new(), lock_acquire),
        ),
        (
            "try_acquire".to_owned(),
            native(closure, "try_acquire", Vec::new(), lock_try_acquire),
        ),
        (
            "release".to_owned(),
            native(closure, "release", Vec::new(), lock_release),
        ),
        (
            "dispose".to_owned(),
            native(closure, "dispose", Vec::new(), lock_dispose),
        ),
    ]
}
fn pool_self(scope: &Scope) -> Option<Rc<RefCell<PoolState>>> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::Pool(state) => Some(state.clone()),
        _ => None,
    }
}
fn lock_self(scope: &Scope) -> Option<Rc<RefCell<LockState>>> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::Lock(state) => Some(state.clone()),
        _ => None,
    }
}
fn call_args(value: &Obj) -> Option<Vec<Obj>> {
    match obj::payload(value) {
        Payload::Tup(values, _) => Some((*values).clone()),
        _ => None,
    }
}
fn pool_submit(scope: &Scope) -> Obj {
    let Some(state) = pool_self(scope) else {
        return obj::err("expected 'pool' object");
    };
    if state.borrow().closed {
        return obj::err("pool is closed");
    }
    let func = scope::get(scope, "fn");
    if !function::is_func(&func) {
        return obj::err("expected 'fn' argument to be of type 'func'");
    }
    let args = scope::get(scope, "args");
    let Some(values) = call_args(&args) else {
        return obj::err("expected 'args' argument to be of type 'collection'");
    };
    let value = function::fn_call(&func, &Args::named(values, Vec::new()));
    future_of(value)
}
fn pool_map(scope: &Scope) -> Obj {
    let Some(state) = pool_self(scope) else {
        return obj::err("expected 'pool' object");
    };
    if state.borrow().closed {
        return obj::err("pool is closed");
    }
    let func = scope::get(scope, "fn");
    if !function::is_func(&func) {
        return obj::err("expected 'fn' argument to be of type 'func'");
    }
    let vargs = scope::get(scope, "vargs");
    let Some(values) = call_args(&vargs) else {
        return obj::err("expected 'iterable' argument to be of type 'iterable'");
    };
    let mut results: Vec<Obj> = Vec::with_capacity(values.len());
    for value in values {

        let args = match obj::payload(&value) {
            Payload::Tup(items, _) => (*items).clone(),
            _ => vec![value.clone()],
        };
        results.push(function::fn_call(&func, &Args::named(args, Vec::new())));
    }
    collections::list(results)
}
fn pool_close(scope: &Scope) -> Obj {
    let Some(state) = pool_self(scope) else {
        return obj::err("expected 'pool' object");
    };
    state.borrow_mut().closed = true;
    obj::none()
}
fn lock_acquire(scope: &Scope) -> Obj {
    let Some(state) = lock_self(scope) else {
        return obj::err("expected 'lock' object");
    };
    state.borrow_mut().held = true;
    scope::get(scope, "self")
}
fn lock_try_acquire(scope: &Scope) -> Obj {
    let Some(state) = lock_self(scope) else {
        return obj::err("expected 'lock' object");
    };
    let mut state = state.borrow_mut();
    if state.held {
        return obj::boolean(false);
    }
    state.held = true;
    obj::boolean(true)
}
fn lock_release(scope: &Scope) -> Obj {
    let Some(state) = lock_self(scope) else {
        return obj::err("expected 'lock' object");
    };
    let mut state = state.borrow_mut();
    if !state.held {
        return obj::err("lock not held");
    }
    state.held = false;
    scope::get(scope, "self")
}
fn lock_dispose(scope: &Scope) -> Obj {
    let Some(state) = lock_self(scope) else {
        return obj::err("expected 'lock' object");
    };
    state.borrow_mut().held = false;
    obj::none()
}
