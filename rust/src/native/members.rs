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
use crate::runtime::function;
use crate::runtime::iters;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use crate::runtime::world;

use super::{arg, native};

/// registers the member table of every builtin type.
pub fn init_members(closure: &Context) {
    world::register_original_class("dict", dict_members(closure));
    world::register_original_class("list", list_members(closure));
    world::register_original_class("set", set_members(closure));
    world::register_original_class("str", str_members(closure));
    world::register_original_class("iter", iter_members(closure));
    world::register_original_class("date", crate::native::date_members::members(closure));
    world::register_original_class("pool", crate::native::flow::pool_members(closure));
    world::register_original_class("lock", crate::native::flow::lock_members(closure));
    world::register_original_class("stream", crate::native::stream::members(closure));
    world::register_original_class("stopwatch", crate::native::stopwatch::members(closure));

    // the iterator subclasses have their own type name, and the members live in the
    // `iter` table. the reference walks the `originalBaseTypes` chain for them.
    for name in ["range", "counter", "repeat", "reverse"] {
        world::register_original_class(name, Vec::new());
        world::register_original_base(name, "iter");
    }
}

// ---------------------------------------------------------------------------
// iter
// ---------------------------------------------------------------------------

fn iter_members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "take".to_owned(),
            native(closure, "take", vec![arg("n", true, false, false)], iter_take),
        ),
        (
            "skip".to_owned(),
            native(closure, "skip", vec![arg("n", true, false, false)], iter_skip),
        ),
        (
            "count".to_owned(),
            native(closure, "count", Vec::new(), iter_count),
        ),
        (
            "to_list".to_owned(),
            native(closure, "to_list", Vec::new(), iter_to_list),
        ),
        (
            "to_tuple".to_owned(),
            native(closure, "to_tuple", Vec::new(), iter_to_tuple),
        ),
        (
            "map".to_owned(),
            native(closure, "map", vec![arg("fn", true, false, false)], iter_map),
        ),
        (
            "filter".to_owned(),
            native(closure, "filter", vec![arg("fn", true, false, false)], iter_filter),
        ),
        (
            "any".to_owned(),
            native(closure, "any", vec![arg("fn", true, false, false)], iter_any),
        ),
        (
            "all".to_owned(),
            native(closure, "all", vec![arg("fn", true, false, false)], iter_all),
        ),
        (
            "sum".to_owned(),
            native(closure, "sum", Vec::new(), iter_sum),
        ),
        (
            "first".to_owned(),
            native(closure, "first", Vec::new(), iter_first),
        ),
        (
            "last".to_owned(),
            native(closure, "last", Vec::new(), iter_last),
        ),
    ]
}

/// the receiver of a member call, as an iterator payload.
fn iter_self(scope: &Scope) -> Option<iters::IterCell> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::Iter(st) => Some(st.clone()),
        _ => None,
    }
}

/// the values of an iterator.
fn iter_values(value: &Obj) -> Result<Vec<Obj>, Obj> {
    let list = obj::unary(value, obj::UnOp::ToList);
    match obj::payload(&list) {
        Payload::List(values) => Ok(values.borrow().clone()),
        _ => Err(obj::err("counter is infinite")),
    }
}

fn iter_take(scope: &Scope) -> Obj {
    let Some(st) = iter_self(scope) else {
        return obj::err("expected 'iter' object");
    };
    let n = scope::get(scope, "n");

    let n = match obj::payload(&n) {
        Payload::Int(v) => v,
        _ => return obj::err("invalid argument: 'n'"),
    };

    iters::take_of(&st, n)
}

fn iter_skip(scope: &Scope) -> Obj {
    let Some(st) = iter_self(scope) else {
        return obj::err("expected 'iter' object");
    };
    let n = scope::get(scope, "n");

    let n = match obj::payload(&n) {
        Payload::Int(v) => v,
        _ => return obj::err("invalid argument: 'n'"),
    };

    iters::skip_of(&st, n)
}

fn iter_count(scope: &Scope) -> Obj {
    let Some(st) = iter_self(scope) else {
        return obj::err("expected 'iter' object");
    };

    iters::count_of(&st)
}

fn iter_to_list(scope: &Scope) -> Obj {
    let Some(_st) = iter_self(scope) else {
        return obj::err("expected 'iter' object");
    };

    let list = obj::unary(&scope::get(scope, "self"), obj::UnOp::ToList);
    list
}

fn iter_to_tuple(scope: &Scope) -> Obj {
    let Some(_st) = iter_self(scope) else {
        return obj::err("expected 'iter' object");
    };

    let tuple = obj::unary(&scope::get(scope, "self"), obj::UnOp::ToTuple);
    tuple
}

