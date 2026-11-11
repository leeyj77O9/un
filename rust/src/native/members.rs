//! the member tables of the builtin types.
//!
//! the reference registers the `[Native]` members of a `[NativeType]` class in
//! `Global.originalClasses`, so `value.member` finds them through
//! `Global.TryGetOriginalValue`. the `self` of the member is the receiver, so every
//! function here reads the `self` local.

use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::collections::{self, KeyMap};
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use crate::runtime::world;

use super::{arg, native};

/// registers the member table of every builtin type.
pub fn init_members(closure: &Context) {
    world::register_original_class("dict", dict_members(closure));
}

// ---------------------------------------------------------------------------
// dict
// ---------------------------------------------------------------------------

fn dict_members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "add".to_owned(),
            native(
                closure,
                "add",
                vec![
                    arg("key", true, false, false),
                    arg("value", true, false, false),
                ],
                dict_add,
            ),
        ),
        (
            "remove".to_owned(),
            native(
                closure,
                "remove",
                vec![arg("key", true, false, false)],
                dict_remove,
            ),
        ),
        (
            "get".to_owned(),
            native(
                closure,
                "get",
                vec![
                    arg("key", true, false, false),
                    arg("default_value", false, true, false),
                ],
                dict_get,
            ),
        ),
        (
            "contains_key".to_owned(),
            native(
                closure,
                "contains_key",
                vec![arg("key", true, false, false)],
                dict_contains_key,
            ),
        ),
        (
            "contains_value".to_owned(),
            native(
                closure,
                "contains_value",
                vec![arg("value", true, false, false)],
                dict_contains_value,
            ),
        ),
        (
            "clear".to_owned(),
            native(closure, "clear", Vec::new(), dict_clear),
        ),
        (
            "keys".to_owned(),
            native(closure, "keys", Vec::new(), dict_keys),
        ),
        (
            "values".to_owned(),
            native(closure, "values", Vec::new(), dict_values),
        ),
    ]
}

/// the receiver of a member call, as a `Dict` payload.
fn dict_self(scope: &Scope) -> Option<Rc<RefCell<KeyMap>>> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::Dict(km) => Some(km.clone()),
        _ => None,
    }
}

fn dict_add(scope: &Scope) -> Obj {
    let Some(km) = dict_self(scope) else {
        return obj::err("expected 'dict' object");
    };
    let key = scope::get(scope, "key");
    let value = scope::get(scope, "value");

    // `Dictionary.Add` throws on a duplicate key
    if km.borrow().contains_key(&key) {
        return obj::panic(
            "System.ArgumentException",
            format!(
                "An item with the same key has already been added. Key: {}",
                obj::display(&key)
            ),
        );
    }

    km.borrow_mut().set(&key, &value);
    obj::none()
}

fn dict_remove(scope: &Scope) -> Obj {
    let Some(km) = dict_self(scope) else {
        return obj::err("expected 'dict' object");
    };
    let key = scope::get(scope, "key");

    let removed = km.borrow_mut().remove(&key);
    world::bool_obj(removed)
}

fn dict_get(scope: &Scope) -> Obj {
    let Some(km) = dict_self(scope) else {
        return obj::err("expected 'dict' object");
    };
    let key = scope::get(scope, "key");
    let default = scope::get(scope, "default_value");

    let found = km.borrow().get(&key);
    match found {
        Some(value) => value,
        None => {
            if obj::is_none(&default) {
                obj::none()
            } else {
                default
            }
        }
    }
}

fn dict_contains_key(scope: &Scope) -> Obj {
    let Some(km) = dict_self(scope) else {
        return obj::err("expected 'dict' object");
    };
    let key = scope::get(scope, "key");

    let found = km.borrow().contains_key(&key);
    world::bool_obj(found)
}

fn dict_contains_value(scope: &Scope) -> Obj {
    let Some(km) = dict_self(scope) else {
        return obj::err("expected 'dict' object");
    };
    let value = scope::get(scope, "value");

    let found = km
        .borrow()
        .values()
        .iter()
        .any(|v| obj::key_eq(v, &value));

    world::bool_obj(found)
}

fn dict_clear(scope: &Scope) -> Obj {
    let Some(km) = dict_self(scope) else {
        return obj::err("expected 'dict' object");
    };
    km.borrow_mut().clear();
    obj::none()
}

fn dict_keys(scope: &Scope) -> Obj {
    let Some(km) = dict_self(scope) else {
        return obj::err("expected 'dict' object");
    };
    let keys = km.borrow().keys();
    collections::list(keys)
}

fn dict_values(scope: &Scope) -> Obj {
    let Some(km) = dict_self(scope) else {
        return obj::err("expected 'dict' object");
    };
    let values = km.borrow().values();
    collections::list(values)
}
