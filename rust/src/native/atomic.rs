//! the `atomic_int` / `atomic_bool` payloads and the `atomic` native module.
//!
//! the reference stores the value in an `Interlocked` target. the runtime has no threads
//! for the object model (the objects are not `Send`), so the cell holds the same value the
//! interlocked operations produce, and the operations keep the reference's order of the
//! checks and messages.

use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};

use super::{arg, native};

/// a new atomic integer counter.
pub fn new_atomic_int(initial: i64) -> Obj {
    obj::obj_new("atomic_int", Payload::AtomicInt(Rc::new(RefCell::new(initial))))
}

/// a new atomic boolean flag.
pub fn new_atomic_bool(initial: bool) -> Obj {
    obj::obj_new("atomic_bool", Payload::AtomicBool(Rc::new(RefCell::new(initial))))
}

/// `Atomic` — the `atomic` native module. the parameter names are the reference's
/// `[ArgInfo]` names, so a keyword call binds the same slot.
pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "int".to_owned(),
            native(
                closure,
                "int",
                vec![arg("initial", false, true, false)],
                int_impl,
            ),
        ),
        (
            "bool".to_owned(),
            native(
                closure,
                "bool",
                vec![arg("initial", false, true, false)],
                bool_impl,
            ),
        ),
    ]
}

/// `Atomic.Int(initial)` — no argument is `null` in the reference, which binds `none`
/// in the runtime, so `is_none` is the `initial is null` test.
fn int_impl(scope: &Scope) -> Obj {
    let initial = scope::get(scope, "initial");

    if obj::is_none(&initial) {
        return new_atomic_int(0);
    }

    match obj::payload(&initial) {
        Payload::Int(v) => new_atomic_int(v),
        _ => obj::err("expected 'initial' argument to be of type 'int'"),
    }
}

/// `Atomic.Bool(initial)`
fn bool_impl(scope: &Scope) -> Obj {
    let initial = scope::get(scope, "initial");

    if obj::is_none(&initial) {
        return new_atomic_bool(false);
    }

    match obj::payload(&initial) {
        Payload::Bool(b) => new_atomic_bool(b),
        _ => obj::err("expected 'initial' argument to be of type 'bool'"),
    }
}

/// `AtomicInt.Init(args)` — the class call form, `atomic_int(10)`.
pub fn atomic_int_init(_o: &Obj, args: &[Obj]) -> Obj {
    match args.len() {
        0 => new_atomic_int(0),
        1 => match obj::payload(&args[0]) {
            Payload::Int(v) => new_atomic_int(v),
            _ => obj::err("atomic_int takes at most one argument"),
        },
        _ => obj::err("atomic_int takes at most one argument"),
    }
}

/// `AtomicBool.Init(args)`
pub fn atomic_bool_init(_o: &Obj, args: &[Obj]) -> Obj {
    match args.len() {
        0 => new_atomic_bool(false),
        1 => match obj::payload(&args[0]) {
            Payload::Bool(b) => new_atomic_bool(b),
            _ => obj::err("atomic_bool takes at most one argument"),
        },
        _ => obj::err("atomic_bool takes at most one argument"),
    }
}

// ---------------------------------------------------------------------------
// the member tables of the counter and the flag
// ---------------------------------------------------------------------------

pub fn int_members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        ("get".to_owned(), native(closure, "get", Vec::new(), int_get)),
        (
            "set".to_owned(),
            native(
                closure,
                "set",
                vec![arg("amount", true, false, false)],
                int_set,
            ),
        ),
        (
            "add".to_owned(),
            native(
                closure,
                "add",
                vec![arg("amount", true, false, false)],
                int_add,
            ),
        ),
        ("inc".to_owned(), native(closure, "inc", Vec::new(), int_inc)),
        ("dec".to_owned(), native(closure, "dec", Vec::new(), int_dec)),
        (
            "exchange".to_owned(),
            native(
                closure,
                "exchange",
                vec![arg("amount", true, false, false)],
                int_exchange,
            ),
        ),
        (
            "compare_and_swap".to_owned(),
            native(
                closure,
                "compare_and_swap",
                vec![
                    arg("expected", true, false, false),
                    arg("newValue", true, false, false),
                ],
                int_compare_and_swap,
            ),
        ),
    ]
}

pub fn bool_members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        ("get".to_owned(), native(closure, "get", Vec::new(), bool_get)),
        (
            "set".to_owned(),
            native(
                closure,
                "set",
                vec![arg("amount", true, false, false)],
                bool_set,
            ),
        ),
        (
            "exchange".to_owned(),
            native(
                closure,
                "exchange",
                vec![arg("amount", true, false, false)],
                bool_exchange,
            ),
        ),
        (
            "compare_and_swap".to_owned(),
            native(
                closure,
                "compare_and_swap",
                vec![
                    arg("expected", true, false, false),
                    arg("newValue", true, false, false),
                ],
                bool_compare_and_swap,
            ),
        ),
    ]
}

