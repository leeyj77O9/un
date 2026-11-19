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
    print!("  un build <file.un> [-o out.exe] [--embed]  (emit .obj and link into a native executable; --embed bakes the whole VM runtime for any-program support){}", nl());
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
    // Desugaring is folded into the optimizer as of 2026-11-16; printing the
    // optimized AST shows exactly what the compiler consumes.
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
    // Compile mode requires a clean static analysis pass; print diagnostics
    // and bail out otherwise.
    let checked = match typeck::check_program(&source, &root) {
        Ok(checked) => checked,
        Err(errors) => {
            for error in &errors {
                print!("{}{}", error.display(&source), nl());
            }
            eprintln!("{} compile-mode error(s)", errors.len());
            exit(1);
        }
    };
    // Lower the checked program to TIR and register the native entry points.
    let lowered = match unvm::tir::TirProgram::lower(&source, &root, &checked) {
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
    let failed = unvm::natcode::native_compile(program.id, &program, &lowered);
    for name in &failed {
        eprintln!("warning: '{name}' could not be natively compiled; the VM runs it in the interpreter");
    }
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
            eprintln!("{} compile-mode error(s)", errors.len());
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

    // Needed for `io.write(...)` and friends to be recognized as native calls.
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    native::init(cwd.to_str().unwrap_or("."));

    if input.ends_with(".unbc") {
        // Bytecode path: Int core subset from a precompiled .unbc.
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

    // Source path: parse -> optimize -> typeck -> TIR -> .obj (loops included).
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
            eprintln!("{} compile-mode error(s)", errors.len());
            exit(1);
        }
    };
    let lowered = match unvm::tir::TirProgram::lower(&source, &root, &checked) {
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

/// Locate rustup's stable-x86_64-pc-windows-gnu toolchain (rustc + self-contained
/// mingw gcc, used as the linker). The default `rustc` on PATH may belong to a
/// different target (e.g. MSVC), which cannot link GNU object files.
fn find_gnu_toolchain() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let toolchains = std::path::Path::new(&home).join(".rustup").join("toolchains");
    let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&toolchains)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    // Prefer the repo-standard toolchain explicitly.
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

fn build_cmd(args: &[String]) {
    let mut input: Option<&String> = None;
    let mut output: Option<String> = None;
    let mut embed = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                let Some(path) = args.get(i) else {
                    eprintln!("usage: un build <file.un> [-o out.exe] [--embed]");
                    exit(2);
                };
                output = Some(path.clone());
            }
            "--embed" => {
                embed = true;
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
        eprintln!("usage: un build <file.un> [-o out.exe] [--embed]");
        exit(2);
    };
    // Needed for `io.write(...)` and friends to be recognized as native calls.
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    native::init(cwd.to_str().unwrap_or("."));
    let output = output.unwrap_or_else(|| {
        if let Some(stripped) = input.strip_suffix(".un") {
            format!("{stripped}.exe")
        } else {
            format!("{input}.exe")
        }
    });

    let temp_dir = std::env::temp_dir();
    let driver_path = temp_dir.join(format!("un_build_driver_{}.rs", std::process::id()));

    if embed {
        // --embed: bake the .unbc image into the generated driver and link it
        // against libunvm.a (the whole VM runtime, statically). Works for any
        // Un program, not just the Int core subset.
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
        let status = std::process::Command::new(&rustc)
            .arg(&driver_path)
            .arg("--target")
            .arg("x86_64-pc-windows-gnu")
            .arg("--edition")
            .arg("2021")
            .arg("-C")
            .arg(format!("linker={}", gcc.display()))
            // libunvm.a is placed last by rustc, so GNU ld would never re-scan
            // the Windows import libraries for its undefined symbols. Repeat
            // them here, after the static library.
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
            .status();
        let _ = std::fs::remove_file(&image_path);
        let _ = std::fs::remove_file(&driver_path);
        match status {
            Ok(status) if status.success() => {
                println!("wrote {output}");
            }
            Ok(status) => {
                eprintln!("link failed (rustc exited with {status})");
                exit(1);
            }
            Err(e) => {
                eprintln!("cannot spawn rustc: {e}");
                exit(1);
            }
        }
        return;
    }

    // Compile source to TIR and emit the object file into a temp directory.
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
            eprintln!("{} compile-mode error(s)", errors.len());
            exit(1);
        }
    };
    let lowered = match unvm::tir::TirProgram::lower(&source, &root, &checked) {
        Ok(program) => program,
        Err(error) => {
            eprintln!("tir: {error}");
            exit(1);
        }
    };

    let temp_dir = std::env::temp_dir();
    let obj_path = temp_dir.join(format!("un_build_{}.obj", std::process::id()));
    let driver_path = temp_dir.join(format!("un_build_driver_{}.rs", std::process::id()));
    match unvm::natcode::obj_compile(0, &lowered, &obj_path, None) {
        Ok(outcome) => {
            if !outcome.skipped.is_empty() {
                for reason in &outcome.skipped {
                    eprintln!("skipped: {reason}");
                }
            }
        }
        Err(message) => {
            eprintln!("un build: {message}");
            exit(1);
        }
    }

    // Generate the driver: initialise the VM runtime, call un_main(), flush
    // stdout, exit with the i64 result.
    let driver = "extern \"C\" {\n    fn unrt_init();\n    fn unrt_flush_stdout();\n    fn un_main() -> i64;\n}\nfn main() {\n    unsafe { unrt_init(); }\n    let code = unsafe { un_main() };\n    unsafe { unrt_flush_stdout(); }\n    std::process::exit(code as i32)\n}\n";
    if let Err(e) = std::fs::write(&driver_path, driver) {
        eprintln!("cannot write {}: {e}", driver_path.display());
        exit(1);
    }

    let Some((rustc, gcc)) = find_gnu_toolchain() else {
        eprintln!("cannot locate the stable-x86_64-pc-windows-gnu toolchain (with the self-contained mingw gcc) under ~/.rustup/toolchains");
        exit(1);
    };
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let staticlib = exe.with_file_name("libunvm.a");
    if !staticlib.is_file() {
        eprintln!("missing {} (run `cargo build --release --manifest-path rust/Cargo.toml`)", staticlib.display());
        exit(1);
    }
    let status = std::process::Command::new(&rustc)
        .arg(&driver_path)
        .arg("--target")
        .arg("x86_64-pc-windows-gnu")
        .arg("--edition")
        .arg("2021")
        .arg("-C")
        .arg(format!("linker={}", gcc.display()))
        .arg("-C")
        .arg(format!("link-arg={}", obj_path.display()))
        // libunvm.a comes last from rustc's perspective; repeat the Windows
        // import libraries so GNU ld can resolve the runtime's references.
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
        .status();
    let _ = std::fs::remove_file(&obj_path);
    let _ = std::fs::remove_file(&driver_path);
    match status {
        Ok(status) if status.success() => {
            println!("wrote {output}");
        }
        Ok(status) => {
            eprintln!("link failed (rustc exited with {status})");
            exit(1);
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
