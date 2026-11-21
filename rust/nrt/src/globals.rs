//! Process-wide global slots for inlined user-module state.
//!
//! Each global is an NObj handle stored in a flat table. The generated driver
//! registers global names at startup; TIR emits `GlobalGet`/`GlobalSet` as
//! `unrt_global_get`/`unrt_global_set` calls.

use std::sync::Mutex;

use crate::obj::{obj, NObj};

static GLOBALS: Mutex<Vec<u64>> = Mutex::new(Vec::new());

fn str_from(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len) }).into_owned()
}

#[no_mangle]
pub extern "C" fn nrt_register_global(name_ptr: *const u8, name_len: usize) -> i64 {
    let _name = str_from(name_ptr, name_len);
    let mut g = GLOBALS.lock().unwrap();
    let id = g.len();
    g.push(0);
    id as i64
}

#[no_mangle]
pub extern "C" fn unrt_global_get(index: i64) -> u64 {
    // Return a valid Int handle for unset slots instead of raw 0 (which
    // callers would dereference as an NObj pointer).
    GLOBALS.lock().unwrap().get(index as usize).copied()
        .filter(|v| *v != 0)
        .unwrap_or_else(|| crate::obj::unrt_obj_int(0))
}

#[no_mangle]
pub extern "C" fn unrt_global_set(index: i64, value: u64) -> u64 {
    let mut g = GLOBALS.lock().unwrap();
    if let Some(slot) = g.get_mut(index as usize) {
        *slot = value;
    }
    value
}

/// Debug helper: verify a handle is a live object (used by generated init code).
#[allow(dead_code)]
pub(crate) fn is_live(p: u64) -> bool {
    if p == 0 { return false; }
    let o: &NObj = unsafe { obj(p) };
    o.tag != 0 || o.a != i64::MIN
}
