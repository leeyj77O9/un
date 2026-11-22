use crate::runtime::context::Context;
use crate::runtime::collections;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use crate::runtime::types::Types;
use crate::runtime::world;
use super::{ess, native, opt};
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
    out.push(("repr".to_owned(), native(closure, "repr", vec![ess("value")], repr_impl)));
    out
}
fn len_impl(scope: &Scope) -> Obj {
    obj::len(&scope::get(scope, "value"))
}
fn exit_impl(scope: &Scope) -> Obj {
    let code = scope::get(scope, "code");
    let code = match obj::payload(&code) {
        Payload::Int(v) => v as i32,
        _ => return obj::err("exit() expects an 'int' code"),
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
fn repr_impl(scope: &Scope) -> Obj {
    obj::repr(&scope::get(scope, "value"))
}
