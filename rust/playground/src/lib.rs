
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

use unvm::lexer::lex;
use unvm::native;
use unvm::optimize::optimize;
use unvm::parser::parse;
use unvm::runner;
use unvm::typeck;
use unvm::runtime::context;
use unvm::runtime::obj::{self, Payload};
use unvm::runtime::world;
use unvm::source::Source;
use unvm::vm::{err_header, err_message, panic_display, Outcome, VM};
use unvm::error::LangError;

const DEFAULT_ALLOWED: &[&str] = &[
    "io", "math", "iter", "re", "random", "time", "builtin", "atomic",
];

pub struct PlaygroundResult {
    pub stdout: String,
    pub error: Option<String>,
    pub timed_out: bool,
    pub exit_code: i32,
    pub needs_input: bool,
}

fn is_need_input(o: &obj::Obj) -> bool {
    matches!(obj::payload(o), Payload::Panic { name, .. } if name == "need_input")
}

fn render_error(
    ctx: &context::Context,
    text: String,
    root: Option<(Rc<Source>, usize)>,
) -> String {
    let root_hint = root.as_ref().map(|(source, start)| (source.as_ref(), *start));
    match context::trace_string(ctx, root_hint) {
        Some(trace) => format!("{trace}{text}"),
        None => text,
    }
}

pub fn run(
    code: &str,
    file_name: &str,
    allowed: Option<Vec<String>>,
    stdin: &str,
    allow_eof: bool,
) -> PlaygroundResult {
    world::reset();

    native::init("memory://");
    world::set_path("memory://");
    world::set_allowed(
        allowed.unwrap_or_else(|| DEFAULT_ALLOWED.iter().map(|s| (*s).to_owned()).collect()),
    );
    world::set_allow_exit(false);
    world::log_clear();
    world::set_stdin(stdin);
    world::set_allow_eof(allow_eof);

    let mut exit_code = 0;
    let mut error: Option<String> = None;
    let mut needs_input = false;

    let source = Rc::new(Source::new(file_name, code));
    let program = match runner::compile_source(&source) {
        Ok(program) => Some(program),
        Err(runner::RunError::Parse(parse_error)) => {
            error = Some(parse_error.display(&source));
            exit_code = 1;
            None
        }
        Err(runner::RunError::Panic(panic)) => {
            error = Some(panic.display());
            exit_code = 1;
            None
        }
    };

    if let Some(program) = program {
        match VM::new(Rc::new(program), source.clone()) {
            Ok(mut vm) => match vm.run() {
                Outcome::Value(_) => {}
                Outcome::Error(e) => {
                    let root = Some((
                        e.source.clone().unwrap_or_else(|| source.clone()),
                        e.start,
                    ));
                    let text = e.display(&source);
                    error = Some(render_error(&vm.ctx, text, root));
                    exit_code = 1;
                }
                Outcome::Propagate(err, start, length) => {
                    let e = LangError::new(start, length, err_message(&err)).with_header(err_header(&err));
                    let text = e.display(&source);
                    error = Some(render_error(&vm.ctx, text, Some((source.clone(), start))));
                    exit_code = 1;
                }
                Outcome::Return(_, start, length) => {
                    let e = LangError::new(start, length, "'->' outside function");
                    let text = e.display(&source);
                    error = Some(render_error(&vm.ctx, text, Some((source.clone(), start))));
                    exit_code = 1;
                }
                Outcome::Break(start, length) => {
                    let e = LangError::new(start, length, "'break' outside loop");
                    let text = e.display(&source);
                    error = Some(render_error(&vm.ctx, text, Some((source.clone(), start))));
                    exit_code = 1;
                }
                Outcome::Skip(start, length) => {
                    let e = LangError::new(start, length, "'skip' outside loop");
                    let text = e.display(&source);
                    error = Some(render_error(&vm.ctx, text, Some((source.clone(), start))));
                    exit_code = 1;
                }
                Outcome::Panic(o) => {
                    if is_need_input(&o) {
                        needs_input = true;
                    } else {
                        let text = panic_display(&o);
                        error = Some(render_error(&vm.ctx, text, None));
                    }
                    exit_code = 1;
                }
            },
            Err(message) => {
                error = Some(format!("vm: {message}"));
                exit_code = 1;
            }
        }
    }

    let stdout = world::log_text();
    unvm::jit_cache::clear();
    world::reset();

    PlaygroundResult {
        stdout,
        error,
        timed_out: false,
        exit_code,
        needs_input,
    }
}

