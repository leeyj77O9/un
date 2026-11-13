use crate::compiler::tests::compile_text;
use crate::error::nl;
use crate::native;
use crate::runtime::context;
use crate::runtime::obj::{self, Obj};
use crate::runtime::scope;
use crate::runtime::world;
use super::{Outcome, VM};
pub(crate) fn run(text: &str) -> (Outcome, String) {
    native::init(".");
    let (program, source) = compile_text(text);
    let mut vm = VM::new(program, source.clone()).expect("vm");
    let outcome = vm.run();
    world::flush_stdout();
    (outcome, world::log_text())
}
pub(crate) fn message(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Error(e) => e.message.clone(),
        Outcome::Propagate(o, _, _) => super::err_message(o),
        Outcome::Panic(o) => super::panic_display(o),
        Outcome::Value(o) => obj::display(o),
        _ => "not an error outcome".to_string(),
    }
}
pub(crate) fn value(outcome: &Outcome) -> Obj {
    match outcome {
        Outcome::Value(o) => o.clone(),
        _ => panic!("not a value outcome"),
    }
}
pub(crate) fn assert_out(text: &str, expected: &str) {
    let (outcome, out) = run(text);
    assert!(
        matches!(outcome, Outcome::Value(_)),
        "expected a value outcome in {text}"
    );
    assert_eq!(out, expected, "stdout of {text}");
}
pub(crate) fn assert_abort(text: &str, expected: &str) {
    let (outcome, out) = run(text);
    assert_eq!(message(&outcome), expected, "message of {text}");
    assert_eq!(out, "", "stdout of {text} must be empty");
}
pub(crate) fn assert_abort_deep(text: &'static str, expected: &'static str) {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || assert_abort(text, expected))
        .expect("thread")
        .join()
        .expect("join");
}
#[test]
fn io_write_writes_the_reference_line() {
    assert_out("use io\nio.write(\"v\", 1)", "v 1\n");
}
#[test]
fn the_block_value_is_the_last_child() {
    let (outcome, out) = run("use io\na = 1\nb = 2");
    assert_eq!(obj::display(&value(&outcome)), "2");
    assert_eq!(out, "");
}
#[test]
fn compound_assignment_runs_as_a_binary_operation() {
    assert_out("use io\nx = 1\nx += 2\nio.write(\"x\", x)", "x 3\n");
}
#[test]
fn tuple_destructuring_binds_both_names() {
    assert_out("use io\nt = (1, 2)\na, b = t\nio.write(\"ab\", a, b)", "ab 1 2\n");
}
#[test]
fn index_assignment_writes_the_slot() {
    assert_out("use io\nl = [1, 2, 3]\nl[1] = 9\nio.write(\"l\", l)", "l [1, 9, 3]\n");
}
#[test]
fn slice_assignment_replaces_the_range() {
    assert_out("use io\nq = [1, 2, 3, 4]\nq[1:3] = [7]\nio.write(\"q\", q)", "q [1, 7, 4]\n");
}
#[test]
fn a_step_slice_is_an_empty_list() {
    assert_out("use io\nl = [1, 2, 3, 4]\nio.write(\"s\", l[3:0:-2])", "s []\n");
}
#[test]
fn a_negative_step_past_the_end_is_the_host_exception() {
    assert_abort(
        "use io\nl = [1, 2, 3, 4]\nio.write(\"s\", l[4:0:-1])",
        "System.IndexOutOfRangeException: Index was outside the bounds of the array.",
    );
}
#[test]
fn an_optional_call_on_none_is_none() {
    assert_out("use io\nm = none\nio.write(\"c\", m?.call())", "c none\n");
}
#[test]
fn an_optional_index_on_none_is_a_type_error() {
    assert_abort(
        "use io\nm = none\nio.write(\"i\", m?[0])",
        "unsupported operand type(s) for []: 'none'",
    );
}
#[test]
fn an_unsupported_operand_reports_the_reference_message() {
    assert_abort(
        "use io\nio.write(\"m\", \"ab\" * 2)",
        "unsupported operand type(s) for *: 'str' and 'int'",
    );
}
#[test]
fn a_host_exception_aborts_at_the_instruction_that_produced_it() {
    assert_abort(
        "use io\nio.write(\"m\", 7 % 0)",
        "System.DivideByZeroException: Attempted to divide by zero.",
    );
}
#[test]
fn an_undefined_variable_uses_the_reference_message() {
    assert_abort("use io\nio.write(\"v\", zz)", "undefined variable 'zz'");
}
#[test]
fn a_top_level_propagate_rethrows_the_original_error() {
    assert_abort("use io\nio.write(\"q\", throw(\"boom\")?)", "boom");
}
#[test]
fn an_uncaught_throw_is_reported_as_an_error() {
    assert_abort("use io\nthrow(\"boom\")", "boom");
}
#[test]
fn a_ternary_condition_that_errors_aborts_at_the_condition() {
    assert_abort("use io\nio.write(\"t\", throw(\"cond\") ? 1 : 2)", "cond");
}
#[test]
fn f_string_parts_are_converted_with_to_string() {
    assert_out("use io\ns = `a{[1, 2]}c`\nio.write(\"s\", s)", "s a[1, 2]c\n");
}
#[test]
fn a_missing_dict_key_is_a_lang_error() {
    assert_abort("use io\nd = {\"a\": 1}\nio.write(\"v\", d[\"z\"])", "key 'z' not found in dictionary");
}
#[test]
fn a_list_index_out_of_range_is_a_lang_error() {
    assert_abort("use io\nio.write(\"i\", [1, 2][5])", "list index out of range");
}
#[test]
fn a_missing_file_module_is_a_panic() {
    let (outcome, out) = run("use nosuchmod\nx = 1");
    assert!(matches!(outcome, Outcome::Panic(_)));
    assert_eq!(
        message(&outcome),
        format!(
            "{}panic : module '{}' not found",
            nl(),
            std::path::Path::new(".").join("nosuchmod").display()
        )
    );
    assert_eq!(out, "");
}
#[test]
fn a_builtin_module_is_not_a_native_module() {
    let (outcome, out) = run("use builtin\nx = 1");
    assert!(matches!(outcome, Outcome::Panic(_)));
    assert_eq!(
        message(&outcome),
        format!(
            "{}panic : module '{}' not found",
            nl(),
            std::path::Path::new(".").join("builtin").display()
        )
    );
    assert_eq!(out, "");
}
#[test]
fn a_module_alias_binds_the_alias_name() {
    assert_out("use io as w\nw.write(\"v\", 1)", "v 1\n");
}
#[test]
fn and_tests_the_left_with_the_reference_truth() {
    assert_out("use io\nio.write(\"a\", \"a\" and 2)", "a 2\n");
    assert_out("use io\nio.write(\"b\", \"\" and 2)", "b \n");
    assert_out("use io\nio.write(\"c\", 0 and 1)", "c 0\n");
    assert_out("use io\nio.write(\"d\", [1, 2] and 3)", "d 3\n");
    assert_abort(
        "use io\nio.write(\"e\", none and 3)",
        "unsupported operand type(s) for bool(): 'none'",
    );
}
#[test]
fn coalescing_skips_the_right_side_for_a_none_left() {
    assert_out("use io\nx = none\nio.write(\"v\", x ?? 7)", "v 7\n");
    assert_out("use io\nx = 3\nio.write(\"v\", x ?? 7)", "v 3\n");
}
#[test]
fn the_root_scope_is_the_global_scope() {
    native::init(".");
    let (program, source) = compile_text("use io\nzz = 1");
    let mut vm = VM::new(program.clone(), source.clone()).expect("vm");
    vm.run();
    let global = world::global_scope();
    assert_eq!(obj::display(&scope::get(&global, "zz")), "1");
    assert_eq!(obj::display(&scope::get(&context::scope(&vm.ctx), "zz")), "1");
}
#[test]
fn a_function_returns_its_value() {
    assert_out(
        "use io\nfn add(a, b)\n    -> a + b\nio.write(\"r\", add(3, 4))",
        "r 7\n",
    );
}
#[test]
fn a_function_without_a_return_returns_none() {
    assert_out(
        "use io\nfn f()\n    io.write(\"in\")\nio.write(\"r\", f())",
        "in\nr none\n",
    );
}
#[test]
fn a_default_parameter_is_used() {
    assert_out(
        "use io\nfn greet(name = \"world\")\n    -> name\nio.write(\"g\", greet())",
        "g world\n",
    );
    assert_out(
        "use io\nfn greet(name = \"world\")\n    -> name\nio.write(\"g\", greet(\"un\"))",
        "g un\n",
    );
}
#[test]
fn a_function_can_recurse() {
    assert_out(
        "use io\nfn fib(n)\n    if n < 2\n        -> 1\n    -> fib(n-1) + fib(n-2)\nio.write(\"f\", fib(10))",
        "f 89\n",
    );
}
#[test]
fn a_lambda_is_callable() {
    assert_out("use io\nmul = fn(a, b) -> a * b\nio.write(\"m\", mul(6, 7))", "m 42\n");
}
#[test]
fn a_keyword_argument_binds_by_name() {
    assert_out(
        "use io\nfn f(a, b)\n    -> a - b\nio.write(\"k\", f(b = 10, a = 3))",
        "k -7\n",
    );
}
#[test]
fn a_star_parameter_collects_the_rest() {
    assert_out("use io\nfn f(a, *rest)\n    -> rest\nio.write(\"r\", f(1, 2, 3))", "r (2, 3)\n");
    assert_out(
        "use io\nfn f(**kwargs)\n    -> kwargs\nio.write(\"k\", f(a = 1, b = 2))",
        "k {a:1, b:2}\n",
    );
}
#[test]
fn a_function_reads_the_enclosing_scope() {
    assert_out("use io\nx = 10\nfn f()\n    -> x\nio.write(\"x\", f())", "x 10\n");
}
#[test]
fn a_function_body_assignment_is_local() {
    assert_out(
        "use io\ncounter = 0\nfn bump()\n    counter += 1\n    -> counter\nbump()\nio.write(\"c\", counter)",
        "c 0\n",
    );
}
#[test]
fn a_wrong_argument_count_is_the_reference_message() {
    assert_abort(
        "use io\nfn f(a, b)\n    -> a + b\nio.write(\"v\", f(1))",
        "missing required argument 'b'",
    );
    assert_abort(
        "use io\nfn f(a)\n    -> a\nio.write(\"v\", f(1, 2))",
        "too many positional arguments",
    );
}
#[test]
fn the_recursion_limit_is_the_reference_one() {
    assert_abort_deep(
        "use io\nfn deep(n)\n    -> deep(n + 1)\ndeep(0)",
        "maximum recursion depth exceeded",
    );
}
#[test]
fn an_error_inside_a_function_keeps_its_position() {
    let (outcome, out) = run("use io\nfn f()\n    -> undefined_thing\nf()");
    assert!(matches!(outcome, Outcome::Error(_)));
    assert_eq!(message(&outcome), "undefined variable 'undefined_thing'");
    assert_eq!(out, "");
}
#[test]
fn a_bare_return_is_the_host_exception() {
    assert_abort(
        "use io\nfn f()\n    ->\nf()",
        "System.ArgumentOutOfRangeException: Index was out of range. Must be non-negative and less than the size of the collection. (Parameter 'index')",
    );
}
#[test]
fn a_return_outside_a_function_is_an_error() {
    let (outcome, out) = run("use io\n-> 1");
    assert!(matches!(outcome, Outcome::Return(..)));
    assert_eq!(out, "");
}
#[test]
fn an_error_inside_a_function_body_is_reported_at_its_node() {
    assert_abort(
        "use io\nfn f()\n    -> 1 / 0\nio.write(\"v\", f())",
        "int division by zero",
    );
}
#[test]
fn a_function_value_prints_its_signature() {
    assert_out("use io\nfn f(a: int, b: str)\n    -> a\nio.write(\"f\", f)", "f fn(int, str) -> any\n");
}
#[test]
fn a_go_future_sees_the_variables_of_its_go_point() {
    assert_out(
        "use io\nuse iter\nfuts = []\nfor i in [1, 2, 3]\n    futs.add(go (i * 2))\n\nout = []\nfor f in futs\n    out.add(wait f)\n\nio.write(\"out = \", out)\n",
        "out =  [2, 4, 6]\n",
    );
}
#[test]
fn a_go_future_in_a_function_body_sees_the_parameters() {
    assert_out(
        "use io\nfn sq(n)\n    -> go (n * n)\n\nf = sq(5)\nio.write(\"f=\", wait f)\n",
        "f= 25\n",
    );
}
#[test]
fn a_waited_future_keeps_its_value() {
    assert_out(
        "use io\nf = go (1 + 2)\nio.write(wait f)\nio.write(wait f)\n",
        "3\n3\n",
    );
}
#[test]
fn go_binds_tighter_than_a_binary_operator() {
    assert_abort(
        "use io\nfn sq(n)\n    -> go n * n\nio.write(wait sq(5))",
        "unsupported operand type(s) for *: 'future' and 'int'",
    );
}