fn iter_map(scope: &Scope) -> Obj {
    let Some(st) = iter_self(scope) else {
        return obj::err("expected 'iter' object");
    };
    let func = scope::get(scope, "fn");

    if !function::is_func(&func) {
        return obj::err("invalid argument: 'fn'");
    }

    // the reference counts the function's arguments to decide whether the index is
    // passed as well
    let wants_index = function::arg_count(&func) >= 2;

    // the sequence stays lazy, the transform runs while it is consumed
    let def = iters::def_of(&st);
    let mapped = iters::map_new(def, func, wants_index);

    iters::seq_obj(mapped)
}

fn iter_filter(scope: &Scope) -> Obj {
    let Some(st) = iter_self(scope) else {
        return obj::err("expected 'iter' object");
    };
    let func = scope::get(scope, "fn");

    if !function::is_func(&func) {
        return obj::err("invalid argument: 'fn'");
    }

    let def = iters::def_of(&st);
    let filtered = iters::filter_new(def, func);

    iters::seq_obj(filtered)
}

fn iter_any(scope: &Scope) -> Obj {
    let Some(_st) = iter_self(scope) else {
        return obj::err("expected 'iter' object");
    };
    let func = scope::get(scope, "fn");

    if !function::is_func(&func) {
        return obj::err("invalid argument: 'fn'");
    }

    let values = match iter_values(&scope::get(scope, "self")) {
        Ok(values) => values,
        Err(e) => return e,
    };

    for item in values {
        let value = function::fn_call(&func, &function::Args::positional(vec![item]));
        if let Payload::Bool(true) = obj::payload(&value) {
            return obj::boolean(true);
        }
    }

    obj::boolean(false)
}

fn iter_all(scope: &Scope) -> Obj {
    let Some(_st) = iter_self(scope) else {
        return obj::err("expected 'iter' object");
    };
    let func = scope::get(scope, "fn");

    if !function::is_func(&func) {
        return obj::err("invalid argument: 'fn'");
    }

    let values = match iter_values(&scope::get(scope, "self")) {
        Ok(values) => values,
        Err(e) => return e,
    };

    for item in values {
        let value = function::fn_call(&func, &function::Args::positional(vec![item]));
        if let Payload::Bool(false) = obj::payload(&value) {
            return obj::boolean(false);
        }
    }

    obj::boolean(true)
}

fn iter_sum(scope: &Scope) -> Obj {
    let Some(_st) = iter_self(scope) else {
        return obj::err("expected 'iter' object");
    };

    let values = match iter_values(&scope::get(scope, "self")) {
        Ok(values) => values,
        Err(e) => return e,
    };

    let mut sum: Option<Obj> = None;
    for value in values {
        sum = Some(match sum {
            None => value,
            Some(total) => obj::binop(&total, obj::BinOp::Add, &value),
        });
    }

    sum.unwrap_or_else(obj::none)
}

fn iter_first(scope: &Scope) -> Obj {
    let Some(st) = iter_self(scope) else {
        return obj::err("expected 'iter' object");
    };

    let taken = iters::take_of(&st, 1);
    let list = obj::unary(&taken, obj::UnOp::ToList);

    match obj::payload(&list) {
        Payload::List(values) => match values.borrow().first() {
            Some(value) => value.clone(),
            None => obj::none(),
        },
        _ => obj::none(),
    }
}

fn iter_last(scope: &Scope) -> Obj {
    let Some(_st) = iter_self(scope) else {
        return obj::err("expected 'iter' object");
    };

    let list = obj::unary(&scope::get(scope, "self"), obj::UnOp::ToList);

    match obj::payload(&list) {
        Payload::List(values) => match values.borrow().last() {
            Some(value) => value.clone(),
            None => obj::none(),
        },
        _ => obj::none(),
    }
}

// ---------------------------------------------------------------------------
// str
// ---------------------------------------------------------------------------

