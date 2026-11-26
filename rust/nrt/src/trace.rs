//! Source locations, call frames and traceback formatting for no-VM executables.
//! Mirrors `runtime/context.rs::trace_string` and `error.rs::LangError::display`.
//!
//! Thread-safety (W-2026-11-26-P04, A-13..A-16): all mutable trace/error state is
//! task-local (`thread_local!`). A native task runs from entry to result on one
//! worker thread, so a worker's frames, error site and pending error cannot be
//! observed by another worker. [`snapshot`] copies that state into an owned,
//! `Send` [`TraceSnapshot`] which P06 attaches to the task result; the wait
//! thread then serializes it with [`write_traceback`]. The default (single
//! threaded) path is unchanged: [`dump_uncaught_from`] snapshots its own
//! thread and prints exactly the same bytes as before.

use std::cell::Cell;
use std::io::Write;
use std::sync::Mutex;

use crate::obj::display;

struct SourceInfo {
    stem: String,
    chars: Vec<char>,
    line_starts: Vec<usize>,
}

// A-17 (won't-fix): registered once before any task runs, read-only afterwards.
static SOURCE: Mutex<Option<SourceInfo>> = Mutex::new(None);

const MAX_FRAMES: usize = 512;

thread_local! {
    // A-14: replaces the former raw-pointer read-modify-write on a process-global
    // depth cell. `Cell` is a plain load/store — the recursion benchmark keeps
    // the no-atomic-RMW property, and there is no shared alias to race on.
    static FRAME_DEPTH: Cell<usize> = const { Cell::new(0) };

    // A-15: replaces the process-global 512-slot atomic table. Plain `Cell<u64>`
    // slots are sufficient because only the owning thread ever reads/writes
    // them; the relaxed atomics existed solely to make the global table memory
    // safe and are no longer needed.
    static FRAME_SLOTS: Vec<[Cell<u64>; 4]> = {
        (0..MAX_FRAMES)
            .map(|_| std::array::from_fn(|_| Cell::new(0)))
            .collect()
    };

    // A-16: replaces the single global error-site mutex.
    static ERROR_SITE: Cell<Option<(usize, usize)>> = const { Cell::new(None) };

    // A-13: replaces obj.rs's process-global `PENDING` atomic. The C ABI
    // (`unrt_write_error` / `unrt_take_error` / `unrt_dump_uncaught`) is
    // unchanged; only its backing cell moves here so each task owns its error.
    static PENDING_ERROR: Cell<u64> = const { Cell::new(0) };
}

/// Owned copy of one call frame, safe to move to another thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameSnap {
    pub(crate) name: String,
    pub(crate) start: usize,
}

/// Task-local trace + error state captured at a task boundary.
///
/// All fields are owned (no raw pointers, no `Rc`), so a snapshot is `Send` and
/// can be placed in a scheduler result slot for the wait thread to serialize.
/// `Eq` is required because `scheduler::TaskSlot` derives `Eq` and now carries
/// a snapshot in both its `Ready` and `Panicked` variants.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TraceSnapshot {
    pub(crate) frames: Vec<FrameSnap>,
    pub(crate) error_site: Option<(usize, usize)>,
    pub(crate) pending_error: u64,
}

// Compile-time proof that a snapshot may cross a thread boundary (P06 needs
// this when attaching it to `scheduler::TaskResult`).
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<TraceSnapshot>();
};

fn str_from(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf8_lossy(bytes).into_owned()
}

fn line_of(src: &SourceInfo, start: usize) -> usize {
    let start = start.min(src.chars.len());
    match src.line_starts.binary_search(&start) {
        Ok(i) => i + 1,
        Err(i) => i,
    }
}

fn line_bounds(src: &SourceInfo, start: usize) -> (usize, usize) {
    let i = line_of(src, start).saturating_sub(1);
    let from = src.line_starts[i];
    let to = src.line_starts.get(i + 1).copied().unwrap_or(src.chars.len());
    (from, to)
}

fn line_text(src: &SourceInfo, start: usize) -> String {
    let (from, to) = line_bounds(src, start);
    let mut text: String = src.chars[from..to].iter().collect();
    while text.ends_with('\n') || text.ends_with('\r') {
        text.pop();
    }
    text.trim().to_string()
}

fn column_of(src: &SourceInfo, start: usize) -> usize {
    let (from, _) = line_bounds(src, start);
    start.saturating_sub(from) + 1
}

