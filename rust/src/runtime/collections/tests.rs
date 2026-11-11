//! Phase 7 collection tests. every message is copied verbatim from the reference
//! implementation, and the host-exception cases record the inner .NET message with the
//! `TargetInvocationException` wrapper dropped (D-024).

use super::*;
use crate::runtime::obj;

fn message(o: &Obj) -> String {
    match obj::payload(o) {
        Payload::Error { message, .. } => message,
        Payload::Panic { name, message } => format!("{name}: {message}"),
        _ => obj::display(o),
    }
}

fn s(v: &str) -> Obj {
    obj::str(v)
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

#[test]
fn list_index_rules() {
    let l = list(vec![obj::int(10), obj::int(20), obj::int(30)]);
    assert_eq!(obj::display(&obj::get_item(&l, &obj::int(0))), "10");
    // `index + Count` is the negative wrap, cast through `(int)`
    assert_eq!(obj::display(&obj::get_item(&l, &obj::int(-1))), "30");
    assert_eq!(obj::display(&obj::get_item(&l, &obj::int(-3))), "10");
    assert_eq!(message(&obj::get_item(&l, &obj::int(3))), "list index out of range");
    assert_eq!(
        message(&obj::get_item(&l, &obj::int(-4))),
        "list index out of range"
    );
    assert_eq!(message(&obj::get_item(&l, &s("a"))), "invalid index type");

    let v = obj::int(7);
    obj::set_item(&l, &obj::int(-2), &v);
    assert_eq!(obj::display(&obj::get_item(&l, &obj::int(1))), "7");
    assert_eq!(
        message(&obj::set_item(&l, &obj::int(9), &v)),
        "list index out of range"
    );
    assert_eq!(message(&obj::set_item(&l, &s("a"), &v)), "invalid index type");
}

#[test]
fn list_display_and_bool() {
    let l = list(vec![s("a"), obj::int(1), obj::none()]);
    assert_eq!(obj::display(&l), "['a', 1, none]");
    assert_eq!(obj::display(&list(Vec::new())), "[]");
    assert_eq!(obj::display(&obj::to_bool(&l)), "true");
    assert_eq!(obj::display(&obj::to_bool(&list(Vec::new()))), "false");
    assert_eq!(obj::display(&obj::len(&l)), "3");
}

#[test]
fn list_slice_rules() {
    let l = list(vec![obj::int(1), obj::int(2), obj::int(3), obj::int(4)]);
    assert_eq!(obj::display(&obj::slice(&l, None, None, None)), "[1, 2, 3, 4]");
    assert_eq!(obj::display(&obj::slice(&l, Some(obj::int(1)), None, None)), "[2, 3, 4]");
    assert_eq!(
        obj::display(&obj::slice(&l, Some(obj::int(-2)), Some(obj::int(-1)), None)),
        "[3]"
    );
    assert_eq!(
        obj::display(&obj::slice(&l, Some(obj::int(3)), Some(obj::int(0)), Some(obj::int(-2)))),
        "[4, 2]"
    );
    assert_eq!(obj::display(&obj::slice(&l, Some(obj::int(2)), Some(obj::int(2)), None)), "[]");
    assert_eq!(
        message(&obj::slice(&l, Some(obj::int(0)), None, Some(obj::int(0)))),
        "slice step cannot be zero"
    );
    assert_eq!(
        message(&obj::slice(&l, Some(s("a")), None, None)),
        "slice start must be int"
    );
    assert_eq!(
        message(&obj::slice(&l, None, Some(s("a")), None)),
        "slice end must be int"
    );
    assert_eq!(
        message(&obj::slice(&l, None, None, Some(s("a")))),
        "slice step must be int"
    );
}

#[test]
fn list_membership_and_unops() {
    let l = list(vec![obj::int(1), obj::int(2)]);
    // `List.In` only accepts a list or tuple needle, the type name comes from `Type`
    assert_eq!(
        message(&obj::obj_in(&l, &obj::int(1))),
        "cannot check if 'int' is in 'list'"
    );
    assert_eq!(obj::display(&obj::obj_in(&l, &list(vec![obj::int(1)]))), "true");
    assert_eq!(
        obj::display(&obj::obj_in(&l, &list(vec![obj::int(3)]))),
        "false"
    );
    // a tuple needle works, the element comparison is `Eq`
    assert_eq!(
        obj::display(&obj::obj_in(&l, &tup(vec![obj::int(2)], Vec::new()))),
        "true"
    );

    // `Spreads.Spread() => this`, and `Spreads` has no `ToStr` override, so the
    // display falls back to `Repr()`, the type name
    let sp = obj::spread(&l);
    assert!(Rc::ptr_eq(&obj::spread(&sp), &sp));
    assert_eq!(obj::display(&sp), "spread");

    // `List.ToList` copies the elements, `List.ToTuple` keeps them with empty names
    assert_eq!(obj::display(&obj::to_list(&l)), "[1, 2]");
    assert_eq!(obj::display(&obj::to_tuple(&l)), "(1, 2)");
    // `Iter` is wired in the iterator phase
}

#[test]
fn list_equality_is_pairwise() {
    let a = list(vec![obj::int(1), obj::int(2)]);
    let b = list(vec![obj::int(1), obj::int(2)]);
    let c = list(vec![obj::int(1)]);
    assert_eq!(obj::display(&obj::binop(&a, BinOp::Eq, &b)), "true");
    assert_eq!(obj::display(&obj::binop(&a, BinOp::Eq, &c)), "false");
    assert_eq!(obj::display(&obj::binop(&a, BinOp::Eq, &obj::int(1))), "false");

    // `NEq` of two str/int elements is an Err, and `Eq` propagates it.
    // the reference `Str.NEq` prints `==` in its message, a documented quirk.
    let x = list(vec![s("a")]);
    let y = list(vec![obj::int(1)]);
    assert_eq!(
        message(&obj::binop(&x, BinOp::Eq, &y)),
        "unsupported operand type(s) for ==: 'str' and 'int'"
    );
    // a non-boolean `NEq` result is the recorded message
    let i = list(vec![obj::int(1)]);
    let t = list(vec![obj::float(1.0)]);
    assert_eq!(obj::display(&obj::binop(&i, BinOp::Eq, &t)), "true");
}

#[test]
fn list_copy_is_the_same_cell_and_clone_is_deep() {
    let inner = list(vec![obj::int(1)]);
    let outer = list(vec![inner.clone()]);
    assert!(Rc::ptr_eq(&obj::copy_obj(&outer), &outer));
    let cloned = obj::clone_obj(&outer);
    assert!(!Rc::ptr_eq(&cloned, &outer));
    let deep = obj::clone_obj(&cloned);
    assert!(!Rc::ptr_eq(&deep, &cloned));
}

#[test]
fn list_init_rules() {
    let l = list(vec![obj::int(1)]);
    assert!(Rc::ptr_eq(&obj::init(&l, &[]), &l));
    // one argument converts, an unconvertible argument becomes the argument list
    let converted = obj::init(&list(Vec::new()), &[list(vec![obj::int(5)])]);
    assert_eq!(obj::display(&converted), "[5]");
    let kept = obj::init(&list(Vec::new()), &[obj::int(5)]);
    assert_eq!(obj::display(&kept), "[5]");
    let many = obj::init(&list(Vec::new()), &[obj::int(1), obj::int(2)]);
    assert_eq!(obj::display(&many), "[1, 2]");
}

// ---------------------------------------------------------------------------
// Dict
// ---------------------------------------------------------------------------

#[test]
fn dict_key_lookup() {
    let d = dict(vec![(s("a"), obj::int(1))]);
    assert_eq!(obj::display(&obj::get_item(&d, &s("a"))), "1");
    assert_eq!(
        message(&obj::get_item(&d, &s("b"))),
        "key 'b' not found in dictionary"
    );
    // an int key keeps insertion order in the reference too, so display is stable
    let di = dict(vec![(obj::int(1), s("x"))]);
    assert_eq!(obj::display(&obj::get_item(&di, &obj::int(1))), "x");
    assert_eq!(
        message(&obj::get_item(&di, &obj::int(2))),
        "key '2' not found in dictionary"
    );

    obj::set_item(&d, &s("b"), &obj::int(2));
    assert_eq!(obj::display(&obj::len(&d)), "2");
    // `Dict.ToStr` prints `key:value` with `Str.To`, so str keys are not quoted
    assert_eq!(obj::display(&d), "{a:1, b:2}");
    assert_eq!(obj::display(&obj::obj_in(&d, &s("a"))), "true");
    assert_eq!(obj::display(&obj::obj_in(&d, &s("z"))), "false");
}

#[test]
fn dict_add_rejects_a_duplicate_key() {
    let d = dict(vec![(s("a"), obj::int(1))]);
    let km = match obj::payload(&d) {
        Payload::Dict(km) => km,
        _ => panic!("dict payload"),
    };
    // the inner .NET message, wrapper dropped (D-024)
    assert_eq!(
        message(&km.borrow_mut().add(&s("a"), &obj::int(2))),
        "System.ArgumentException: An item with the same key has already been added. Key: a"
    );
    assert_eq!(km.borrow().entries.len(), 1);
    // the key has to be present before the re-add throws, so it is inserted first
    km.borrow_mut().add(&obj::int(5), &obj::int(1));
    // a str key prints its value, an int key prints its CLR number
    assert_eq!(
        message(&km.borrow_mut().add(&obj::int(5), &obj::int(2))),
        "System.ArgumentException: An item with the same key has already been added. Key: 5"
    );
    // `True` is the CLR form, not Un's display
    let boolkey = obj::boolean(true);
    km.borrow_mut().add(&boolkey, &obj::int(1));
    assert_eq!(
        message(&km.borrow_mut().add(&boolkey, &obj::int(2))),
        "System.ArgumentException: An item with the same key has already been added. Key: True"
    );
    km.borrow_mut().remove(&boolkey);
    assert!(!km.borrow_mut().remove(&boolkey));
    assert!(km.borrow_mut().remove(&obj::int(5)));
    km.borrow_mut().clear();
    assert!(km.borrow().is_empty());
}

#[test]
fn dict_conversion_and_init() {
    let d = dict(vec![(s("a"), obj::int(1))]);
    // every entry becomes a named pair
    let pairs = obj::to_list(&d);
    assert_eq!(obj::display(&pairs), "[(a, 1)]");
    // `Spreads` has no `ToStr`, so the spread cell displays as its type name
    assert_eq!(obj::display(&obj::spread(&d)), "spread");
    assert_eq!(obj::display(&obj::to_tuple(&d)), "((a, 1))");

    // 0 args is a new empty dict, never the receiver
    let fresh = obj::init(&d, &[]);
    assert!(!Rc::ptr_eq(&fresh, &d));
    assert_eq!(obj::display(&fresh), "{}");

    // a named tuple becomes name -> value entries. the names survive on the
    // arguments tuple, so `dict(t)` keys are the names (`tuple(key = 1, value = 2)`
    // prints `(1, 2)` and `dict` of it prints `{key:1, value:2}` in the reference)
    let t = tup(
        vec![obj::int(1), obj::int(2)],
        vec!["key".to_owned(), "value".to_owned()],
    );
    let made = obj::init(&dict(Vec::new()), &[t]);
    assert_eq!(obj::display(&made), "{key:1, value:2}");
    assert_eq!(
        message(&obj::init(&dict(Vec::new()), &[obj::int(1)])),
        "invaild 'dict' initialize"
    );
    assert_eq!(
        message(&obj::init(
            &dict(Vec::new()),
            &[obj::int(1), obj::int(2)]
        )),
        "invaild 'dict' initialize"
    );
}

#[test]
fn dict_overlap_and_clone() {
    let a = dict(vec![(s("a"), obj::int(1)), (s("b"), obj::int(2))]);
    let sub = dict(vec![(s("a"), obj::int(1))]);
    assert_eq!(obj::display(&obj::obj_in(&a, &sub)), "true");
    let bad = dict(vec![(s("a"), obj::int(9))]);
    assert_eq!(obj::display(&obj::obj_in(&a, &bad)), "false");

    // `Copy` is `this`, `Clone` is a shallow new cell
    assert!(Rc::ptr_eq(&obj::copy_obj(&a), &a));
    let c = obj::clone_obj(&a);
    assert!(!Rc::ptr_eq(&c, &a));
    assert_eq!(obj::display(&c), obj::display(&a));
    obj::set_item(&c, &s("c"), &obj::int(3));
    assert_eq!(obj::display(&a), "{a:1, b:2}");
    assert_eq!(obj::display(&c), "{a:1, b:2, c:3}");
}

// ---------------------------------------------------------------------------
// Set
// ---------------------------------------------------------------------------

#[test]
fn set_lookup_and_operators() {
    let s1 = set(vec![obj::int(1), obj::int(2), obj::int(2)]);
    // `HashSet.TryGetValue` returns the stored element
    assert_eq!(obj::display(&obj::get_item(&s1, &obj::int(2))), "2");
    assert_eq!(obj::display(&obj::len(&s1)), "2");
    assert_eq!(obj::display(&s1), "{1, 2}");
    assert_eq!(
        message(&obj::get_item(&s1, &obj::int(5))),
        "key 5 not found in set"
    );

    let s2 = set(vec![obj::int(2), obj::int(3)]);
    assert_eq!(obj::display(&obj::binop(&s1, BinOp::Add, &s2)), "{1, 2, 3}");
    assert_eq!(obj::display(&obj::binop(&s1, BinOp::Sub, &s2)), "{1}");
    assert_eq!(obj::display(&obj::binop(&s1, BinOp::BXor, &s2)), "{2}");
    // the type name in the operator message is hard-coded 'set'
    assert_eq!(
        message(&obj::binop(&s1, BinOp::Add, &obj::int(1))),
        "unsupported operand type(s) for +: 'set' and 'int'"
    );
    // `Set` has no `In` override, and `x in set` is the base error
    assert_eq!(
        message(&obj::obj_in(&s1, &obj::int(1))),
        "unsupported operand type(s) for in: 'set'"
    );
    assert_eq!(
        message(&obj::obj_in(&s1, &s2)),
        "unsupported operand type(s) for in: 'set'"
    );
    assert_eq!(
        message(&obj::to_bool(&s1)),
        "unsupported operand type(s) for bool(): 'set'"
    );
    // `Set.Spread` is a `Spreads` cell, whose display is the type name (no `ToStr`)
    let sp = obj::spread(&s1);
    assert_eq!(obj::display(&sp), "spread");
    // `repr(s)` is the base `Repr()`, the type name
    assert_eq!(obj::display(&obj::repr(&s1)), "set");

    // `Init` is `new Set([.. args.ToList()])`, so it deduplicates
    let made = obj::init(
        &set(Vec::new()),
        &[obj::int(1), obj::int(1), obj::int(3)],
    );
    assert_eq!(obj::display(&made), "{1, 3}");
    // `Copy` is `this`
    assert!(Rc::ptr_eq(&obj::copy_obj(&s1), &s1));
    let c = obj::clone_obj(&s1);
    assert!(!Rc::ptr_eq(&c, &s1));
    assert_eq!(obj::display(&c), "{1, 2}");
}

// ---------------------------------------------------------------------------
// Tup
// ---------------------------------------------------------------------------

#[test]
fn tup_index_and_names() {
    let t = tup(
        vec![obj::int(1), obj::int(2)],
        vec!["x".to_owned(), "y".to_owned()],
    );
    assert_eq!(obj::display(&obj::get_item(&t, &obj::int(0))), "1");
    assert_eq!(obj::display(&obj::get_item(&t, &obj::int(-1))), "2");
    assert_eq!(obj::display(&obj::get_item(&t, &s("x"))), "1");
    assert_eq!(obj::display(&obj::get_item(&t, &s("y"))), "2");
    assert_eq!(
        message(&obj::get_item(&t, &s("z"))),
        "tuple has no attribute 'z'"
    );
    assert_eq!(message(&obj::get_item(&t, &obj::int(2))), "tuple index out of range");
    assert_eq!(
        message(&obj::get_item(&t, &obj::int(-3))),
        "tuple index out of range"
    );
    assert_eq!(message(&obj::get_item(&t, &obj::float(1.0))), "invalid index type");

    assert_eq!(obj::display(&obj::len(&t)), "2");
    assert_eq!(obj::display(&obj::to_bool(&t)), "true");
    assert_eq!(obj::display(&obj::to_bool(&tup(Vec::new(), Vec::new()))), "false");
    assert_eq!(obj::display(&t), "(1, 2)");

    // a slice keeps the parallel names
    let sliced = obj::slice(&t, Some(obj::int(0)), Some(obj::int(2)), Some(obj::int(1)));
    assert_eq!(obj::display(&obj::get_item(&sliced, &s("x"))), "1");
    assert_eq!(obj::display(&obj::get_item(&sliced, &s("y"))), "2");
    // a negative step runs `i > e` from the clamped `s`, so it is not a reversal
    let rev = obj::slice(&t, Some(obj::int(1)), Some(obj::int(0)), Some(obj::int(-1)));
    assert_eq!(obj::display(&rev), "(2)");
    assert_eq!(obj::display(&obj::get_item(&rev, &s("y"))), "2");
    // `s` clamps to `len`, and the reference indexes past the end, so this throws
    assert_eq!(
        message(&obj::slice(
            &t,
            Some(obj::int(2)),
            Some(obj::int(0)),
            Some(obj::int(-1))
        )),
        "System.IndexOutOfRangeException: Index was outside the bounds of the array."
    );
    // `end` clamps to 0, so a negative step can never reach index 0: slicing cannot
    // reverse a tuple in the reference
    let from_last = obj::slice(&t, Some(obj::int(1)), Some(obj::int(-3)), Some(obj::int(-1)));
    assert_eq!(obj::display(&from_last), "(2)");
}

#[test]
fn tup_membership_and_equality() {
    let t = tup(vec![obj::int(1), obj::int(2)], Vec::new());
    assert_eq!(obj::display(&obj::obj_in(&t, &obj::int(2))), "true");
    assert_eq!(obj::display(&obj::obj_in(&t, &obj::int(3))), "false");
    // a str needle is an `Eq` error from the int element, and `In` propagates it
    assert_eq!(
        message(&obj::obj_in(&t, &s("a"))),
        "unsupported operand type(s) for ==: 'int' and 'str'"
    );

    let a = tup(vec![obj::int(1), obj::int(2)], vec!["x".to_owned(), "y".to_owned()]);
    let b = tup(vec![obj::int(1), obj::int(2)], Vec::new());
    assert_eq!(obj::display(&obj::binop(&a, BinOp::Eq, &b)), "true");
    assert_eq!(obj::display(&obj::binop(&a, BinOp::Eq, &obj::int(1))), "false");
    // names are not part of equality
    let c = tup(vec![obj::int(1)], vec!["x".to_owned()]);
    assert_eq!(obj::display(&obj::binop(&a, BinOp::Eq, &c)), "false");
}

#[test]
fn tup_init_copy_clone() {
    let t = tup(vec![obj::int(1)], Vec::new());
    assert!(Rc::ptr_eq(&obj::init(&t, &[]), &t));
    // 1 argument converts when it can
    let from_list = obj::init(&t, &[list(vec![obj::int(5), obj::int(6)])]);
    assert_eq!(obj::display(&from_list), "(5, 6)");
    // 2+ arguments become the args tuple
    let many = obj::init(&t, &[obj::int(1), obj::int(2)]);
    assert_eq!(obj::display(&many), "(1, 2)");
    // `Copy` copies the elements through their own `Copy`, and `List.Copy` is `this`,
    // so a list element stays the same cell
    let inner = list(vec![obj::int(1)]);
    let outer = tup(vec![inner.clone()], Vec::new());
    let copied = obj::copy_obj(&outer);
    assert!(!Rc::ptr_eq(&copied, &outer));
    let copied_inner = obj::get_item(&copied, &obj::int(0));
    assert!(Rc::ptr_eq(&copied_inner, &inner));
    assert!(Rc::ptr_eq(&obj::get_item(&outer, &obj::int(0)), &inner));
    // `Clone` copies through `Clone`, and `Int.Clone()` keeps its value, so the cloned
    // element is a fresh int cell holding the same number (D-026)
    let cloned = obj::clone_obj(&outer);
    let cloned_inner = obj::get_item(&cloned, &obj::int(0));
    assert!(!Rc::ptr_eq(&cloned_inner, &inner));
    assert_eq!(obj::display(&cloned_inner), "[1]");
}

// ---------------------------------------------------------------------------
// Spread
// ---------------------------------------------------------------------------

#[test]
fn spread_is_a_one_way_cell() {
    let sp = spread(vec![obj::int(1), obj::int(2)]);
    // `Spreads` has no `ToStr` / `Len` / `ToBool` override, so `display` is the base
    // `Repr()` and `len` is the base message without a type name
    assert_eq!(obj::display(&sp), "spread");
    assert!(Rc::ptr_eq(&obj::spread(&sp), &sp));
    assert_eq!(
        message(&obj::len(&sp)),
        "unsupported operand type(s) for len()"
    );
    assert_eq!(
        message(&obj::to_bool(&sp)),
        "unsupported operand type(s) for bool(): 'spread'"
    );
    // the base clone rebuilds the payload as a new cell
    let c = obj::clone_obj(&sp);
    assert!(!Rc::ptr_eq(&c, &sp));
    assert!(Rc::ptr_eq(&obj::spread(&c), &c));
    assert_eq!(obj::display(&c), "spread");
}

// ---------------------------------------------------------------------------
// Json
// ---------------------------------------------------------------------------

#[test]
fn json_init_and_display() {
    let j0 = obj::init(&json(obj::none()), &[]);
    assert_eq!(obj::display(&j0), "null");

    let j = obj::init(&json(obj::none()), &[s("{\"a\": 1, \"b\": [1, 2]}")]);
    assert_eq!(obj::display(&obj::len(&j)), "2");
    assert_eq!(obj::display(&j), "{\n \"a\": 1,\n \"b\": [1, 2]\n}");

    let from_dict = obj::init(&json(obj::none()), &[dict(vec![(s("a"), obj::int(1))])]);
    assert_eq!(obj::display(&from_dict), "{\n \"a\": 1\n}");

    // `len` of a scalar document is 1
    let jn = obj::init(&json(obj::none()), &[s("7")]);
    assert_eq!(obj::display(&obj::len(&jn)), "1");
    assert_eq!(obj::display(&jn), "7");

    assert_eq!(
        message(&obj::init(&json(obj::none()), &[obj::int(1)])),
        "invalid json: expected string, dict, or list, got 'int'"
    );
    assert_eq!(
        message(&obj::init(
            &json(obj::none()),
            &[s("1"), s("2")]
        )),
        "invalid json: expected 1 argument, got 2"
    );
}

#[test]
fn json_number_and_string_rules() {
    // `TryGetInt64` runs first, so exponent notation is an int
    assert_eq!(obj::display(&json_parse("1e2")), "100");
    assert_eq!(obj::display(&json_parse("1.5")), "1.5");
    assert_eq!(obj::display(&json_parse("-3")), "-3");
    // the reference ground truth for this value is the scientific form (probe)
    assert_eq!(
        obj::display(&json_parse("12345678901234567890123")),
        "1.2345678901234568E+22"
    );
    assert_eq!(obj::display(&json_parse("true")), "true");
    // the parsed document is wrapped in `Json`, so `null` prints as `null` and a
    // string document prints as a quoted, escaped JSON string
    assert_eq!(obj::display(&json_parse("null")), "null");
    // comments are skipped, as `JsonDocument.Parse` does
    assert_eq!(obj::display(&json_parse("// c\n{ \"a\": 1 }")), "{\n \"a\": 1\n}");
    assert_eq!(obj::display(&json_parse("/* c */ 3")), "3");
    assert_eq!(obj::display(&json_parse("[ 1 , 2 ]")), "[1, 2]");
    // escapes
    assert_eq!(obj::display(&json_parse("\"a\\tb\\u0041\"")), "\"a\\tbA\"");
    // malformed documents throw inside the host
    let bad = json_parse("{ \"a\": }");
    assert!(obj::is_panic(&bad));
    match obj::payload(&bad) {
        Payload::Panic { name, .. } => {
            assert_eq!(name, "System.Text.Json.JsonException");
        }
        _ => panic!("json parse must yield a panic payload"),
    }
}

#[test]
fn json_escape_matches_the_reference() {
    assert_eq!(escape("a\"b\\c"), "a\\\"b\\\\c");
    assert_eq!(escape("a\nb\rc\td"), "a\\nb\\rc\\td");
    assert_eq!(escape("a\u{8}b\u{c}c"), "a\\bb\\fc");
    assert_eq!(escape("plain"), "plain");
}

// ---------------------------------------------------------------------------
// Stru
// ---------------------------------------------------------------------------

#[test]
fn stru_display_len_and_init() {
    let p = stru("Point", vec!["x".to_owned(), "y".to_owned()]);
    obj::set_attr(&p, "x", &obj::int(1));
    obj::set_attr(&p, "y", &obj::int(2));
    assert_eq!(obj::display(&p), "Point(x: 1, y: 2)");
    assert_eq!(obj::display(&obj::len(&p)), "2");

    // `Init` requires exactly one argument per name
    let q = stru("Point", vec!["x".to_owned(), "y".to_owned()]);
    let filled = obj::init(&q, &[obj::int(3), obj::int(4)]);
    assert_eq!(obj::display(&filled), "Point(x: 3, y: 4)");
    assert_eq!(
        message(&obj::init(
            &stru("Point", vec!["x".to_owned(), "y".to_owned()]),
            &[obj::int(1)]
        )),
        "invalid initialize"
    );

    // a name with no stored value is the host `KeyNotFoundException` (D-024)
    let r = stru("Point", vec!["x".to_owned(), "y".to_owned()]);
    obj::set_attr(&r, "x", &obj::int(1));
    assert_eq!(
        message(&obj::to_str(&r)),
        "System.Collections.Generic.KeyNotFoundException: The given key 'y' was not present in the dictionary."
    );

    // `Copy` / `Clone` keep the names, an empty `Members`, `Annotations` and `Parents`
    let c = obj::copy_obj(&p);
    assert!(!Rc::ptr_eq(&c, &p));
    assert_eq!(obj::display(&obj::len(&c)), "2");
    assert!(obj::member(&c, "x").is_none());
    assert_eq!(obj::type_name(&c), obj::type_name(&p));
}

// ---------------------------------------------------------------------------
// Enu
// ---------------------------------------------------------------------------

#[test]
fn enu_rules() {
    let e = enu("Color", 3);
    assert_eq!(obj::display(&e), "Color:3");
    assert_eq!(obj::display(&obj::unary(&e, UnOp::ToInt)), "3");
    assert_eq!(obj::display(&obj::binop(&e, BinOp::Eq, &obj::int(3))), "true");
    assert_eq!(obj::display(&obj::binop(&e, BinOp::Eq, &obj::int(4))), "false");
    assert_eq!(obj::display(&obj::binop(&e, BinOp::Lt, &obj::int(4))), "true");
    let other = enu("Color", 4);
    assert_eq!(obj::display(&obj::binop(&e, BinOp::Eq, &other)), "false");
    assert_eq!(obj::display(&obj::binop(&e, BinOp::Lt, &other)), "true");
    let different = enu("Hue", 3);
    assert_eq!(obj::display(&obj::binop(&e, BinOp::Eq, &different)), "false");

    assert_eq!(obj::display(&obj::init(&e, &[])), "Color:0");
    assert_eq!(obj::display(&obj::init(&e, &[obj::int(2)])), "Color:2");
    assert_eq!(
        message(&obj::init(&e, &[s("a")])),
        "'Color' takes at most 1 argument, 1 given"
    );
    assert_eq!(
        message(&obj::init(&e, &[obj::int(1), obj::int(2)])),
        "'Color' takes at most 1 argument, 2 given"
    );

    // `Copy` is `this`, `Clone` is a new cell
    assert!(Rc::ptr_eq(&obj::copy_obj(&e), &e));
    let c = obj::clone_obj(&e);
    assert!(!Rc::ptr_eq(&c, &e));
    assert_eq!(obj::display(&c), "Color:3");
    // `Enu` overrides `GetHashCode` but not `Hash`, so `hash(enu)` is the base error
    // while `hash_code` is the enum index
    assert_eq!(message(&obj::hash(&e)), "cannot hashable object");
    assert_eq!(obj::hash_code(&e), 3);
}

// ---------------------------------------------------------------------------
// TObj
// ---------------------------------------------------------------------------

#[test]
fn tobj_rules() {
    let t = tobj(Types::single("int"));
    assert_eq!(obj::display(&t), "<type: int>");
    let u = tobj(Types::create(&[Types::single("int"), Types::single("str")]));
    assert_eq!(obj::display(&u), "<type: int | str>");

    // `BOr` makes a union
    let combined = obj::binop(&t, BinOp::BOr, &tobj(Types::single("str")));
    assert_eq!(obj::display(&combined), "<type: int | str>");

    // `Eq` compares the type value, `Is` matches objects
    assert_eq!(obj::display(&obj::binop(&t, BinOp::Eq, &t)), "true");
    assert_eq!(obj::display(&obj::binop(&t, BinOp::Eq, &u)), "false");
    assert_eq!(obj::display(&obj::obj_is(&t, &t)), "true");
    // `x is y` routes through the type object (`right.Is(left)`), so `1 is int` is
    // `TObj.Is(Int)`, and `Matches` compares the object's own `Type`
    assert_eq!(obj::display(&obj::obj_is(&t, &obj::int(1))), "true");
    assert_eq!(obj::display(&obj::obj_is(&t, &s("a"))), "false");
    // `UnionType.Contains(BaseType)` is set membership, not a subset test
    assert_eq!(obj::display(&obj::obj_is(&u, &s("a"))), "false");
    assert_eq!(obj::display(&obj::obj_is(&u, &obj::int(1))), "false");

    // `x in y` is `y.In(x)`. `int in int|str` is true, `int|str in int` is false
    assert_eq!(obj::display(&obj::obj_in(&u, &t)), "true");
    assert_eq!(obj::display(&obj::obj_in(&t, &u)), "false");
    assert_eq!(obj::display(&obj::obj_in(&t, &obj::int(1))), "true");
    assert_eq!(obj::display(&obj::obj_in(&u, &obj::int(1))), "true");
    assert_eq!(obj::display(&obj::obj_in(&u, &tobj(Types::single("str")))), "true");
    assert_eq!(obj::display(&obj::obj_in(&t, &t)), "true");

    // a class name is resolved through the original-class registry
    let missing = obj::call(&tobj(Types::single("missing")), &[]);
    assert_eq!(message(&missing), "class 'missing' not found");
    let union_call = obj::call(&u, &[]);
    assert_eq!(message(&union_call), "type 'int | str' is not a class");
}

// ---------------------------------------------------------------------------
// hash and key helpers
// ---------------------------------------------------------------------------

#[test]
fn hashability() {
    assert_eq!(
        message(&obj::hash(&obj::int(5))),
        "cannot hashable object"
    );
    assert_eq!(obj::display(&obj::hash(&s("a"))), prims::str_hash("a").to_string());
    // every other type keeps the base `Hash`, so it is the same error message
    assert_eq!(message(&obj::hash(&obj::float(1.5))), "cannot hashable object");
    assert_eq!(message(&obj::hash(&obj::boolean(true))), "cannot hashable object");
    assert_eq!(message(&obj::hash(&list(vec![obj::int(1)]))), "cannot hashable object");

    // `List`/`Tup` bucket by combining the element hashes, an empty one is the seed
    assert_eq!(obj::hash_code(&list(Vec::new())), 0);
    assert_eq!(obj::hash_code(&list(vec![obj::int(1)])), 1);
    assert_eq!(obj::hash_code(&tup(vec![obj::int(1)], Vec::new())), 1);
    // the unhashable payloads fall back to the type-name FNV stand-in (D-025)
    assert_eq!(obj::hash_code(&set(Vec::new())), prims::str_hash("set") as u64);

    // key equality is `Eq()` coerced to a boolean
    assert!(obj::key_eq(&obj::int(1), &obj::int(1)));
    assert!(!obj::key_eq(&obj::int(1), &obj::int(2)));
    // a non-boolean result is not a key match
    assert!(!obj::key_eq(&s("a"), &obj::int(1)));
}

#[test]
fn int_keys_bucket_by_value() {
    // an int key hashes to its own value, so `hash_code` of 7 is 7
    assert_eq!(obj::hash_code(&obj::int(7)), 7);
    assert_eq!(obj::hash_code(&obj::boolean(true)), 1);
    assert_eq!(obj::hash_code(&obj::boolean(false)), 0);
    let d = dict(vec![(obj::int(7), s("v"))]);
    assert_eq!(obj::display(&obj::get_item(&d, &obj::int(7))), "v");
    assert_eq!(
        message(&obj::get_item(&d, &obj::int(8))),
        "key '8' not found in dictionary"
    );
    // an int key is printed through `Str.To`, so display is `{7:v}`
    assert_eq!(obj::display(&d), "{7:v}");
}