fn str_members(closure: &Context) -> Vec<(String, Obj)> {
    let one = |name: &str| vec![arg(name, true, false, false)];

    vec![
        (
            "is_empty".to_owned(),
            native(closure, "is_empty", Vec::new(), str_is_empty),
        ),
        (
            "is_number".to_owned(),
            native(closure, "is_number", Vec::new(), str_is_number),
        ),
        (
            "is_alphabet".to_owned(),
            native(closure, "is_alphabet", Vec::new(), str_is_alphabet),
        ),
        (
            "index_of".to_owned(),
            native(closure, "index_of", one("value"), str_index_of),
        ),
        (
            "contains".to_owned(),
            native(closure, "contains", one("value"), str_contains),
        ),
        (
            "starts_with".to_owned(),
            native(closure, "starts_with", one("value"), str_starts_with),
        ),
        (
            "ends_with".to_owned(),
            native(closure, "ends_with", one("value"), str_ends_with),
        ),
        (
            "to_upper".to_owned(),
            native(closure, "to_upper", Vec::new(), str_to_upper),
        ),
        (
            "to_lower".to_owned(),
            native(closure, "to_lower", Vec::new(), str_to_lower),
        ),
        (
            "split".to_owned(),
            native(closure, "split", one("sep"), str_split),
        ),
        (
            "trim".to_owned(),
            native(closure, "trim", one("chars"), str_trim),
        ),
        (
            "join".to_owned(),
            native(closure, "join", one("values"), str_join),
        ),
        (
            "center".to_owned(),
            native(
                closure,
                "center",
                vec![arg("width", true, false, false), arg("fill", false, true, false)],
                str_center,
            ),
        ),
        (
            "left".to_owned(),
            native(
                closure,
                "left",
                vec![arg("width", true, false, false), arg("fill", false, true, false)],
                str_left,
            ),
        ),
        (
            "right".to_owned(),
            native(
                closure,
                "right",
                vec![arg("width", true, false, false), arg("fill", false, true, false)],
                str_right,
            ),
        ),
        (
            "replace".to_owned(),
            native(
                closure,
                "replace",
                vec![
                    arg("old_value", true, false, false),
                    arg("new_value", false, true, false),
                ],
                str_replace,
            ),
        ),
        (
            "find".to_owned(),
            native(closure, "find", one("sub_str"), str_find),
        ),
    ]
}

/// the receiver of a member call, as a `Str` payload.
fn str_self(scope: &Scope) -> Option<String> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

/// the `Str` argument of a member, the reference's `value.As<Str>(out var str)`.
fn str_arg(value: &Obj) -> Result<String, Obj> {
    match obj::payload(value) {
        Payload::Str(s) => Ok(s.to_string()),
        _ => Err(obj::err("invalid argument: value")),
    }
}

fn str_is_empty(scope: &Scope) -> Obj {
    match str_self(scope) {
        Some(s) => obj::boolean(s.is_empty()),
        None => obj::err("expected 'str' object"),
    }
}

fn str_is_number(scope: &Scope) -> Obj {
    match str_self(scope) {
        Some(s) => obj::boolean(s.chars().all(|c| c.is_ascii_digit())),
        None => obj::err("expected 'str' object"),
    }
}

fn str_is_alphabet(scope: &Scope) -> Obj {
    match str_self(scope) {
        Some(s) => obj::boolean(s.chars().all(|c| c.is_alphabetic())),
        None => obj::err("expected 'str' object"),
    }
}

fn str_index_of(scope: &Scope) -> Obj {
    let Some(text) = str_self(scope) else {
        return obj::err("expected 'str' object");
    };
    let value = match str_arg(&scope::get(scope, "value")) {
        Ok(value) => value,
        Err(e) => return e,
    };

    // `String.IndexOf` counts UTF-16 code units
    let index = utf16_index(&text, &value);
    obj::int(index)
}

fn str_contains(scope: &Scope) -> Obj {
    let Some(text) = str_self(scope) else {
        return obj::err("expected 'str' object");
    };
    let value = match str_arg(&scope::get(scope, "value")) {
        Ok(value) => value,
        Err(e) => return e,
    };

    obj::boolean(text.contains(&value))
}

fn str_starts_with(scope: &Scope) -> Obj {
    let Some(text) = str_self(scope) else {
        return obj::err("expected 'str' object");
    };
    let value = match str_arg(&scope::get(scope, "value")) {
        Ok(value) => value,
        Err(e) => return e,
    };

    obj::boolean(text.starts_with(&value))
}

fn str_ends_with(scope: &Scope) -> Obj {
    let Some(text) = str_self(scope) else {
        return obj::err("expected 'str' object");
    };
    let value = match str_arg(&scope::get(scope, "value")) {
        Ok(value) => value,
        Err(e) => return e,
    };

    obj::boolean(text.ends_with(&value))
}

fn str_to_upper(scope: &Scope) -> Obj {
    match str_self(scope) {
        Some(s) => obj::str(&s.to_uppercase()),
        None => obj::err("expected 'str' object"),
    }
}

fn str_to_lower(scope: &Scope) -> Obj {
    match str_self(scope) {
        Some(s) => obj::str(&s.to_lowercase()),
        None => obj::err("expected 'str' object"),
    }
}

fn str_split(scope: &Scope) -> Obj {
    let Some(text) = str_self(scope) else {
        return obj::err("expected 'str' object");
    };
    let sep = match str_arg(&scope::get(scope, "sep")) {
        Ok(sep) => sep,
        Err(e) => return e,
    };

    let parts: Vec<Obj> = if sep.is_empty() {
        text.chars().map(|c| obj::str(&c.to_string())).collect()
    } else {
        text.split(&sep).map(obj::str).collect()
    };

    collections::list(parts)
}