fn escape_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub fn result_to_json(result: &PlaygroundResult) -> String {
    let error = match &result.error {
        Some(text) => format!("\"{}\"", escape_json(text)),
        None => "null".to_string(),
    };

    format!(
        "{{\"stdout\": \"{}\", \"error\": {}, \"timedOut\": {}, \"exitCode\": {}, \"needsInput\": {}}}",
        escape_json(&result.stdout),
        error,
        if result.timed_out { "true" } else { "false" },
        result.exit_code,
        if result.needs_input { "true" } else { "false" }
    )
}

// ── Type check + inference ──────────────────────────────────────────────
// The LSP (`rust/src/lsp.rs`) does two separate things: it runs the real
// `typeck::check_program` for diagnostics, and it has a lightweight textual
// `infer_types` pass for variable/function types. The playground needs both,
// so they are mirrored here and exposed through the `playground_check` ABI.

#[derive(Debug, Clone)]
pub struct CheckDiagnostic {
    /// UTF-16 offset into the original source (what a JS string indexes by).
    pub start: usize,
    /// UTF-16 length of the highlighted span.
    pub length: usize,
    /// 0-based line.
    pub line: usize,
    /// 0-based UTF-16 column.
    pub column: usize,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct CheckResult {
    pub ok: bool,
    pub diagnostics: Vec<CheckDiagnostic>,
    pub vars: Vec<(String, String)>,
    pub fn_params: Vec<(String, Vec<(String, String)>)>,
    pub fn_returns: Vec<(String, String)>,
}

/// Mirrors `Source::new` normalization and maps each normalized char index to
/// its UTF-16 offset in the original text, so spans work for CRLF/BOM/CJK.
fn normalized_with_map(raw: &str) -> (String, Vec<usize>, usize) {
    let mut out = String::with_capacity(raw.len());
    let mut map = Vec::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    if chars.peek() == Some(&'\u{feff}') {
        chars.next();
    }
    let mut utf16 = 0usize;
    while let Some(ch) = chars.next() {
        map.push(utf16);
        utf16 += ch.len_utf16();
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                let nl = chars.next().expect("peeked newline");
                utf16 += nl.len_utf16();
            }
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    (out, map, utf16)
}

fn span_utf16(map: &[usize], end: usize, start: usize, length: usize) -> (usize, usize) {
    let from = map.get(start).copied().unwrap_or(end);
    let to = map.get(start.saturating_add(length)).copied().unwrap_or(end);
    (from, to.max(from))
}

fn diagnostic(error: &LangError, source: &Source, map: &[usize], end: usize) -> CheckDiagnostic {
    let (start, stop) = span_utf16(map, end, error.start, error.length);
    let line_start = source.get_line_start(error.start);
    let line_base = map.get(line_start).copied().unwrap_or(0);
    CheckDiagnostic {
        start,
        length: stop - start,
        line: source.get_line(error.start).saturating_sub(1),
        column: start.saturating_sub(line_base),
        message: error.message.clone(),
    }
}

/// Lightweight textual inference copied from `lsp.rs::infer_types`. It still
/// produces useful types for top-level scripts (which `typeck` reports through
/// function locals only) and while the document is too broken to type-check.
fn textual_infer(code: &[char]) -> (
    HashMap<String, String>,
    HashMap<String, Vec<(String, String)>>,
    HashMap<String, String>,
) {
    let mut vars = HashMap::new();
    let mut fn_params = HashMap::new();
    let mut fn_returns = HashMap::new();
    let mut i = 0usize;
    while i < code.len() {
        let line_start = i;
        while i < code.len() && code[i] != '\n' {
            i += 1;
        }
        if i < code.len() {
            i += 1;
        }
        let line: String = code[line_start..i].iter().collect();
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        // `name = expr` / `name: Type = expr`
        if let Some(eq) = trimmed.find('=') {
            let left = trimmed[..eq].trim();
            let rhs = trimmed[eq + 1..].trim();
            let (name, declared) = match left.find(':') {
                Some(colon) => (&left[..colon], Some(left[colon + 1..].trim().to_string())),
                None => (left, None),
            };
            if !name.is_empty()
                && name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                && !name.chars().next().unwrap().is_ascii_digit()
            {
                let inferred = declared
                    .filter(|t| is_builtin_type(t))
                    .or_else(|| infer_expr_type(rhs));
                if let Some(t) = inferred {
                    vars.insert(name.to_string(), t);
                }
            }
        }
        // `fn name(params) -> ret`
        if let Some(rest) = trimmed.strip_prefix("fn ") {
            let name: String = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect();
            if !name.is_empty() {
                if let Some(open) = rest.find('(') {
                    if let Some(close) = matching_paren(rest, open) {
                        let mut params = Vec::new();
                        for part in split_top_level(&rest[open + 1..close], ',') {
                            let p = part.trim();
                            if p.is_empty() {
                                continue;
                            }
                            let pname: String = p
                                .chars()
                                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                                .collect();
                            let ptype = if let Some(colon) = p.find(':') {
                                p[colon + 1..]
                                    .trim()
                                    .split(['=', ' '])
                                    .next()
                                    .unwrap_or("")
                                    .to_string()
                            } else {
                                let default = p.find('=').map(|e| p[e + 1..].trim()).unwrap_or("");
                                infer_expr_type(default).unwrap_or_else(|| "any".to_string())
                            };
                            params.push((pname, ptype));
                        }
                        fn_params.insert(name.clone(), params);
                        let after = rest[close + 1..].trim();
                        if let Some(ret) = after.strip_prefix("->") {
                            let ret = ret.split([' ', ':']).next().unwrap_or("").to_string();
                            if !ret.is_empty() {
                                fn_returns.insert(name, ret);
                            }
                        }
                    }
                }
            }
        }
    }
    (vars, fn_params, fn_returns)
}

fn is_builtin_type(name: &str) -> bool {
    matches!(name, "list" | "dict" | "set" | "iter" | "str" | "date" | "stopwatch")
}

fn infer_expr_type(expr: &str) -> Option<String> {
    let text = expr.trim();
    if text.is_empty() {
        return None;
    }
    if text.starts_with('"') || text.starts_with('\'') || text.starts_with('`') {
        return Some("str".to_string());
    }
    if text == "true" || text == "false" {
        return Some("bool".to_string());
    }
    if text.starts_with('[') {
        return Some("list".to_string());
    }
    if text.starts_with('{') {
        return Some("dict".to_string());
    }
    if text.starts_with("set(") {
        return Some("set".to_string());
    }
    if text.starts_with("iter.") || text.starts_with("iter(") {
        return Some("iter".to_string());
    }
    if text.chars().all(|ch| ch.is_ascii_digit() || ch == '_' || ch == '-')
        && text.chars().any(|ch| ch.is_ascii_digit())
    {
        return Some("int".to_string());
    }
    if text.chars().all(|ch| ch.is_ascii_digit() || ch == '_' || ch == '-' || ch == '.')
        && text.contains('.')
    {
        return Some("float".to_string());
    }
    None
}

fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0i64;
    for (i, ch) in text.char_indices().skip_while(|(i, _)| *i < open) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top_level(text: &str, sep: char) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i64;
    for (i, ch) in text.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            c if c == sep && depth == 0 => {
                result.push(&text[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    result.push(&text[start..]);
    result
}

pub fn check_source(code: &str, file_name: &str) -> CheckResult {
    let file_name = if file_name.is_empty() { "playground.un" } else { file_name };
    let (normalized, map, end) = normalized_with_map(code);
    let mut result = CheckResult::default();

    let mut source = Source::new(file_name, &normalized);
    let (mut vars, mut fn_params, mut fn_returns) = textual_infer(&source.code);

    let tokens = match lex(&mut source) {
        Ok(tokens) => tokens,
        Err(error) => {
            result.diagnostics.push(diagnostic(&error, &source, &map, end));
            result.vars.extend(vars);
            result.fn_params.extend(fn_params);
            result.fn_returns.extend(fn_returns);
            return result;
        }
    };
    let shared = Rc::new(source.clone());
    let root = match parse(tokens, shared.clone()) {
        Ok(root) => root,
        Err(error) => {
            result.diagnostics.push(diagnostic(&error, &source, &map, end));
            result.vars.extend(vars);
            result.fn_params.extend(fn_params);
            result.fn_returns.extend(fn_returns);
            return result;
        }
    };
    let optimized = match optimize(&root) {
        Ok(result) => result.root,
        Err(panic) => {
            let error = LangError::new(0, source.code.len(), panic.message);
            result.diagnostics.push(diagnostic(&error, &source, &map, end));
            result.vars.extend(vars);
            result.fn_params.extend(fn_params);
            result.fn_returns.extend(fn_returns);
            return result;
        }
    };

    let mut checked = false;
    match typeck::check_program(&shared, &optimized) {
        Ok(program) => {
            checked = true;
            for (name, sig) in &program.sigs {
                if name.is_empty() {
                    continue;
                }
                let params = sig
                    .param_names
                    .iter()
                    .zip(sig.params.iter())
                    .map(|(name, ty)| (name.clone(), ty.display()))
                    .collect();
                fn_params.insert(name.clone(), params);
                let ret = sig.ret.display();
                if ret != "<unknown>" {
                    fn_returns.insert(name.clone(), ret);
                }
            }
            for function in &program.functions {
                for (name, ty) in &function.local_types {
                    let display = ty.display();
                    if display != "<unknown>" && display != "any" {
                        vars.insert(name.clone(), display);
                    }
                }
            }
        }
        Err(errors) => {
            for error in &errors {
                result.diagnostics.push(diagnostic(error, &source, &map, end));
            }
        }
    }

    result.ok = checked;
    result.vars = vars.into_iter().collect();
    result.vars.sort();
    result.fn_params = fn_params.into_iter().collect();
    result.fn_params.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, params) in &mut result.fn_params {
        params.sort_by(|a, b| a.0.cmp(&b.0));
    }
    result.fn_returns = fn_returns.into_iter().collect();
    result.fn_returns.sort();
    result
}

fn check_result_to_json(result: &CheckResult) -> String {
    let diagnostics: Vec<String> = result
        .diagnostics
        .iter()
        .map(|d| {
            format!(
                "{{\"start\":{},\"length\":{},\"line\":{},\"column\":{},\"message\":\"{}\"}}",
                d.start,
                d.length,
                d.line,
                d.column,
                escape_json(&d.message)
            )
        })
        .collect();
    let vars: Vec<String> = result
        .vars
        .iter()
        .map(|(name, ty)| format!("[\"{}\",\"{}\"]", escape_json(name), escape_json(ty)))
        .collect();
    let fn_params: Vec<String> = result
        .fn_params
        .iter()
        .map(|(name, params)| {
            let params: Vec<String> = params
                .iter()
                .map(|(name, ty)| format!("[\"{}\",\"{}\"]", escape_json(name), escape_json(ty)))
                .collect();
            format!("[\"{}\",[{}]]", escape_json(name), params.join(","))
        })
        .collect();
    let fn_returns: Vec<String> = result
        .fn_returns
        .iter()
        .map(|(name, ty)| format!("[\"{}\",\"{}\"]", escape_json(name), escape_json(ty)))
        .collect();
    format!(
        "{{\"ok\":{},\"diagnostics\":[{}],\"vars\":[{}],\"fnParams\":[{}],\"fnReturns\":[{}]}}",
        if result.ok { "true" } else { "false" },
        diagnostics.join(","),
        vars.join(","),
        fn_params.join(","),
        fn_returns.join(",")
    )
}

/// Runs the real `typeck` checker plus LSP-style type inference over `code`.
/// Returns JSON: `{ok, diagnostics, vars, fnParams, fnReturns}`.
#[no_mangle]
pub unsafe extern "C" fn playground_check(
    code_ptr: *const u8,
    code_len: usize,
    file_ptr: *const u8,
    file_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    if out_len.is_null() {
        return std::ptr::null_mut();
    }
    let code = String::from_utf8_lossy(read_bytes(code_ptr, code_len)).into_owned();
    let file_name = String::from_utf8_lossy(read_bytes(file_ptr, file_len)).into_owned();
    let result = catch_unwind(AssertUnwindSafe(|| check_source(&code, &file_name)))
        .unwrap_or_else(|_| {
            let mut result = CheckResult::default();
            result.diagnostics.push(CheckDiagnostic {
                start: 0,
                length: 0,
                line: 0,
                column: 0,
                message: "type checker crashed".to_string(),
            });
            result
        });
    let json = check_result_to_json(&result);
    let len = json.len();
    let buf = playground_alloc(len);
    if buf.is_null() {
        return std::ptr::null_mut();
    }
    std::ptr::copy_nonoverlapping(json.as_ptr(), buf, len);
    *out_len = len;
    buf
}

#[no_mangle]
pub unsafe extern "C" fn playground_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }

