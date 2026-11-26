//! Native string objects (tag 6). The payload is an owned `String`.

use crate::obj::{alloc, err, obj, NObj};

pub(crate) unsafe fn str_of(p: u64) -> &'static String {
    &*(obj(p).a as *const String)
}
pub(crate) fn str_new(s: String) -> u64 {
    unsafe {
        let p = Box::into_raw(Box::new(s));
        alloc(NObj { tag: 6, a: p as i64, b: 0 })
    }
}

#[no_mangle]
pub extern "C" fn unrt_str_from_bytes(ptr: *const u8, len: usize) -> u64 {
    if ptr.is_null() {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    str_new(String::from_utf8_lossy(bytes).into_owned())
}

#[no_mangle]
pub extern "C" fn unrt_str_concat(a: u64, b: u64) -> u64 {
    unsafe {
        if obj(a).tag != 6 || obj(b).tag != 6 {
            return err("unrt: str concat needs two Str");
        }
        str_new(format!("{}{}", str_of(a), str_of(b)))
    }
}

/// W-2026-11-26-STRINDEX2: `s[i]` on a Str, reached through the existing
/// `("str", "get")` `unrt_call_native` dispatch (no new C ABI symbol).
///
/// This mirrors `runtime::obj::get_item`'s `Payload::Str` arm byte for byte:
/// ASCII strings index by byte, non-ASCII strings by UTF-16 unit (an index
/// landing on a surrogate half yields U+FFFD via `from_utf16_lossy`), negative
/// indices wrap exactly once, and the two error messages are identical.
pub(crate) fn str_get(args: &[u64]) -> u64 {
    let (Some(s_handle), Some(key_handle)) = (args.first().copied(), args.get(1).copied()) else {
        return err("invalid index type");
    };
    if unsafe { obj(key_handle).tag != 0 } {
        return err("invalid index type");
    }
    if unsafe { obj(s_handle).tag != 6 } {
        return err("invalid index type");
    }
    let i = unsafe { obj(key_handle).a };
    let s = unsafe { str_of(s_handle) };
    if s.is_ascii() {
        let len = s.len() as i64;
        let mut index = i;
        if index < 0 {
            index += len;
        }
        if index < 0 || index >= len {
            return err("list index out of range");
        }
        let start = index as usize;
        return str_new(s[start..start + 1].to_string());
    }
    let len = s.encode_utf16().count() as i64;
    let index = i as i32 as i64;
    let unit_index = if (0..len).contains(&index) {
        index
    } else {
        let wrapped = (i + len) as i32 as i64;
        if !(0..len).contains(&wrapped) {
            return err("list index out of range");
        }
        wrapped
    };
    let unit = s.encode_utf16().nth(unit_index as usize).unwrap_or(0);
    str_new(String::from_utf16_lossy(&[unit]))
}