fn str_trim(scope: &Scope) -> Obj {
    let Some(text) = str_self(scope) else {
        return obj::err("expected 'str' object");
    };
    let chars = match str_arg(&scope::get(scope, "chars")) {
        Ok(chars) => chars,
        Err(e) => return e,
    };

    let set: Vec<char> = chars.chars().collect();
    let trimmed = text.trim_matches(|c| set.contains(&c));

    obj::str(trimmed)
}

fn str_join(scope: &Scope) -> Obj {
    let Some(text) = str_self(scope) else {
        return obj::err("expected 'str' object");
    };
    let values = scope::get(scope, "values");

    let iter = obj::unary(&values, obj::UnOp::Iter);
    if !matches!(obj::payload(&iter), Payload::Iter(_)) {
        return obj::err("invalid argument: values");
    }

    let list = obj::unary(&iter, obj::UnOp::ToList);
    let items = match obj::payload(&list) {
        Payload::List(values) => values.borrow().clone(),
        _ => return obj::err("invalid argument: values"),
    };

    let mut parts: Vec<String> = Vec::new();
    for item in items {
        let converted = obj::to_str(&item);
        match obj::payload(&converted) {
            Payload::Str(s) => parts.push(s.to_string()),
            Payload::Error { message, .. } => return obj::err(message),
            _ => {
                return obj::err(format!(
                    "cannot join '{}' with 'str'",
                    obj::type_name(&item)
                ))
            }
        }
    }

    obj::str(&parts.join(&text))
}

fn str_center(scope: &Scope) -> Obj {
    let Some(text) = str_self(scope) else {
        return obj::err("expected 'str' object");
    };

    let (pad, fill_char) = match str_pad(scope, &text) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let left = pad / 2;
    let right = pad - left;

    obj::str(&format!(
        "{}{text}{}",
        fill_char.to_string().repeat(left as usize),
        fill_char.to_string().repeat(right as usize)
    ))
}

fn str_left(scope: &Scope) -> Obj {
    let Some(text) = str_self(scope) else {
        return obj::err("expected 'str' object");
    };

    let (pad, fill_char) = match str_pad(scope, &text) {
        Ok(v) => v,
        Err(e) => return e,
    };

    obj::str(&format!(
        "{text}{}",
        fill_char.to_string().repeat(pad as usize)
    ))
}

fn str_right(scope: &Scope) -> Obj {
    let Some(text) = str_self(scope) else {
        return obj::err("expected 'str' object");
    };

    let (pad, fill_char) = match str_pad(scope, &text) {
        Ok(v) => v,
        Err(e) => return e,
    };

    obj::str(&format!(
        "{}{text}",
        fill_char.to_string().repeat(pad as usize)
    ))
}

/// the padding of `center` / `left` / `right`.
fn str_pad(scope: &Scope, text: &str) -> Result<(i64, char), Obj> {
    let width = scope::get(scope, "width");
    let width = match obj::payload(&width) {
        Payload::Int(v) => v,
        _ => return Err(obj::err("invalid argument: width")),
    };

    let fill = scope::get(scope, "fill");
    let fill = if obj::is_none(&fill) {
        " ".to_owned()
    } else {
        match obj::payload(&fill) {
            Payload::Str(s) => s.to_string(),
            _ => return Err(obj::err("invalid argument: fill")),
        }
    };

    let length = text.encode_utf16().count() as i64;
    let pad = (width - length).max(0);
    let fill_char = fill.chars().next().unwrap_or(' ');

    Ok((pad, fill_char))
}

fn str_replace(scope: &Scope) -> Obj {
    let Some(text) = str_self(scope) else {
        return obj::err("expected 'str' object");
    };

    let old = scope::get(scope, "old_value");
    let old = match obj::payload(&old) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("invalid argument: old_value"),
    };

    let new = scope::get(scope, "new_value");
    let new = if obj::is_none(&new) {
        // the reference calls `As<Str>` on the missing argument, which throws
        return obj::panic(
            "System.NullReferenceException",
            "Object reference not set to an instance of an object.",
        );
    } else {
        match obj::payload(&new) {
            Payload::Str(s) => s.to_string(),
            _ => return obj::err("invalid argument: new_value"),
        }
    };

    // `String.Replace` rejects an empty old value
    if old.is_empty() {
        return obj::panic(
            "System.ArgumentException",
            "String cannot be of zero length.",
        );
    }

    obj::str(&text.replace(&old, &new))
}

