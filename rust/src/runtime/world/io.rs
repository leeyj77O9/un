//! Sandboxed I/O surface: stdout, the captured log buffer, stdin, the
//! sandbox module allow-list, the `exit` gate, and EOF policy.

use super::WORLD;
use std::cell::RefCell;
use std::rc::Rc;

pub fn set_allowed(names: Vec<String>) {
    WORLD.with(|w| w.borrow_mut().allowed = Some(names.into_iter().collect()));
}
pub fn set_allow_exit(allowed: bool) {
    WORLD.with(|w| w.borrow_mut().allow_exit = allowed);
}
pub fn allow_exit() -> bool {
    WORLD.with(|w| w.borrow().allow_exit)
}
pub fn is_allowed(name: &str) -> bool {
    WORLD.with(|w| match &w.borrow().allowed {
        None => true,
        Some(set) => set.contains(name),
    })
}

pub fn stdout() -> Rc<RefCell<String>> {
    WORLD.with(|w| w.borrow().stdout.clone())
}
pub fn stdout_text() -> String {
    WORLD.with(|w| w.borrow().stdout.clone()).borrow().clone()
}
pub fn stdout_clear() {
    stdout().borrow_mut().clear();
}

pub fn flush_stdout() {
    use std::io::Write;
    let text = stdout_text();
    stdout_clear();
    if text.is_empty() {
        return;
    }
    WORLD.with(|w| w.borrow().log.borrow_mut().push_str(&text));
    let out = std::io::stdout();
    let mut out = out.lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}
pub fn log_text() -> String {
    WORLD.with(|w| w.borrow().log.clone()).borrow().clone()
}
pub fn log_clear() {
    WORLD.with(|w| w.borrow().log.borrow_mut().clear());
}

pub fn set_stdin(input: &str) {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        let text = if input.is_empty() || input.ends_with('\n') {
            input.to_owned()
        } else {
            format!("{input}\n")
        };
        w.stdin = Some(
            text.lines()
                .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
                .collect(),
        );
    });
}
pub fn set_allow_eof(allowed: bool) {
    WORLD.with(|w| w.borrow_mut().allow_eof = allowed);
}
pub fn allow_eof() -> bool {
    WORLD.with(|w| w.borrow().allow_eof)
}

pub enum StdinLine {
    Line(String),
    Eof,
    NotSet,
}
pub fn stdin_next_line() -> StdinLine {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        match &mut w.stdin {
            None => StdinLine::NotSet,
            Some(buffer) => buffer
                .pop_front()
                .map(StdinLine::Line)
                .unwrap_or(StdinLine::Eof),
        }
    })
}
