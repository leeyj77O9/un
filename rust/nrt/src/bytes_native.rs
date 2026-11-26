//! Byte-string operations for the no-VM runtime.
//!
//! The VM currently represents `bytes` as a `List<Int>` with the type name
//! `"bytes"` (see `rust/src/native/bytes.rs`); D-098 moves both runtimes to a
//! dedicated bytes payload. NRT already uses a dedicated opaque tag (13) and
//! keeps reading the legacy `List<Int>` form. Display, equality, `len` and all
//! conversions stay byte-identical with the VM, so neither the representation
//! difference nor the gradual migration is observable from Un code.

use crate::obj::{alloc, err, obj, unrt_obj_bool, unrt_obj_int, unrt_obj_none, NObj};

pub const TAG_BYTES: i64 = 13;

fn bytes_obj(bytes: Vec<u8>) -> u64 {
    let len = bytes.len() as i64;
    unsafe {
        alloc(NObj {
            tag: TAG_BYTES,
            a: Box::into_raw(Box::new(bytes)) as i64,
            b: len,
        })
    }
}

unsafe fn bytes_slice(p: u64) -> &'static [u8] {
    &*(obj(p).a as *const Vec<u8>)
}

/// Byte length mirrored in `b` (same layout as Tuple/Set) so `unrt_len`
/// never has to dereference the boxed vector. The slice length is the
/// fallback for a legacy handle with `b == 0`.
pub(crate) fn bytes_len(p: u64) -> i64 {
    unsafe {
        let mirrored = obj(p).b;
        if mirrored > 0 { mirrored } else { bytes_slice(p).len() as i64 }
    }
}

