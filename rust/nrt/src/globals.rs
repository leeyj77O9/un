//! Process-wide global slots for inlined user-module state.
//!
//! Each global is an NObj handle stored in a flat table. The generated driver
//! registers global names at startup; TIR emits `GlobalGet`/`GlobalSet` as
//! `unrt_global_get`/`unrt_global_set` calls.
//!
//! ## A-10 (W-2026-11-26-P03) — opt-in concurrency contract
//!
//! `GLOBALS` is a `Mutex<Vec<u64>>`, so the table itself is memory-safe: a
//! worker can never observe a torn handle or race a `Vec` reallocation. That
//! is a *memory* guarantee, not an *ordering* guarantee. In a plain
//! single-threaded exe, slot writes follow source order exactly. With opt-in
//! `un build --parallel`, two workers writing the *same* slot have a
//! last-writer-wins outcome determined by thread scheduling; there is no
//! happens-before edge between separate `unrt_global_set` calls.
//!
//! This is an intentionally **documented divergence** for the opt-in path
//! (RFC §3.2 places shared global state outside the synchronized-object
//! contract; P09 records it in the user docs), not a defect to repair here.
//! Adding per-slot versioning or changing the C signatures would alter the
//! handle layout of every built program and break byte-identical gates.
//! Therefore A-10 makes **no structural or signature change** in this file:
//! slot numbering and all three exported symbols stay exactly as they were.

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

/// Store `value` in `index`, returning the same bits.
///
/// A-10: concurrent writes to one slot from opt-in workers are a documented
/// last-writer nondeterminism. The `Mutex` protects the table's memory, not
/// the logical write order; default (single-threaded) builds are deterministic.
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
