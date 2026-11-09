use std::process::exit;
use unvm::lexer::lex;
use unvm::source::Source;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("tokens") => tokens(&args[1..]),
        Some("run") => run(&args[1..]),
        _ => {
            eprintln!("usage: unvm <tokens|run> <file>");
            exit(2);
        }
    }
}

fn tokens(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: unvm tokens <file>");
        exit(2);
    }

    let path = &args[0];
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {}: {}", path, e);
            exit(1);
        }
    };

    let mut source = Source::new(path.clone(), &text);

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

fn run(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: unvm run <file>");
        exit(2);
    }
    eprintln!("the VM is not implemented yet");
    exit(1);
}
