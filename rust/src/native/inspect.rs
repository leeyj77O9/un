//! `Un.Native.Inspect` — the native `inspect` module.
//!
//! the reference's `Global.GetAllAttrKeys` / `GetAttrDeep` / `HasAttrDeep` walk the
//! object's own members, the original class of its type, and its base chain.

use crate::runtime::collections;
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use crate::runtime::world;

use super::{arg, native};

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "attr".to_owned(),
            native(closure, "attr", vec![arg("value", true, false, false)], attr_impl),
        ),
        (
            "global".to_owned(),
            native(closure, "global", Vec::new(), global_impl),
        ),
        (
            "getattr".to_owned(),
            native(
                closure,
                "getattr",
                vec![
                    arg("value", true, false, false),
                    arg("name", true, false, false),
                    arg("default_value", false, true, false),
                ],
                getattr_impl,
            ),
        ),
        (
            "hasattr".to_owned(),
            native(
                closure,
                "hasattr",
                vec![arg("value", true, false, false), arg("name", true, false, false)],
                hasattr_impl,
            ),
        ),
        (
            "setattr".to_owned(),
            native(
                closure,
                "setattr",
                vec![
                    arg("value", true, false, false),
                    arg("name", true, false, false),
                    arg("new_value", true, false, false),
                ],
                setattr_impl,
            ),
        ),
        (
            "delattr".to_owned(),
            native(
                closure,
                "delattr",
                vec![arg("value", true, false, false), arg("name", true, false, false)],
                delattr_impl,
            ),
        ),
        (
            "meta".to_owned(),
            native(
                closure,
                "meta",
                vec![arg("value", true, false, false), arg("name", true, false, false)],
                meta_impl,
            ),
        ),
    ]
}

/// `Global.GetAllAttrKeys` — the object's own members, the original class of its type
/// and the members of its base chain, without duplicates.
fn attr_keys(value: &Obj) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();

    fn push(name: String, keys: &mut Vec<String>) {
        if !keys.contains(&name) {
            keys.push(name);
        }
    }

    for (name, _) in value.borrow().members.iter() {
        push(name.clone(), &mut keys);
    }

    let type_name = obj::type_name(value).to_string();
    if let Some(attrs) = world::original_class_attrs(&type_name) {
        for (name, _) in attrs {
            push(name, &mut keys);
        }
    }

    for base in obj::base_chain(value) {
        for (name, _) in base.borrow().members.iter() {
            push(name.clone(), &mut keys);
        }
    }

    keys
}

fn attr_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let keys = attr_keys(&value);

    collections::list(keys.into_iter().map(|k| obj::str(&k)).collect())
}

fn global_impl(_scope: &Scope) -> Obj {
    let scope = world::global_scope();
    let entries: Vec<(Obj, Obj)> = scope::local_symbols(&scope)
        .into_iter()
        .map(|(name, value)| (obj::str(&name), value))
        .collect();

    collections::dict(entries)
}

/// `Global.GetAttrDeep`
fn getattr_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let name = scope::get(scope, "name");
    let default = scope::get(scope, "default_value");

    let name = match obj::payload(&obj::to_str(&name)) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("expected 'name' argument to be of type 'str'"),
    };

    match get_attr_deep(&value, &name) {
        Some(result) => result,
        None => {
            if obj::is_none(&default) {
                obj::err(format!(
                    "'{}' object has no attribute '{name}'",
                    obj::type_name(&value)
                ))
            } else {
                default
            }
        }
    }
}

/// `Global.HasAttrDeep`
fn hasattr_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let name = scope::get(scope, "name");

    let name = match obj::payload(&obj::to_str(&name)) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("expected 'name' argument to be of type 'str'"),
    };

    world::bool_obj(has_attr_deep(&value, &name))
}

fn setattr_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let name = scope::get(scope, "name");
    let new_value = scope::get(scope, "new_value");

    let name = match obj::payload(&obj::to_str(&name)) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("expected 'name' argument to be of type 'str'"),
    };

    obj::set_attr(&value, &name, &new_value);
    new_value
}

fn delattr_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let name = scope::get(scope, "name");

    let name = match obj::payload(&obj::to_str(&name)) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("expected 'name' argument to be of type 'str'"),
    };

    let removed = {
        let mut d = value.borrow_mut();
        let before = d.members.len();
        d.members.retain(|(k, _)| *k != name);
        d.members.len() != before
    };
    if !removed {
        return obj::err(format!("attribute '{name}' not found"));
    }

    obj::none()
}

fn meta_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let name = scope::get(scope, "name");

    let name = match obj::payload(&obj::to_str(&name)) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("expected 'name' argument to be of type 'str'"),
    };

    let meta = value
        .borrow()
        .annotations
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, meta)| meta.clone());

    match meta {
        Some(meta) => meta,
        None => obj::err(format!("attribute '{name}' not found")),
    }
}

/// `Global.GetAttrDeep` — the own member, the original class value, then the base chain.
fn get_attr_deep(value: &Obj, name: &str) -> Option<Obj> {
    if let Some(member) = obj::member(value, name) {
        return Some(member);
    }

    let type_name = obj::type_name(value).to_string();
    if let Some(found) = world::try_get_original_value(&type_name, name) {
        return Some(found);
    }

    for base in obj::base_chain(value) {
        if let Some(member) = obj::member(&base, name) {
            return Some(member);
        }
    }

    None
}

/// `Global.HasAttrDeep`
fn has_attr_deep(value: &Obj, name: &str) -> bool {
    if obj::member(value, name).is_some() {
        return true;
    }

    let type_name = obj::type_name(value).to_string();
    if world::try_get_original_value(&type_name, name).is_some() {
        return true;
    }

    obj::base_chain(value)
        .iter()
        .any(|base| obj::member(base, name).is_some())
}
