//! First-class closure objects for escaping lambdas.
//!
//! A closure is an NObj with tag 8: `a` is the code pointer (obtained with
//! Cranelift's `func_addr`) and `b` points to a `ClosureData` holding the
//! captured values (already boxed as handles), the parameter kinds for all
//! arguments, and the return kind.

use crate::class::{box_return, call_native, Kind};
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
    let raw = call_native(unsafe { obj(callable).a as usize }, &a);
    box_return(raw, d.ret_kind)
}