fn str_find(scope: &Scope) -> Obj {
    let Some(text) = str_self(scope) else {
        return obj::err("expected 'str' object");
    };
    let sub = scope::get(scope, "sub_str");
    let sub = match obj::payload(&sub) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("invalid argument: sub_str"),
    };

    obj::int(utf16_index(&text, &sub))
}

/// `String.IndexOf` — the index is a UTF-16 code unit offset, `-1` when it is missing.
fn utf16_index(text: &str, needle: &str) -> i64 {
    let units: Vec<u16> = text.encode_utf16().collect();
    let pattern: Vec<u16> = needle.encode_utf16().collect();

    if pattern.is_empty() {
        return 0;
    }

    if pattern.len() > units.len() {
        return -1;
    }

    for i in 0..=(units.len() - pattern.len()) {
        if units[i..i + pattern.len()] == pattern[..] {
            return i as i64;
        }
    }

    -1
}

// ---------------------------------------------------------------------------
// set
// ---------------------------------------------------------------------------

fn set_members(closure: &Context) -> Vec<(String, Obj)> {
    let one = |name: &str| vec![arg(name, true, false, false)];

    vec![
        (
            "add".to_owned(),
            native(closure, "add", one("value"), set_add),
        ),
        (
            "remove".to_owned(),
            native(closure, "remove", one("value"), set_remove),
        ),
        (
            "contains".to_owned(),
            native(closure, "contains", one("value"), set_contains),
        ),
        (
            "clear".to_owned(),
            native(closure, "clear", Vec::new(), set_clear),
        ),
        (
            "clone".to_owned(),
            native(closure, "clone", Vec::new(), set_clone),
        ),
        (
            "union".to_owned(),
            native(closure, "union", one("other"), set_union),
        ),
        (
            "intersect".to_owned(),
            native(closure, "intersect", one("other"), set_intersect),
        ),
        (
            "difference".to_owned(),
            native(closure, "difference", one("other"), set_difference),
        ),
    ]
}

/// the receiver of a member call, as a `Set` payload.
fn set_self(scope: &Scope) -> Option<Rc<RefCell<Vec<Obj>>>> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::Set(v) => Some(v.clone()),
        _ => None,
    }
}

fn set_add(scope: &Scope) -> Obj {
    let Some(set) = set_self(scope) else {
        return obj::err("expected 'set' object");
    };
    let value = scope::get(scope, "value");

    let mut items = set.borrow_mut();
    if items.iter().any(|item| obj::key_eq(item, &value)) {
        return obj::boolean(false);
    }
    items.push(value);
    obj::boolean(true)
}

fn set_remove(scope: &Scope) -> Obj {
    let Some(set) = set_self(scope) else {
        return obj::err("expected 'set' object");
    };
    let value = scope::get(scope, "value");

    let mut items = set.borrow_mut();
    let before = items.len();
    items.retain(|item| !obj::key_eq(item, &value));

    obj::boolean(items.len() != before)
}

fn set_contains(scope: &Scope) -> Obj {
    let Some(set) = set_self(scope) else {
        return obj::err("expected 'set' object");
    };
    let value = scope::get(scope, "value");

    let items = set.borrow().clone();
    obj::boolean(items.iter().any(|item| obj::key_eq(item, &value)))
}

fn set_clear(scope: &Scope) -> Obj {
    let Some(set) = set_self(scope) else {
        return obj::err("expected 'set' object");
    };
    set.borrow_mut().clear();
    obj::none()
}

fn set_clone(scope: &Scope) -> Obj {
    let Some(set) = set_self(scope) else {
        return obj::err("expected 'set' object");
    };
    let items = set.borrow().clone();
    collections::set(items)
}

/// the other set of `union` / `intersect` / `difference`.
fn set_other(scope: &Scope) -> Result<Vec<Obj>, Obj> {
    let other = scope::get(scope, "other");
    match obj::payload(&other) {
        Payload::Set(values) => Ok(values.borrow().clone()),
        _ => Err(obj::err("invalid argument: other")),
    }
}

fn set_union(scope: &Scope) -> Obj {
    let Some(set) = set_self(scope) else {
        return obj::err("expected 'set' object");
    };
    let other = match set_other(scope) {
        Ok(other) => other,
        Err(e) => return e,
    };

    let mut items = set.borrow().clone();
    for item in other {
        if !items.iter().any(|existing| obj::key_eq(existing, &item)) {
            items.push(item);
        }
    }

    collections::set(items)
}

fn set_intersect(scope: &Scope) -> Obj {
    let Some(set) = set_self(scope) else {
        return obj::err("expected 'set' object");
    };
    let other = match set_other(scope) {
        Ok(other) => other,
        Err(e) => return e,
    };

    let items = set.borrow().clone();
    let result: Vec<Obj> = items
        .into_iter()
        .filter(|item| other.iter().any(|o| obj::key_eq(o, item)))
        .collect();

    collections::set(result)
}

