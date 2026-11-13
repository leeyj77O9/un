use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::rc::Rc;
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use super::{arg, native};
pub enum StreamHandle {

    Read(BufReader<File>),

    Write(File),

    ReadWrite(File),
}
pub struct StreamState {
    pub path: String,
    pub mode: String,
    pub handle: Option<StreamHandle>,

    pub pending: Option<String>,
    pub closed: bool,
}
pub fn new_stream(path: &str, mode: &str, handle: StreamHandle) -> Obj {
    obj::obj_new(
        "stream",
        Payload::Stream(Rc::new(RefCell::new(StreamState {
            path: path.to_owned(),
            mode: mode.to_owned(),
            handle: Some(handle),
            pending: None,
            closed: false,
        }))),
    )
}
fn stream_self(scope: &Scope) -> Option<Rc<RefCell<StreamState>>> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::Stream(state) => Some(state.clone()),
        _ => None,
    }
}
pub fn can_read(mode: &str) -> bool {
    mode == "r" || mode == "rw"
}
pub fn can_write(mode: &str) -> bool {
    mode == "w" || mode == "a" || mode == "rw"
}
pub fn can_seek(mode: &str) -> bool {
    mode == "r" || mode == "rw"
}
fn value_text(value: &Obj) -> String {
    let converted = obj::to_str(value);
    match obj::payload(&converted) {
        Payload::Str(s) => s.to_string(),
        _ => obj::display(value),
    }
}
fn trim_line_end(line: &str) -> String {
    let mut text = line.to_string();
    if text.ends_with('\n') {
        text.pop();
    }
    if text.ends_with('\r') {
        text.pop();
    }
    text
}
pub fn open_file(path: &str, mode: &str) -> Result<StreamHandle, Obj> {
    let result = match mode {
        "r" => OpenOptions::new()
            .read(true)
            .open(path)
            .map(|file| StreamHandle::Read(BufReader::new(file))),
        "w" => OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map(StreamHandle::Write),
        "a" => OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .map(StreamHandle::Write),
        "rw" => OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
            .map(StreamHandle::ReadWrite),
        _ => {
            return Err(obj::err(
                "invalid 'mode' argument, expected one of 'r', 'w', 'a', or 'rw'",
            ))
        }
    };
    result.map_err(|e| obj::err(&e.to_string()))
}
pub fn read_all(state: &Rc<RefCell<StreamState>>) -> Result<String, Obj> {
    let mut state = state.borrow_mut();
    if state.closed {
        return Err(obj::err("stream is closed"));
    }
    let mut out = String::new();
    match state.handle.as_mut() {
        Some(StreamHandle::Read(reader)) => {
            let mut text = String::new();
            reader
                .read_to_string(&mut text)
                .map_err(|e| obj::err(&e.to_string()))?;
            out.push_str(&text);
        }
        Some(StreamHandle::ReadWrite(file)) => {
            let mut text = String::new();
            file.read_to_string(&mut text)
                .map_err(|e| obj::err(&e.to_string()))?;
            out.push_str(&text);
        }
        Some(StreamHandle::Write(_)) => return Err(obj::err("stream is not readable")),
        None => return Err(obj::err("stream is closed")),
    }
    Ok(out)
}
pub fn read_line(state: &Rc<RefCell<StreamState>>) -> Result<Option<String>, Obj> {
    let mut state = state.borrow_mut();
    if state.closed {
        return Err(obj::err("stream is closed"));
    }

    if let Some(pending) = state.pending.take() {
        return Ok(Some(pending));
    }
    match state.handle.as_mut() {
        Some(StreamHandle::Read(reader)) => {
            let mut line = String::new();
            let read = reader
                .read_line(&mut line)
                .map_err(|e| obj::err(&e.to_string()))?;
            Ok(if read == 0 { None } else { Some(trim_line_end(&line)) })
        }
        Some(StreamHandle::ReadWrite(file)) => {
            let mut line = String::new();
            let mut byte = [0u8; 1];
            loop {
                match file.read(&mut byte) {
                    Ok(0) => break,
                    Ok(_) => {
                        if byte[0] == b'\n' {
                            break;
                        }
                        line.push(byte[0] as char);
                    }
                    Err(e) => return Err(obj::err(&e.to_string())),
                }
            }
            if line.is_empty() {
                Ok(None)
            } else {
                Ok(Some(line))
            }
        }
        Some(StreamHandle::Write(_)) => Err(obj::err("stream is not readable")),
        None => Err(obj::err("stream is closed")),
    }
}
pub fn write_text(state: &Rc<RefCell<StreamState>>, text: &str) -> Result<(), Obj> {
    let mut state = state.borrow_mut();
    if state.closed {
        return Err(obj::err("stream is closed"));
    }
    match state.handle.as_mut() {
        Some(StreamHandle::Write(file)) => file
            .write_all(text.as_bytes())
            .map_err(|e| obj::err(&e.to_string())),
        Some(StreamHandle::ReadWrite(file)) => file
            .write_all(text.as_bytes())
            .map_err(|e| obj::err(&e.to_string())),
        Some(StreamHandle::Read(_)) => Err(obj::err("stream is not writable")),
        None => Err(obj::err("stream is closed")),
    }
}
pub fn flush(state: &Rc<RefCell<StreamState>>) -> Result<(), Obj> {
    let mut state = state.borrow_mut();
    if state.closed {
        return Err(obj::err("stream is closed"));
    }
    match state.handle.as_mut() {
        Some(StreamHandle::Write(file)) => file.flush().map_err(|e| obj::err(&e.to_string())),
        Some(StreamHandle::ReadWrite(file)) => file.flush().map_err(|e| obj::err(&e.to_string())),
        Some(StreamHandle::Read(_)) => Err(obj::err("stream is not writable")),
        None => Err(obj::err("stream is closed")),
    }
}
pub fn close(state: &Rc<RefCell<StreamState>>) {
    let mut state = state.borrow_mut();
    state.closed = true;
    state.handle = None;
}
pub fn seek(state: &Rc<RefCell<StreamState>>, position: i64) -> Result<(), Obj> {
    let mut state = state.borrow_mut();
    if state.closed {
        return Err(obj::err("stream is closed"));
    }
    match state.handle.as_mut() {
        Some(StreamHandle::Read(reader)) => reader
            .seek(SeekFrom::Start(position.max(0) as u64))
            .map(|_| ())
            .map_err(|e| obj::err(&e.to_string())),
        Some(StreamHandle::ReadWrite(file)) => file
            .seek(SeekFrom::Start(position.max(0) as u64))
            .map(|_| ())
            .map_err(|e| obj::err(&e.to_string())),
        _ => Err(obj::err("stream is not seekable")),
    }
}
pub fn position(state: &Rc<RefCell<StreamState>>) -> Result<i64, Obj> {
    let mut state = state.borrow_mut();
    if state.closed {
        return Err(obj::err("stream is closed"));
    }
    match state.handle.as_mut() {
        Some(StreamHandle::Read(reader)) => reader
            .stream_position()
            .map(|p| p as i64)
            .map_err(|e| obj::err(&e.to_string())),
        Some(StreamHandle::ReadWrite(file)) => file
            .stream_position()
            .map(|p| p as i64)
            .map_err(|e| obj::err(&e.to_string())),
        _ => Err(obj::err("stream is not seekable")),
    }
}
pub fn length(state: &Rc<RefCell<StreamState>>) -> Result<i64, Obj> {
    let state = state.borrow();
    if state.closed {
        return Err(obj::err("stream is closed"));
    }
    match state.handle.as_ref() {
        Some(StreamHandle::Read(reader)) => reader
            .get_ref()
            .metadata()
            .map(|m| m.len() as i64)
            .map_err(|e| obj::err(&e.to_string())),
        Some(StreamHandle::ReadWrite(file)) => file
            .metadata()
            .map(|m| m.len() as i64)
            .map_err(|e| obj::err(&e.to_string())),
        _ => Err(obj::err("stream is not seekable")),
    }
}
pub fn text_of(value: &Obj) -> String {
    value_text(value)
}
pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    let one = |name: &str| vec![arg(name, true, false, false)];
    vec![
        (
            "read".to_owned(),
            native(closure, "read", Vec::new(), read_impl),
        ),
        (
            "read_line".to_owned(),
            native(closure, "read_line", Vec::new(), read_line_impl),
        ),
        (
            "write".to_owned(),
            native(closure, "write", one("value"), write_impl),
        ),
        (
            "write_line".to_owned(),
            native(closure, "write_line", one("value"), write_line_impl),
        ),
        (
            "flush".to_owned(),
            native(closure, "flush", Vec::new(), flush_impl),
        ),
        (
            "close".to_owned(),
            native(closure, "close", Vec::new(), close_impl),
        ),
        (
            "seek".to_owned(),
            native(closure, "seek", one("position"), seek_impl),
        ),
        (
            "position".to_owned(),
            native(closure, "position", Vec::new(), position_impl),
        ),
        (
            "set_position".to_owned(),
            native(closure, "set_position", one("position"), set_position_impl),
        ),
        (
            "length".to_owned(),
            native(closure, "length", Vec::new(), length_impl),
        ),
        (
            "eof".to_owned(),
            native(closure, "eof", Vec::new(), eof_impl),
        ),
        (
            "can_read".to_owned(),
            native(closure, "can_read", Vec::new(), can_read_impl),
        ),
        (
            "can_write".to_owned(),
            native(closure, "can_write", Vec::new(), can_write_impl),
        ),
        (
            "can_seek".to_owned(),
            native(closure, "can_seek", Vec::new(), can_seek_impl),
        ),
    ]
}
fn state_of(scope: &Scope) -> Option<Rc<RefCell<StreamState>>> {
    stream_self(scope)
}
fn mode_of(scope: &Scope) -> String {
    match state_of(scope) {
        Some(state) => state.borrow().mode.clone(),
        None => String::new(),
    }
}
fn read_impl(scope: &Scope) -> Obj {
    let Some(state) = state_of(scope) else {
        return obj::err("expected 'stream' object");
    };
    if !can_read(&mode_of(scope)) {
        return obj::err("stream is not readable");
    }
    match read_all(&state) {
        Ok(text) => obj::str(&text),
        Err(e) => e,
    }
}
fn read_line_impl(scope: &Scope) -> Obj {
    let Some(state) = state_of(scope) else {
        return obj::err("expected 'stream' object");
    };
    if !can_read(&mode_of(scope)) {
        return obj::err("stream is not readable");
    }
    match read_line(&state) {
        Ok(Some(line)) => obj::str(&line),
        Ok(None) => obj::none(),
        Err(e) => e,
    }
}
fn write_impl(scope: &Scope) -> Obj {
    let Some(state) = state_of(scope) else {
        return obj::err("expected 'stream' object");
    };
    if !can_write(&mode_of(scope)) {
        return obj::err("stream is not writable");
    }
    let value = scope::get(scope, "value");
    match write_text(&state, &text_of(&value)) {
        Ok(()) => obj::none(),
        Err(e) => e,
    }
}
fn write_line_impl(scope: &Scope) -> Obj {
    let Some(state) = state_of(scope) else {
        return obj::err("expected 'stream' object");
    };
    if !can_write(&mode_of(scope)) {
        return obj::err("stream is not writable");
    }
    let value = scope::get(scope, "value");
    match write_text(&state, &format!("{}\n", text_of(&value))) {
        Ok(()) => obj::none(),
        Err(e) => e,
    }
}
fn flush_impl(scope: &Scope) -> Obj {
    let Some(state) = state_of(scope) else {
        return obj::err("expected 'stream' object");
    };
    if !can_write(&mode_of(scope)) {
        return obj::err("stream is not writable");
    }
    match flush(&state) {
        Ok(()) => obj::none(),
        Err(e) => e,
    }
}
fn close_impl(scope: &Scope) -> Obj {
    let Some(state) = state_of(scope) else {
        return obj::err("expected 'stream' object");
    };
    close(&state);
    obj::none()
}
fn seek_impl(scope: &Scope) -> Obj {
    let Some(state) = state_of(scope) else {
        return obj::err("expected 'stream' object");
    };
    if !can_seek(&mode_of(scope)) {
        return obj::err("stream is not seekable");
    }
    let position = scope::get(scope, "position");
    let position = match obj::payload(&position) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'position' argument to be of type 'int'"),
    };
    match seek(&state, position) {
        Ok(()) => obj::none(),
        Err(e) => e,
    }
}
fn position_impl(scope: &Scope) -> Obj {
    let Some(state) = state_of(scope) else {
        return obj::err("expected 'stream' object");
    };
    if !can_seek(&mode_of(scope)) {
        return obj::err("stream is not seekable");
    }
    match position(&state) {
        Ok(position) => obj::int(position),
        Err(e) => e,
    }
}
fn set_position_impl(scope: &Scope) -> Obj {
    let Some(state) = state_of(scope) else {
        return obj::err("expected 'stream' object");
    };
    if !can_seek(&mode_of(scope)) {
        return obj::err("stream is not seekable");
    }
    let position = scope::get(scope, "position");
    let position = match obj::payload(&position) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'position' argument to be of type 'int'"),
    };
    match seek(&state, position) {
        Ok(()) => obj::none(),
        Err(e) => e,
    }
}
fn length_impl(scope: &Scope) -> Obj {
    let Some(state) = state_of(scope) else {
        return obj::err("expected 'stream' object");
    };
    if !can_seek(&mode_of(scope)) {
        return obj::err("stream is not seekable");
    }
    match length(&state) {
        Ok(length) => obj::int(length),
        Err(e) => e,
    }
}
fn eof_impl(scope: &Scope) -> Obj {
    let Some(state) = state_of(scope) else {
        return obj::err("expected 'stream' object");
    };
    if !can_read(&mode_of(scope)) {
        return obj::err("stream is not readable");
    }

    match read_line(&state) {
        Ok(Some(line)) => {

            let mut pending = state.borrow_mut();
            pending.pending = Some(line);
            obj::boolean(false)
        }
        Ok(None) => obj::boolean(true),
        Err(e) => e,
    }
}
fn can_read_impl(scope: &Scope) -> Obj {
    obj::boolean(can_read(&mode_of(scope)))
}
fn can_write_impl(scope: &Scope) -> Obj {
    obj::boolean(can_write(&mode_of(scope)))
}
fn can_seek_impl(scope: &Scope) -> Obj {
    obj::boolean(can_seek(&mode_of(scope)))
}
