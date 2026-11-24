//! Byte-string operations for the no-VM runtime.
//!
//! The VM currently represents `bytes` as a `List<Int>` with the type name
//! `"bytes"` (see `rust/src/native/bytes.rs`). NRT uses a dedicated opaque tag
//! internally; display/equality/conversions are kept byte-identical with the
//! VM, so the representation difference is not observable from Un code.

use crate::obj::{alloc, err, obj, unrt_obj_bool, unrt_obj_int, unrt_obj_none, NObj};

pub const TAG_BYTES: i64 = 13;

fn bytes_obj(bytes: Vec<u8>) -> u64 {
    unsafe {
        alloc(NObj {
            tag: TAG_BYTES,
            a: Box::into_raw(Box::new(bytes)) as i64,
            b: 0,
        })
    }
}

unsafe fn bytes_slice(p: u64) -> &'static [u8] {
    &*(obj(p).a as *const Vec<u8>)
}

fn as_bytes(p: u64) -> Option<Vec<u8>> {
    unsafe {
        match obj(p).tag {
            TAG_BYTES => Some(bytes_slice(p).to_vec()),
            6 => Some(crate::strings::str_of(p).as_bytes().to_vec()),
            3 => {
                let mut out = Vec::new();
                for item in crate::list::items(p) {
                    if obj(*item).tag != 0 {
                        return None;
                    }
                    let v = obj(*item).a;
                    if !(0..=255).contains(&v) {
                        return None;
                    }
                    out.push(v as u8);
                }
                Some(out)
            }
            _ => None,
        }
    }
}

fn bytes_arg(args: &[u64], name: &str) -> Result<Vec<u8>, u64> {
    args.first()
        .and_then(|p| as_bytes(*p))
        .ok_or_else(|| err(&format!("expected '{name}' is bytes")))
}

fn int_arg(args: &[u64], name: &str) -> Result<i64, u64> {
    args.first()
        .filter(|p| unsafe { obj(**p).tag } == 0)
        .map(|p| unsafe { obj(*p).a })
        .ok_or_else(|| err(&format!("expected '{name}' is integer")))
}

fn normalize_index(index: i64, len: usize) -> Option<usize> {
    let shifted = if index < 0 {
        len as i64 + index
    } else {
        index
    };
    if shifted >= 0 && (shifted as usize) < len {
        Some(shifted as usize)
    } else {
        None
    }
}

