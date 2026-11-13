use crate::runtime::context::Context;
use crate::runtime::collections;
use crate::runtime::function::ArgData;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use crate::runtime::types::Types;
use crate::runtime::world;
use super::native;
pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    let mut out: Vec<(String, Obj)> = Vec::new();
    out.push(("len".to_owned(), native(closure, "len", vec![ess("value")], len_impl)));
    out.push(("exit".to_owned(), native(closure, "exit", vec![opt("code")], exit_impl)));
    out.push(("type".to_owned(), native(closure, "type", vec![ess("value")], type_impl)));
    out.push(("hash".to_owned(), native(closure, "hash", vec![ess("value")], hash_impl)));
    out.push((
        "throw".to_owned(),
        native(closure, "throw", vec![ess("message"), opt("name")], throw_impl),
    ));
    out.push((
        "throw_if".to_owned(),
        native(
            closure,
            "throw_if",
            vec![ess("condition"), ess("message"), opt("name")],
            throw_if_impl,
        ),
    ));
    out.push((
        "throw_unless".to_owned(),
        native(
            closure,
            "throw_unless",
            vec![ess("condition"), ess("message"), opt("name")],
            throw_unless_impl,
        ),
    ));
    out.push(("chr".to_owned(), native(closure, "chr", vec![ess("code")], chr_impl)));
    out.push(("ord".to_owned(), native(closure, "ord", vec![ess("ch")], ord_impl)));
    out.push(("bin".to_owned(), native(closure, "bin", vec![ess("value")], bin_impl)));
    out.push(("oct".to_owned(), native(closure, "oct", vec![ess("value")], oct_impl)));
    out.push(("hex".to_owned(), native(closure, "hex", vec![ess("value")], hex_impl)));
    out.push(("repr".to_owned(), native(closure, "repr", vec![ess("value")], repr_impl)));
    out.push(("ascii".to_owned(), native(closure, "ascii", vec![ess("value")], ascii_impl)));
    out
}
fn ess(name: &str) -> ArgData {
    super::arg(name, true, false, false)
}
fn opt(name: &str) -> ArgData {
    super::arg(name, false, true, false)
}
fn len_impl(scope: &Scope) -> Obj {
    obj::len(&scope::get(scope, "value"))
}
fn exit_impl(scope: &Scope) -> Obj {
    let code = scope::get(scope, "code");
    let code = match obj::payload(&code) {
        Payload::Int(v) => v as i32,
        _ => 0,
    };
    if !world::allow_exit() {

        return obj::panic("panic", "exit() is not allowed in this environment");
    }

    world::flush_stdout();
    std::process::exit(code);
}
fn type_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    collections::tobj(Types::single(&obj::type_name(&value)))
}
fn hash_impl(scope: &Scope) -> Obj {
    obj::hash(&scope::get(scope, "value"))
}
fn throw_impl(scope: &Scope) -> Obj {
    throw_value(scope::get(scope, "message"), scope::get(scope, "name"))
}
fn throw_value(message: Obj, name: Obj) -> Obj {
    let message = obj::to_str(&message);
    let message_value = match obj::payload(&message) {
        Payload::Str(s) => s,
        _ => return obj::err("cannot convert argument 'message' to a str"),
    };
    let name = if obj::is_none(&name) {
        obj::str("error")
    } else {
        name
    };
    let name = obj::to_str(&name);
    let name_value = match obj::payload(&name) {
        Payload::Str(s) => s,
        _ => return obj::err("cannot convert argument 'name' to a str"),
    };
    obj::err_with_header(name_value.as_ref(), message_value.as_ref())
}
fn throw_if_impl(scope: &Scope) -> Obj {
    let condition = scope::get(scope, "condition");
    match obj::payload(&condition) {
        Payload::Bool(true) => throw_value(scope::get(scope, "message"), scope::get(scope, "name")),
        Payload::Bool(false) => obj::none(),
        _ => obj::err("throw_if: expected bool condition"),
    }
}
fn throw_unless_impl(scope: &Scope) -> Obj {
    let condition = scope::get(scope, "condition");
    match obj::payload(&condition) {
        Payload::Bool(false) => throw_value(scope::get(scope, "message"), scope::get(scope, "name")),
        Payload::Bool(true) => obj::none(),
        _ => obj::err("throw_unless: expected bool condition"),
    }
}
fn chr_impl(scope: &Scope) -> Obj {
    let code = scope::get(scope, "code");
    let code = match obj::payload(&code) {
        Payload::Int(v) => v,
        _ => return obj::err("chr: expected int code"),
    };
    if code < 0 || code > 0x10FFFF {
        return obj::err("chr: code point out of range (0..1114111)");
    }

    match char::from_u32(code as u32) {
        Some(c) => obj::str(&c.to_string()),
        None => obj::panic(
            "System.ArgumentOutOfRangeException",
            "A valid UTF32 value is between 0x000000 and 0x10ffff, inclusive, and should not \
             include surrogate codepoint values (0x00d800 ~ 0x00dfff). (Parameter 'utf32')",
        ),
    }
}
fn ord_impl(scope: &Scope) -> Obj {
    let ch = scope::get(scope, "ch");
    let s = match obj::payload(&ch) {
        Payload::Str(s) => s,
        _ => return obj::err("ord: expected str"),
    };
    if s.is_empty() {
        return obj::err("ord: expected a single character, got empty string");
    }

    obj::int(s.chars().next().unwrap() as u32 as i64)
}
fn int_of(scope: &Scope, name: &str, message: &str) -> Result<i64, Obj> {
    match obj::payload(&scope::get(scope, name)) {
        Payload::Int(v) => Ok(v),
        _ => Err(obj::err(message)),
    }
}
fn bin_impl(scope: &Scope) -> Obj {
    match int_of(scope, "value", "bin: expected int") {
        Ok(v) if v < 0 => obj::str(&format!("-0b{:b}", v.wrapping_neg())),
        Ok(v) => obj::str(&format!("0b{v:b}")),
        Err(e) => e,
    }
}
fn oct_impl(scope: &Scope) -> Obj {
    match int_of(scope, "value", "oct: expected int") {
        Ok(v) if v < 0 => obj::str(&format!("-0o{:o}", v.wrapping_neg())),
        Ok(v) => obj::str(&format!("0o{v:o}")),
        Err(e) => e,
    }
}
fn hex_impl(scope: &Scope) -> Obj {
    match int_of(scope, "value", "hex: expected int") {
        Ok(v) if v < 0 => obj::str(&format!("-0x{:x}", v.wrapping_neg())),
        Ok(v) => obj::str(&format!("0x{v:x}")),
        Err(e) => e,
    }
}
fn repr_impl(scope: &Scope) -> Obj {
    obj::repr(&scope::get(scope, "value"))
}
fn ascii_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    match obj::payload(&value) {

        Payload::Str(s) => match s.encode_utf16().next() {
            Some(unit) if s.encode_utf16().count() == 1 => obj::int(unit as i64),
            _ => obj::err("ascii: expected str"),
        },
        _ => obj::err("ascii: expected str"),
    }
}
