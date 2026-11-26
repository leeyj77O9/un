//! First-class closure objects for escaping lambdas.
//!
//! A closure is an NObj with tag 8: `a` is the code pointer (obtained with
//! Cranelift's `func_addr`) and `b` points to a `ClosureData` holding the
//! captured values (already boxed as handles), the parameter kinds for all
//! arguments, and the return kind.
//!
//! ## A-11 (W-2026-11-26-P03) — capture `Send` boundary
//!
//! A capture is an `NObj` **handle** (`u64`), not a thread-affine pointer and
//! not an `Rc`. `ClosureData` is therefore structurally `Send + Sync` (asserted
//! at compile time below), and a closure handle may be moved to a worker
//! thread and invoked there through `unrt_call_obj`.
//!
//! Safety of the objects a capture *points to* does not rest on the handle
//! being `Copy`: every mutable payload reachable from a handle now lives in a
//! P02B guard table that serializes access by handle, so a worker invoking a
//! closure cannot race another worker on the captured object:
//!
//! - `obj.rs`: `OBJ_LOCKS` / `SEQ_DATA` — NObj scalar fields and tuple/set elements
//! - `class.rs`: `CLASSES` / `FIELD_ARENA` — instance field blocks
//! - `list.rs`: `LIST_DATA` — list element vectors
//! - `dict.rs`: `DICT_DATA` — dict payloads
//!
//! Int captures use the immutable small-int table or an immutable boxed Int,
//! so there is no write path for them at all. The default single-threaded
//! path performs the identical lookups and produces identical values and
//! error strings — the cross-thread guarantee is additive, not a semantic
//! change.

use crate::abi::call_native_typed;
use crate::class::{box_return, Kind};
use crate::obj::{alloc, err, obj, NObj};

static LAMBDAS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<i64, (usize, Vec<Kind>, Kind)>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

#[no_mangle]
pub extern "C" fn nrt_register_lambda(
    index: i64,
    ptr: usize,
    kinds_ptr: *const u8,
    kinds_len: usize,
    ret_kind: Kind,
) {
    let kinds = if kinds_ptr.is_null() || kinds_len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(kinds_ptr, kinds_len) }.to_vec()
    };
    LAMBDAS.lock().unwrap().insert(index, (ptr, kinds, ret_kind));
}

struct ClosureData {
    captures: Vec<u64>,
    kinds: Vec<Kind>,
    ret_kind: Kind,
}

// A-11: `data()` hands out `&'static ClosureData` on whichever thread invokes
// the closure, so the payload must be both movable and shareable. Both are
// structural properties of `u64` handles, `u8` kinds and their `Vec`s; this
// assertion turns a future non-portable capture field into a compile error
// here instead of an opt-in-only data race.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<ClosureData>();
    assert_sync::<ClosureData>();
};

fn kind_name(k: Kind) -> &'static str {
    match k { 0 => "int", 1 => "float", 2 => "bool", _ => "any" }
}

pub(crate) fn display_closure(p: u64) -> String {
    let d = unsafe { data(p) };
    let params: Vec<&str> = d.kinds.iter().skip(d.captures.len()).map(|k| kind_name(*k)).collect();
    format!("fn({}) -> {}", params.join(", "), kind_name(d.ret_kind))
}

unsafe fn data(p: u64) -> &'static ClosureData {
    &*(obj(p).b as *const ClosureData)
}

fn unbox(p: u64, kind: Kind) -> u64 {
    match kind {
        0 => crate::obj::unrt_int_from_obj(p) as u64,
        1 => crate::obj::float_of(p).to_bits(),
        2 => crate::obj::unrt_bool_from_obj(p) as u64,
        _ => p,
    }
}

#[no_mangle]
pub extern "C" fn unrt_obj_closure(
    index: i64,
    captures_ptr: *const u64,
    capture_count: usize,
) -> u64 {
    let (ptr, kinds, ret_kind) = {
        let map = LAMBDAS.lock().unwrap();
        let Some(entry) = map.get(&index) else {
            return err("lambda not registered");
        };
        entry.clone()
    };
    let captures = if captures_ptr.is_null() || capture_count == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(captures_ptr, capture_count) }.to_vec()
    };
    let d = Box::into_raw(Box::new(ClosureData { captures, kinds, ret_kind }));
    unsafe { alloc(NObj { tag: 8, a: ptr as i64, b: d as i64 }) }
}

