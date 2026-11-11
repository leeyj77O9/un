//! VM tests — the reference evaluator's observable behaviour, opcode by opcode.
//!
//! every test compiles a probe program through the full pipeline and checks the run's
//! `Outcome` and the stdout text, the two things `Program.cs` prints.

use crate::compiler::tests::compile_text;
use crate::error::nl;
use crate::native;
use crate::runtime::context;
use crate::runtime::obj::{self, Obj};
use crate::runtime::scope;
use crate::runtime::world;

use super::{Outcome, VM};

/// compiles and runs `text`, returning the run's outcome and the whole stdout text.
/// `native::init` resets the world, so each test starts from a fresh `Global.Init`.
pub(crate) fn run(text: &str) -> (Outcome, String) {
    native::init(".");

    let (program, source) = compile_text(text);
    let mut vm = VM::new(program, source.clone()).expect("vm");
    let outcome = vm.run();

    world::flush_stdout();
    (outcome, world::log_text())
}

/// the text `Program.cs` prints for an outcome
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

/// compiles, runs, and asserts the run prints `expected` on stdout
pub(crate) fn assert_out(text: &str, expected: &str) {
    let (outcome, out) = run(text);

    assert!(
        matches!(outcome, Outcome::Value(_)),
        "expected a value outcome in {text}"
    );
    assert_eq!(out, expected, "stdout of {text}");
}

/// compiles, runs, and asserts the run aborts with `expected` and prints nothing
pub(crate) fn assert_abort(text: &str, expected: &str) {
    let (outcome, out) = run(text);

    assert_eq!(message(&outcome), expected, "message of {text}");
    assert_eq!(out, "", "stdout of {text} must be empty");
}

/// `Console.WriteLine` of the reference prints `io.write`'s text, LF terminated
#[test]
fn io_write_writes_the_reference_line() {
    assert_out("use io\nio.write(\"v\", 1)", "v 1\n");
}

/// `EvalBlock` returns the last child's value, the top level result `RunCore` discards
#[test]
fn the_block_value_is_the_last_child() {
    let (outcome, out) = run("use io\na = 1\nb = 2");

    assert_eq!(obj::display(&value(&outcome)), "2");
    assert_eq!(out, "");
}

/// compound assignment is desugared to `Binary` before compiling, so `+=` is a plain add
#[test]
fn compound_assignment_runs_as_a_binary_operation() {
    assert_out("use io\nx = 1\nx += 2\nio.write(\"x\", x)", "x 3\n");
}

/// `BindPattern`'s tuple pattern binds forward, so the first binder gets the first item
#[test]
fn tuple_destructuring_binds_both_names() {
    assert_out("use io\nt = (1, 2)\na, b = t\nio.write(\"ab\", a, b)", "ab 1 2\n");
}

/// `BindPattern`'s Index branch writes through the target list
#[test]
fn index_assignment_writes_the_slot() {
    assert_out("use io\nl = [1, 2, 3]\nl[1] = 9\nio.write(\"l\", l)", "l [1, 9, 3]\n");
}

/// a slice assignment replaces the range, the step must be 1
#[test]
fn slice_assignment_replaces_the_range() {
    assert_out("use io\nq = [1, 2, 3, 4]\nq[1:3] = [7]\nio.write(\"q\", q)", "q [1, 7, 4]\n");
}

/// `List.Slice` with a step != 1 builds a capacity-0 list, so every `Add` is dropped
#[test]
fn a_step_slice_is_an_empty_list() {
    assert_out("use io\nl = [1, 2, 3, 4]\nio.write(\"s\", l[3:0:-2])", "s []\n");
}

/// the bounds check of `this[i]` runs even when the result is dropped, so a negative step
/// starting at the length raises the .NET host exception
#[test]
fn a_negative_step_past_the_end_is_the_host_exception() {
    assert_abort(
        "use io\nl = [1, 2, 3, 4]\nio.write(\"s\", l[4:0:-1])",
        "System.IndexOutOfRangeException: Index was outside the bounds of the array.",
    );
}

/// `EvalCall` tests the optional chain before `EvalArguments`, so the argument
/// expressions never run on a `none` callee
#[test]
fn an_optional_call_on_none_is_none() {
    assert_out("use io\nm = none\nio.write(\"c\", m?.call())", "c none\n");
}

/// `EvalIndex` has no `none` index arm, so the chain guard stops only property chains
#[test]
fn an_optional_index_on_none_is_a_type_error() {
    assert_abort(
        "use io\nm = none\nio.write(\"i\", m?[0])",
        "unsupported operand type(s) for []: 'none'",
    );
}

