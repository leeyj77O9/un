mod lsp;

use std::io::Write;
use std::process::exit;
use std::rc::Rc;
use unvm::ast::{Const, Node};
use unvm::bytecode::stack_effect;
use unvm::compiler;
use unvm::error::{nl, LangError};
use unvm::lexer::lex;
use unvm::native;
use unvm::optimize::optimize;
use unvm::parser::parse;
use unvm::runtime::context;
use unvm::runtime::world;
use unvm::runner;
use unvm::typeck;
use unvm::source::Source;
use unvm::vm::{err_header, err_message, panic_display, Outcome, VM};
fn main() {

    let worker = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(dispatch)
        .expect("failed to spawn the worker thread");
    match worker.join() {
        Ok(()) => {
            unvm::jit_cache::clear();
            exit(0)
        }
        Err(_) => exit(1),
    }
}
fn dispatch() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("tokens") => tokens(&args[1..]),
        Some("ast") => ast(&args[1..]),
        Some("desugar") => desugar_cmd(&args[1..]),
        Some("opt") => opt_cmd(&args[1..]),
        Some("dis") => dis(&args[1..]),
        Some("dis-reg") => dis_reg(&args[1..]),
        Some("run") => run(&args[1..]),
        Some("compile") => compile(&args[1..]),
        Some("check") => check(&args[1..]),
        Some("native") => native(&args[1..]),
        Some("obj") => obj_cmd(&args[1..]),
        Some("build") => build_cmd(&args[1..]),
        Some("help") => print_help(),
        Some("lsp") => exit(lsp::run()),
        None => print_help(),
        Some(cmd) => {

            print!("Unknown command: {cmd}{}", nl());
            print_help();
        }
    }
}
fn print_help() {
    print!("Help:{}", nl());
    print!("  un run <file.un>{}", nl());
    print!("  un compile <file.un>  (emits .unbc bytecode){}", nl());
    print!("  un check <file.un>    (compile mode: static type analysis only){}", nl());
    print!("  un native <file.un>   (compile mode: static analysis + native execution){}", nl());
    print!("  un obj <file.un|file.unbc> [-o out.obj] [--entry name]  (emit a relocatable object file){}", nl());
    print!("  un build <file.un> [-o out.exe] [--embed] [--allow-dynamic] [--opt-level N]  (N=0|1|2|3|s|z; fully typed by default){}", nl());
    print!("  un lsp                (run the language server over stdio for the VS Code extension){}", nl());
}
fn read_source(path: &str) -> Source {

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
        eprintln!("usage: un tokens <file>");
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
        eprintln!("usage: un ast <file>");
        exit(2);
    }
    let (source, node) = parse_file(&args[0]);
    print_node(&node, &source, 0);
}
fn desugar_cmd(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: un desugar <file>");
        exit(2);
    }
    let (source, node) = parse_file(&args[0]);
    match optimize(&node) {
        Ok(result) => print_node(&result.root, &source, 0),
        Err(panic) => {
            eprintln!("{}", panic.display());
            exit(1);
        }
    }
}
fn opt_cmd(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: un opt <file>");
        exit(2);
    }
    let (source, node) = parse_file(&args[0]);
    match optimize(&node) {
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
fn dis_reg(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: un dis-reg <file>");
        exit(2);
    }
    let (source, node) = parse_file(&args[0]);
    let root = match optimize(&node) {
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
    match unvm::vm::dump_register_plan(&program) {
        Ok(text) => print!("{text}"),
        Err(err) => eprintln!("{err}"),
    }
}

fn dis(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: un dis <file>");
        exit(2);
    }
    let (source, node) = parse_file(&args[0]);
    let root = match optimize(&node) {
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
            program.names[proto.name],
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

        print!("Usage: un run <file>{}", nl());
        exit(0);
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    native::init(cwd.to_str().unwrap_or("."));

    if args[0].ends_with(".unbc") {
        run_bytecode(&args[0]);
        return;
    }
    let source = Rc::new(read_source(&args[0]));

    let program = match runner::compile_source(&source) {
        Ok(program) => program,
        Err(runner::RunError::Parse(error)) => {

            print!("{}{}", error.display(&source), nl());
            exit(1);
        }
        Err(runner::RunError::Panic(panic)) => {
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
            let root = Some((
                e.source.clone().unwrap_or_else(|| source.clone()),
                e.start,
            ));
            report(&vm.ctx, e.display(&source), root);
        }
        Outcome::Propagate(err, start, length) => {

            world::flush_stdout();
            let e = LangError::new(start, length, err_message(&err)).with_header(err_header(&err));
            report(&vm.ctx, e.display(&source), Some((source.clone(), start)));
        }
        Outcome::Return(_, start, length) => {
            world::flush_stdout();
            report(&vm.ctx, LangError::new(start, length, "'->' outside function").display(&source), Some((source.clone(), start)));
        }
        Outcome::Break(start, length) => {
            world::flush_stdout();
            report(&vm.ctx, LangError::new(start, length, "'break' outside loop").display(&source), Some((source.clone(), start)));
        }
        Outcome::Skip(start, length) => {
            world::flush_stdout();
            report(&vm.ctx, LangError::new(start, length, "'skip' outside loop").display(&source), Some((source.clone(), start)));
        }
        Outcome::Panic(o) => {
            world::flush_stdout();
            report(&vm.ctx, panic_display(&o), None);
        }
    }
}
fn native(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: un native <file>");
        exit(2);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    native::init(cwd.to_str().unwrap_or("."));
    let (source, node) = parse_file(&args[0]);
    let root = match optimize(&node) {
        Ok(result) => result.root,
        Err(panic) => {
            eprintln!("{}", panic.display());
            exit(1);
        }
    };
    let checked = match typeck::check_program(&source, &root) {
        Ok(checked) => checked,
        Err(errors) => {
            for error in &errors {
                print!("{}{}", error.display(&source), nl());
            }
            exit(1);
        }
    };
    let lowered = match unvm::tir::TirProgram::lower(&source, &root, &checked, &[]) {
        Ok(program) => program,
        Err(error) => {
            eprintln!("tir: {error}");
            exit(1);
        }
    };
    let program = match runner::compile_source(&source) {
        Ok(program) => program,
        Err(runner::RunError::Parse(error)) => {
            print!("{}{}", error.display(&source), nl());
            exit(1);
        }
        Err(runner::RunError::Panic(panic)) => {
            eprintln!("{}", panic.display());
            exit(1);
        }
    };
    // Falling back to the interpreter is normal in Vm mode (Str/heap/native
    // calls are not emitted there). Stay silent so `un run` and `un native`
    // produce byte-identical output.
    let _failed = unvm::natcode::native_compile(program.id, &program, &lowered);
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
            let root = Some((
                e.source.clone().unwrap_or_else(|| source.clone()),
                e.start,
            ));
            report(&vm.ctx, e.display(&source), root);
        }
        Outcome::Propagate(err, start, length) => {
            world::flush_stdout();
            let e = LangError::new(start, length, err_message(&err)).with_header(err_header(&err));
            report(&vm.ctx, e.display(&source), Some((source.clone(), start)));
        }
        Outcome::Return(_, start, length) => {
            world::flush_stdout();
            report(&vm.ctx, LangError::new(start, length, "'->' outside function").display(&source), Some((source.clone(), start)));
        }
        Outcome::Break(start, length) => {
            world::flush_stdout();
            report(&vm.ctx, LangError::new(start, length, "'break' outside loop").display(&source), Some((source.clone(), start)));
        }
        Outcome::Skip(start, length) => {
            world::flush_stdout();
            report(&vm.ctx, LangError::new(start, length, "'skip' outside loop").display(&source), Some((source.clone(), start)));
        }
        Outcome::Panic(o) => {
            world::flush_stdout();
            report(&vm.ctx, panic_display(&o), None);
        }
    }
}

