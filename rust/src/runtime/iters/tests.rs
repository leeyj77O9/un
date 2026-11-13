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
fn ints(v: &[i64]) -> Vec<Obj> {
    v.iter().map(|n| obj::int(*n)).collect()
}
fn cell(o: &Obj) -> IterCell {
    match obj::payload(o) {
        Payload::Iter(st) => st,
        _ => panic!("expected an iter payload"),
    }
}
fn next_of(o: &Obj) -> String {
    message(&obj::next(o))
}
fn collected(o: &Obj) -> String {
    obj::display(&obj::to_list(o))
}
#[test]
fn range_display_and_len() {
    let r = range_new(0, 5, 1);
    assert_eq!(obj::display(&r), "0..5");
    assert_eq!(obj::display(&obj::len(&r)), "5");
    assert_eq!(collected(&r), "[0, 1, 2, 3, 4]");
    assert_eq!(obj::display(&obj::to_tuple(&r)), "(0, 1, 2, 3, 4)");

    assert_eq!(obj::display(&obj::len(&r)), "5");
    let r2 = range_new(0, 10, 2);
    assert_eq!(obj::display(&r2), "0..10..2");
    assert_eq!(obj::display(&obj::len(&r2)), "5");

    let r3 = range_new(5, 0, -1);
    assert_eq!(obj::display(&r3), "5..0..-1");
    assert_eq!(obj::display(&obj::len(&r3)), "5");
    assert_eq!(collected(&r3), "[5, 4, 3, 2, 1]");

    assert_eq!(obj::display(&obj::len(&range_new(10, 0, 1))), "0");
    assert_eq!(obj::display(&obj::len(&range_new(0, 0, 1))), "0");
    assert_eq!(obj::display(&obj::len(&range_new(5, 0, -1))), "5");

    assert_eq!(obj::display(&obj::len(&range_new(0, 6, 3))), "2");
}
#[test]
fn range_contains_uses_the_step() {
    let r = range_new(0, 10, 2);
    assert!(obj::bool_value(&obj::obj_in(&r, &obj::int(4))));
    assert!(!obj::bool_value(&obj::obj_in(&r, &obj::int(3))));

    assert!(!obj::bool_value(&obj::obj_in(&r, &obj::int(10))));
    let d = range_new(5, 0, -1);

    assert!(obj::bool_value(&obj::obj_in(&d, &obj::int(5))));
    assert!(obj::bool_value(&obj::obj_in(&d, &obj::int(1))));
    assert!(!obj::bool_value(&obj::obj_in(&d, &obj::int(0))));
    assert!(!obj::bool_value(&obj::obj_in(&d, &obj::int(6))));

    assert_eq!(
        message(&obj::obj_in(&d, &s("a"))),
        "unsupported operand type(s) for in: 'range'"
    );
}
#[test]
fn range_eq_compares_the_three_fields() {
    let a = range_new(0, 5, 1);
    let b = range_new(0, 5, 1);
    let c = range_new(0, 5, 2);
    assert!(obj::bool_value(&obj::binop(&a, BinOp::Eq, &b)));
    assert!(!obj::bool_value(&obj::binop(&a, BinOp::Eq, &c)));

    assert!(obj::is_err(&obj::binop(&a, BinOp::Eq, &collected_obj(&a))));
    assert_eq!(
        message(&obj::binop(&a, BinOp::Eq, &collected_obj(&a))),
        "unsupported operand type(s) for ==: 'range' and 'list'"
    );
}
fn collected_obj(o: &Obj) -> Obj {
    obj::to_list(o)
}
#[test]
fn range_iter_is_the_same_object() {
    let r = range_new(0, 5, 1);

    assert!(Rc::ptr_eq(&obj::iter(&r), &r));
    assert!(Rc::ptr_eq(&obj::copy_obj(&r), &r));

    assert_eq!(next_of(&r), "0");
    assert_eq!(next_of(&r), "1");
    let cl = obj::clone_obj(&r);
    assert_eq!(obj::display(&obj::len(&cl)), "5");
    assert_eq!(next_of(&cl), "0");
    assert_eq!(next_of(&cl), "1");

    assert_eq!(next_of(&r), "2");
}
#[test]
fn zero_step_panics_lazily() {

    let r = range_new(0, 5, 0);
    assert_eq!(obj::display(&r), "0..5..0");
    assert_eq!(message(&obj::next(&r)), "panic: step cannot be zero");

    assert_eq!(
        message(&obj::len(&r)),
        "System.DivideByZeroException: Attempted to divide by zero."
    );
}
#[test]
fn range_operator_builds_a_half_open_range() {

    let r = obj::range_to(&obj::int(1), &obj::int(4));
    assert_eq!(obj::display(&r), "1..4");
    assert_eq!(collected(&r), "[1, 2, 3]");
    assert_eq!(obj::display(&obj::len(&obj::range_to(&obj::int(0), &obj::int(10)))), "10");
    assert_eq!(obj::display(&obj::to_str(&obj::range_to(&obj::int(1), &obj::int(4)))), "1..4");
}
#[test]
fn counter_is_infinite() {
    let c = counter_new(3);

    assert_eq!(obj::display(&c), "counter");
    assert_eq!(message(&obj::to_str(&c)), "counter is infinite");

    assert_eq!(obj::display(&obj::len(&c)), i64::MAX.to_string());
    assert_eq!(message(&obj::to_list(&c)), "counter is infinite");
    assert_eq!(message(&obj::to_tuple(&c)), "counter is infinite");
    assert_eq!(message(&obj::spread(&c)), "counter is infinite");
    assert_eq!(next_of(&c), "3");
    assert_eq!(next_of(&c), "4");
    assert_eq!(message(&obj::next(&obj::clone_obj(&c))), "3");

    assert_eq!(
        message(&obj::to_list(&skip_of(&cell(&c), 1))),
        "panic: iteration is infinite"
    );
}
#[test]
fn counter_clone_restarts_at_the_start() {
    let c = counter_new(0);
    assert_eq!(next_of(&c), "0");
    assert_eq!(next_of(&c), "1");

    let cl = obj::clone_obj(&c);
    assert_eq!(next_of(&cl), "0");
    assert_eq!(next_of(&cl), "1");
    assert_eq!(next_of(&c), "2");
}
#[test]
fn counter_init_spellings() {
    let c = counter_new(7);
    assert_eq!(next_of(&obj::init(&c, &[])), "0");
    assert_eq!(next_of(&obj::init(&c, &[obj::int(9)])), "9");
    assert_eq!(
        message(&obj::init(&c, &[s("x")])),
        "invalid 'counter' initialize"
    );
    assert_eq!(
        message(&obj::init(&c, &[obj::int(1), obj::int(2)])),
        "invalid 'counter' initialize"
    );
}
#[test]
fn repeat_repeats_the_source() {
    let rp = repeat_source(ints(&[1, 2]), 3);

    assert_eq!(obj::display(&rp), "repeat");
    assert_eq!(collected(&rp), "[1, 2, 1, 2, 1, 2]");
    assert_eq!(obj::display(&obj::len(&rp)), "6");
    assert_eq!(obj::display(&obj::to_tuple(&rp)), "(1, 2, 1, 2, 1, 2)");

    assert_eq!(collected(&rp), "[1, 2, 1, 2, 1, 2]");
    assert_eq!(next_of(&rp), "1");
    let inf = repeat_source(ints(&[1, 2]), -1);
    assert_eq!(obj::display(&inf), "repeat");
    assert_eq!(message(&obj::to_list(&inf)), "repeat is infinite");
    assert_eq!(message(&obj::to_tuple(&inf)), "repeat is infinite");
    assert_eq!(message(&obj::spread(&inf)), "repeat is infinite");

    assert_eq!(obj::display(&obj::len(&inf)), "-2");
}
#[test]
fn repeat_init_rules() {
    let proto = repeat_source(ints(&[1, 2]), 1);

    let one = obj::init(&proto, &[collections::list(ints(&[1, 2]))]);
    assert_eq!(message(&obj::to_list(&one)), "repeat is infinite");
    let pair = obj::init(&proto, &[collections::list(ints(&[1, 2])), obj::int(2)]);
    assert_eq!(collected(&pair), "[1, 2, 1, 2]");
    let tup = obj::init(&proto, &[collections::tup(ints(&[1, 2]), Vec::new()), obj::int(2)]);
    assert_eq!(collected(&tup), "[1, 2, 1, 2]");
    assert_eq!(
        message(&obj::init(&proto, &[obj::int(1)])),
        "invalid 'repeat' initialize"
    );
    assert_eq!(
        message(&obj::init(&proto, &[collections::list(ints(&[1, 2])), s("2")])),
        "invalid 'repeat' initialize"
    );
    assert_eq!(
        message(&obj::init(&proto, &[])),
        "invalid 'repeat' initialize"
    );
}
#[test]
fn reverse_walks_the_values_backwards() {
    let rv = reverse_new(ints(&[1, 2, 3]));

    assert_eq!(obj::display(&rv), "3, 2, 1");
    assert_eq!(obj::display(&obj::len(&rv)), "3");
    assert_eq!(collected(&rv), "[3, 2, 1]");
    assert_eq!(obj::display(&obj::to_tuple(&rv)), "(3, 2, 1)");

    assert_eq!(collected(&rv), "[3, 2, 1]");
    assert_eq!(next_of(&rv), "3");

    let src = collections::list(ints(&[1, 2, 3]));
    let rv2 = obj::init(&reverse_new(Vec::new()), &[src.clone()]);
    assert_eq!(collected(&rv2), "[3, 2, 1]");
    push(&src, obj::int(4));
    assert_eq!(collected(&rv2), "[3, 2, 1]");
    assert_eq!(
        message(&obj::init(&reverse_new(Vec::new()), &[])),
        "invalid 'reverse' initialization"
    );
    assert_eq!(
        message(&obj::init(&reverse_new(Vec::new()), &[obj::int(1)])),
        "invalid 'reverse' initialization"
    );
}
fn push(o: &Obj, value: Obj) {
    if let Payload::List(v) = obj::payload(o) {
        v.borrow_mut().push(value);
    }
}
#[test]
fn iter_seq_is_reusable() {
    let it = seq(ints(&[1, 2, 3]));
    assert_eq!(obj::display(&it), "1, 2, 3");
    assert_eq!(obj::display(&obj::len(&it)), "3");
    assert_eq!(next_of(&it), "1");
    assert_eq!(next_of(&it), "2");
    assert_eq!(next_of(&it), "3");
    assert_eq!(message(&obj::next(&it)), "iteration stopped");

    assert_eq!(obj::display(&obj::to_str(&it)), "1, 2, 3");

    assert!(Rc::ptr_eq(&obj::copy_obj(&it), &it));
    assert_eq!(next_of(&obj::clone_obj(&it)), "1");
}
#[test]
fn live_list_source_sees_appended_items() {
    let src = Rc::new(RefCell::new(ints(&[1, 2, 3])));
    let it = seq_live(&src);
    assert_eq!(collected(&it), "[1, 2, 3]");
    src.borrow_mut().push(obj::int(4));
    assert_eq!(collected(&it), "[1, 2, 3, 4]");

    let l = collections::list(ints(&[1, 2, 3]));
    let li = obj::iter(&l);
    assert_eq!(collected(&li), "[1, 2, 3]");
    push(&l, obj::int(4));
    assert_eq!(collected(&li), "[1, 2, 3, 4]");

    let proto = seq(Vec::new());
    let made = obj::init(&proto, &[collections::list(ints(&[1, 2]))]);
    assert_eq!(collected(&made), "[1, 2]");
    assert_eq!(
        message(&obj::init(&proto, &[])),
        "invaild 'iter' initialize"
    );
    assert_eq!(
        message(&obj::init(&proto, &[obj::int(1), obj::int(2)])),
        "invaild 'iter' initialize"
    );
}
#[test]
fn take_and_skip_wrappers_leave_the_receiver() {
    let r = range_new(0, 10, 1);

    let tk = take_of(&cell(&r), 2);
    assert_eq!(obj::display(&tk), "0, 1");
    assert_eq!(obj::display(&obj::len(&tk)), "2");
    assert_eq!(collected(&tk), "[0, 1]");
    assert_eq!(collected(&tk), "[0, 1]");
    assert_eq!(obj::display(&r), "0..10");
    assert_eq!(collected(&r), "[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]");
    let sk = skip_of(&cell(&r), 3);
    assert_eq!(collected(&sk), "[3, 4, 5, 6, 7, 8, 9]");
    assert_eq!(obj::display(&obj::len(&sk)), "7");

    let c = counter_new(0);
    let tc = take_of(&cell(&c), 2);
    assert_eq!(collected(&tc), "[0, 1]");

    assert_eq!(message(&obj::to_list(&skip_of(&cell(&c), 1))), "panic: iteration is infinite");
}
#[test]
fn str_units_are_utf16_code_units() {
    assert_eq!(str_units("abc"), ["a", "b", "c"]);

    assert_eq!(obj::display(&obj::to_list(&s("ab"))), "['a', 'b']");
    assert_eq!(obj::display(&obj::to_tuple(&s("ab"))), "(a, b)");
    assert_eq!(obj::display(&obj::iter(&s("ab"))), "a, b");

    assert_eq!(str_units("\u{1F600}"), ["\u{1F600}"]);
    assert_eq!(str_units("a\u{1F600}b").len(), 3);
}
#[test]
fn dict_iteration_yields_named_pairs() {
    let d = collections::dict(vec![(s("a"), obj::int(1)), (s("b"), obj::int(2))]);
    let it = obj::iter(&d);
    assert_eq!(obj::display(&it), "(a, 1), (b, 2)");

    let p = obj::next(&it);
    assert_eq!(obj::display(&p), "(a, 1)");
    assert_eq!(obj::display(&obj::member(&p, "key").unwrap()), "a");
    assert_eq!(obj::display(&obj::member(&p, "value").unwrap()), "1");

    assert_eq!(obj::display(&obj::to_list(&d)), "[(a, 1), (b, 2)]");
}
#[test]
fn tuple_iteration_walks_the_values() {
    let t = collections::tup(ints(&[1, 2, 3]), Vec::new());
    assert_eq!(obj::display(&obj::iter(&t)), "1, 2, 3");
    assert_eq!(obj::display(&obj::len(&obj::iter(&t))), "3");
    assert_eq!(collected(&obj::iter(&t)), "[1, 2, 3]");
}
#[test]
fn iter_unop_messages_come_from_the_type_name() {
    let it = seq(ints(&[1]));
    assert_eq!(
        message(&obj::unary(&it, UnOp::Neg)),
        "unsupported operand type(s) for -: 'iter'"
    );
    assert_eq!(message(&obj::hash(&it)), "cannot hashable object");
    assert_eq!(
        message(&obj::entry(&it)),
        "unsupported operand type(s) for using entry: 'iter'"
    );
    assert_eq!(
        message(&obj::exit(&it)),
        "unsupported operand type(s) for using exit: 'iter'"
    );

    assert_eq!(
        message(&obj::unary(&counter_new(0), UnOp::ToInt)),
        "unsupported operand type(s) for int(): 'counter'"
    );
}
#[test]
fn counter_subclasses_report_their_own_type_name() {
    assert_eq!(obj::type_name(&counter_new(0)).as_ref(), "counter");
    assert_eq!(obj::type_name(&range_new(0, 5, 1)).as_ref(), "range");
    assert_eq!(obj::type_name(&repeat_source(ints(&[1]), 1)).as_ref(), "repeat");
    assert_eq!(obj::type_name(&reverse_new(ints(&[1]))).as_ref(), "reverse");
    assert_eq!(obj::type_name(&seq(ints(&[1]))).as_ref(), "iter");
}
#[test]
fn iter_hash_uses_the_type_name_fallback() {

    assert_eq!(
        obj::hash_code(&counter_new(0)),
        obj::hash_code(&counter_new(7))
    );

    assert_eq!(message(&obj::hash(&counter_new(0))), "cannot hashable object");
}