fn frame_name(name: &str) -> String {
    if name.is_empty() {
        "<module>".to_string()
    } else if name == "lambda" {
        "<lambda>".to_string()
    } else {
        name.to_string()
    }
}

#[no_mangle]
pub extern "C" fn nrt_register_source(
    path_ptr: *const u8,
    path_len: usize,
    text_ptr: *const u8,
    text_len: usize,
) {
    let path = str_from(path_ptr, path_len);
    let stem = std::path::Path::new(&path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());
    let text = str_from(text_ptr, text_len);
    let chars: Vec<char> = text.chars().collect();
    let mut line_starts = vec![0usize];
    for (i, c) in chars.iter().enumerate() {
        if *c == '\n' {
            line_starts.push(i + 1);
        }
    }
    *SOURCE.lock().unwrap() = Some(SourceInfo { stem, chars, line_starts });
}

#[no_mangle]
pub extern "C" fn nrt_push_frame(name_ptr: *const u8, name_len: usize, start: usize, len: usize) {
    let d = FRAME_DEPTH.with(|c| {
        let d = c.get();
        c.set(d + 1);
        d
    });
    if d < MAX_FRAMES {
        FRAME_SLOTS.with(|slots| {
            let slot = &slots[d];
            slot[0].set(name_ptr as u64);
            slot[1].set(name_len as u64);
            slot[2].set(start as u64);
            slot[3].set(len as u64);
        });
    }
}

#[no_mangle]
pub extern "C" fn nrt_pop_frame() {
    FRAME_DEPTH.with(|c| c.set(c.get().saturating_sub(1)));
}

/// Clear the current thread's frame depth (kept as an exported symbol for ABI
/// compatibility; `take_error` is the internal all-in-one reset).
#[no_mangle]
pub extern "C" fn nrt_clear_trace() {
    FRAME_DEPTH.with(|c| c.set(0));
}

#[no_mangle]
pub extern "C" fn nrt_set_error_site(start: usize, len: usize) {
    ERROR_SITE.with(|c| c.set(Some((start, len))));
}

#[no_mangle]
pub extern "C" fn nrt_take_error_site() -> u64 {
    match ERROR_SITE.with(|c| c.take()) {
        Some((s, l)) => ((s as u64) << 32) | (l as u64),
        None => 0,
    }
}

/// Set the current task's pending error handle (replaces `PENDING.store`).
pub(crate) fn pending_error_set(e: u64) {
    PENDING_ERROR.with(|c| c.set(e));
}

/// Take and reset the current task's pending error handle (replaces
/// `PENDING.swap(0, ..)`).
pub(crate) fn pending_error_take() -> u64 {
    PENDING_ERROR.with(|c| c.replace(0))
}

/// Reset all task-local trace/error state and return the previous pending
/// error. This is the exact sequence the old `unrt_take_error` performed
/// (`nrt_clear_trace` + `clear_pending_site` + `PENDING.swap(0)`), and is also
/// the hook P06 calls when a worker starts a new task.
pub(crate) fn take_error() -> u64 {
    FRAME_DEPTH.with(|c| c.set(0));
    clear_pending_site();
    pending_error_take()
}

/// Snapshot the current thread's trace/error state into an owned, `Send` value.
/// Called by the default traceback path and by P06 when a task finishes.
pub(crate) fn snapshot() -> TraceSnapshot {
    let depth = FRAME_DEPTH.with(|c| c.get().min(MAX_FRAMES));
    let frames = FRAME_SLOTS.with(|slots| {
        (0..depth)
            .map(|i| {
                let s = &slots[i];
                FrameSnap {
                    name: str_from(s[0].get() as *const u8, s[1].get() as usize),
                    start: s[2].get() as usize,
                }
            })
            .collect()
    });
    TraceSnapshot {
        frames,
        error_site: ERROR_SITE.with(|c| c.get()),
        pending_error: PENDING_ERROR.with(|c| c.get()),
    }
}

