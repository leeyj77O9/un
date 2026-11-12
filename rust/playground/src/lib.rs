//! `unvm-playground` ??the WASM entry point of the Un sandbox.
//!
//! `PlaygroundRunner.RunCode` of the reference C# implementation, ported to Rust:
//! it runs a script against an in-memory file system with a fixed module allowlist,
//! captures the sandbox stdout, and reports the outcome as JSON.
//!
//! The crate produces a `cdylib` (`playground.wasm`) exporting the `playground_*`
//! ABI that the web playground loader uses. on non-wasm targets it builds as a
//! native library so the ABI can be tested directly.
//!
//! ABI (documented, stable):
//!
//! ```text
//! playground_alloc(len: usize) -> *mut u8
//! playground_free(ptr: *mut u8, len: usize)
//! playground_run(code_ptr: *const u8, code_len: usize,
//!                file_ptr: *const u8, file_len: usize,
//!                allowed_ptr: *const u8, allowed_len: usize,   // newline-separated names; empty = default
//!                stdin_ptr: *const u8, stdin_len: usize,       // replayed input, '\n'-joined; may be empty
//!                allow_eof: i32,                               // nonzero: at stdin EOF `read()` returns ""
//!                out_len: *mut usize) -> *mut u8               // NUL-free; length in out_len
//! ```
//!
//! The caller writes the input buffers with `playground_alloc`, reads the result
//! into a UTF-8 string of `out_len` bytes, then frees both buffers and the result.
//!
//! Interactive `read()`: the web playground replays the whole script with the
//! input lines gathered so far as `stdin`. when the script calls `read()` and no
//! replayed line remains, the run aborts with a `need_input` panic and the JSON
//! result reports `"needsInput": true` (with whatever stdout was produced so far).
//! `allow_eof` nonzero turns the abort into an empty-string line instead.
//!
//! The timeout parameter is accepted for ABI compatibility with the reference and is
//! ignored: like the reference's browser path, a preemptive timeout cannot exist on
//! WASM (the host enforces it), and a native build runs the same in-browser path.

use std::rc::Rc;

use unvm::native;
use unvm::runner;
use unvm::runtime::obj::{self, Payload};
use unvm::runtime::world;
use unvm::source::Source;
use unvm::vm::{err_header, err_message, panic_display, Outcome, VM};
use unvm::error::LangError;

/// the reference's `DefaultAllowed`
const DEFAULT_ALLOWED: &[&str] = &[
    "io", "math", "iter", "re", "random", "time", "builtin", "atomic",
];

pub struct PlaygroundResult {
    pub stdout: String,
    pub error: Option<String>,
    pub timed_out: bool,
    pub exit_code: i32,
    /// the run aborted at `read()`: the replayed stdin ran out and `allow_eof` was
    /// not set. the browser should ask the user for the next line and replay.
    pub needs_input: bool,
}

fn is_need_input(o: &obj::Obj) -> bool {
    matches!(obj::payload(o), Payload::Panic { name, .. } if name == "need_input")
}

/// Runs `code` against the sandbox, mirrors `PlaygroundRunner.RunCode`.
/// `stdin` is the replayed input the browser collected by asking the user line by
/// line; `allow_eof` mirrors `UnInterop.SetAllowEof`.
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
    // `Runner.RunCore` ??parse, desugar, optimize, compile
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
                    error = Some(e.display(&source));
                    exit_code = 1;
                }
                Outcome::Propagate(err, start, length) => {
                    // the header is preserved, as in `catch (PropagateFlow pf)`
                    let e = LangError::new(start, length, err_message(&err)).with_header(err_header(&err));
                    error = Some(e.display(&source));
                    exit_code = 1;
                }
                Outcome::Return(_, start, length) => {
                    let e = LangError::new(start, length, "'->' outside function");
                    error = Some(e.display(&source));
                    exit_code = 1;
                }
                Outcome::Break(start, length) => {
                    let e = LangError::new(start, length, "'break' outside loop");
                    error = Some(e.display(&source));
                    exit_code = 1;
                }
                Outcome::Skip(start, length) => {
                    let e = LangError::new(start, length, "'skip' outside loop");
                    error = Some(e.display(&source));
                    exit_code = 1;
                }
                Outcome::Panic(o) => {
                    if is_need_input(&o) {
                        // the browser asks for the next line and replays with more
                        // stdin; the error stays hidden, as `NeedInputException` was
                        needs_input = true;
                    } else {
                        error = Some(panic_display(&o));
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

/// `{"stdout": "...", "error": null|"...", "timedOut": false, "exitCode": 0, "needsInput": false}`
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

        // io.write(a, b) prints a + " " + b + "\n", matching the reference exactly
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
        // no stdin: the run aborts at `read()` and reports needs_input, keeping the
        // stdout (the prompt) it produced so far
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

        // replay with the collected line: the script completes without asking again
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

        // the replayed buffer is consumed in order, then asks again
        let result = run(
            "use io\nexample = io.read(\"first: \")\nother = io.read(\"second: \")\nio.write(example, other)",
            "playground.un",
            None,
            "kim\n",
            false,
        );
        assert!(result.needs_input);
        assert_eq!(result.stdout, "first: second: ");

        // allow_eof turns the abort into an empty line
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