fn set_difference(scope: &Scope) -> Obj {
    let Some(set) = set_self(scope) else {
        return obj::err("expected 'set' object");
    };
    let other = match set_other(scope) {
        Ok(other) => other,
        Err(e) => return e,
    };

    let items = set.borrow().clone();
    let result: Vec<Obj> = items
        .into_iter()
        .filter(|item| !other.iter().any(|o| obj::key_eq(o, item)))
        .collect();

    collections::set(result)
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

fn list_members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "add".to_owned(),
            native(
                closure,
                "add",
                vec![arg("value", true, false, false)],
                list_add,
            ),
        ),
        (
            "extend".to_owned(),
            native(
                closure,
                "extend",
                vec![arg("value", true, false, false)],
                list_extend,
            ),
        ),
        (
            "insert".to_owned(),
            native(
                closure,
                "insert",
                vec![arg("obj", true, false, false), arg("index", true, false, false)],
                list_insert,
            ),
        ),
        (
            "extend_insert".to_owned(),
            native(
                closure,
                "extend_insert",
                vec![arg("obj", true, false, false), arg("index", true, false, false)],
                list_extend_insert,
            ),
        ),
        (
            "remove".to_owned(),
            native(
                closure,
                "remove",
                vec![arg("obj", true, false, false)],
                list_remove,
            ),
        ),
        (
            "remove_at".to_owned(),
            native(
                closure,
                "remove_at",
                vec![arg("index", true, false, false)],
                list_remove_at,
            ),
        ),
        (
            "index_of".to_owned(),
            native(
                closure,
                "index_of",
                vec![arg("obj", true, false, false)],
                list_index_of,
            ),
        ),
        (
            "contains".to_owned(),
            native(
                closure,
                "contains",
                vec![arg("obj", true, false, false)],
                list_contains,
            ),
        ),
        (
            "order".to_owned(),
            native(
                closure,
                "order",
                vec![arg("fn", true, false, false)],
                list_order,
            ),
        ),
        (
            "sort".to_owned(),
            native(closure, "sort", Vec::new(), list_sort),
        ),
        (
            "reverse".to_owned(),
            native(closure, "reverse", Vec::new(), list_reverse),
        ),
        (
            "binary_search".to_owned(),
            native(
                closure,
                "binary_search",
                vec![arg("obj", true, false, false)],
                list_binary_search,
            ),
        ),
        (
            "lower_bound".to_owned(),
            native(
                closure,
                "lower_bound",
                vec![arg("obj", true, false, false)],
                list_lower_bound,
            ),
        ),
        (
            "upper_bound".to_owned(),
            native(
                closure,
                "upper_bound",
                vec![arg("obj", true, false, false)],
                list_upper_bound,
            ),
        ),
        (
            "hpush".to_owned(),
            native(
                closure,
                "hpush",
                vec![arg("obj", true, false, false)],
                list_hpush,
            ),
        ),
        (
            "hpop".to_owned(),
            native(closure, "hpop", Vec::new(), list_hpop),
        ),
        (
            "pop".to_owned(),
            native(
                closure,
                "pop",
                vec![arg("index", false, true, false)],
                list_pop,
            ),
        ),
        (
            "map".to_owned(),
            native(
                closure,
                "map",
                vec![arg("type", true, false, false)],
                list_map,
            ),
        ),
        (
            "resize".to_owned(),
            native(closure, "resize", Vec::new(), list_resize),
        ),
    ]
}

/// the receiver of a member call, as a `List` payload.
fn list_self(scope: &Scope) -> Option<(Obj, Rc<RefCell<Vec<Obj>>>)> {
    let receiver = scope::get(scope, "self");
    match obj::payload(&receiver) {
        Payload::List(v) => Some((receiver, v.clone())),
        _ => None,
    }
}

fn list_add(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let value = scope::get(scope, "value");

    list.borrow_mut().push(value);
    obj::none()
}

fn list_extend(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let value = scope::get(scope, "value");

    // `value.Iter()` — a value that is not iterable is appended as it is
    let iter = obj::unary(&value, obj::UnOp::Iter);
    if !matches!(obj::payload(&iter), Payload::Iter(_)) {
        list.borrow_mut().push(value);
        return obj::none();
    }

    // extending a list with itself iterates over a snapshot
    let snapshot: Vec<Obj> = if same_list(&list, &value) {
        list.borrow().clone()
    } else {
        Vec::new()
    };

    if same_list(&list, &value) {
        for item in snapshot {
            list.borrow_mut().push(item);
        }
        return obj::none();
    }

    for item in iterate(&iter) {
        list.borrow_mut().push(item);
    }

    obj::none()
}

