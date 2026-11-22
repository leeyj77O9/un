//! Source locations, call frames and traceback formatting for no-VM executables.
//! Mirrors `runtime/context.rs::trace_string` and `error.rs::LangError::display`.

use std::io::Write;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::obj::display;

struct SourceInfo {
    stem: String,
    chars: Vec<char>,
    line_starts: Vec<usize>,
}

static SOURCE: Mutex<Option<SourceInfo>> = Mutex::new(None);
// Lock-free call frames. Each slot holds (name pointer, name length, start,
// source length). Plain no-VM executables are single-threaded; atomics are used
// only so the statics are Sync. This removes a mutex lock/unlock per call.
const MAX_FRAMES: usize = 512;
static FRAME_DEPTH: AtomicUsize = AtomicUsize::new(0);
static FRAME_SLOTS: std::sync::OnceLock<Vec<[AtomicU64; 4]>> = std::sync::OnceLock::new();

fn frame_slots() -> &'static Vec<[AtomicU64; 4]> {
    FRAME_SLOTS.get_or_init(|| {
        (0..MAX_FRAMES)
            .map(|_| std::array::from_fn(|_| AtomicU64::new(0)))
            .collect()
    })
}
static ERROR_SITE: Mutex<Option<(usize, usize)>> = Mutex::new(None);

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
    let d = FRAME_DEPTH.fetch_add(1, Ordering::Relaxed);
    if d < MAX_FRAMES {
        let slot = &frame_slots()[d];
        slot[0].store(name_ptr as u64, Ordering::Relaxed);
        slot[1].store(name_len as u64, Ordering::Relaxed);
        slot[2].store(start as u64, Ordering::Relaxed);
        slot[3].store(len as u64, Ordering::Relaxed);
    }
}

#[no_mangle]
pub extern "C" fn nrt_pop_frame() {
    FRAME_DEPTH.fetch_sub(1, Ordering::Relaxed);
}

#[no_mangle]
pub extern "C" fn nrt_clear_trace() {
    FRAME_DEPTH.store(0, Ordering::Relaxed);
}

#[no_mangle]
pub extern "C" fn nrt_set_error_site(start: usize, len: usize) {
    *ERROR_SITE.lock().unwrap() = Some((start, len));
}

#[no_mangle]
pub extern "C" fn nrt_take_error_site() -> u64 {
    let site = ERROR_SITE.lock().unwrap().take();
    match site {
        Some((s, l)) => ((s as u64) << 32) | (l as u64),
        None => 0,
    }
}

pub(crate) fn traceback(message: &str) {
    let src = SOURCE.lock().unwrap();
    let depth = FRAME_DEPTH.load(Ordering::Relaxed).min(MAX_FRAMES);
    let slots = frame_slots();
    let frames: Vec<(usize, usize, usize, usize)> = (0..depth)
        .map(|i| {
            let s = &slots[i];
            (
                s[0].load(Ordering::Relaxed) as usize,
                s[1].load(Ordering::Relaxed) as usize,
                s[2].load(Ordering::Relaxed) as usize,
                s[3].load(Ordering::Relaxed) as usize,
            )
        })
        .collect();
    let site = *ERROR_SITE.lock().unwrap();
    let nl = if cfg!(windows) { "\r\n" } else { "\n" };

    if frames.len() > 1 {
        let mut rows: Vec<(String, usize, String)> = Vec::new();
        for (name_ptr, name_len, start, _) in frames.iter() {
            let name = str_from(*name_ptr as *const u8, *name_len);
            let (line, text) = match src.as_ref() {
                Some(s) => (line_of(s, *start), line_text(s, *start)),
                None => (0, String::new()),
            };
            rows.push((frame_name(&name), line, text));
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
        let mut out = format!("un: error trace{nl}");
        for (name, line, text, repeated) in &collapsed {
            let file = src.as_ref().map(|s| format!("<{}>", s.stem)).unwrap_or_default();
            out.push_str(&format!("    -> {name:<width$}  {file}, line [{line}]{nl}"));
            if *repeated > 0 {
                out.push_str(&format!("    ... (repeated {repeated} more times){nl}"));
            } else if !compact && !text.is_empty() {
                out.push_str(&format!("         {text}{nl}"));
            }
        }
        print!("{out}");
    }

    if site.is_some() || frames.len() <= 1 { print!("{nl}"); }
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
        print!("{file}, line [{line}], column [{col}]{nl}");
        print!("    {text}{nl}");
        print!("    {caret}{nl}");
    }
    print!("error: {message}{nl}");
    let _ = std::io::stdout().flush();
}

pub(crate) fn clear_pending_site() {
    *ERROR_SITE.lock().unwrap() = None;
}

pub(crate) fn error_message(p: u64) -> String {
    display(p)
}
