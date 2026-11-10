use std::process::exit;
use std::rc::Rc;

use unvm::ast::{Const, Node};
use unvm::lexer::lex;
use unvm::parser::parse;
use unvm::source::Source;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("tokens") => tokens(&args[1..]),
        Some("ast") => ast(&args[1..]),
        Some("run") => run(&args[1..]),
        _ => {
            eprintln!("usage: unvm <tokens|ast|run> <file>");
            exit(2);
        }
    }
}

fn read_source(path: &str) -> Source {
    match std::fs::read_to_string(path) {
        Ok(text) => Source::new(path, &text),
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

fn ast(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: unvm ast <file>");
        exit(2);
    }

    let path = &args[0];
    let mut source = read_source(path);

    let tokens = match lex(&mut source) {
        Ok(tokens) => tokens,
        Err(err) => {
            eprintln!("{}", err.display(&source));
            exit(1);
        }
    };

    let source = Rc::new(source);

    match parse(tokens, source.clone()) {
        Ok(node) => print_node(&node, &source, 0),
        Err(err) => {
            eprintln!("{}", err.display(&source));
            exit(1);
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
    eprintln!("the VM is not implemented yet");
    exit(1);
}