    let mut buf = Vec::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn playground_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }

    let layout = std::alloc::Layout::array::<u8>(len).expect("layout");
    std::alloc::dealloc(ptr, layout);
}

fn read_bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if ptr.is_null() || len == 0 {
        return &[];
    }

    unsafe { std::slice::from_raw_parts(ptr, len) }
}

#[no_mangle]
pub unsafe extern "C" fn playground_run(
    code_ptr: *const u8,
    code_len: usize,
    file_ptr: *const u8,
    file_len: usize,
    allowed_ptr: *const u8,
    allowed_len: usize,
    stdin_ptr: *const u8,
    stdin_len: usize,
    allow_eof: i32,
    out_len: *mut usize,
) -> *mut u8 {
    if out_len.is_null() {
        return std::ptr::null_mut();
    }

    let code = String::from_utf8_lossy(read_bytes(code_ptr, code_len)).into_owned();
    let file_name = String::from_utf8_lossy(read_bytes(file_ptr, file_len)).into_owned();
    let file_name = if file_name.is_empty() {
        "playground.un".to_string()
    } else {
        file_name
    };
    let allowed = read_bytes(allowed_ptr, allowed_len);

    let allowed = if allowed.is_empty() {
        None
    } else {
        Some(
            String::from_utf8_lossy(allowed)
                .split('\n')
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty())
                .collect(),
        )
    };

    let stdin = String::from_utf8_lossy(read_bytes(stdin_ptr, stdin_len)).into_owned();
    let allow_eof = allow_eof != 0;

    let result = run(&code, &file_name, allowed, &stdin, allow_eof);
    let json = result_to_json(&result);
    let len = json.len();
    let buf = playground_alloc(len);
    if buf.is_null() {
        return std::ptr::null_mut();
    }

    std::ptr::copy_nonoverlapping(json.as_ptr(), buf, len);
    *out_len = len;
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_captures_stdout_and_exit_code() {
        let result = run("use io\nio.write(\"hello\")", "playground.un", None, "", false);
        assert_eq!(result.stdout, "hello\n");
        assert!(result.error.is_none());
        assert_eq!(result.exit_code, 0);
        assert!(!result.timed_out);
        assert!(!result.needs_input);

        let result = run("use io\nio.write(\"hello\", \"\\n\")", "playground.un", None, "", false);
        assert_eq!(result.stdout, "hello \n\n");
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn run_reports_script_errors() {
        let result = run(
            "use io\nio.write(undefined_variable, \"\\n\")",
            "playground.un",
            None,
            "",
            false,
        );
        assert!(result.error.is_some());
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn run_aborts_at_read_until_input_is_replayed() {
        let result = run(
            "use io\nname = io.read(\"Name: \")\nio.write(\"hi\", name)",
            "playground.un",
            None,
            "",
            false,
        );
        assert!(result.needs_input);
        assert_eq!(result.stdout, "Name: ");
        assert!(result.error.is_none());

        let result = run(
            "use io\nname = io.read(\"Name: \")\nio.write(\"hi\", name)",
            "playground.un",
            None,
            "kim\n",
            false,
        );
        assert!(!result.needs_input);
        assert_eq!(result.stdout, "Name: hi kim\n");
        assert_eq!(result.exit_code, 0);

        let result = run(
            "use io\nexample = io.read(\"first: \")\nother = io.read(\"second: \")\nio.write(example, other)",
            "playground.un",
            None,
            "kim\n",
            false,
        );
        assert!(result.needs_input);
        assert_eq!(result.stdout, "first: second: ");

        let result = run(
            "use io\nname = io.read(\"Name: \")\nio.write(\"hi\", name)",
            "playground.un",
            None,
            "",
            true,
        );
        assert!(!result.needs_input);
        assert_eq!(result.stdout, "Name: hi \n");
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn io_open_confines_to_sandbox_root() {
        let result = run(
            "use io\nio.open(\"C:/Windows/System32/drivers/etc/hosts\", \"w\")",
            "playground.un",
            None,
            "",
            false,
        );
        assert_eq!(result.exit_code, 1);
        assert!(
            result
                .error
                .as_ref()
                .is_some_and(|e| e.contains("escapes the sandbox")),
            "expected a sandbox rejection, got: {:?}",
            result.error
        );

        let result = run(
            "use io\nio.open(\"../../escape.txt\", \"w\")",
            "playground.un",
            None,
            "",
            false,
        );
        assert_eq!(result.exit_code, 1);
        assert!(
            result
                .error
                .as_ref()
                .is_some_and(|e| e.contains("escapes the sandbox")),
            "expected a sandbox rejection, got: {:?}",
            result.error
        );
    }

    fn utf16_slice(text: &str, start: usize, length: usize) -> String {
        let units: Vec<u16> = text.encode_utf16().skip(start).take(length).collect();
        String::from_utf16_lossy(&units)
    }

    #[test]
    fn check_infers_types_for_valid_source() {
        let result = check_source(
            "use io\nfn add(a: int, b: int) -> int\n    -> a + b\nio.write(add(1, 2))",
            "playground.un",
        );
        assert!(result.ok, "{:?}", result.diagnostics);
        assert!(result.diagnostics.is_empty());
        assert_eq!(result.vars, vec![("a".into(), "int".into()), ("b".into(), "int".into())]);
        assert_eq!(
            result.fn_params,
            vec![("add".into(), vec![("a".into(), "int".into()), ("b".into(), "int".into())])]
        );
        assert_eq!(result.fn_returns, vec![("add".into(), "int".into())]);
    }

    #[test]
    fn check_reports_type_error_with_source_span() {
        let code = "use io\nx: int = \"oops\"\nio.write(x)";
        let result = check_source(code, "playground.un");
        assert!(!result.ok);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = &result.diagnostics[0];
        assert!(diagnostic.message.contains("cannot assign"), "{}", diagnostic.message);
        assert_eq!(diagnostic.line, 1);
        assert_eq!(utf16_slice(code, diagnostic.start, diagnostic.length), "x");
    }

    #[test]
    fn check_utf16_offsets_survive_crlf_and_wide_characters() {
        let code = "use io\r\ns = \"😀\"\r\nx: int = \"wide\"";
        let result = check_source(code, "playground.un");
        assert!(!result.ok, "{:?}", result.diagnostics);
        let diagnostic = &result.diagnostics[0];
        let expected = code[..code.find("x: int").unwrap()].encode_utf16().count();
        assert_eq!(diagnostic.start, expected);
        assert_eq!(utf16_slice(code, diagnostic.start, diagnostic.length), "x");
    }

    #[test]
    fn json_quotes_correctly() {
        let result = PlaygroundResult {
            stdout: "a\"b\\c\nd".to_string(),
            error: Some("bad\nerror".to_string()),
            timed_out: false,
            exit_code: 1,
            needs_input: false,
        };
        assert_eq!(
            result_to_json(&result),
            "{\"stdout\": \"a\\\"b\\\\c\\nd\", \"error\": \"bad\\nerror\", \"timedOut\": false, \"exitCode\": 1, \"needsInput\": false}"
        );
    }
}
