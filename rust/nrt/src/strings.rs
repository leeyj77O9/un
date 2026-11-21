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
