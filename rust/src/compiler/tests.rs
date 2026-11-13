use std::rc::Rc;
use crate::bytecode::instr::*;
use crate::bytecode::{stack_effect, ExcKind, Program};
use crate::desugar::desugar;
use crate::lexer::lex;
use crate::optimize::optimize;
use crate::parser::parse;
use crate::source::Source;
use super::compile;
pub(crate) fn compile_text(text: &str) -> (Rc<Program>, Rc<Source>) {
    let mut source = Source::new("test.un", text);
    let tokens = lex(&mut source).expect("lex");
    let source = Rc::new(source);
    let node = parse(tokens, source.clone()).expect("parse");
    let desugared = desugar(&node);
    let root = optimize(&desugared).expect("optimize").root;
    (Rc::new(compile(&source, &root).expect("compile")), source)
}
pub(crate) fn code(program: &Program) -> Vec<(Op, u16, i32)> {
    let proto = &program.protos[program.root];
    (0..proto.code.len())
        .filter_map(|ip| proto.instruction(ip))
        .collect()
}
pub(crate) fn depth(program: &Program) -> Vec<i64> {
    let mut depth = 0i64;
    code(program)
        .iter()
        .map(|(op, flags, operand)| {
            depth += stack_effect(*op, *flags, *operand);
            depth
        })
        .collect()
}
pub(crate) fn find(program: &Program, op: Op) -> usize {
    let ops = code(program);
    match ops.iter().position(|(o, _, _)| *o == op) {
        Some(ip) => ip,
        None => panic!("no {:?} in {ops:?}", op),
    }
}
pub(crate) fn at(program: &Program, ip: usize) -> (Op, u16, i32) {
    code(program)[ip]
}
pub(crate) fn name(program: &Program, slot: usize) -> String {
    program.name(&program.protos[program.root], slot).to_string()
}
pub(crate) fn constant(program: &Program, slot: usize) -> String {
    format!("{:?}", program.constant(&program.protos[program.root], slot))
}
pub(crate) fn count(ops: &[(Op, u16, i32)], op: Op) -> usize {
    ops.iter().filter(|(o, _, _)| *o == op).count()
}
#[test]
fn phase_8_encodings_validate_and_keep_the_stack_balanced() {
    let programs = [
        "use io\na = 1\nio.write(\"a\", a)",
        "use io\nx = a and b",
        "use io\nx = a or b",
        "use io\nx = a ?? b",
        "use io\nx = a ? b : c",
        "use io\nx = m?.value",
        "use io\nx = m?.value.more",
        "use io\nx = m?.call()",
        "use io\nx = m?[0]",
        "use io\nx = m[1:3:2]",
        "use io\na, b = (1, 2)",
        "use io\nl = [1, 2, 3]\nl[1:3] = [9]",
        "use io\ns = `x{1}y`",
        "use io\nx = -y",
        "use io\nx = y?",
        "use io\nx = d[\"k\"]",
        "use io\no.f = 1",
        "use io\nuse fs as f",
        "use io\nx = f(x, y, k = 2, *a, **b)",
        "use io\nx = not y",
        "use io\nx = (1, 2, 3)",
        "use io\nx = [1, 2]",
        "use io\nx = {\"a\": 1}",
        "use io\nx = a isnot b",
        "use io\nx = a in b",
    ];
    for text in programs {
        let (program, _) = compile_text(text);
        program.validate().expect("validate");
        for (ip, d) in depth(&program).iter().enumerate() {
            assert!(*d >= 0, "stack underflow at {ip} in {text}");
        }

        assert!(depth(&program).last().copied().unwrap() >= 1, "no block value in {text}");
    }
}
#[test]
fn a_block_pops_every_child_except_the_last() {
    let (program, _) = compile_text("use io\na = 1\nb = 2\nio.write(\"v\", a)");
    let ops = code(&program);

    assert_eq!(count(&ops, Op::Pop), 3);
    assert_eq!(ops.last().unwrap().0, Op::Unwrap);

    assert_eq!(count(&ops, Op::Unwrap), 5);
}
#[test]
fn container_literals_have_no_element_unwraps() {
    let (program, _) = compile_text("use io\nx = [1, a, {\"k\": b}]");
    let ops = code(&program);

    assert_eq!(count(&ops, Op::Unwrap), 2);
    let dict = find(&program, Op::BuildDict);
    let list = find(&program, Op::BuildList);
    let store = find(&program, Op::StoreName);
    assert!(dict < list && list < store);
    assert!(!ops[dict..store].iter().any(|(op, _, _)| *op == Op::Unwrap));
}
#[test]
fn a_child_that_raises_leaves_no_statement_unwrap() {

    let (program, _) = compile_text("use io\nbreak\ny = 1");
    let ops = code(&program);
    let unsupported = find(&program, Op::Unsupported);
    assert_ne!(ops[unsupported + 1].0, Op::Unwrap);
    assert_ne!(ops[unsupported + 1].0, Op::Pop);

    assert_eq!(count(&ops, Op::Unwrap), 2);
}
#[test]
fn and_short_circuits_after_testing_the_left() {
    let (program, _) = compile_text("use io\nx = a and b");
    let ops = code(&program);
    let dup = find(&program, Op::Dup);
    let to_bool = find(&program, Op::ToBool);
    let jmp = find(&program, Op::JmpIf);
    assert!(dup < to_bool && to_bool < jmp);

    assert_eq!(at(&program, to_bool).1 & FLAG_STRICT, FLAG_STRICT);
    assert_eq!(at(&program, jmp).1 & FLAG_JUMP_TRUE, FLAG_JUMP_TRUE);
    assert_eq!(at(&program, jmp).1 & FLAG_JUMP_POP, FLAG_JUMP_POP);

    assert_eq!(ops[jmp + 1].0, Op::Jmp);
    assert_eq!(ops[jmp + 2].0, Op::Pop);
}
#[test]
fn or_short_circuits_without_testing_the_truth_of_the_result() {
    let (program, _) = compile_text("use io\nx = a or b");
    let to_bool = find(&program, Op::ToBool);
    let jmp = find(&program, Op::JmpIf);
    assert_eq!(at(&program, to_bool).1 & FLAG_STRICT, FLAG_STRICT);
    assert_eq!(at(&program, jmp).1 & FLAG_JUMP_POP, FLAG_JUMP_POP);
    assert_eq!(at(&program, jmp).1 & FLAG_JUMP_TRUE, 0);

    let (ternary, _) = compile_text("use io\nx = a ? b : c");
    let tjmp = find(&ternary, Op::JmpIf);
    assert_eq!(at(&ternary, tjmp).1 & FLAG_JUMP_POP, FLAG_JUMP_POP);
    assert_eq!(at(&ternary, tjmp).1 & FLAG_JUMP_TRUE, FLAG_JUMP_TRUE);
    assert_eq!(count(&code(&ternary), Op::Pop), 1);
}
#[test]
fn the_optional_call_guard_is_before_the_arguments() {
    let (program, _) = compile_text("use io\nx = m?.call()");
    let ops = code(&program);
    let guard = ops
        .iter()
        .position(|(op, flags, _)| *op == Op::JmpIf && flags & FLAG_JUMP_NONE == FLAG_JUMP_NONE)
        .expect("guard");
    let begin = find(&program, Op::ArgBegin);
    let call = find(&program, Op::Call);
    assert!(guard < begin, "the guard must precede the arguments");
    assert!(begin < call, "the arguments must precede the call");

    let proto = &program.protos[program.root];
    let target = proto.jump_target(guard, ops[guard].2);
    assert_eq!(ops[target].0, Op::Pop);
    assert_eq!(ops[target + 1].0, Op::LoadNone);

    assert_eq!(ops[guard].1 & FLAG_JUMP_TRUE, FLAG_JUMP_TRUE);
    assert_eq!(ops[guard].1 & FLAG_JUMP_NONE, FLAG_JUMP_NONE);
}
#[test]
fn slice_bounds_run_only_when_the_target_is_not_none() {
    let (program, _) = compile_text("use io\nx = m?.v[i + 1:2]");
    let ops = code(&program);
    let guard = ops
        .iter()
        .position(|(op, flags, _)| *op == Op::JmpIf && flags & FLAG_JUMP_NONE == FLAG_JUMP_NONE)
        .expect("guard");
    let add = find(&program, Op::Add);
    let slice = find(&program, Op::Slice);
    assert!(guard < add && add < slice, "the bounds must be inside the guarded branch");
}
#[test]
fn slice_operand_matches_its_bound_flags() {
    let (program, _) = compile_text("use io\nx = l[1:3:2]");
    let (op, flags, operand) = at(&program, find(&program, Op::Slice));
    assert_eq!(op, Op::Slice);
    assert_eq!(operand, flags.count_ones() as i32);
    assert_eq!(flags & FLAG_SLICE_START, FLAG_SLICE_START);
    assert_eq!(flags & FLAG_SLICE_END, FLAG_SLICE_END);
    assert_eq!(flags & FLAG_SLICE_STEP, FLAG_SLICE_STEP);
}
#[test]
fn tuple_destructuring_binds_forward() {
    let (program, _) = compile_text("use io\na, b = t");
    let ops = code(&program);
    let unpack = find(&program, Op::Unpack);
    assert_eq!(ops[unpack].2, 2);
    let stores: Vec<String> = ops[unpack + 1..]
        .iter()
        .filter(|(op, _, _)| *op == Op::StoreName)
        .map(|(_, _, operand)| name(&program, *operand as usize))
        .collect();
    assert_eq!(stores, ["a", "b"]);
}
#[test]
fn f_string_parts_are_converted_to_strings() {
    let (program, _) = compile_text("use io\ns = `a{b}c{1}`");
    let ops = code(&program);
    let fstr = find(&program, Op::FString);
    let converts: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, (op, _, _))| *op == Op::ToStr)
        .map(|(ip, _)| ip)
        .collect();

    assert_eq!(converts.len(), 2);
    assert!(converts.iter().all(|ip| *ip < fstr));

    assert_eq!(ops[fstr].2, 4);
}
#[test]
fn the_propagate_handler_is_a_propagate_instruction() {
    let (program, _) = compile_text("use io\nx = throw(\"boom\")?");
    let proto = &program.protos[program.root];
    assert_eq!(proto.exc.len(), 1);
    let entry = proto.exc[0];
    assert_eq!(entry.kind, ExcKind::Propagate);
    let (op, _, _) = proto.instruction(entry.handler).expect("handler");
    assert_eq!(op, Op::Propagate);
    assert!(entry.start <= entry.handler && entry.handler < entry.end);
}
#[test]
fn call_records_the_static_argument_count() {
    let (program, _) = compile_text("use io\nio.write(\"a\", 1, 2)");
    let (op, names, operand) = at(&program, find(&program, Op::Call));
    assert_eq!(op, Op::Call);
    assert_eq!(operand, 3);

    let recorded = program.protos[program.root].call_names[names as usize].clone();
    assert_eq!(recorded, [-1, -1, -1]);
}
#[test]
fn keyword_arguments_record_their_names() {
    let (program, _) = compile_text("use io\nf(1, k = 2)");
    let (op, names, operand) = at(&program, find(&program, Op::Call));
    assert_eq!(op, Op::Call);
    assert_eq!(operand, 2);
    let recorded = program.protos[program.root].call_names[names as usize].clone();
    assert_eq!(recorded[0], -1);
    assert_eq!(name(&program, recorded[1] as usize), "k");
}
#[test]
fn use_dispatch_is_deferred_to_the_vm() {
    let (program, _) = compile_text("use io\nx = 1");
    let ops = code(&program);
    assert_eq!(ops[0].0, Op::Use);
    assert_eq!(name(&program, ops[0].2 as usize), "io");
    let (program, _) = compile_text("use nosuchmod\nx = 1");
    let ops = code(&program);
    assert!(!ops.iter().any(|(op, _, _)| *op == Op::Unsupported));
    assert_eq!(ops[0].0, Op::Use);
}
#[test]
fn source_map_matches_code() {
    let (program, _) = compile_text("use io\na = 1\nb = a + 2");
    let proto = &program.protos[program.root];
    assert_eq!(proto.source_map.len(), proto.code.len());
    assert!(proto.instruction(proto.code.len()).is_none());
}
#[test]
fn slots_resolve_through_the_proto() {
    let (program, _) = compile_text("use io\nx = \"abc\"");
    let (_, _, operand) = at(&program, find(&program, Op::LoadConst));
    assert_eq!(constant(&program, operand as usize), "Str(\"abc\")");
    let store = code(&program)
        .iter()
        .position(|(op, _, _)| *op == Op::StoreName)
        .expect("store");
    let (_, _, slot) = at(&program, store);
    assert_eq!(name(&program, slot as usize), "x");
}
#[test]
fn a_function_definition_creates_a_nested_proto() {
    let (program, _) = compile_text("use io\nfn add(a, b)\n    -> a + b");
    assert_eq!(program.protos.len(), 2);
    let root = &program.protos[program.root];
    assert_eq!(root.protos.len(), 1);
    let nested = &program.protos[root.protos[0]];
    assert!(nested.is_function());
    assert_eq!(nested.params.len(), 2);
    assert_eq!(program.names[nested.params[0].name], "a");
    assert_eq!(program.names[nested.params[1].name], "b");

    let (op, flags, operand) = at(&program, find(&program, Op::MakeFunc));
    assert_eq!(op, Op::MakeFunc);
    assert_eq!(flags & FLAG_FUNC_BIND, FLAG_FUNC_BIND);
    assert_eq!(operand as usize, 0);
    assert_eq!(root.protos[operand as usize], 1);
}
#[test]
fn a_function_body_ends_with_a_return() {
    let (program, _) = compile_text("use io\nfn add(a, b)\n    -> a + b");
    let root = &program.protos[program.root];
    let nested = &program.protos[root.protos[0]];
    let ops: Vec<(Op, u16, i32)> = (0..nested.code.len())
        .filter_map(|ip| nested.instruction(ip))
        .collect();

    let returns: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, (op, _, _))| *op == Op::Return)
        .map(|(ip, _)| ip)
        .collect();
    assert_eq!(returns.len(), 2);
    assert_eq!(ops.last().unwrap().0, Op::Return);
    assert!(returns[0] < ops.len() - 1);
}
#[test]
fn a_function_body_does_not_unwrap_its_statements() {
    let (program, _) = compile_text("use io\nfn f()\n    x = 1\n    y = 2");
    let root = &program.protos[program.root];
    let nested = &program.protos[root.protos[0]];
    let ops: Vec<(Op, u16, i32)> = (0..nested.code.len())
        .filter_map(|ip| nested.instruction(ip))
        .collect();

    assert!(!ops.iter().any(|(op, _, _)| *op == Op::Unwrap));
    assert_eq!(ops.iter().filter(|(op, _, _)| *op == Op::Pop).count(), 2);
}
#[test]
fn a_default_parameter_is_evaluated_at_definition_time() {
    let (program, _) = compile_text("use io\nfn greet(name = \"world\")\n    -> name");
    let ops = code(&program);
    let make = find(&program, Op::MakeFunc);
    let load = ops
        .iter()
        .position(|(op, _, _)| *op == Op::LoadConst)
        .expect("load");
    assert!(load < make, "the default is evaluated before the function is created");

    let (_, flags, _) = at(&program, make);
    assert_eq!(flags >> 8, 1);
    let root = &program.protos[program.root];
    let nested = &program.protos[root.protos[0]];
    assert_eq!(nested.params[0].flags & PARAM_OPTIONAL, PARAM_OPTIONAL);
    assert_eq!(nested.params[0].flags & PARAM_ESSENTIAL, 0);
}
#[test]
fn the_star_parameters_have_the_reference_flags() {
    let (program, _) = compile_text("use io\nfn f(a, *rest, **opts)\n    -> a");
    let root = &program.protos[program.root];
    let nested = &program.protos[root.protos[0]];
    assert_eq!(nested.params.len(), 3);
    assert_eq!(nested.params[0].flags & PARAM_ESSENTIAL, PARAM_ESSENTIAL);
    assert_eq!(nested.params[1].flags & PARAM_POSITIONAL, PARAM_POSITIONAL);
    assert_eq!(nested.params[2].flags & PARAM_KEYWORD, PARAM_KEYWORD);
}
#[test]
fn a_lambda_is_a_function_without_a_binding() {
    let (program, _) = compile_text("use io\nmul = fn(a, b) -> a * b");
    let make = find(&program, Op::MakeFunc);
    let (_, flags, _) = at(&program, make);
    assert_eq!(flags & FLAG_FUNC_BIND, 0);
    let root = &program.protos[program.root];
    let nested = &program.protos[root.protos[0]];
    assert_eq!(program.names[nested.name], "lambda");
}