fn check(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: un check <file>");
        exit(2);
    }
    let (source, node) = parse_file(&args[0]);
    let root = match optimize(&node) {
        Ok(result) => result.root,
        Err(panic) => {
            eprintln!("{}", panic.display());
            exit(1);
        }
    };
    let source = Rc::new(source);
    match typeck::check_program(&source, &root) {
        Ok(_) => {}
        Err(errors) => {
            for error in &errors {
                print!("{}{}", error.display(&source), nl());
            }
            exit(1);
        }
    }
}

fn obj_cmd(args: &[String]) {
    let mut input: Option<&String> = None;
    let mut output: Option<String> = None;
    let mut entry: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                let Some(path) = args.get(i) else {
                    eprintln!("usage: un obj <file.un|file.unbc> [-o out.obj] [--entry name]");
                    exit(2);
                };
                output = Some(path.clone());
            }
            "--entry" => {
                i += 1;
                let Some(name) = args.get(i) else {
                    eprintln!("usage: un obj <file.un|file.unbc> [-o out.obj] [--entry name]");
                    exit(2);
                };
                entry = Some(name.clone());
            }
            other => {
                if input.is_some() {
                    eprintln!("unexpected argument: {other}");
                    exit(2);
                }
                input = Some(&args[i]);
            }
        }
        i += 1;
    }
    let Some(input) = input else {
        eprintln!("usage: un obj <file.un|file.unbc> [-o out.obj] [--entry name]");
        exit(2);
    };
    let output = output.unwrap_or_else(|| format!("{input}.obj"));

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    native::init(cwd.to_str().unwrap_or("."));

    if input.ends_with(".unbc") {
        let bytes = match std::fs::read(input) {            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("cannot read {input}: {e}");
                exit(1);
            }
        };
        let program = match unvm::bytecode::unbc::read(&bytes) {
            Ok(program) => program,
            Err(message) => {
                eprintln!("{input}: {message}");
                exit(1);
            }
        };
        match unvm::objify::emit(&program, std::path::Path::new(&output), entry.as_deref()) {
            Ok(outcome) => {
                println!("wrote {} (emitted: {})", outcome.output.display(), outcome.emitted.join(", "));
                for reason in &outcome.skipped {
                    println!("skipped: {reason}");
                }
            }
            Err(message) => {
                eprintln!("un obj: {message}");
                exit(1);
            }
        }
        return;
    }

    let (source, node) = parse_file(input);
    let root = match optimize(&node) {
        Ok(result) => result.root,
        Err(panic) => {
            eprintln!("{}", panic.display());
            exit(1);
        }
    };
    let source = Rc::new(source);
    let checked = match typeck::check_program(&source, &root) {
        Ok(checked) => checked,
        Err(errors) => {
            for error in &errors {
                print!("{}{}", error.display(&source), nl());
            }
            exit(1);
        }
    };
    let lowered = match unvm::tir::TirProgram::lower(&source, &root, &checked, &[]) {
        Ok(program) => program,
        Err(error) => {
            eprintln!("tir: {error}");
            exit(1);
        }
    };
    match unvm::natcode::obj_compile(0, &lowered, std::path::Path::new(&output), entry.as_deref()) {
        Ok(outcome) => {
            println!("wrote {} (emitted: {})", outcome.output.display(), outcome.emitted.join(", "));
            for reason in &outcome.skipped {
                println!("skipped: {reason}");
            }
        }
        Err(message) => {
            eprintln!("un obj: {message}");
            exit(1);
        }
    }
}

fn find_gnu_toolchain() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let toolchains = std::path::Path::new(&home).join(".rustup").join("toolchains");
    let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&toolchains)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for dir in dirs {
        let gcc = dir
            .join("lib")
            .join("rustlib")
            .join("x86_64-pc-windows-gnu")
            .join("bin")
            .join("self-contained")
            .join("x86_64-w64-mingw32-gcc.exe");
        let rustc = dir.join("bin").join("rustc.exe");
        if gcc.is_file() && rustc.is_file() {
            return Some((rustc, gcc));
        }
    }
    None
}