/// the values of an iterator payload.
fn iterate(iter: &Obj) -> Vec<Obj> {
    let mut values = Vec::new();
    loop {
        let next = obj::unary(iter, obj::UnOp::Next);
        match obj::payload(&next) {
            Payload::Error { message, .. } if message == "iteration stopped" => break,
            Payload::Panic { .. } => break,
            _ => values.push(next),
        }
    }
    values
}

/// whether two values are the same list object.
fn same_list(list: &Rc<RefCell<Vec<Obj>>>, other: &Obj) -> bool {
    match obj::payload(other) {
        Payload::List(v) => Rc::ptr_eq(&v, list),
        _ => false,
    }
}

/// the index of an `Int` argument, the reference's `index.As<Int>(out var idx)`.
fn list_index(scope: &Scope, name: &str) -> Result<i64, Obj> {
    match obj::payload(&scope::get(scope, name)) {
        Payload::Int(v) => Ok(v),
        _ => Err(obj::err("invalid arguments: index")),
    }
}

fn list_insert(scope: &Scope) -> Obj {
    let Some((receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let value = scope::get(scope, "obj");
    let index = match list_index(scope, "index") {
        Ok(index) => index,
        Err(e) => return e,
    };

    let mut items = list.borrow_mut();
    if items.is_empty() {
        items.push(value);
        return receiver;
    }

    let index = index.max(0) as usize;
    let index = index.min(items.len());
    items.insert(index, obj::clone_obj(&value));

    receiver
}

fn list_extend_insert(scope: &Scope) -> Obj {
    let Some((receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let value = scope::get(scope, "obj");

    let iter = obj::unary(&value, obj::UnOp::Iter);
    if matches!(obj::payload(&iter), Payload::Iter(_)) {
        for item in iterate(&iter) {
            insert_at(&list, item, scope);
        }
    } else {
        insert_at(&list, value, scope);
    }

    receiver
}

/// `Insert(self, obj, index)` — the index comes from the scope.
fn insert_at(list: &Rc<RefCell<Vec<Obj>>>, value: Obj, scope: &Scope) {
    let index = match list_index(scope, "index") {
        Ok(index) => index,
        Err(_) => return,
    };

    let mut items = list.borrow_mut();
    if items.is_empty() {
        items.push(value);
        return;
    }

    let index = index.max(0) as usize;
    let index = index.min(items.len());
    items.insert(index, obj::clone_obj(&value));
}

fn list_remove(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let value = scope::get(scope, "obj");

    let items = list.borrow().clone();
    for (index, item) in items.iter().enumerate() {
        if !obj::key_eq(item, &value) {
            continue;
        }
        let removed = remove_at_index(&list, index as i64);
        return world::bool_obj(removed);
    }

    world::bool_obj(false)
}

/// `RemoveAt` — the index is out of range when it is negative or past the end.
fn remove_at_index(list: &Rc<RefCell<Vec<Obj>>>, index: i64) -> bool {
    let mut items = list.borrow_mut();
    let count = items.len() as i64;
    if index < 0 || index >= count {
        return false;
    }
    items.remove(index as usize);
    true
}

fn list_remove_at(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let index = match list_index(scope, "index") {
        Ok(index) => index,
        Err(e) => return e,
    };

    world::bool_obj(remove_at_index(&list, index))
}

fn list_index_of(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let value = scope::get(scope, "obj");

    let items = list.borrow().clone();
    for (index, item) in items.iter().enumerate() {
        if obj::key_eq(item, &value) {
            return obj::int(index as i64);
        }
    }

    obj::int(-1)
}

fn list_contains(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let value = scope::get(scope, "obj");

    let items = list.borrow().clone();
    let found = items.iter().any(|item| obj::key_eq(item, &value));

    world::bool_obj(found)
}

fn list_order(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let func = scope::get(scope, "fn");

    if !function::is_func(&func) {
        return obj::err("invalid arguments: fn");
    }

    let mut items = list.borrow().clone();
    items.sort_by(|a, b| {
        let left = function::fn_call(&func, &function::Args::positional(vec![a.clone()]));
        let right = function::fn_call(&func, &function::Args::positional(vec![b.clone()]));
        match obj::compare_to(&left, &right) {
            Ok(order) => order.cmp(&0),
            Err(_) => std::cmp::Ordering::Equal,
        }
    });

    *list.borrow_mut() = items;
    obj::none()
}

fn list_sort(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };

    let mut items = list.borrow().clone();
    items.sort_by(|a, b| match obj::compare_to(a, b) {
        Ok(order) => order.cmp(&0),
        Err(_) => std::cmp::Ordering::Equal,
    });

    *list.borrow_mut() = items;
    obj::none()
}

fn list_reverse(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };

    list.borrow_mut().reverse();
    obj::none()
}

fn list_binary_search(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let value = scope::get(scope, "obj");

    let items = list.borrow().clone();

    let mut low = 0i64;
    let mut high = items.len() as i64 - 1;
    while low <= high {
        let middle = low + (high - low) / 2;
        let order = obj::compare_to(&items[middle as usize], &value).unwrap_or(0);
        match order.cmp(&0) {
            std::cmp::Ordering::Equal => return obj::int(middle),
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle - 1,
        }
    }

    // `Array.BinarySearch` returns `~insertionPoint` when the value is missing
    obj::int(!low)
}

fn list_lower_bound(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let value = scope::get(scope, "obj");

    let items = list.borrow().clone();
    let mut low = 0i64;
    let mut high = items.len() as i64 - 1;

    while high > low {
        let middle = (low + high) / 2;
        let less = obj::binop(&items[middle as usize], obj::BinOp::Lt, &value);
        match obj::payload(&less) {
            Payload::Bool(true) => low = middle + 1,
            Payload::Bool(false) => high = middle,
            Payload::Error { message, .. } => return obj::err(message),
            _ => return obj::err("lower_bound requires '<' to return bool"),
        }
    }

    obj::int(high)
}

fn list_upper_bound(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let value = scope::get(scope, "obj");

    let items = list.borrow().clone();
    let mut low = 0i64;
    let mut high = items.len() as i64 - 1;

    while high > low {
        let middle = (low + high) / 2;
        let greater = obj::binop(&items[middle as usize], obj::BinOp::Gt, &value);
        match obj::payload(&greater) {
            Payload::Bool(true) => high = middle,
            Payload::Bool(false) => low = middle + 1,
            Payload::Error { message, .. } => return obj::err(message),
            _ => return obj::err("upper_bound requires '>' to return bool"),
        }
    }

    obj::int(high)
}

fn list_hpush(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let value = scope::get(scope, "obj");

    let mut child = list.borrow().len();
    list.borrow_mut().push(value);

    while child != 0 {
        let parent = (child - 1) / 2;
        let items = list.borrow().clone();
        if obj::compare_to(&items[parent], &items[child]).unwrap_or(0) <= 0 {
            break;
        }
        list.borrow_mut().swap(parent, child);
        child = parent;
    }

    obj::none()
}

fn list_hpop(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };

    if list.borrow().is_empty() {
        return obj::err("list is empty");
    }

    let value = list.borrow()[0].clone();
    let last = list.borrow().len() - 1;
    list.borrow_mut().swap(0, last);
    list.borrow_mut().pop();

    let mut parent = 0usize;
    loop {
        let items = list.borrow().clone();
        let mut smallest = parent;
        let left = 2 * parent + 1;
        let right = 2 * parent + 2;

        if left < items.len()
            && obj::compare_to(&items[left], &items[smallest]).unwrap_or(0) < 0
        {
            smallest = left;
        }
        if right < items.len()
            && obj::compare_to(&items[right], &items[smallest]).unwrap_or(0) < 0
        {
            smallest = right;
        }

        if smallest == parent {
            break;
        }

        list.borrow_mut().swap(parent, smallest);
        parent = smallest;
    }

    value
}

fn list_pop(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let index = scope::get(scope, "index");

    let count = list.borrow().len() as i64;
    let mut index = if obj::is_none(&index) {
        count - 1
    } else {
        match obj::payload(&index) {
            Payload::Int(v) => v,
            _ => return obj::err("invalid argumnets: index"),
        }
    };

    if index < 0 {
        index += count;
    }

    if index < 0 || index >= count {
        return obj::err("list index out of range");
    }

    let value = list.borrow()[index as usize].clone();
    remove_at_index(&list, index);
    value
}

fn list_map(scope: &Scope) -> Obj {
    let Some((_receiver, list)) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    let func = scope::get(scope, "type");

    let items = list.borrow().clone();
    let mut result = Vec::with_capacity(items.len());

    for (index, item) in items.iter().enumerate() {
        let value = function::fn_call(
            &func,
            &function::Args::positional(vec![item.clone(), obj::int(index as i64)]),
        );
        if obj::is_err(&value) {
            return value;
        }
        result.push(value);
    }

    collections::list(result)
}

/// the list payload is a `Vec`, so the reference's capacity growth is a no-op.
fn list_resize(scope: &Scope) -> Obj {
    let Some(_list) = list_self(scope) else {
        return obj::err("expected 'list' object");
    };
    obj::none()
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
