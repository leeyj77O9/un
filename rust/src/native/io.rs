//! `Un.Native.IO` — the native `io` module. phase 8 registers `write` and `read`; the
//! stream payload, `io.open`, `io.lines` and the file functions land with phase 14.
//!
//! stdout is a world-owned buffer flushed at the same points as the reference
//! (`cw.Write(...)` then `cw.Flush()`), so the real stdout output stays byte-identical.

use std::io;

use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use crate::runtime::world;

use super::{arg, native};

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "write".to_owned(),
            native(
                closure,
                "write",
                vec![
                    // `[ArgInfo(Positional = true)]` is the star collector: every positional
                    // argument is bound into `values` as a `Tup`
                    arg("values", false, false, true),
                    arg("sep", false, true, false),
                    arg("end", false, true, false),
                    arg("stream", false, true, false),
                ],
                write_impl,
            ),
        ),
        (
            "read".to_owned(),
            native(
                closure,
                "read",
                vec![
                    arg("prompt", false, true, false),
                    arg("input", false, true, false),
                    arg("output", false, true, false),
                ],
                read_impl,
            ),
        ),
        (
            "open".to_owned(),
            native(
                closure,
                "open",
                vec![arg("path", true, false, false), arg("mode", false, true, false)],
                open_impl,
            ),
        ),
    ]
}

/// `io.open(path, mode)` — the path is relative to `Global.PATH`, the mode is one of
/// `"r"`, `"w"`, `"a"` and `"rw"`.
fn open_impl(scope: &Scope) -> Obj {
    let path = scope::get(scope, "path");
    let mode = scope::get(scope, "mode");

    let path = match obj::payload(&obj::to_str(&path)) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("expected 'path' argument to be a string"),
    };

    let mode = if obj::is_none(&mode) {
        "r".to_owned()
    } else {
        match obj::payload(&obj::to_str(&mode)) {
            Payload::Str(s) => s.to_string(),
            _ => return obj::err("expected 'mode' argument to be a string"),
        }
    };

    if !matches!(mode.as_str(), "r" | "w" | "a" | "rw") {
        return obj::err("invalid 'mode' argument, expected one of 'r', 'w', 'a', or 'rw'");
    }

    // `Path.Combine(Global.PATH, path)` — an absolute path wins
    let full = {
        let base = std::path::Path::new(&world::path()).join(&path);
        base.display().to_string()
    };

    if mode == "r" && !std::path::Path::new(&full).is_file() {
        return obj::err("file not found");
    }

    let handle = match super::stream::open_file(&full, &mode) {
        Ok(handle) => handle,
        Err(e) => return e,
    };

    super::stream::new_stream(&full, &mode, handle)
}

fn write_impl(scope: &Scope) -> Obj {
    let values = scope::get(scope, "values");
    let sep = scope::get(scope, "sep");
    let end = scope::get(scope, "end");
    let stream = scope::get(scope, "stream");

    // `stream ??= _stdout`. the stream payload is phase 14, so any explicit stream argument
    // is rejected with the reference's type check message
    if !obj::is_none(&stream) {
        return obj::err("expected 'stream' argument to be of type 'stream'");
    }

    let sep = if obj::is_none(&sep) { obj::str(" ") } else { sep };
    let end = if obj::is_none(&end) { obj::str("\n") } else { end };

    let items = match obj::payload(&obj::to_tuple(&values)) {
        Payload::Tup(v, _) => v,
        _ => return obj::err("expected 'value' argument to be a tuple"),
    };

    let sep = match obj::payload(&obj::to_str(&sep)) {
        Payload::Str(s) => s,
        _ => return obj::err("expected 'sep' argument to be a string"),
    };

    let end = match obj::payload(&obj::to_str(&end)) {
        Payload::Str(s) => s,
        _ => return obj::err("expected 'end' argument to be a string"),
    };

    let count = items.len();

    for (i, item) in items.iter().enumerate() {
        if let Err(e) = write_text(item) {
            return e;
        }

        if i != count - 1 {
            world::stdout().borrow_mut().push_str(&sep);
        }
    }

    world::stdout().borrow_mut().push_str(&end);
    world::flush_stdout();

    obj::none()
}

fn read_impl(scope: &Scope) -> Obj {
    let prompt = scope::get(scope, "prompt");
    let input = scope::get(scope, "input");
    let output = scope::get(scope, "output");

    // `input ??= _stdin` / `output ??= _stdout`; stream payloads are phase 14
    if !obj::is_none(&input) {
        return obj::err("expected 'input' argument to be of type 'stream'");
    }
    if !obj::is_none(&output) {
        return obj::err("expected 'output' argument to be of type 'stream'");
    }

    let prompt = if obj::is_none(&prompt) { obj::str("") } else { prompt };

    match write_text(&prompt) {
        Ok(()) => {}
        Err(e) => return e,
    }

    world::flush_stdout();

    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return obj::none();
    }

    // `Stream.Reader.ReadLine()` drops `\n` and `\r\n`
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }

    obj::str(&line)
}

/// `ToStr()` first, then `Repr()`, the reference's write fallback
fn write_text(item: &Obj) -> Result<(), Obj> {
    let text = match obj::payload(&obj::to_str(item)) {
        Payload::Str(s) => s,
        _ => match obj::payload(&obj::repr(item)) {
            Payload::Str(s) => s,
            _ => {
                return Err(obj::err(
                    "expected all values to be strings or representable as strings",
                ))
            }
        },
    };
    world::stdout().borrow_mut().push_str(&text);
    Ok(())
}