/// Resolve simple user modules for the no-VM build path. Native modules
/// (`io`, `math`, ...) are left alone. Only flat `use a.b as alias` imports of
/// files containing functions are supported; the files are textually inlined
/// with mangled names so spans stay consistent with the combined source.
fn inline_user_modules_text(code: &str, input: &str) -> Result<String, String> {
    // Discover native modules from the same registry the runtime uses. A
    // hand-maintained list here previously omitted `bytes`, making plain build
    // mistake `use bytes` for a user module file.
    let native = unvm::runtime::world::native_module_names();
    let mut out = String::new();
    let mut aliases: Vec<(String, String)> = Vec::new();
    let mut appended = String::new();
    for line in code.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("use ") {
            let spec = rest.split('#').next().unwrap_or(rest).trim().trim_end_matches(';');
            let mut parts = spec.split_whitespace();
            let path = parts.next().unwrap_or("");
            let mut alias = None;
            if let Some(a) = parts.next() {
                if let Some(name) = a.strip_prefix("as ") { alias = Some(name.trim().to_string()); }
            }
            if spec.contains(" as ") {
                let bits: Vec<&str> = spec.split(" as ").collect();
                if bits.len() == 2 { alias = Some(bits[1].trim().to_string()); }
            }
            let path_full = path.trim();
            // Explicit member-import syntax and nested module `use` need the VM.
            if path_full.contains('{') || path_full.contains('}') {
                return Err(format!("module import '{}' needs `un build --embed`", path_full));
            }
            let first = path_full.split('.').next().unwrap_or(path_full);
            if native.iter().any(|name| name == first) {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            let alias = alias.unwrap_or_else(|| path_full.rsplit('.').next().unwrap_or(path_full).to_string());
            if native.iter().any(|name| name == &alias) || aliases.iter().any(|(a, _)| a == &alias) {
                return Err(format!("module alias '{alias}' conflicts with a native module or another alias"));
            }
            let mut rel = std::path::PathBuf::new();
            for part in path_full.split('.') { rel.push(part); }
            let input_path = std::path::Path::new(input);
            let mut candidates = Vec::new();
            if input_path.is_file() {
                candidates.push(input_path.parent().unwrap_or(std::path::Path::new(".")).join(&rel));
            }
            candidates.push(rel.clone());
            let file = candidates.into_iter().map(|c| {
                if c.extension().is_some() { c } else {
                    let mut p = c.into_os_string(); p.push(".un"); std::path::PathBuf::from(p)
                }
            }).find(|p| p.is_file()).ok_or_else(|| {
                format!("cannot read module '{}': file not found", rel.display())
            })?;
            let text = std::fs::read_to_string(&file)
                .map_err(|e| format!("cannot read module '{}': {e}", file.display()))?;
            // Collect module-level global names first (simple `name = ...` lines).
            let mut globals: Vec<String> = Vec::new();
            for l in text.lines() {
                let t = l.trim_start();
                if l.starts_with(' ') || l.starts_with('\t') || t.is_empty() || t.starts_with('#') { continue; }
                if t.starts_with("use ") {
                    let p = t[4..].split_whitespace().next().unwrap_or("");
                    let first = p.split('.').next().unwrap_or(p);
                    if !native.iter().any(|name| name == first) {
                        return Err(format!("module '{}' imports another module (needs `un build --embed`)", file.display()));
                    }
                    appended.push_str(l);
                    appended.push('\n');
                    continue;
                }
                if t.starts_with("fn ") || t.starts_with("class ") || t.starts_with("enum ") { continue; }
                if let Some((lhs, _)) = t.split_once('=') {
                    let name = lhs.trim();
                    if name.chars().all(|c| c.is_alphanumeric() || c == '_')
                        && name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
                    { globals.push(name.to_string()); }
                }
            }
            let mangle_globals = |line: &str| -> String {
                let mut s = line.to_string();
                for g in &globals {
                    s = replace_word(&s, g, &format!("{alias}_{g}"));
                }
                s
            };
            for l in text.lines() {
                let t = l.trim_start();
                if t.is_empty() || t.starts_with('#') || t.starts_with("use ") { continue; }
                let mangled = mangle_globals(l);
                if t.starts_with("fn ") || l.starts_with(' ') || l.starts_with('\t') {
                    appended.push_str(&mangle_line(&mangled, &format!("{alias}_")));
                    appended.push('\n');
                } else if t.starts_with("enum ") {
                    // Enum declarations are compile-time metadata: no runtime
                    // assignment to rewrite. Pass through unchanged so a module
                    // can export an enum like it exports a class.
                    appended.push_str(&mangled);
                    appended.push('\n');
                } else if t.split_once('=').is_some() {
                    appended.push_str(&mangled);
                    appended.push('\n');
                } else {
                    return Err(format!("module '{}' has unsupported top-level statement", file.display()));
                }
            }
            aliases.push((alias, path_full.to_string()));
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    // Rewrite `alias.member` -> `alias_member` only at identifier positions.
    let mut rewritten = String::with_capacity(out.len() + appended.len() + 1);
    for line in out.lines() {
        let mut l = line.to_string();
        for (alias, _) in &aliases {
            l = replace_alias(&l, alias);
        }
        rewritten.push_str(&l);
        rewritten.push('\n');
    }
    // Module text first so its globals initialize before main-file globals.
    let mut result = appended;
    result.push_str(&rewritten);
    Ok(result)
}

fn mangle_line(line: &str, prefix: &str) -> String {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("fn ") {
        let indent = &line[..line.len() - trimmed.len()];
        return format!("{indent}fn {prefix}{rest}");
    }
    line.to_string()
}
/// Rewrite `alias.member` to `alias_member` at identifier positions only,
/// skipping string literals and comments.
fn replace_alias(line: &str, alias: &str) -> String {
    let marker = format!("{alias}.");
    let chars: Vec<char> = line.chars().collect();
    let m: Vec<char> = marker.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    let mut in_string = false;
    let mut in_comment = false;
    let mut escaped = false;
    while i < chars.len() {
        let c = chars[i];
        if in_comment { out.push(c); i += 1; continue; }
        if in_string {
            out.push(c);
            if escaped { escaped = false; }
            else if c == '\\' { escaped = true; }
            else if c == '"' { in_string = false; }
            i += 1;
            continue;
        }
        if c == '#' { in_comment = true; out.push(c); i += 1; continue; }
        if c == '"' { in_string = true; out.push(c); i += 1; continue; }
        if i + m.len() <= chars.len() && chars[i..i + m.len()] == m[..]
            && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_'))
            && i + m.len() < chars.len() && (chars[i + m.len()].is_alphanumeric() || chars[i + m.len()] == '_')
        {
            out.push_str(alias);
            out.push('_');
            i += m.len();
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Replace identifier occurrences of `name`, skipping string literals and
/// comments (char-based to preserve UTF-8).
fn replace_word(line: &str, name: &str, with: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let name_chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    let mut in_string = false;
    let mut in_comment = false;
    let mut escaped = false;
    while i < chars.len() {
        let c = chars[i];
        if in_comment {
            out.push(c);
            i += 1;
            continue;
        }
        if in_string {
            out.push(c);
            if escaped { escaped = false; }
            else if c == '\\' { escaped = true; }
            else if c == '"' { in_string = false; }
            i += 1;
            continue;
        }
        if c == '#' { in_comment = true; out.push(c); i += 1; continue; }
        if c == '"' { in_string = true; out.push(c); i += 1; continue; }
        if i + name_chars.len() <= chars.len() && chars[i..i + name_chars.len()] == name_chars[..] {
            let before = i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
            let after = i + name_chars.len() == chars.len()
                || !(chars[i + name_chars.len()].is_alphanumeric() || chars[i + name_chars.len()] == '_');
            if before && after {
                out.push_str(with);
                i += name_chars.len();
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn collect_global_names(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in code.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with("fn ") || t.starts_with("use ")
            || t.starts_with("class ") || t.starts_with("enum ") {
            continue;
        }
        if let Some((lhs, rhs)) = t.split_once('=') {
            // Type aliases look like `T = int | str` (no digits/quotes in RHS).
            let r = rhs.trim();
            if r.contains('|') && !r.chars().any(|c| c.is_ascii_digit() || c == '"') { continue; }
            let name = lhs.trim();
            if name.chars().all(|c| c.is_alphanumeric() || c == '_')
                && name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
                && !out.iter().any(|n| n == name)
            {
                out.push(name.to_string());
            }
        }
    }
    out
}

// ---- build cache (TOOL1) -------------------------------------------------
// Fingerprint helpers are std-only: two independent FNV-1a lanes give a
// 128-bit key without adding a dependency.
const CACHE_FNV_SEED_A: u64 = 0xcbf2_9ce4_8422_2325;
const CACHE_FNV_SEED_B: u64 = 0x8422_2325_cbf2_9ce4;

fn cache_fnv(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = seed;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_1b3);
    }
    hash
}
fn cache_digest(bytes: &[u8]) -> String {
    format!(
        "{:016x}{:016x}",
        cache_fnv(bytes, CACHE_FNV_SEED_A),
        cache_fnv(bytes, CACHE_FNV_SEED_B)
    )
}
fn cache_push(buf: &mut Vec<u8>, text: &str) {
    buf.extend_from_slice(&(text.len() as u64).to_le_bytes());
    buf.extend_from_slice(text.as_bytes());
}
fn cache_file_digest(path: &std::path::Path) -> Option<String> {
    Some(cache_digest(&std::fs::read(path).ok()?))
}

/// Cache lives in the OS temp dir — never inside the repository.
fn build_cache_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("un-build-cache-v1")
}
fn build_cache_disabled() -> bool {
    matches!(std::env::var("UV_BUILD_CACHE").as_deref(), Ok("0" | "off" | "false"))
}
fn source_path_of(path: &str) -> std::path::PathBuf {
    if path.ends_with(".un") {
        std::path::PathBuf::from(path)
    } else {
        std::path::PathBuf::from(format!("{path}.un"))
    }
}

fn tool_identity(path: &std::path::Path, version_arg: &str) -> String {
    let mut out = cache_file_digest(path).unwrap_or_else(|| "missing".to_string());
    if let Ok(meta) = std::fs::metadata(path) {
        out.push(':');
        out.push_str(&meta.len().to_string());
    }
    if let Ok(result) = std::process::Command::new(path).arg(version_arg).output() {
        out.push('|');
        out.push_str(&String::from_utf8_lossy(&result.stdout));
        out.push_str(&String::from_utf8_lossy(&result.stderr));
    }
    out
}

/// Identity of everything outside the source that can change the linked exe:
/// rustc/gcc versions, this executable (the Un code generator) and the static
/// runtime library. If any is unavailable, the cache is skipped so the normal
/// build reports the usual error instead.
fn build_toolchain_identity(embed: bool) -> Option<String> {
    let (rustc, gcc) = find_gnu_toolchain()?;
    let current = std::env::current_exe().ok()?;
    let library = current.with_file_name(if embed { "libunvm.a" } else { "libnrt.a" });
    let un_hash = cache_file_digest(&current)?;
    let lib_hash = cache_file_digest(&library)?;
    Some(format!(
        "rustc={}|gcc={}|un={}|lib={}",
        tool_identity(&rustc, "-vV"),
        tool_identity(&gcc, "--version"),
        un_hash,
        lib_hash
    ))
}

/// Key = source bytes + inlined user modules + build flags + target triple +
/// toolchain/runtime identity. Hashing happens before validation; on a hit the
/// validation would have succeeded too, so stdout/stderr/exit stay identical.
fn build_cache_key(
    input: &str,
    source_bytes: &[u8],
    embed: bool,
    allow_dynamic: bool,
    opt_level: &str,
) -> Option<String> {
    let identity = build_toolchain_identity(embed)?;
    let text = String::from_utf8_lossy(source_bytes);
    let normalized: String = Source::new(input, &text).code.iter().collect();
    // Reuse the real inliner so module contents are part of the key. Its
    // diagnostics are still emitted later by the normal build path.
    let bundle = inline_user_modules_text(&normalized, input);
    let mut buf = Vec::new();
    cache_push(&mut buf, "un-build-cache-v1");
    cache_push(&mut buf, input);
    buf.extend_from_slice(&(source_bytes.len() as u64).to_le_bytes());
    buf.extend_from_slice(source_bytes);
    match &bundle {
        Ok(code) => {
            cache_push(&mut buf, "inline");
            cache_push(&mut buf, code);
        }
        Err(message) => {
            cache_push(&mut buf, "inline-error");
            cache_push(&mut buf, message);
        }
    }
    cache_push(&mut buf, if embed { "embed" } else { "plain" });
    cache_push(&mut buf, if allow_dynamic { "dynamic" } else { "typed" });
    cache_push(&mut buf, opt_level);
    cache_push(&mut buf, "x86_64-pc-windows-gnu");
    cache_push(&mut buf, &identity);
    Some(cache_digest(&buf))
}

fn cache_entry_paths(key: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = build_cache_dir();
    (dir.join(format!("{key}.exe")), dir.join(format!("{key}.skipped")))
}
fn cache_diag_paths(key: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = build_cache_dir();
    (dir.join(format!("{key}.stdout")), dir.join(format!("{key}.stderr")))
}

/// Restores a cached executable. Returns false (without output) when the copy
/// fails, so the caller falls back to the ordinary build path.
fn build_cache_restore(key: &str, output: &str) -> bool {
    if build_cache_disabled() {
        return false;
    }
    let (cached_exe, skipped_path) = cache_entry_paths(key);
    if !cached_exe.is_file() || std::fs::copy(&cached_exe, output).is_err() {
        return false;
    }
    // `skipped:` warnings are part of the plain-build stderr; replay them.
    if let Ok(text) = std::fs::read_to_string(&skipped_path) {
        if !text.is_empty() {
            eprint!("{text}");
        }
    }
    true
}

/// rustc can emit warnings for the generated driver. Replay the exact bytes
/// captured on the miss so a hit has identical stdout/stderr/exit.
fn build_cache_replay_diag(key: &str) {
    let (stdout_path, stderr_path) = cache_diag_paths(key);
    if let Ok(bytes) = std::fs::read(stdout_path) {
        let _ = std::io::stdout().write_all(&bytes);
    }
    if let Ok(bytes) = std::fs::read(stderr_path) {
        let _ = std::io::stderr().write_all(&bytes);
    }
}

/// Best effort: a cache write failure must never change build success.
fn build_cache_save(key: &str, output: &str, skipped: &[String], stdout: &[u8], stderr: &[u8]) {
    if build_cache_disabled() {
        return;
    }
    let dir = build_cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let temp = dir.join(format!("{key}.{}.tmp", std::process::id()));
    if std::fs::copy(output, &temp).is_err() {
        return;
    }
    let (cached_exe, skipped_path) = cache_entry_paths(key);
    let _ = std::fs::remove_file(&cached_exe);
    if std::fs::rename(&temp, &cached_exe).is_err() {
        let _ = std::fs::remove_file(&temp);
        return;
    }
    if skipped.is_empty() {
        let _ = std::fs::remove_file(&skipped_path);
    } else {
        let mut text = String::new();
        for line in skipped {
            text.push_str(line);
            text.push('\n');
        }
        let _ = std::fs::write(&skipped_path, text);
    }
    let (stdout_path, stderr_path) = cache_diag_paths(key);
    if stdout.is_empty() {
        let _ = std::fs::remove_file(&stdout_path);
    } else {
        let _ = std::fs::write(&stdout_path, stdout);
    }
    if stderr.is_empty() {
        let _ = std::fs::remove_file(&stderr_path);
    } else {
        let _ = std::fs::write(&stderr_path, stderr);
    }
}

fn build_cmd(args: &[String]) {
    let mut input: Option<&String> = None;
    let mut output: Option<String> = None;
    let mut embed = false;
    let mut allow_dynamic = false;
    let mut opt_level: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                let Some(path) = args.get(i) else {
                    eprintln!("usage: un build <file.un> [-o out.exe] [--embed] [--allow-dynamic] [--opt-level N]");
                    exit(2);
                };
                output = Some(path.clone());
            }
            "--embed" => {
                embed = true;
            }
            "--allow-dynamic" => {
                allow_dynamic = true;
            }
            "--opt-level" => {
                i += 1;
                let Some(level) = args.get(i) else {
                    eprintln!("usage: un build <file.un> [--opt-level 0|1|2|3|s|z]");
                    exit(2);
                };
                if !matches!(level.as_str(), "0" | "1" | "2" | "3" | "s" | "z") {
                    eprintln!("invalid opt-level '{level}' (expected 0|1|2|3|s|z)");
                    exit(2);
                }
                opt_level = Some(level.clone());
            }
            other => {
                if input.is_some() {
                    eprintln!("unexpected argument: {other}");
                    exit(2);
                }
                input = Some(&args[i]);
            }
        }
        i += 1;
    }
    let Some(input) = input else {
        eprintln!("usage: un build <file.un> [-o out.exe] [--embed] [--allow-dynamic]");
        exit(2);
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    native::init(cwd.to_str().unwrap_or("."));
    let output = output.unwrap_or_else(|| {
        if let Some(stripped) = input.strip_suffix(".un") {
            format!("{stripped}.exe")
        } else {
            format!("{input}.exe")
        }
    });

    let opt_level = opt_level.as_deref().unwrap_or("0");
    let key = if build_cache_disabled() {
        None
    } else {
        std::fs::read(source_path_of(input))
            .ok()
            .and_then(|bytes| build_cache_key(input, &bytes, embed, allow_dynamic, opt_level))
    };
    if let Some(ref key) = key {
        if build_cache_restore(key, &output) {
            if !embed {
                println!("un build: no-VM native runtime (no libunvm.a)");
            }
            build_cache_replay_diag(key);
            println!("wrote {output}");
            return;
        }
    }

    let temp_dir = std::env::temp_dir();
    let driver_path = temp_dir.join(format!("un_build_driver_{}.rs", std::process::id()));

    if embed {
        // Fully typed mode applies to the embedded build too: a compiled
        // executable should never rely on dynamic typing. `--allow-dynamic`
        // restores the previous permissive behaviour for both build modes.
        if !allow_dynamic {
            let (parsed, node) = parse_file(input);
            let root = match optimize(&node) {
                Ok(result) => result.root,
                Err(panic) => {
                    eprintln!("{}", panic.display());
                    exit(1);
                }
            };
            if let Err(errors) = typeck::check_program_with_options(
                &parsed,
                &root,
                typeck::CheckOptions { forbid_dynamic: true },
            ) {
                for error in &errors {
                    print!("{}{}", error.display(&parsed), nl());
                }
                exit(1);
            }
        }
        let program = match runner::compile_source(&Rc::new(read_source(input))) {
            Ok(program) => program,
            Err(runner::RunError::Parse(error)) => {
                print!("{}{}", error.display(&Rc::new(read_source(input))), nl());
                exit(1);
            }
            Err(runner::RunError::Panic(panic)) => {
                eprintln!("{}", panic.display());
                exit(1);
            }
        };
        let bytes = unvm::bytecode::unbc::write(&program);
        let image_path = temp_dir.join(format!("un_build_embed_{}.unbc", std::process::id()));
        if let Err(e) = std::fs::write(&image_path, &bytes) {
            eprintln!("cannot write {}: {e}", image_path.display());
            exit(1);
        }
        let driver = format!(
            "extern \"C\" {{\n    fn unvm_run_embedded(bytes: *const u8, len: usize) -> i32;\n}}\nconst UNBC: &[u8] = include_bytes!({:?});\nfn main() -> std::process::ExitCode {{\n    std::process::ExitCode::from(unsafe {{ unvm_run_embedded(UNBC.as_ptr(), UNBC.len()) }} as u8)\n}}\n",
            image_path.display().to_string()
        );
        if let Err(e) = std::fs::write(&driver_path, driver) {
            eprintln!("cannot write {}: {e}", driver_path.display());
            let _ = std::fs::remove_file(&image_path);
            exit(1);
        }
        let Some((rustc, gcc)) = find_gnu_toolchain() else {
            eprintln!("cannot locate the stable-x86_64-pc-windows-gnu toolchain (with the self-contained mingw gcc) under ~/.rustup/toolchains");
            let _ = std::fs::remove_file(&image_path);
            let _ = std::fs::remove_file(&driver_path);
            exit(1);
        };
        let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let staticlib = exe.with_file_name("libunvm.a");
        if !staticlib.is_file() {
            eprintln!("missing {} (run `cargo build --release --manifest-path rust/Cargo.toml`)", staticlib.display());
            let _ = std::fs::remove_file(&image_path);
            let _ = std::fs::remove_file(&driver_path);
            exit(1);
        }
        let link = std::process::Command::new(&rustc)
            .arg(&driver_path)
            .arg("--target")
            .arg("x86_64-pc-windows-gnu")
            .arg("--edition")
            .arg("2021")
            .arg("-C")
            .arg(format!("opt-level={opt_level}"))
            .arg("-C")
            .arg(format!("linker={}", gcc.display()))
            .arg("-C")
            .arg(format!("link-arg={}", staticlib.display()))
            .arg("-C")
            .arg("link-arg=-lmingw32")
            .arg("-C")
            .arg("link-arg=-lmsvcrt")
            .arg("-C")
            .arg("link-arg=-lmingwex")
            .arg("-C")
            .arg("link-arg=-lmsvcrt")
            .arg("-C")
            .arg("link-arg=-lmingwex")
            .arg("-C")
            .arg("link-arg=-lgcc")
            .arg("-C")
            .arg("link-arg=-lgcc_eh")
            .arg("-C")
            .arg("link-arg=-lgcc_s")
            .arg("-C")
            .arg("link-arg=-lkernel32")
            .arg("-C")
            .arg("link-arg=-lgcc")
            .arg("-C")
            .arg("link-arg=-lntdll")
            .arg("-C")
            .arg("link-arg=-luserenv")
            .arg("-C")
            .arg("link-arg=-lws2_32")
            .arg("-C")
            .arg("link-arg=-ldbghelp")
            .arg("-C")
            .arg("link-arg=-lsynchronization")
            .arg("-C")
            .arg("link-arg=-lgcc")
            .arg("-C")
            .arg("link-arg=-lmsvcrt")
            .arg("-C")
            .arg("link-arg=-lmingwex")
            .arg("-o")
            .arg(&output)
            .output();
        let _ = std::fs::remove_file(&image_path);
        let _ = std::fs::remove_file(&driver_path);
        match link {
            Ok(link) => {
                // `status()` used to inherit the streams; replay the captured
                // bytes verbatim so diagnostics stay byte-identical.
                let _ = std::io::stdout().write_all(&link.stdout);
                let _ = std::io::stderr().write_all(&link.stderr);
                if link.status.success() {
                    if let Some(ref key) = key {
                        build_cache_save(key, &output, &[], &link.stdout, &link.stderr);
                    }
                    println!("wrote {output}");
                } else {
                    eprintln!("link failed (rustc exited with {})", link.status);
                    exit(1);
                }
            }
            Err(e) => {
                eprintln!("cannot spawn rustc: {e}");
                exit(1);
            }
        }
        return;
    }

    let (source, node) = parse_file(input);
    fn has_class(node: &unvm::ast::Node) -> bool {
        if node.kind == unvm::ast::NodeKind::Class {
            return true;
        }
        node.children.iter().any(has_class)
    }
    let nested_class = node.children.iter().any(|c| {
        c.kind != unvm::ast::NodeKind::Class && has_class(c)
    });
    if nested_class {
        eprintln!("un build: class declarations nested inside control flow are not supported in a native executable yet; rebuild with `un build --embed`");
        exit(1);
    }
    let inline_code = match inline_user_modules_text(&source.code.iter().collect::<String>(), input) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("un build: {message}; rebuild with `un build --embed`");
            exit(1);
        }
    };
    let source = Rc::new(unvm::source::Source::new(input, &inline_code));
    let toks = match unvm::lexer::lex(&mut source.as_ref().clone()) {
        Ok(t) => t,
        Err(e) => { eprintln!("lex: {}", e.message); exit(1); }
    };
    let node = match unvm::parser::parse(toks, source.clone()) {
        Ok(n) => n,
        Err(e) => { eprintln!("parse: {}", e.message); exit(1); }
    };
    let root = match optimize(&node) {
        Ok(result) => result.root,
        Err(panic) => {
            eprintln!("{}", panic.display());
            exit(1);
        }
    };
    // Plain builds lower only functions and global initializers. Report any
    // other top-level statement instead of silently dropping it.
    if let Some(ignored) = unvm::compiler::find_ignored_top_level_statement(&root) {
        let diagnostic = LangError::new(ignored.start, ignored.length, ignored.message());
        print!("{}{}", diagnostic.display(&source), nl());
        exit(1);
    }
    if let Some(async_node) = unvm::compiler::find_unsupported_async_node(&root) {
        let diagnostic = LangError::new(
            async_node.start,
            async_node.length,
            async_node.message(),
        );
        print!("{}{}", diagnostic.display(&source), nl());
        exit(1);
    }
    let checked = match typeck::check_program_with_options(
        &source,
        &root,
        typeck::CheckOptions { forbid_dynamic: !allow_dynamic },
    ) {
        Ok(checked) => checked,
        Err(errors) => {
            for error in &errors {
                print!("{}{}", error.display(&source), nl());
            }
            exit(1);
        }
    };
    // A base class with optional ctor params produces a different constructor
    // argument order in the VM; keep those programs on the --embed path.
    let bad_base = checked.classes.values().any(|info| {
        info.bases.iter().any(|base| {
            checked.classes.get(base).is_some_and(|b| b.ctor_required < b.ctor_params.len())
        })
    });
    if bad_base {
        eprintln!("un build: base classes with default constructor arguments need `un build --embed`");
        exit(1);
    }
    let global_names = collect_global_names(&inline_code);
    let lowered = match unvm::tir::TirProgram::lower(&source, &root, &checked, &global_names) {
        Ok(program) => program,
        Err(error) => {
            eprintln!("tir: {error}");
            exit(1);
        }
    };

    let temp_dir = std::env::temp_dir();
    let obj_path = temp_dir.join(format!("un_build_{}.obj", std::process::id()));
    let driver_path = temp_dir.join(format!("un_build_driver_{}.rs", std::process::id()));
    let nrt_ok = unvm::tir::nrt_supported(&lowered);
    if !nrt_ok {
        eprintln!("un build: this program uses a feature that requires the VM runtime; rebuild with `un build --embed`");
        exit(1);
    }
    // Classes are registered natively from TIR metadata; no `<classes_init>`
    // bytecode image and no VM static library are needed in a plain build.
    let cache_skipped: Vec<String>;
    let emit_outcome = match unvm::natcode::obj_compile(0, &lowered, &obj_path, None) {
        Ok(outcome) => {
            cache_skipped = outcome.skipped.iter().map(|reason| format!("skipped: {reason}")).collect();
            for line in &cache_skipped {
                eprintln!("{line}");
            }
            outcome
        }
        Err(message) => {
            eprintln!("un build: {message}");
            let hints: Vec<String> = checked
                .functions
                .iter()
                .filter_map(unvm::tir::core_rejection_reason)
                .collect();
            if !hints.is_empty() {
                eprintln!("hint: native compilation requires core types:");
                for hint in hints.iter().take(5) {
                    eprintln!("  - {hint}");
                }
            }
            exit(1);
        }
    };

    // Standalone executable with no VM: link libnrt.a instead of libunvm.a.
    // Class descriptors are registered before `un_main` runs; methods are
    // the `un_tir_N` functions emitted into the object file.
    let mut d = String::new();
    d.push_str("extern \"C\" {\n");
    d.push_str("    fn un_main() -> i64;\n");
    d.push_str("    fn nrt_register_source(path: *const u8, plen: usize, text: *const u8, tlen: usize);\n");
    d.push_str("    fn nrt_push_frame(name: *const u8, nlen: usize, start: usize, len: usize);\n");


    if !lowered.class_names.is_empty() {
        d.push_str("    fn nrt_register_class(name: *const u8, len: usize) -> i64;\n");
        d.push_str("    fn nrt_register_field(class: i64, name: *const u8, len: usize);\n");
        d.push_str("    fn nrt_register_method(class: i64, name: *const u8, len: usize, ptr: usize, kinds: *const u8, kinds_len: usize, ret_kind: u8);\n");
        d.push_str("    fn nrt_freeze_classes();\n");
    }
    let mut declared: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for (_, _, tir_i, _, _) in &emit_outcome.class_methods {
        declared.insert(*tir_i);
    }
    let lambda_indices: Vec<usize> = lowered.functions.iter().enumerate()
        .filter(|(_, f)| f.name.starts_with("__lambda_"))
        .map(|(i, _)| i)
        .collect();
    if !lambda_indices.is_empty() {
        d.push_str("    fn nrt_register_lambda(index: i64, ptr: usize, kinds: *const u8, klen: usize, ret_kind: u8);\n");
    }
    if !global_names.is_empty() {
        d.push_str("    fn nrt_register_global(name: *const u8, nlen: usize) -> i64;\n");
    }
    for i in &lambda_indices { declared.insert(*i); }
    for i in &declared {
        d.push_str(&format!("    fn un_tir_{i}();\n"));
    }
    d.push_str("}\nfn main() -> std::process::ExitCode {\n");
    let path_lit = format!("{:?}", input);
    let src_lit = format!("{:?}", inline_code);
    d.push_str(&format!(
        "    unsafe {{ nrt_register_source({path_lit}.as_bytes().as_ptr(), {path_lit}.len(), {src_lit}.as_bytes().as_ptr(), {src_lit}.len()) }};\n"
    ));
    // The VM records a `<module>` frame for the top-level `main()` call.
    let main_call_byte = inline_code.rfind("main()").unwrap_or(0);
    // `Source` offsets are char-based; `str::rfind` returns a byte offset.
    let main_call_offset = inline_code[..main_call_byte].chars().count();
    d.push_str(&format!(
        "    unsafe {{ nrt_push_frame(\"\".as_bytes().as_ptr(), 0, {main_call_offset}, 6) }};\n"
    ));
    for name in &global_names {
        d.push_str(&format!(
            "    unsafe {{ nrt_register_global({name:?}.as_bytes().as_ptr(), {name:?}.len()) }};\n"
        ));
    }
    // INHERIT-WIRING decision (W-2026-11-25-INHERITWIRING): `class_parents` is
    // recorded in TIR, but the driver deliberately does NOT call
    // `nrt_register_class_parent`. Measured on `class Child: Base`, the VM's
    // type-name pattern goes through `is_obj` -> `Types::accepts` (exact name),
    // so `match c { Base -> .. }` falls through to `_` in `un run` and
    // `un native`. NRT's `unrt_type_is` instead walks registered parents via
    // `class_chain_accepts`; emitting the edges would make a plain exe match
    // `Base` and break byte-identical parity. Wire them only together with a
    // VM semantics change (and renewed 3-lane evidence).
    for (class_id, class_name) in lowered.class_names.iter().enumerate() {
        d.push_str(&format!(
            "    let c{class_id} = unsafe {{ nrt_register_class({class_name:?}.as_bytes().as_ptr(), {}) }};\n",
            class_name.len()
        ));
    }
    for (class_id, names) in lowered.class_field_order.iter().enumerate() {
        for name in names {
            d.push_str(&format!(
                "    unsafe {{ nrt_register_field(c{class_id}, {name:?}.as_bytes().as_ptr(), {}) }};\n",
                name.len()
            ));
        }
    }
    for (class_name, method_name, tir_i, _arity, ret_kind) in &emit_outcome.class_methods {
        let class_id = lowered.class_names.iter().position(|n| n == class_name).unwrap_or(0);
        let f = &lowered.functions[*tir_i];
        let kinds: Vec<u8> = f.params.iter().skip(1).map(|t| match t {
            unvm::tir::TirTy::Int => 0u8,
            unvm::tir::TirTy::Float => 1,
            unvm::tir::TirTy::Bool => 2,
            _ => 3,
        }).collect();
        let kind_const = format!("KINDS_{class_id}_{tir_i}");
        d.push_str(&format!("    const {kind_const}: &[u8] = &{kinds:?};\n"));
        d.push_str(&format!(
            "    unsafe {{ nrt_register_method(c{class_id}, {method_name:?}.as_bytes().as_ptr(), {}, un_tir_{tir_i} as *const () as usize, {kind_const}.as_ptr(), {kind_const}.len(), {ret_kind}) }};\n",
            method_name.len()
        ));
    }
    for i in &lambda_indices {
        let f = &lowered.functions[*i];
        let kinds: Vec<u8> = f.params.iter().map(|t| match t {
            unvm::tir::TirTy::Int => 0u8,
            unvm::tir::TirTy::Float => 1,
            unvm::tir::TirTy::Bool => 2,
            _ => 3,
        }).collect();
        let ret_kind = match f.ret {
            Some(unvm::tir::TirTy::Int) => 0u8,
            Some(unvm::tir::TirTy::Float) => 1,
            Some(unvm::tir::TirTy::Bool) => 2,
            _ => 3,
        };
        let kc = format!("LKINDS_{i}");
        d.push_str(&format!("    const {kc}: &[u8] = &{kinds:?};\n"));
        d.push_str(&format!(
            "    unsafe {{ nrt_register_lambda({i} as i64, un_tir_{i} as *const () as usize, {kc}.as_ptr(), {kc}.len(), {ret_kind}) }};\n"
        ));
    }
    if !lowered.class_names.is_empty() {
        d.push_str("    unsafe { nrt_freeze_classes() };\n");
    }
    d.push_str("    let code = unsafe { un_main() };\n");
    d.push_str("    std::process::ExitCode::from(code as u8)\n}\n");
    let driver = d;
    if let Err(e) = std::fs::write(&driver_path, driver) {
        eprintln!("cannot write {}: {e}", driver_path.display());
        exit(1);
    }

    let Some((rustc, gcc)) = find_gnu_toolchain() else {
        eprintln!("cannot locate the stable-x86_64-pc-windows-gnu toolchain (with the self-contained mingw gcc) under ~/.rustup/toolchains");
        exit(1);
    };
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let nrtlib = exe.with_file_name("libnrt.a");
    if !nrtlib.is_file() {
        eprintln!("missing {} (run `cargo build --release --manifest-path rust/Cargo.toml`)", nrtlib.display());
        exit(1);
    }
    println!("un build: no-VM native runtime (no libunvm.a)");
    let mut cmd = std::process::Command::new(&rustc);
    cmd.arg(&driver_path)
        .arg("--target")
        .arg("x86_64-pc-windows-gnu")
        .arg("--edition")
        .arg("2021")
        .arg("-C")
        .arg(format!("opt-level={opt_level}"))
        .arg("-C")
        .arg(format!("linker={}", gcc.display()))
        .arg("-C")
        .arg(format!("link-arg={}", obj_path.display()));
    cmd.arg("-C").arg(format!("link-arg={}", nrtlib.display()));
    let link = cmd
        .arg("-C")
        .arg("link-arg=-lmingw32")
        .arg("-C")
        .arg("link-arg=-lmsvcrt")
        .arg("-C")
        .arg("link-arg=-lmingwex")
        .arg("-C")
        .arg("link-arg=-lmsvcrt")
        .arg("-C")
        .arg("link-arg=-lmingwex")
        .arg("-C")
        .arg("link-arg=-lgcc")
        .arg("-C")
        .arg("link-arg=-lgcc_eh")
        .arg("-C")
        .arg("link-arg=-lkernel32")
        .arg("-C")
        .arg("link-arg=-lntdll")
        .arg("-C")
        .arg("link-arg=-luserenv")
        .arg("-C")
        .arg("link-arg=-lws2_32")
        .arg("-C")
        .arg("link-arg=-ldbghelp")
        .arg("-C")
        .arg("link-arg=-lsynchronization")
        .arg("-C")
        .arg("link-arg=-lgcc")
        .arg("-C")
        .arg("link-arg=-lmsvcrt")
        .arg("-C")
        .arg("link-arg=-lmingwex")
        .arg("-C")
        .arg("link-arg=-lgcc")
        .arg("-o")
        .arg(&output)
        .output();
    let _ = std::fs::remove_file(&obj_path);
    let _ = std::fs::remove_file(&driver_path);
    match link {
        Ok(link) => {
            let _ = std::io::stdout().write_all(&link.stdout);
            let _ = std::io::stderr().write_all(&link.stderr);
            if link.status.success() {
                if let Some(ref key) = key {
                    build_cache_save(key, &output, &cache_skipped, &link.stdout, &link.stderr);
                }
                println!("wrote {output}");
            } else {
                eprintln!("link failed (rustc exited with {})", link.status);
                exit(1);
            }
        }
        Err(e) => {
            eprintln!("cannot spawn rustc: {e}");
            exit(1);
        }
    }
}

fn compile(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: un compile <file>");
        exit(2);
    }
    let source = Rc::new(read_source(&args[0]));
    let mut program = match runner::compile_source(&source) {
        Ok(program) => program,
        Err(runner::RunError::Parse(error)) => {
            print!("{}{}", error.display(&source), nl());
            exit(1);
        }
        Err(runner::RunError::Panic(panic)) => {
            eprintln!("{}", panic.display());
            exit(1);
        }
    };

    program.source = Some(source.code.iter().collect());
    let path = format!("{}.unbc", args[0]);
    let bytes = unvm::bytecode::unbc::write(&program);
    match std::fs::write(&path, bytes) {
        Ok(()) => {
            println!("wrote {path}");
        }
        Err(e) => {
            eprintln!("cannot write {path}: {e}");
            exit(1);
        }
    }
}
fn run_bytecode(path: &str) {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            exit(1);
        }
    };
    let program = match unvm::bytecode::unbc::read(&bytes) {
        Ok(program) => program,
        Err(message) => {
            eprintln!("{path}: {message}");
            exit(1);
        }
    };
    let text = program.source.clone().unwrap_or_default();
    let source = Rc::new(Source::new(&program.source_path, &text));
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
            let root = Some((
                e.source.clone().unwrap_or_else(|| source.clone()),
                e.start,
            ));
            report(&vm.ctx, e.display(&source), root);
        }
        Outcome::Propagate(err, start, length) => {
            world::flush_stdout();
            let e = LangError::new(start, length, err_message(&err)).with_header(err_header(&err));
            report(&vm.ctx, e.display(&source), Some((source.clone(), start)));
        }
        Outcome::Return(_, start, length) => {
            world::flush_stdout();
            report(&vm.ctx, LangError::new(start, length, "'->' outside function").display(&source), Some((source.clone(), start)));
        }
        Outcome::Break(start, length) => {
            world::flush_stdout();
            report(&vm.ctx, LangError::new(start, length, "'break' outside loop").display(&source), Some((source.clone(), start)));
        }
        Outcome::Skip(start, length) => {
            world::flush_stdout();
            report(&vm.ctx, LangError::new(start, length, "'skip' outside loop").display(&source), Some((source.clone(), start)));
        }
        Outcome::Panic(o) => {
            world::flush_stdout();
            report(&vm.ctx, panic_display(&o), None);
        }
    }
}
fn report(ctx: &context::Context, text: String, root: Option<(Rc<Source>, usize)>) {
    world::flush_stdout();
    unvm::jit_cache::clear();
    let root_hint = root.as_ref().map(|(source, start)| (source.as_ref(), *start));
    if let Some(trace) = context::trace_string(ctx, root_hint) {
        print!("{trace}");
    }
    print!("{text}{}", nl());
    exit(1);
}