/// the receiver of a member call, as the counter cell.
fn int_cell(scope: &Scope) -> Option<Rc<RefCell<i64>>> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::AtomicInt(cell) => Some(cell.clone()),
        _ => None,
    }
}

/// the receiver of a member call, as the flag cell.
fn bool_cell(scope: &Scope) -> Option<Rc<RefCell<bool>>> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::AtomicBool(cell) => Some(cell.clone()),
        _ => None,
    }
}

fn int_get(scope: &Scope) -> Obj {
    match int_cell(scope) {
        Some(cell) => obj::int(*cell.borrow()),
        None => obj::err("expected 'atomic_int' object"),
    }
}

fn int_set(scope: &Scope) -> Obj {
    let Some(cell) = int_cell(scope) else {
        return obj::err("expected 'atomic_int' object");
    };

    match obj::payload(&scope::get(scope, "amount")) {
        Payload::Int(v) => {
            *cell.borrow_mut() = v;
            obj::none()
        }
        _ => obj::err("expected 'value' argument to be of type 'int'"),
    }
}

fn int_add(scope: &Scope) -> Obj {
    let Some(cell) = int_cell(scope) else {
        return obj::err("expected 'atomic_int' object");
    };

    match obj::payload(&scope::get(scope, "amount")) {
        Payload::Int(v) => {
            let mut value = cell.borrow_mut();
            *value += v;
            obj::int(*value)
        }
        _ => obj::err("expected 'value' argument to be of type 'int'"),
    }
}

fn int_inc(scope: &Scope) -> Obj {
    match int_cell(scope) {
        Some(cell) => {
            let mut value = cell.borrow_mut();
            *value += 1;
            obj::int(*value)
        }
        None => obj::err("expected 'atomic_int' object"),
    }
}

fn int_dec(scope: &Scope) -> Obj {
    match int_cell(scope) {
        Some(cell) => {
            let mut value = cell.borrow_mut();
            *value -= 1;
            obj::int(*value)
        }
        None => obj::err("expected 'atomic_int' object"),
    }
}

fn int_exchange(scope: &Scope) -> Obj {
    let Some(cell) = int_cell(scope) else {
        return obj::err("expected 'atomic_int' object");
    };

    match obj::payload(&scope::get(scope, "amount")) {
        Payload::Int(v) => {
            let mut value = cell.borrow_mut();
            let previous = *value;
            *value = v;
            obj::int(previous)
        }
        _ => obj::err("expected 'value' argument to be of type 'int'"),
    }
}

/// `Interlocked.CompareExchange(value, new_value, expected) == expected`
fn int_compare_and_swap(scope: &Scope) -> Obj {
    let Some(cell) = int_cell(scope) else {
        return obj::err("expected 'atomic_int' object");
    };

    let expected = match obj::payload(&scope::get(scope, "expected")) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'expected' argument to be of type 'int'"),
    };

    let new_value = match obj::payload(&scope::get(scope, "newValue")) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'new_value' argument to be of type 'int'"),
    };

    let mut value = cell.borrow_mut();
    let swapped = *value == expected;

    if swapped {
        *value = new_value;
    }

    obj::boolean(swapped)
}

fn bool_get(scope: &Scope) -> Obj {
    match bool_cell(scope) {
        Some(cell) => obj::boolean(*cell.borrow()),
        None => obj::err("expected 'atomic_bool' object"),
    }
}

fn bool_set(scope: &Scope) -> Obj {
    let Some(cell) = bool_cell(scope) else {
        return obj::err("expected 'atomic_bool' object");
    };

    match obj::payload(&scope::get(scope, "amount")) {
        Payload::Bool(b) => {
            *cell.borrow_mut() = b;
            obj::none()
        }
        _ => obj::err("expected 'value' argument to be of type 'bool'"),
    }
}

fn bool_exchange(scope: &Scope) -> Obj {
    let Some(cell) = bool_cell(scope) else {
        return obj::err("expected 'atomic_bool' object");
    };

    match obj::payload(&scope::get(scope, "amount")) {
        Payload::Bool(b) => {
            let mut value = cell.borrow_mut();
            let previous = *value;
            *value = b;
            obj::boolean(previous)
        }
        _ => obj::err("expected 'value' argument to be of type 'bool'"),
    }
}

fn bool_compare_and_swap(scope: &Scope) -> Obj {
    let Some(cell) = bool_cell(scope) else {
        return obj::err("expected 'atomic_bool' object");
    };

    let expected = match obj::payload(&scope::get(scope, "expected")) {
        Payload::Bool(b) => b,
        _ => return obj::err("expected 'expected' argument to be of type 'bool'"),
    };

    let new_value = match obj::payload(&scope::get(scope, "newValue")) {
        Payload::Bool(b) => b,
        _ => return obj::err("expected 'new_value' argument to be of type 'bool'"),
    };

    let mut value = cell.borrow_mut();
    let swapped = *value == expected;

    if swapped {
        *value = new_value;
    }

    obj::boolean(swapped)
}