/// Reinstall a worker snapshot into the *calling* thread's task-local state.
///
/// `unrt_wait` calls this so the error raised inside a task is rethrown at the
/// wait point with the originating task's frames, error site and pending error
/// handle. The frame name bytes are leaked for the process lifetime (the same
/// policy as every other NRT allocation) because `FRAME_SLOTS` stores raw
/// pointers, and the original code pointer may belong to a worker that has
/// already moved on to the next task.
pub(crate) fn install(snap: &TraceSnapshot) {
    let depth = snap.frames.len().min(MAX_FRAMES);
    FRAME_SLOTS.with(|slots| {
        for (i, frame) in snap.frames.iter().take(MAX_FRAMES).enumerate() {
            let leaked: &'static str = Box::leak(frame.name.clone().into_boxed_str());
            let slot = &slots[i];
            slot[0].set(leaked.as_ptr() as u64);
            slot[1].set(leaked.len() as u64);
            slot[2].set(frame.start as u64);
            // `len` is recorded by `nrt_push_frame` but never read by the
            // formatter; zero keeps the reinstalled slot well-defined.
            slot[3].set(0);
        }
    });
    FRAME_DEPTH.with(|cell| cell.set(depth));
    error_site_set(snap.error_site);
    pending_error_set(snap.pending_error);
}

/// Set the calling thread's error site from a snapshot or an explicit value.
pub(crate) fn error_site_set(site: Option<(usize, usize)>) {
    ERROR_SITE.with(|cell| cell.set(site));
}

/// Serialize a snapshot exactly as `dump_uncaught_from` would, including the
/// panic (tag 10) error-site suppression. `unrt_trace` uses this so the bytes
/// returned to the caller are identical to the default-path traceback.
pub(crate) fn traceback_text(snap: &TraceSnapshot) -> String {
    let e = snap.pending_error;
    let message = if e == 0 { "unknown error".to_string() } else { error_message(e) };
    let effective = if e != 0 && unsafe { crate::obj::obj(e).tag == 10 } {
        let mut copy = snap.clone();
        copy.error_site = None;
        copy
    } else {
        snap.clone()
    };
    format_traceback(&message, &effective)
}

/// Serialize a traceback from a captured snapshot. This is the wait-thread
/// entry point: the bytes are identical to formatting the originating task's
/// own thread-local state.
pub(crate) fn format_traceback(message: &str, snap: &TraceSnapshot) -> String {
    let src = SOURCE.lock().unwrap();
    let frames = &snap.frames;
    let site = snap.error_site;
    let nl = if cfg!(windows) { "\r\n" } else { "\n" };
    let mut out = String::new();

    if frames.len() > 1 {
        let mut rows: Vec<(String, usize, String)> = Vec::new();
        for frame in frames.iter() {
            let (line, text) = match src.as_ref() {
                Some(s) => (line_of(s, frame.start), line_text(s, frame.start)),
                None => (0, String::new()),
            };
            rows.push((frame_name(&frame.name), line, text));
        }
        // Collapse consecutive identical rows.
        let mut collapsed: Vec<(String, usize, String, usize)> = Vec::new();
        for (name, line, text) in rows {
            if let Some(last) = collapsed.last_mut() {
                if last.0 == name && last.1 == line && last.2 == text {
                    last.3 += 1;
                    continue;
                }
            }
            collapsed.push((name, line, text, 0));
        }
        let width = collapsed.iter().map(|r| r.0.chars().count()).max().unwrap_or(0);
        let frame_rows = collapsed.iter().filter(|r| r.3 == 0).count();
        let compact = frame_rows > 10;
        out.push_str(&format!("un: error trace{nl}"));
        for (name, line, text, repeated) in &collapsed {
            let file = src.as_ref().map(|s| format!("<{}>", s.stem)).unwrap_or_default();
            out.push_str(&format!("    -> {name:<width$}  {file}, line [{line}]{nl}"));
            if *repeated > 0 {
                out.push_str(&format!("    ... (repeated {repeated} more times){nl}"));
            } else if !compact && !text.is_empty() {
                out.push_str(&format!("         {text}{nl}"));
            }
        }
    }

    if site.is_some() || frames.len() <= 1 { out.push_str(nl); }
    let (file, line, col, text, caret) = match (src.as_ref(), site) {
        (Some(s), Some((start, len))) => {
            let line = line_of(s, start);
            let col = column_of(s, start);
            let (from, _) = line_bounds(s, start);
            let raw: String = s.chars[from..].iter().take_while(|c| **c != '\n' && **c != '\r').collect();
            let indent = raw.chars().take_while(|c| c.is_whitespace()).count();
            let text = raw.chars().skip(indent).collect();
            let column_in_line = start.saturating_sub(from);
            let caret_pad = column_in_line.saturating_sub(indent);
            let caret_len = len.max(1);
            let caret = format!("{}{}", " ".repeat(caret_pad), "^".repeat(caret_len));
            (format!("<{}>", s.stem), line, col, text, caret)
        }
        (Some(s), None) => (format!("<{}>", s.stem), 0, 0, String::new(), String::new()),
        (None, _) => (String::new(), 0, 0, String::new(), String::new()),
    };
    if !text.is_empty() {
        out.push_str(&format!("{file}, line [{line}], column [{col}]{nl}"));
        out.push_str(&format!("    {text}{nl}"));
        out.push_str(&format!("    {caret}{nl}"));
    }
    out.push_str(&format!("error: {message}{nl}"));
    out
}

