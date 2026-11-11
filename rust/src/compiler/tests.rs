//! compiler tests — Phase 8 expression encodings.
//!
//! the assertions are encoding invariants the reference evaluator forces: the stack never
//! goes negative, `Unwrap` sits at the reference's `Unwrap(..., node)` points, the
//! short-circuit guards are placed where `Evaluator.cs` returns early, and the block keeps
//! its value slot.

use std::rc::Rc;

use crate::bytecode::instr::*;
use crate::bytecode::{stack_effect, ExcKind, Program};
use crate::desugar::desugar;
use crate::lexer::lex;
use crate::optimize::optimize;
use crate::parser::parse;
use crate::source::Source;

use super::compile;

/// source text → compiled program, the same pipeline `unvm run` uses
pub(crate) fn compile_text(text: &str) -> (Rc<Program>, Rc<Source>) {
    let mut source = Source::new("test.un", text);
    let tokens = lex(&mut source).expect("lex");
    let source = Rc::new(source);

    let node = parse(tokens, source.clone()).expect("parse");
    let desugared = desugar(&node);
    let root = optimize(&desugared).expect("optimize").root;

    (Rc::new(compile(&source, &root).expect("compile")), source)
}

/// the root proto's instruction list
pub(crate) fn code(program: &Program) -> Vec<(Op, u16, i32)> {
    let proto = &program.protos[program.root];
    (0..proto.code.len())
        .filter_map(|ip| proto.instruction(ip))
        .collect()
}

/// running stack depth, `stack_effect` summed instruction by instruction
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

/// first index of `op` in the root code
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

/// every program the phase covers must validate, and the stack must never underflow
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

        // the block keeps a value slot for the top level
        assert!(depth(&program).last().copied().unwrap() >= 1, "no block value in {text}");
    }
}

/// `EvalBlock` pops every child result except the last, so the last statement of a block
/// ends with its `UNWRAP` and no `POP`
#[test]
fn a_block_pops_every_child_except_the_last() {
    let (program, _) = compile_text("use io\na = 1\nb = 2\nio.write(\"v\", a)");
    let ops = code(&program);

    // three statements are popped, the last one keeps its value for the top level
    assert_eq!(count(&ops, Op::Pop), 3);
    assert_eq!(ops.last().unwrap().0, Op::Unwrap);

    // `EvalProperty` unwraps the callee `io.write` at its own node, so `UNWRAP` count is
    // one more than the statement count
    assert_eq!(count(&ops, Op::Unwrap), 5);
}

/// literal containers do not unwrap their elements, `EvalTuple` / `EvalList` / `EvalDict`
/// push values directly
#[test]
fn container_literals_have_no_element_unwraps() {
    let (program, _) = compile_text("use io\nx = [1, a, {\"k\": b}]");
    let ops = code(&program);

    // one `UNWRAP` for `use io`, one for the assignment statement, none inside the
    // container: the dict and the list sit between the two
    assert_eq!(count(&ops, Op::Unwrap), 2);
    let dict = find(&program, Op::BuildDict);
    let list = find(&program, Op::BuildList);
    let store = find(&program, Op::StoreName);

    assert!(dict < list && list < store);
    assert!(!ops[dict..store].iter().any(|(op, _, _)| *op == Op::Unwrap));
}

/// a statement child whose code ends in a control transfer gets no statement `UNWRAP`,
/// the reference's `Unwrap` would run on a stack that no longer has the value
#[test]
fn a_child_that_raises_leaves_no_statement_unwrap() {
    let (program, _) = compile_text("use io\nx = {1, 2}\ny = 1");
    let ops = code(&program);

    let unsupported = find(&program, Op::Unsupported);
    assert_ne!(ops[unsupported + 1].0, Op::Unwrap);
    assert_ne!(ops[unsupported + 1].0, Op::Pop);

    // the block continues with the next child, and only the last statement is unwrapped
    assert_eq!(count(&ops, Op::Unwrap), 2);
}

/// `and` evaluates the left, tests its truth, and only then evaluates the right
#[test]
fn and_short_circuits_after_testing_the_left() {
    let (program, _) = compile_text("use io\nx = a and b");
    let ops = code(&program);

    let dup = find(&program, Op::Dup);
    let to_bool = find(&program, Op::ToBool);
    let jmp = find(&program, Op::JmpIf);

    assert!(dup < to_bool && to_bool < jmp);

    // `Unwrap(left.ToBool(), node)` is the strict test, and the tested value is popped
    assert_eq!(at(&program, to_bool).1 & FLAG_STRICT, FLAG_STRICT);
    assert_eq!(at(&program, jmp).1 & FLAG_JUMP_TRUE, FLAG_JUMP_TRUE);
    assert_eq!(at(&program, jmp).1 & FLAG_JUMP_POP, FLAG_JUMP_POP);

    // the false path falls through to a jump that skips the right operand
    assert_eq!(ops[jmp + 1].0, Op::Jmp);
    assert_eq!(ops[jmp + 2].0, Op::Pop);
}

