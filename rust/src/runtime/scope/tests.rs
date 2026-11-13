use super::*;
use crate::runtime::obj;
fn message(o: &Obj) -> String {
    match obj::payload(o) {
        obj::Payload::Error { message, .. } => message,
        obj::Payload::Panic { name, message } => format!("{name}: {message}"),
        _ => obj::display(o),
    }
}
fn slot(scope: &Scope, key: &str) -> Obj {
    get(scope, key)
}
#[test]
fn declaring_a_slot_stores_the_value() {
    let s = empty();
    declare(&s, "x", Some(obj::int(1)));
    assert_eq!(obj::display(&slot(&s, "x")), "1");
    assert!(contains_key_in_top(&s, "x"));
    assert_eq!(slot_count(&s), 1);
}
#[test]
fn declare_does_not_overwrite_an_existing_slot() {
    let s = empty();
    declare(&s, "x", Some(obj::int(1)));
    declare(&s, "x", Some(obj::int(2)));
    assert_eq!(obj::display(&slot(&s, "x")), "1");
    assert_eq!(slot_count(&s), 1);
}
#[test]
fn missing_variables_use_the_reference_message() {
    let s = empty();
    assert_eq!(message(&get(&s, "x")), "variable 'x' not found");
    assert!(!contains_key(&s, "x"));
}
#[test]
fn child_scopes_read_outer_slots() {
    let outer = empty();
    declare(&outer, "x", Some(obj::int(1)));
    let inner = new(Some(outer.clone()), false);
    assert_eq!(obj::display(&slot(&inner, "x")), "1");
    assert!(contains_key(&inner, "x"));
    assert!(!contains_key_in_top(&inner, "x"));
}
#[test]
fn assignment_in_a_block_scope_updates_the_outer_slot() {

    let outer = empty();
    declare(&outer, "sum", Some(obj::int(0)));
    let body = new(Some(outer.clone()), false);
    set(&body, "sum", obj::int(3));
    assert_eq!(obj::display(&slot(&outer, "sum")), "3");
    assert_eq!(slot_count(&outer), 1);
    assert_eq!(slot_count(&body), 0);
}
#[test]
fn assignment_in_a_function_scope_declares_a_local() {

    let outer = empty();
    declare(&outer, "x", Some(obj::int(1)));
    let fn_scope = new(Some(outer.clone()), true);
    set_local(&fn_scope, "x", obj::int(9));
    assert_eq!(obj::display(&slot(&fn_scope, "x")), "9");
    assert_eq!(obj::display(&slot(&outer, "x")), "1");
    assert_eq!(slot_count(&fn_scope), 1);
}
#[test]
fn set_declares_a_new_local_when_no_scope_has_the_key() {
    let outer = empty();
    let inner = new(Some(outer.clone()), false);
    set(&inner, "y", obj::int(2));
    assert_eq!(obj::display(&slot(&inner, "y")), "2");
    assert!(contains_key_in_top(&inner, "y"));
    assert_eq!(slot_count(&inner), 1);

    assert!(!contains_key_in_top(&outer, "y"));
    assert!(!contains_key(&outer, "y"));
    assert_eq!(slot_count(&outer), 0);
    assert_eq!(message(&get(&outer, "y")), "variable 'y' not found");
}
#[test]
fn set_walks_to_the_scope_that_declares_the_key() {
    let outer = empty();
    declare(&outer, "x", Some(obj::int(1)));
    let middle = new(Some(outer.clone()), false);
    let inner = new(Some(middle.clone()), false);
    set(&inner, "x", obj::int(7));
    assert_eq!(obj::display(&slot(&outer, "x")), "7");
    assert_eq!(slot_count(&middle), 0);
    assert_eq!(slot_count(&inner), 0);
}
#[test]
fn remove_only_drops_the_name() {
    let s = empty();
    declare(&s, "x", Some(obj::int(1)));
    assert!(remove(&s, "x"));
    assert!(!remove(&s, "x"));
    assert!(!contains_key(&s, "x"));

    assert_eq!(slot_count(&s), 1);
}
#[test]
fn function_scope_flag_is_recorded_per_scope() {
    let outer = new(None, false);
    let inner = new(Some(outer.clone()), true);
    assert!(!is_function_scope(&outer));
    assert!(is_function_scope(&inner));
    assert_eq!(parent(&inner).map(|p| is_function_scope(&p)), Some(false));
}
#[test]
fn assignment_uses_the_slot_index_after_repeated_sets() {
    let s = empty();
    set(&s, "a", obj::int(1));
    set(&s, "a", obj::int(2));
    set(&s, "b", obj::int(3));
    assert_eq!(obj::display(&slot(&s, "a")), "2");
    assert_eq!(obj::display(&slot(&s, "b")), "3");
    assert_eq!(slot_count(&s), 2);
}