/// Dedicated bytes value (`TAG_BYTES`), the legacy `List<Int>` bytes value
/// (tag 3), and mixed pairs compare by byte content. This mirrors the VM's
/// `collections::element_eq`, which compares List payloads structurally
/// regardless of their type name (`bytes` vs `list`), and gives gradual
/// migration between the two NRT representations identical results.
/// `None` means the handle is not a bytes-like value.
fn as_bytes_like(p: u64) -> Option<Vec<u8>> {
    unsafe {
        match obj(p).tag {
            TAG_BYTES => Some(bytes_slice(p).to_vec()),
            // Legacy bytes: a List<Int> with byte values. Unlike `as_bytes`,
            // a Str is *not* bytes-like here — the VM's `==` on a bytes
            // (List payload) value and a Str returns false, while the
            // `bytes.*` helpers deliberately accept Str as a convenience.
            3 => {
                let mut out = Vec::new();
                for item in crate::list::with_list(p, |v| v.clone()) {
                    if obj(item).tag != 0 {
                        return None;
                    }
                    let v = obj(item).a;
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

pub(crate) fn bytes_cmp(a: u64, b: u64) -> Option<std::cmp::Ordering> {
    let left = as_bytes_like(a)?;
    let right = as_bytes_like(b)?;
    Some(left.cmp(&right))
}

pub(crate) fn bytes_eq(a: u64, b: u64) -> Option<bool> {
    bytes_cmp(a, b).map(|ord| ord == std::cmp::Ordering::Equal)
}

fn as_bytes(p: u64) -> Option<Vec<u8>> {
    unsafe {
        match obj(p).tag {
            TAG_BYTES => Some(bytes_slice(p).to_vec()),
            6 => Some(crate::strings::str_of(p).as_bytes().to_vec()),
            3 => {
                let mut out = Vec::new();
                for item in crate::list::with_list(p, |v| v.clone()) {
                    if obj(item).tag != 0 {
                        return None;
                    }
                    let v = obj(item).a;
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
            // VM accepts both List and Tuple payloads here; a dedicated
            // bytes handle is a List payload there as well, so round-tripping
            // `bytes.from_list(bytes.from_str(..))` must keep working.
            let from_handles = |items: Vec<u64>| {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    let v = unsafe { obj(item) };
                    if v.tag == 0 && (0..=255).contains(&v.a) {
                        out.push(v.a as u8);
                    } else {
                        return err("expected byte values (integer 0..255)");
                    }
                }
                bytes_obj(out)
            };
            let input = args.first().copied();
            Some(match input {
                Some(p) if unsafe { obj(p).tag } == TAG_BYTES => {
                    bytes_obj(unsafe { bytes_slice(p) }.to_vec())
                }
                Some(p) if unsafe { obj(p).tag } == 3 => {
                    from_handles(crate::list::with_list(p, |v| v.clone()))
                }
                Some(p) if unsafe { obj(p).tag } == 14 => {
                    from_handles(crate::obj::with_seq_items(p, |v| v.clone()))
                }
                _ => err("expected 'values' is list"),
            })
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

#[cfg(test)]
mod tests {
    use super::{as_bytes, bytes_eq, bytes_len, dispatch, display_bytes, TAG_BYTES};
    use crate::obj::{display, obj, unrt_obj_eq, unrt_obj_int};

    fn str_obj(s: &str) -> u64 {
        crate::strings::str_new(s.to_string())
    }
    fn call(m: &str, name: &str, args: &[u64]) -> u64 {
        dispatch(m, name, args).expect("handled native call")
    }
    fn from_str(s: &str) -> u64 {
        call("bytes", "from_str", &[str_obj(s)])
    }
    fn legacy_bytes(bytes: &[i64]) -> u64 {
        crate::list::list_new(bytes.iter().map(|b| unrt_obj_int(*b)).collect())
    }

    #[test]
    fn dedicated_representation_has_identical_external_behaviour() {
        let b = from_str("abc");
        assert_eq!(unsafe { obj(b).tag }, TAG_BYTES);
        assert_eq!(bytes_len(b), 3);
        assert_eq!(crate::list::unrt_len(b), unrt_obj_int(3));
        assert_eq!(display(b), "[97, 98, 99]");
        assert_eq!(display_bytes(b), "[97, 98, 99]");
        assert_eq!(crate::obj::unrt_obj_type_name(b, b"bytes\0".as_ptr(), 5), 1);
        assert_eq!(crate::obj::unrt_type_is(b, b"bytes\0".as_ptr(), 5), 1);
        assert_eq!(crate::obj::unrt_type_is(b, b"list\0".as_ptr(), 4), 0);
    }

    #[test]
    fn equality_matches_vm_for_both_representations() {
        let abc = from_str("abc");
        let abc2 = from_str("abc");
        let abd = from_str("abd");
        let legacy = legacy_bytes(&[97, 98, 99]);
        let other = legacy_bytes(&[1, 2, 3]);
        assert_eq!(bytes_eq(abc, abc2), Some(true));
        assert_eq!(bytes_eq(abc, abd), Some(false));
        assert_eq!(unrt_obj_eq(abc, abc2), 1);
        assert_eq!(unrt_obj_eq(abc, abd), 0);
        assert_eq!(unrt_obj_eq(abc, legacy), 1);
        assert_eq!(unrt_obj_eq(legacy, abd), 0);
        assert_eq!(unrt_obj_eq(abc, other), 0);
        // Legacy List<Int> bytes (byte literals) compare structurally too.
        assert_eq!(unrt_obj_eq(legacy, legacy_bytes(&[97, 98, 99])), 1);
        assert_eq!(unrt_obj_eq(legacy, legacy_bytes(&[97, 98, 100])), 0);
        // Dynamic comparison (`any` operands) takes a different ABI path.
        assert_eq!(display(crate::dynops::unrt_dyn_bin(24, abc, abd)), "false");
        assert_eq!(display(crate::dynops::unrt_dyn_bin(24, abc, abc2)), "true");
        assert_eq!(display(crate::dynops::unrt_dyn_bin(25, abc, abd)), "true");
    }

    #[test]
    fn all_twelve_bytes_members_accept_dedicated_and_legacy_values() {
        let b = from_str("abc");
        let legacy = legacy_bytes(&[97, 98, 99]);
        for value in [b, legacy] {
            assert_eq!(display(call("bytes", "to_str", &[value])), "abc");
            assert_eq!(display(call("bytes", "to_hex", &[value])), "616263");
            assert_eq!(display(call("bytes", "len", &[value])), "3");
            assert_eq!(display(call("bytes", "get", &[value, unrt_obj_int(-1)])), "99");
            assert_eq!(display(call("bytes", "slice", &[value, unrt_obj_int(1)])), "[98, 99]");
            assert_eq!(
                display(call("bytes", "concat", &[value, value])),
                "[97, 98, 99, 97, 98, 99]",
            );
            assert_eq!(display(call("bytes", "eq", &[value, value])), "true");
            assert_eq!(display(call("bytes", "from_list", &[value])), "[97, 98, 99]");
        }
        assert_eq!(display(call("bytes", "from_hex", &[str_obj("0102ff")])), "[1, 2, 255]");
        assert_eq!(display(call("bytes", "from_list", &[
            crate::list::list_new(vec![unrt_obj_int(65), unrt_obj_int(66)]),
        ])), "[65, 66]");
        assert_eq!(display(call("bytes", "is_bytes", &[b])), "true");
        assert_eq!(display(call("bytes", "is_bytes", &[legacy])), "false");
        assert_eq!(display(call("bytes", "to_list", &[b])), "[97, 98, 99]");
        // Native helpers deliberately accept Str/Tuple as convenience inputs.
        assert_eq!(as_bytes(str_obj("abc")), Some(vec![97, 98, 99]));
        let tup = crate::obj::tuple_new(vec![unrt_obj_int(1), unrt_obj_int(2)]);
        assert_eq!(display(call("bytes", "from_list", &[tup])), "[1, 2]");
    }

    #[test]
    fn fs_bytes_round_trip_uses_dedicated_representation() {
        let path = format!(
            "{}/un_nrt_bytes_{}_{}.bin",
            std::env::temp_dir().to_string_lossy(),
            std::process::id(),
            bytes_len(from_str("x")),
        );
        let data = from_str("abc");
        assert_eq!(
            unsafe { obj(call("fs", "write_bytes", &[str_obj(&path), data])).tag },
            4,
        );
        let read = call("fs", "read_bytes", &[str_obj(&path)]);
        assert_eq!(unsafe { obj(read).tag }, TAG_BYTES);
        assert_eq!(display(read), "[97, 98, 99]");
        let _ = std::fs::remove_file(&path);
    }
}