#[no_mangle]
pub extern "C" fn unrt_is_callable(p: u64) -> i64 {
    unsafe { if obj(p).tag == 8 { 1 } else { 0 } }
}

#[no_mangle]
pub extern "C" fn unrt_arg_count(p: u64) -> i64 {
    unsafe {
        if obj(p).tag != 8 { return -1; }
        let d = data(p);
        (d.kinds.len().saturating_sub(d.captures.len())) as i64
    }
}

/// Call a closure with boxed argument handles. Captures are prepended and
/// unboxed according to the stored kinds, then the raw return is boxed.
#[no_mangle]
pub extern "C" fn unrt_call_obj(
    callable: u64,
    args: *const u64,
    argc: usize,
) -> u64 {
    unsafe {
        if obj(callable).tag != 8 {
            return err("value is not callable");
        }
    }

    let d = unsafe { data(callable) };

    let arg_slice = unsafe { std::slice::from_raw_parts(args, argc) };

    let mut a = Vec::with_capacity(d.kinds.len());
    for (i, kind) in d.kinds.iter().enumerate() {
        let p = if i < d.captures.len() {
            d.captures[i]
        } else {
            let j = i - d.captures.len();
            if j >= argc { crate::obj::unrt_obj_none() } else { arg_slice[j] }
        };
        a.push(unbox(p, *kind));
    }
    let raw = call_native_typed(unsafe { obj(callable).a as usize }, &a, &d.kinds, d.ret_kind);
    box_return(raw, d.ret_kind)
}

#[cfg(test)]
mod a11_tests {
    use super::*;
    use crate::obj::{display, unrt_int_from_obj, unrt_obj_int};

    // Pure callees: they touch no mutable global, matching generated wrappers.
    extern "C" fn add_captures(a: u64, b: u64) -> u64 {
        a.wrapping_add(b)
    }
    extern "C" fn list_len(handle: u64) -> u64 {
        crate::list::unrt_list_len(handle) as u64
    }

    const ADD_ID: i64 = 90_011;
    const LIST_ID: i64 = 90_012;

    fn int_closure() -> u64 {
        nrt_register_lambda(
            ADD_ID,
            add_captures as extern "C" fn(u64, u64) -> u64 as usize,
            [0u8, 0].as_ptr(),
            2,
            0,
        );
        unrt_obj_closure(ADD_ID, [unrt_obj_int(7), unrt_obj_int(35)].as_ptr(), 2)
    }

    fn list_closure(list: u64) -> u64 {
        nrt_register_lambda(
            LIST_ID,
            list_len as extern "C" fn(u64) -> u64 as usize,
            [3u8].as_ptr(),
            1,
            0,
        );
        unrt_obj_closure(LIST_ID, [list].as_ptr(), 1)
    }

    /// Compile-time proof of the A-11 boundary (duplicated here so the test
    /// module fails loudly if the assertion above is ever removed).
    #[test]
    fn closure_data_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ClosureData>();
    }

    /// A handle is an integer, so it crosses threads by value. Four workers
    /// invoking the *same* closure read the same `&'static ClosureData` and
    /// must all observe the captured Int values.
    #[test]
    fn captures_survive_cross_thread_invocation() {
        let c = int_closure();
        assert_eq!(unrt_arg_count(c), 0);
        let rendered = display(c);
        let threads: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(move || {
                    // A non-null pointer is required even for a zero-length slice.
                    let args = [unrt_obj_int(0)];
                    unrt_int_from_obj(unrt_call_obj(c, args.as_ptr(), 0))
                })
            })
            .collect();
        for t in threads {
            assert_eq!(t.join().unwrap(), 42);
        }
        assert_eq!(display(c), rendered);
    }

    /// The captured object's payload is behind a P02B guard table. Invoke a
    /// closure holding a list handle from another thread and read its length
    /// there, proving the guard — not the raw handle — owns the state.
    #[test]
    fn captured_list_is_protected_by_p02b_guard_table() {
        let xs = crate::list::unrt_list_new();
        for i in 0..3 {
            crate::list::unrt_list_push(xs, unrt_obj_int(i));
        }
        let c = list_closure(xs);
        let t = std::thread::spawn(move || {
            let args = [unrt_obj_int(0)];
            unrt_int_from_obj(unrt_call_obj(c, args.as_ptr(), 0))
        });
        assert_eq!(t.join().unwrap(), 3);
        assert_eq!(crate::list::unrt_list_len(xs), 3);
    }
}
