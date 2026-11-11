use std::process::exit;
use std::rc::Rc;

use unvm::ast::{Const, Node};
use unvm::bytecode::stack_effect;
use unvm::compiler;
use unvm::desugar::desugar;
use unvm::error::{nl, LangError};
use unvm::lexer::lex;
use unvm::native;
use unvm::optimize::optimize;
use unvm::parser::parse;
use unvm::runtime::context;
use unvm::runtime::world;
use unvm::source::Source;
use unvm::vm::{err_header, err_message, panic_display, Outcome, VM};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("tokens") => tokens(&args[1..]),
        Some("ast") => ast(&args[1..]),
        Some("desugar") => desugar_cmd(&args[1..]),
        Some("opt") => opt_cmd(&args[1..]),
        Some("dis") => dis(&args[1..]),
        Some("run") => run(&args[1..]),
        _ => {
            eprintln!("usage: unvm <tokens|ast|desugar|opt|dis|run> <file>");
            exit(2);
        }
    }
}

fn read_source(path: &str) -> Source {
    // `Runner.Load` appends `.un` when it is missing, so `Source.Name` matches
    let path = if path.ends_with(".un") {
        path.to_string()
    } else {
        format!("{path}.un")
    };

    match std::fs::read_to_string(&path) {
        Ok(text) => Source::new(&path, &text),
        Err(e) => {
            eprintln!("cannot read {}: {}", path, e);
            exit(1);
        }
    }
}

fn tokens(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: unvm tokens <file>");
        exit(2);
    }

    let path = &args[0];
    let mut source = read_source(path);

    match lex(&mut source) {
        Ok(tokens) => {
            for token in tokens {
                let text = source.span(token.start, token.length);
                println!("{:?} {}..{} {:?}", token.typ, token.start, token.end(), text);
            }
        }
        Err(err) => {
            eprintln!("{}", err.display(&source));
            exit(1);
        }
    }
}

fn parse_file(path: &str) -> (Rc<Source>, Node) {
    let mut source = read_source(path);

    let tokens = match lex(&mut source) {
        Ok(tokens) => tokens,
        Err(err) => {
            // the reference catches the parse `Error` in `Run()` and prints it with
            // `Console.WriteLine`, so the text goes to stdout with `Environment.NewLine`
            print!("{}{}", err.display(&source), nl());
            exit(1);
        }
    };

    let source = Rc::new(source);

    match parse(tokens, source.clone()) {
        Ok(node) => (source, node),
        Err(err) => {
            print!("{}{}", err.display(&source), nl());
            exit(1);
        }
    }
}

fn ast(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: unvm ast <file>");
        exit(2);
    }

    let (source, node) = parse_file(&args[0]);
    print_node(&node, &source, 0);
}

fn desugar_cmd(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: unvm desugar <file>");
        exit(2);
    }

    let (source, node) = parse_file(&args[0]);
    print_node(&desugar(&node), &source, 0);
}

fn opt_cmd(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: unvm opt <file>");
        exit(2);
    }

    let (source, node) = parse_file(&args[0]);
    let desugared = desugar(&node);

    match optimize(&desugared) {
        Ok(result) => {
            println!("passes: {}", result.pass_count);
            print_node(&result.root, &source, 0);
        }
        Err(panic) => {
            eprintln!("{}", panic.display());
            exit(1);
        }
    }
}

/// dumps every proto's code with the running stack depth, so a stack-effect mismatch is
/// visible in the same walk `Program::validate` takes.
fn dis(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: unvm dis <file>");
        exit(2);
    }

    let (source, node) = parse_file(&args[0]);
    let desugared = desugar(&node);

    let root = match optimize(&desugared) {
        Ok(result) => result.root,
        Err(panic) => {
            eprintln!("{}", panic.display());
            exit(1);
        }
    };

    let program = match compiler::compile(&source, &root) {
        Ok(program) => program,
        Err(panic) => {
            eprintln!("{}", panic.display());
            exit(1);
        }
    };

    println!("names: {:?}", program.names);
    println!("consts: {:?}", program.consts);

    for (pi, proto) in program.protos.iter().enumerate() {
        println!(
            "proto {pi} name {:?} flags {:#06x} code {}",
            program.name(proto, proto.name),
            proto.flags,
            proto.code.len()
        );

        let mut depth: i64 = 0;
        for (ip, instr) in proto.code.iter().enumerate() {
            let op = match instr.op_code() {
                Some(op) => op,
                None => {
                    println!("{ip:4} {:02x} INVALID", instr.op);
                    continue;
                }
            };

            depth += stack_effect(op, instr.flags, instr.operand);
            println!(
                "{ip:4} {op:?} flags {:06x} op {:5} depth {depth:3} line {}",
                instr.flags,
                instr.operand,
                source.get_line(proto.source_map[ip].0)
            );
        }

        for entry in &proto.exc {
            println!("exc {entry:?}");
        }
    }
}