/// `or` is the mirror: the jump has no `FLAG_JUMP_TRUE`, so a truthy left is the result
#[test]
fn or_short_circuits_without_testing_the_truth_of_the_result() {
    let (program, _) = compile_text("use io\nx = a or b");

    let to_bool = find(&program, Op::ToBool);
    let jmp = find(&program, Op::JmpIf);

    assert_eq!(at(&program, to_bool).1 & FLAG_STRICT, FLAG_STRICT);
    assert_eq!(at(&program, jmp).1 & FLAG_JUMP_POP, FLAG_JUMP_POP);
    assert_eq!(at(&program, jmp).1 & FLAG_JUMP_TRUE, 0);

    // `EvalTernary` does not unwrap the condition, so a truthy test is not strict here
    let (ternary, _) = compile_text("use io\nx = a ? b : c");
    let tjmp = find(&ternary, Op::JmpIf);

    assert_eq!(at(&ternary, tjmp).1 & FLAG_JUMP_POP, FLAG_JUMP_POP);
    assert_eq!(at(&ternary, tjmp).1 & FLAG_JUMP_TRUE, FLAG_JUMP_TRUE);
    assert_eq!(count(&code(&ternary), Op::Pop), 1);
}

/// a `none` callee is tested before the argument list runs, exactly as `EvalCall` returns
/// `none` before `EvalArguments`
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

    // the guard jumps over the argument list to `POP` + `LOAD_NONE`
    let proto = &program.protos[program.root];
    let target = proto.jump_target(guard, ops[guard].2);
    assert_eq!(ops[target].0, Op::Pop);
    assert_eq!(ops[target + 1].0, Op::LoadNone);

    // `JMP_IF(FLAG_JUMP_NONE)` only computes a jump on the true side, so the guard needs
    // `FLAG_JUMP_TRUE` to reach the skip branch
    assert_eq!(ops[guard].1 & FLAG_JUMP_TRUE, FLAG_JUMP_TRUE);
    assert_eq!(ops[guard].1 & FLAG_JUMP_NONE, FLAG_JUMP_NONE);
}

/// `EvalSlice` returns `none` before the bound expressions, so the bounds sit inside the
/// non-skip branch. `EvalSlice` checks `IsOptionalChain(children[0])` only, so the chain
/// has to come from the target expression itself
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

/// the `SLICE` operand counts the bound flags, the same value the VM uses to pop
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

/// `EvalUnpack` binds the tuple items in order, so the first binder stores the first item,
/// which the VM puts on top of the stack
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

/// `EvalFString` converts every expression part with `ToStr()`, and the type check is part
/// of the `FSTRING` opcode
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

    // `b` and `1` are both converted before the string is built
    assert_eq!(converts.len(), 2);
    assert!(converts.iter().all(|ip| *ip < fstr));

    // parts are plain stack values, the operand is the part count
    assert_eq!(ops[fstr].2, 4);
}

/// the `?` region's handler is the `PROPAGATE` instruction, inside the region it guards
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

/// `CALL`'s operand is the static argument count, the VM reads the runtime count from the
/// accumulator frame so a spread can grow it
#[test]
fn call_records_the_static_argument_count() {
    let (program, _) = compile_text("use io\nio.write(\"a\", 1, 2)");
    let (op, names, operand) = at(&program, find(&program, Op::Call));

    assert_eq!(op, Op::Call);
    assert_eq!(operand, 3);

    // the recorded names of the call site, positional arguments are `-1`
    let recorded = program.protos[program.root].call_names[names as usize].clone();
    assert_eq!(recorded, [-1, -1, -1]);
}

/// keyword arguments record their name slot, positional arguments keep `-1`
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

/// `use` resolves native / file dispatch at run time, so a file module path is compiled
/// and never becomes an `UNSUPPORTED` node
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

/// `Proto::instruction` is bounds-safe, and the source map is one entry per instruction
#[test]
fn source_map_matches_code() {
    let (program, _) = compile_text("use io\na = 1\nb = a + 2");
    let proto = &program.protos[program.root];

    assert_eq!(proto.source_map.len(), proto.code.len());
    assert!(proto.instruction(proto.code.len()).is_none());
}

/// constants and names resolve through the proto-local slot, not the shared module index
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
