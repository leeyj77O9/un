use crate::native;
use crate::runtime::collections;
use crate::runtime::function;
use crate::runtime::global;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::world;
fn message(o: &Obj) -> String {
    match obj::payload(o) {
        Payload::Panic { name, message } => format!("{name}: {message}"),
        Payload::Error { message, .. } => message,
        _ => obj::display(o),
    }
}
fn include(name: &str, alias: Option<String>, imports: Option<Vec<(String, String)>>) -> Obj {
    match global::include(name, alias, imports) {
        Ok(()) => obj::none(),
        Err(e) => e,
    }
}
fn must_ok(name: &str, alias: Option<String>, imports: Option<Vec<(String, String)>>) {
    let e = include(name, alias, imports);
    assert!(obj::is_none(&e), "`use {name}` failed: {}", message(&e));
}
fn global(name: &str) -> Obj {
    match world::try_get_global(name) {
        Some(o) => o,
        None => panic!("'{name}' is not bound"),
    }
}
fn class(name: &str) -> Obj {
    match world::get_class(name) {
        Some(o) => o,
        None => panic!("class '{name}' is missing"),
    }
}
#[test]
fn use_binds_the_module_object() {
    native::init("");
    must_ok("io", None, None);
    let module = global("io");
    assert_eq!(&*obj::type_name(&module), "module");
    assert!(obj::member(&module, "write").is_some());
    assert!(obj::member(&module, "read").is_some());
}
#[test]
fn use_as_binds_the_alias() {
    native::init("");
    must_ok("io", Some("myio".to_owned()), None);
    assert!(world::try_get_global("io").is_none());
    let module = global("myio");
    assert!(obj::member(&module, "write").is_some());
}
#[test]
fn use_with_names_binds_each_member_into_the_global_scope() {
    native::init("");
    must_ok(
        "io",
        None,
        Some(vec![("write".to_owned(), "write".to_owned())]),
    );
    assert!(world::try_get_global("io").is_none());
    let write = global("write");
    assert!(function::is_func(&write));
}
#[test]
fn use_with_names_and_alias_builds_a_module_of_the_alias_name() {
    native::init("");
    must_ok(
        "io",
        Some("myio".to_owned()),
        Some(vec![("write".to_owned(), "w".to_owned())]),
    );
    let module = global("myio");
    assert!(obj::member(&module, "w").is_some());
    assert!(obj::member(&module, "write").is_none());
}
#[test]
fn an_existing_alias_is_rebuilt_as_the_alias_type() {
    native::init("");
    must_ok(
        "io",
        Some("myio".to_owned()),
        Some(vec![("write".to_owned(), "w".to_owned())]),
    );

    world::register_native("fs", world::native_members("io").unwrap());
    must_ok(
        "fs",
        Some("myio".to_owned()),
        Some(vec![("read".to_owned(), "r".to_owned())]),
    );
    let module = global("myio");
    assert_eq!(&*obj::type_name(&module), "myio");
    assert!(obj::member(&module, "r").is_some());
    assert!(obj::member(&module, "w").is_none());
}
#[test]
fn a_missing_member_is_a_panic() {
    native::init("");
    let err = include(
        "io",
        None,
        Some(vec![("nope".to_owned(), "n".to_owned())]),
    );
    assert_eq!(message(&err), "panic: module 'io' has no member 'nope'");
}
#[test]
fn a_native_module_that_is_not_registered_is_a_panic() {
    native::init("");
    let err = include("nope", None, None);
    assert_eq!(message(&err), "panic: native module 'nope' not found");
}
#[test]
fn a_disallowed_module_is_a_panic() {
    native::init("");
    world::set_allowed(vec!["fs".to_owned()]);
    let err = include("io", None, None);
    assert_eq!(
        message(&err),
        "panic: module 'io' is not allowed in this environment"
    );
}
#[test]
fn a_wildcard_that_repeats_a_member_is_a_panic() {
    native::init("");
    let err = include(
        "io",
        Some("m".to_owned()),
        Some(vec![
            ("*".to_owned(), "*".to_owned()),
            ("*".to_owned(), "*".to_owned()),
        ]),
    );
    assert_eq!(message(&err), "panic: 'write' already exists in module 'm'");
}
#[test]
fn builtin_instances_are_registered_and_their_member_tables_are_filled() {
    native::init("");
    for name in [
        "int", "float", "bool", "str", "date", "list", "tuple", "set", "dict", "iter",
    ] {
        assert!(world::get_class(name).is_some(), "class '{name}' is missing");
    }
    assert_eq!(&*obj::type_name(&class("int")), "int");

    assert!(world::try_get_original_value("str", "to_upper").is_some());
    assert!(world::try_get_original_value("list", "add").is_some());
    assert!(world::try_get_original_value("dict", "contains_key").is_some());

    assert!(world::try_get_original_value("range", "map").is_some());
}
#[test]
fn the_global_scope_holds_the_builtin_functions_and_main_name() {
    native::init("");
    for name in [
        "len", "exit", "type", "hash", "throw", "throw_if", "throw_unless", "chr", "ord",
        "bin", "oct", "hex", "repr", "ascii",
    ] {
        assert!(
            world::try_get_global(name).is_some(),
            "builtin '{name}' is not bound"
        );
    }
    assert_eq!(obj::display(&global("__name__")), "__main__");
    assert!(
        world::try_get_global("io").is_none(),
        "io is not bound by Init"
    );
}
#[test]
fn builtins_run_through_the_shared_native_closure() {
    native::init("");

    let t = obj::call(&global("type"), &[obj::int(1)]);
    assert_eq!(obj::display(&t), "<type: int>");
    let items = collections::list(vec![obj::int(1), obj::int(2), obj::int(3)]);
    let n = obj::call(&global("len"), &[items]);
    assert_eq!(obj::display(&n), "3");

    let r = obj::call(&global("repr"), &[obj::str("a")]);
    assert_eq!(obj::display(&r), "str");
    let c = obj::call(&global("chr"), &[obj::int(65)]);
    assert_eq!(obj::display(&c), "A");
    let e = obj::call(&global("throw"), &[obj::str("boom")]);
    assert_eq!(message(&e), "boom");
    match obj::payload(&e) {
        Payload::Error { header, .. } => assert_eq!(header, "error"),
        _ => panic!("throw must produce an Err"),
    }
}