/// `Unwrap` at the operator node turns an `Err` value into a `LangError`
#[test]
fn an_unsupported_operand_reports_the_reference_message() {
    assert_abort(
        "use io\nio.write(\"m\", \"ab\" * 2)",
        "unsupported operand type(s) for *: 'str' and 'int'",
    );
}

/// `Int.Mod` by zero throws a .NET exception, so the value leaves the stack as a `Panic`
/// payload and the run aborts at the instruction that produced it
#[test]
fn a_host_exception_aborts_at_the_instruction_that_produced_it() {
    assert_abort(
        "use io\nio.write(\"m\", 7 % 0)",
        "System.DivideByZeroException: Attempted to divide by zero.",
    );
}

/// `EvalIdentifier`'s last arm is the reference's message, the global scope and the class
/// registry are checked before it
#[test]
fn an_undefined_variable_uses_the_reference_message() {
    assert_abort("use io\nio.write(\"v\", zz)", "undefined variable 'zz'");
}

/// a `?` at the top level rethrows the caught `Error` exactly as `throw;` does, so the
/// header is the original one
#[test]
fn a_top_level_propagate_rethrows_the_original_error() {
    assert_abort("use io\nio.write(\"q\", throw(\"boom\")?)", "boom");
}

/// `throw` outside any `?` region is an uncaught `Error`, the reference prints it with the
/// `error` header
#[test]
fn an_uncaught_throw_is_reported_as_an_error() {
    assert_abort("use io\nthrow(\"boom\")", "boom");
}

/// `Unwrap(cond.ToBool(), node)` is not strict in `EvalTernary`, and an `Err` condition is
/// reported at the condition node
#[test]
fn a_ternary_condition_that_errors_aborts_at_the_condition() {
    assert_abort("use io\nio.write(\"t\", throw(\"cond\") ? 1 : 2)", "cond");
}

/// `EvalFString` converts each expression part with `ToStr()`, so a list part becomes its
/// display text
#[test]
fn f_string_parts_are_converted_with_to_string() {
    assert_out("use io\ns = `a{[1, 2]}c`\nio.write(\"s\", s)", "s a[1, 2]c\n");
}

/// a missing dict key is the reference's `Err`, unwrapped at the `INDEX` node
#[test]
fn a_missing_dict_key_is_a_lang_error() {
    assert_abort("use io\nd = {\"a\": 1}\nio.write(\"v\", d[\"z\"])", "key 'z' not found in dictionary");
}

/// a list index past the end is the reference's `Err`, not a host exception
#[test]
fn a_list_index_out_of_range_is_a_lang_error() {
    assert_abort("use io\nio.write(\"i\", [1, 2][5])", "list index out of range");
}

/// `Global.Load` reports `Path.Combine(PATH, path)`, the PATH-based location, when neither
/// the source directory nor `Global.PATH` has the module
#[test]
fn a_missing_file_module_is_a_panic() {
    let (outcome, out) = run("use nosuchmod\nx = 1");

    assert!(matches!(outcome, Outcome::Panic(_)));
    assert_eq!(
        message(&outcome),
        format!("{}panic : module '.\\nosuchmod' not found", nl())
    );
    assert_eq!(out, "");
}

/// `[BuiltinModule]` is not `[NativeModule]`, so `use builtin` falls through to
/// `Global.Import` and reports a missing file module
#[test]
fn a_builtin_module_is_not_a_native_module() {
    let (outcome, out) = run("use builtin\nx = 1");

    assert!(matches!(outcome, Outcome::Panic(_)));
    assert_eq!(
        message(&outcome),
        format!("{}panic : module '.\\builtin' not found", nl())
    );
    assert_eq!(out, "");
}

/// `use io as w` binds the alias, and the reference's `ImportMap` keeps the module name
/// binding too
#[test]
fn a_module_alias_binds_the_alias_name() {
    assert_out("use io as w\nw.write(\"v\", 1)", "v 1\n");
}

/// `Unwrap(left.ToBool(), node)` decides `and` in the reference: `Str` / `Int` / `List`
/// have their own truth values, and `none` has none, so it is an `Err` at the operator node
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

/// `??` skips the right side only when the left is not `none`
#[test]
fn coalescing_skips_the_right_side_for_a_none_left() {
    assert_out("use io\nx = none\nio.write(\"v\", x ?? 7)", "v 7\n");
    assert_out("use io\nx = 3\nio.write(\"v\", x ?? 7)", "v 3\n");
}

/// the root `Context` is built on `Global.scope`, so a top level assignment lands in the
/// global scope object itself
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