/// Print a serialized traceback. The default path goes through
/// [`dump_uncaught_from`]; P06 calls this with a result snapshot on the wait
/// thread.
pub(crate) fn write_traceback(message: &str, snap: &TraceSnapshot) {
    let out = format_traceback(message, snap);
    print!("{out}");
    let _ = std::io::stdout().flush();
}

/// Print an uncaught error from a captured snapshot, then exit(1).
///
/// Mirrors `unrt_dump_uncaught` but sources the message handle and trace from a
/// snapshot, so P06 can rethrow a failed task's error on the wait thread while
/// keeping the I5 error text and frame bytes unchanged. A panic (tag 10)
/// suppresses the source caret exactly as in the original path.
pub(crate) fn dump_uncaught_from(snap: &TraceSnapshot) -> ! {
    let e = snap.pending_error;
    let msg = if e == 0 { "unknown error".to_string() } else { error_message(e) };
    let effective = if e != 0 && unsafe { crate::obj::obj(e).tag == 10 } {
        let mut s = snap.clone();
        s.error_site = None;
        s
    } else {
        snap.clone()
    };
    write_traceback(&msg, &effective);
    std::process::exit(1)
}

pub(crate) fn clear_pending_site() {
    ERROR_SITE.with(|c| c.set(None));
}