fn clamp_bound(bound: i64, len: usize) -> usize {
    let shifted = if bound < 0 {
        len as i64 + bound
    } else {
        bound
    };
    if shifted < 0 {
        0
    } else if shifted as usize > len {
        len
    } else {
        shifted as usize
    }
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn display_bytes(p: u64) -> String {
    unsafe {
        let parts: Vec<String> = bytes_slice(p)
            .iter()
            .map(|b| b.to_string())
            .collect();
        format!("[{}]", parts.join(", "))
    }
}

/// Returns `None` for unhandled module/member pairs.
pub fn dispatch(m: &str, name: &str, args: &[u64]) -> Option<u64> {
    match (m, name) {
        ("bytes", "from_str") => {
            let text = args
                .first()
                .filter(|p| unsafe { obj(**p).tag } == 6)
                .map(|p| unsafe { crate::strings::str_of(*p) }.clone());
            Some(match text {
                Some(s) => bytes_obj(s.as_bytes().to_vec()),
                None => err("expected 'text' is string"),
            })
        }
        ("bytes", "from_list") => {
            let items = match args.first() {
                Some(p) if unsafe { obj(*p).tag } == 3 => unsafe { crate::list::items(*p) }.clone(),
                _ => return Some(err("expected 'values' is list")),
            };
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let v = unsafe { obj(item) };
                if v.tag == 0 && (0..=255).contains(&v.a) {
                    out.push(v.a as u8);
                } else {
                    return Some(err("expected byte values (integer 0..255)"));
                }
            }
            Some(bytes_obj(out))
        }
        ("bytes", "from_hex") => {
            let text = match args.first() {
                Some(p) if unsafe { obj(*p).tag } == 6 => {
                    unsafe { crate::strings::str_of(*p) }.trim().to_owned()
                }
                _ => return Some(err("expected 'hex' is string")),
            };
            if text.len() % 2 != 0 {
                return Some(err("hex string must have even length"));
            }
            let raw = text.as_bytes();
            let mut out = Vec::with_capacity(raw.len() / 2);
            let mut i = 0;
            while i < raw.len() {
                match (hex_val(raw[i]), hex_val(raw[i + 1])) {
                    (Some(hi), Some(lo)) => out.push((hi << 4) | lo),
                    _ => return Some(err("invalid hex string")),
                }
                i += 2;
            }
            Some(bytes_obj(out))
        }
        ("bytes", "to_str") => Some(match bytes_arg(args, "bytes") {
            Ok(v) => crate::strings::str_new(String::from_utf8_lossy(&v).into_owned()),
            Err(e) => e,
        }),
        ("bytes", "to_list") => Some(match bytes_arg(args, "bytes") {
            Ok(v) => crate::list::list_new(v.into_iter().map(|b| unrt_obj_int(b as i64)).collect()),
            Err(e) => e,
        }),
        ("bytes", "to_hex") => Some(match bytes_arg(args, "bytes") {
            Ok(v) => {
                const DIGITS: &[u8; 16] = b"0123456789abcdef";
                let mut out = String::with_capacity(v.len() * 2);
                for byte in v {
                    out.push(DIGITS[(byte >> 4) as usize] as char);
                    out.push(DIGITS[(byte & 0x0f) as usize] as char);
                }
                crate::strings::str_new(out)
            }
            Err(e) => e,
        }),
        ("bytes", "len") => Some(match bytes_arg(args, "bytes") {
            Ok(v) => unrt_obj_int(v.len() as i64),
            Err(e) => e,
        }),
        ("bytes", "get") => {
            let bytes = match bytes_arg(args, "bytes") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let index = match int_arg(args.get(1..).unwrap_or(&[]), "index") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            Some(match normalize_index(index, bytes.len()) {
                Some(i) => unrt_obj_int(bytes[i] as i64),
                None => err("byte index out of range"),
            })
        }
        ("bytes", "slice") => {
            let bytes = match bytes_arg(args, "bytes") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let start = match int_arg(args.get(1..).unwrap_or(&[]), "start") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let end = if args.get(2).is_none() || unsafe { obj(args[2]).tag } == 4 {
                bytes.len() as i64
            } else {
                match args.get(2) {
                    Some(p) if unsafe { obj(*p).tag } == 0 => unsafe { obj(*p).a },
                    _ => return Some(err("expected 'end' is integer")),
                }
            };
            let lo = clamp_bound(start, bytes.len());
            let hi = clamp_bound(end, bytes.len());
            Some(if lo > hi {
                bytes_obj(Vec::new())
            } else {
                bytes_obj(bytes[lo..hi].to_vec())
            })
        }
        ("bytes", "concat") => {
            let a = match bytes_arg(args, "a") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let b = match bytes_arg(args.get(1..).unwrap_or(&[]), "b") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let mut out = Vec::with_capacity(a.len() + b.len());
            out.extend_from_slice(&a);
            out.extend_from_slice(&b);
            Some(bytes_obj(out))
        }
        ("bytes", "eq") => {
            let a = match bytes_arg(args, "a") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            let b = match bytes_arg(args.get(1..).unwrap_or(&[]), "b") {
                Ok(v) => v,
                Err(e) => return Some(e),
            };
            Some(unrt_obj_bool((a == b) as i64))
        }
        ("bytes", "is_bytes") => Some(unrt_obj_bool(
            args.first().is_some_and(|p| unsafe { obj(*p).tag } == TAG_BYTES) as i64,
        )),
        ("fs", "read_bytes") => {
            let path = match args.first() {
                Some(p) if unsafe { obj(*p).tag } == 6 => unsafe { crate::strings::str_of(*p) }.clone(),
                _ => return Some(err("expected 'path' is string")),
            };
            if !std::path::Path::new(&path).is_file() {
                return Some(err("file not found"));
            }
            Some(match std::fs::read(&path) {
                Ok(data) => bytes_obj(data),
                Err(e) => err(&e.to_string()),
            })
        }
        ("fs", "write_bytes") => {
            let path = match args.first() {
                Some(p) if unsafe { obj(*p).tag } == 6 => unsafe { crate::strings::str_of(*p) }.clone(),
                _ => return Some(err("expected 'path' is string")),
            };
            let data = match args.get(1).and_then(|p| as_bytes(*p)) {
                Some(v) => v,
                None => return Some(err("expected 'bytes' is bytes")),
            };
            Some(match std::fs::write(&path, data) {
                Ok(()) => unrt_obj_none(),
                Err(e) => err(&e.to_string()),
            })
        }
        _ => None,
    }
}