fn print_node(node: &Node, source: &Source, depth: usize) {
    let indent = "  ".repeat(depth);
    let value = match &node.value {
        Some(Const::Int(v)) => format!(" {v}"),
        Some(Const::Float(v)) => format!(" {v}"),
        Some(Const::Str(v)) => format!(" {v:?}"),
        Some(Const::Bool(v)) => format!(" {v}"),
        Some(Const::Null) => String::new(),
        None => String::new(),
    };

    println!(
        "{indent}{:?} {}..{}{value}",
        node.kind,
        node.start,
        node.start + node.length
    );

    for child in &node.children {
        print_node(child, source, depth + 1);
    }
}

fn run(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: unvm run <file>");
        exit(2);
    }

    // `Global.Init(Environment.CurrentDirectory)`, the root of module search
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    native::init(cwd.to_str().unwrap_or("."));

    let (source, node) = parse_file(&args[0]);
    let desugared = desugar(&node);

    let root = match optimize(&desugared) {
        Ok(result) => result.root,
        Err(panic) => {
            eprintln!("{}", panic.display());
            exit(1);
        }
    };

    let program = match compiler::compile(&source, &root) {
        Ok(program) => program,
        Err(panic) => {
            eprintln!("{}", panic.display());
            exit(1);
        }
    };

    let mut vm = match VM::new(Rc::new(program), source.clone()) {
        Ok(vm) => vm,
        Err(message) => {
            eprintln!("vm: {message}");
            exit(1);
        }
    };

    match vm.run() {
        Outcome::Value(_) => {
            world::flush_stdout();
            exit(0);
        }
        Outcome::Error(e) => {
            world::flush_stdout();
            report(&vm, e.display(&source));
        }
        Outcome::Propagate(err, start, length) => {
            // `catch (PropagateFlow pf)` in `RunCore`, the header is preserved
            world::flush_stdout();
            let e = LangError::new(start, length, err_message(&err)).with_header(err_header(&err));
            report(&vm, e.display(&source));
        }
        Outcome::Return(_, start, length) => {
            world::flush_stdout();
            report(&vm, LangError::new(start, length, "'->' outside function").display(&source));
        }
        Outcome::Break(start, length) => {
            world::flush_stdout();
            report(&vm, LangError::new(start, length, "'break' outside loop").display(&source));
        }
        Outcome::Skip(start, length) => {
            world::flush_stdout();
            report(&vm, LangError::new(start, length, "'skip' outside loop").display(&source));
        }
        Outcome::Panic(o) => {
            world::flush_stdout();
            report(&vm, panic_display(&o));
        }
    }
}

/// `PrintError`: the error text goes to `Console.WriteLine` (stdout), then the trace
/// when the frame stack is deeper than one. Exit code is 1 on every uncaught error.
/// `Console.WriteLine` uses `Environment.NewLine`, so every one of these lines ends
/// with `\r\n` on Windows.
fn report(vm: &VM, text: String) {
    world::flush_stdout();
    print!("{text}{}", nl());

    let (lines, truncated) = context::trace_lines(&vm.ctx);
    if lines.is_empty() {
        exit(1);
    }

    print!("    trace:{}", nl());
    for line in lines {
        print!("\t{line}{}", nl());
    }
    if truncated > 0 {
        print!("\t... (truncated [{truncated}+]){}", nl());
    }

    exit(1);
}