pub(crate) fn error_message(p: u64) -> String {
    display(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn name(s: &str) -> (*const u8, usize) {
        (s.as_bytes().as_ptr(), s.len())
    }

    #[test]
    fn a14_frame_depth_is_task_local() {
        take_error();
        let (p, n) = name("main");
        nrt_push_frame(p, n, 0, 1);
        nrt_push_frame(p, n, 10, 1);
        assert_eq!(snapshot().frames.len(), 2);
        let worker = thread::spawn(|| {
            take_error();
            assert_eq!(snapshot().frames.len(), 0, "new thread starts at depth 0");
            let (p, n) = name("worker");
            nrt_push_frame(p, n, 20, 1);
            assert_eq!(snapshot().frames.len(), 1);
            take_error();
        });
        worker.join().unwrap();
        assert_eq!(snapshot().frames.len(), 2, "parent depth is unaffected");
        take_error();
    }

    #[test]
    fn a15_frame_slots_are_task_local() {
        take_error();
        let (p, n) = name("parent");
        nrt_push_frame(p, n, 1, 1);
        let worker = thread::spawn(|| {
            take_error();
            let (p, n) = name("worker");
            nrt_push_frame(p, n, 2, 1);
            let snap = snapshot();
            assert_eq!(snap.frames.len(), 1);
            assert_eq!(snap.frames[0].name, "worker");
            assert_eq!(snap.frames[0].start, 2);
            take_error();
        });
        worker.join().unwrap();
        let snap = snapshot();
        assert_eq!(snap.frames.len(), 1);
        assert_eq!(snap.frames[0].name, "parent");
        assert_eq!(snap.frames[0].start, 1);
        take_error();
    }

    #[test]
    fn a16_error_site_is_task_local() {
        take_error();
        nrt_set_error_site(7, 3);
        assert_eq!(snapshot().error_site, Some((7, 3)));
        let worker = thread::spawn(|| {
            take_error();
            assert_eq!(snapshot().error_site, None);
            nrt_set_error_site(99, 1);
            assert_eq!(snapshot().error_site, Some((99, 1)));
            take_error();
        });
        worker.join().unwrap();
        assert_eq!(snapshot().error_site, Some((7, 3)), "parent site is unaffected");
        assert_eq!(nrt_take_error_site(), (7u64 << 32) | 3);
        take_error();
    }

    #[test]
    fn take_error_clears_all_task_local_state() {
        take_error();
        let (p, n) = name("f");
        nrt_push_frame(p, n, 5, 1);
        nrt_set_error_site(11, 2);
        pending_error_set(77);
        let snap = snapshot();
        assert_eq!(snap.frames.len(), 1);
        assert_eq!(snap.error_site, Some((11, 2)));
        assert_eq!(snap.pending_error, 77);

        assert_eq!(take_error(), 77, "returns the pending handle");
        let cleared = snapshot();
        assert!(cleared.frames.is_empty(), "depth reset hides all frames");
        assert_eq!(cleared.error_site, None);
        assert_eq!(cleared.pending_error, 0);
        assert_eq!(nrt_take_error_site(), 0, "site cell is empty after take_error");
    }

    #[test]
    fn pop_frame_and_clear_trace_reset_depth_without_panicking() {
        take_error();
        nrt_pop_frame(); // saturating_sub at depth 0
        assert_eq!(snapshot().frames.len(), 0);
        nrt_clear_trace();
        assert_eq!(snapshot().frames.len(), 0);
        let (p, n) = name("f");
        nrt_push_frame(p, n, 3, 1);
        nrt_clear_trace();
        assert!(snapshot().frames.is_empty(), "nrt_clear_trace resets depth");
        take_error();
    }

    #[test]
    fn format_traceback_is_stable_for_the_same_snapshot() {
        take_error();
        let snap = snapshot();
        let first = format_traceback("boom", &snap);
        let second = format_traceback("boom", &snap);
        assert_eq!(first, second, "serialization must be deterministic");
        assert!(first.ends_with("error: boom\n") || first.ends_with("error: boom\r\n"));
        take_error();
    }

    #[test]
    fn snapshot_is_owned_and_survives_task_state_reset() {
        take_error();
        let (p, n) = name("task_frame");
        nrt_push_frame(p, n, 42, 1);
        nrt_set_error_site(42, 3);
        pending_error_set(1234);
        let snap = snapshot();
        // Worker boundary clears all task-local state (mirrors P06's hook).
        assert_eq!(take_error(), 1234);
        // The snapshot is an owned copy, not an alias into the thread-local cell.
        assert_eq!(snap.frames.len(), 1);
        assert_eq!(snap.frames[0].name, "task_frame");
        assert_eq!(snap.frames[0].start, 42);
        assert_eq!(snap.error_site, Some((42, 3)));
        assert_eq!(snap.pending_error, 1234);
    }

    #[test]
    fn frame_name_maps_module_and_lambda() {
        take_error();
        assert_eq!(frame_name(""), "<module>");
        assert_eq!(frame_name("lambda"), "<lambda>");
        assert_eq!(frame_name("f"), "f");
        take_error();
    }

    #[test]
    fn a13_pending_error_is_task_local() {
        take_error();
        pending_error_set(42);
        assert_eq!(snapshot().pending_error, 42);
        let worker = thread::spawn(|| {
            take_error();
            assert_eq!(pending_error_take(), 0);
            pending_error_set(99);
            assert_eq!(snapshot().pending_error, 99);
            assert_eq!(pending_error_take(), 99);
            take_error();
        });
        worker.join().unwrap();
        assert_eq!(snapshot().pending_error, 42, "parent error is unaffected");
        assert_eq!(take_error(), 42, "take_error returns the pending handle");
        assert_eq!(snapshot().pending_error, 0);
    }

    /// P06: a worker snapshot must be reinstallable on the waiter thread with
    /// frames, error site and pending error all restored byte-for-byte.
    #[test]
    fn install_restores_worker_snapshot_on_waiter_thread() {
        take_error();
        let (p, n) = name("worker_frame");
        nrt_push_frame(p, n, 31, 1);
        nrt_set_error_site(31, 3);
        pending_error_set(77);
        let snap = snapshot();
        take_error();

        // Simulate the wait thread starting with stale state.
        let (stale, sn) = name("stale");
        nrt_push_frame(stale, sn, 1, 1);
        nrt_set_error_site(1, 1);
        pending_error_set(999);
        install(&snap);

        let restored = snapshot();
        assert_eq!(restored.frames.len(), 1);
        assert_eq!(restored.frames[0].name, "worker_frame");
        assert_eq!(restored.frames[0].start, 31);
        assert_eq!(restored.error_site, Some((31, 3)));
        assert_eq!(restored.pending_error, 77);
        assert_eq!(take_error(), 77);
    }

    #[test]
    fn traceback_text_matches_format_traceback_for_a_snapshot() {
        take_error();
        let snap = snapshot();
        let message = if snap.pending_error == 0 {
            "unknown error".to_string()
        } else {
            error_message(snap.pending_error)
        };
        assert_eq!(traceback_text(&snap), format_traceback(&message, &snap));
        take_error();
    }
}
