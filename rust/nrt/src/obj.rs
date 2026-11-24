//! Core object model and scalar/error bridges.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct NObj {
    // 0 Int, 1 Float, 2 Bool, 3 List, 4 None, 5 Error, 6 Str, 7 Dict,
    // 8 Closure, 9 Instance, 10 Panic, 11 HttpClient, 12 WebSocket, 13 Bytes,
    // 14 Tup, 15 Set (14/15 reserved by ARC-00 for W-2026-11-24-TUPLETAG).
    pub(crate) tag: i64,
    pub(crate) a: i64,
    pub(crate) b: i64,
}

// Tagged sequence objects (Tuple/Set) follow the boxed-payload pattern used by
// Str and Error: `a` is a `Box::into_raw` pointer to `Vec<u64>` and `b` is the
// element count. Nothing else in NRT inspects `a`/`b` generically for these
// tags, so the layout is private to the helpers below.
pub(crate) const TAG_TUP: i64 = 14;
pub(crate) const TAG_SET: i64 = 15;

// Cache window for boxed small integers (loop indices, zero fills).
const SMALL_INTS: i64 = 16384;

// Immutable small integers are pre-built in a static array so the hot path is
// a bounds check plus an address computation — no OnceLock, no atomic load,
// no lazy Box allocation. These objects are never mutated (tag 0 is an
// immutable Int); all `obj_mut` call sites in NRT operate on list/instance
// objects created by `alloc`, never on these handles.
const fn small_int(v: usize) -> NObj {
    NObj { tag: 0, a: v as i64, b: 0 }
}
static SMALL_INT_OBJS: [NObj; SMALL_INTS as usize] = {
    let mut arr = [small_int(0); SMALL_INTS as usize];
    let mut i = 1;
    while i < SMALL_INTS as usize {
        arr[i] = small_int(i);
        i += 1;
    }
    arr
};

// Objects are intentionally leaked: the process is short-lived, so there is
// no need to track allocations for deallocation.
static PENDING: AtomicU64 = AtomicU64::new(0);

pub(crate) unsafe fn alloc(o: NObj) -> u64 {
    Box::into_raw(Box::new(o)) as u64
}
pub(crate) unsafe fn obj(p: u64) -> &'static NObj {
    &*(p as *const NObj)
}
pub(crate) unsafe fn obj_mut(p: u64) -> &'static mut NObj {
    &mut *(p as *mut NObj)
}

// ---- Tag 14 (Tuple) / 15 (Set) -------------------------------------------
// Replicas of the VM's `collections::tup` / `collections::set` value model:
// elements are opaque boxed handles, stored in first-occurrence order.

/// Boxed element vector of a Tuple/Set handle. Callers must check the tag.
pub(crate) unsafe fn seq_items(p: u64) -> &'static mut Vec<u64> {
    &mut *(obj(p).a as *mut Vec<u64>)
}
/// Element count mirrored in `b` so callers can avoid dereferencing `a`.
pub(crate) unsafe fn seq_len(p: u64) -> i64 {
    obj(p).b
}
fn seq_new(tag: i64, items: Vec<u64>) -> u64 {
    let len = items.len() as i64;
    let ptr = Box::into_raw(Box::new(items));
    unsafe { alloc(NObj { tag, a: ptr as i64, b: len }) }
}
pub(crate) fn tuple_new(items: Vec<u64>) -> u64 {
    seq_new(TAG_TUP, items)
}
/// Value equality used by `KeyMap` in the VM (`obj::key_eq`, obj.rs:304):
/// same-type primitives compare by value, numeric types compare across
/// Int/Float/Bool, Str by content, and unrelated types are *not* equal.
/// `cmp_obj` alone cannot be used here because its catch-all arm is Equal.
fn set_key_eq(a: u64, b: u64) -> bool {
    unsafe {
        let (x, y) = (obj(a), obj(b));
        match (x.tag, y.tag) {
            (0, 0) | (2, 2) => x.a == y.a,
            (1, 1) => f64::from_bits(x.a as u64) == f64::from_bits(y.a as u64),
            (0, 1) | (1, 0) | (0, 2) | (2, 0) | (1, 2) | (2, 1) => {
                cmp_obj(a, b) == std::cmp::Ordering::Equal
            }
            (6, 6) => crate::strings::str_of(a) == crate::strings::str_of(b),
            (t, u) if t == u => eq_obj(a, b),
            _ => false,
        }
    }
}

/// VM `collections::set` dedups through `KeyMap::try_insert`: the first
/// occurrence of an equivalent key wins and insertion order is preserved.
pub(crate) fn set_new(items: Vec<u64>) -> u64 {
    let mut unique: Vec<u64> = Vec::with_capacity(items.len());
    for item in items {
        if !unique.iter().any(|p| set_key_eq(*p, item)) {
            unique.push(item);
        }
    }
    seq_new(TAG_SET, unique)
}

pub(crate) fn int_of(p: u64) -> i64 {
    unsafe { if obj(p).tag == 0 { obj(p).a } else { 0 } }
}
pub(crate) fn float_of(p: u64) -> f64 {
    unsafe {
        if obj(p).tag == 1 {
            f64::from_bits(obj(p).a as u64)
        } else {
            0.0
        }
    }
}
pub(crate) fn bool_of(p: u64) -> bool {
    unsafe { obj(p).tag == 2 && obj(p).a != 0 }
}
pub(crate) fn cmp_obj(a: u64, b: u64) -> std::cmp::Ordering {
    unsafe {
        let (x, y) = (obj(a), obj(b));
        match (x.tag, y.tag) {
            (0, 0) | (2, 2) => x.a.cmp(&y.a),
            (1, 1) => f64::from_bits(x.a as u64)
                .partial_cmp(&f64::from_bits(y.a as u64))
                .unwrap_or(std::cmp::Ordering::Equal),
            (0, 1) => (x.a as f64)
                .partial_cmp(&f64::from_bits(y.a as u64))
                .unwrap_or(std::cmp::Ordering::Equal),
            (1, 0) => f64::from_bits(x.a as u64)
                .partial_cmp(&(y.a as f64))
                .unwrap_or(std::cmp::Ordering::Equal),
            (6, 6) => crate::strings::str_of(a).cmp(crate::strings::str_of(b)),
            _ => std::cmp::Ordering::Equal,
        }
    }
}
pub(crate) fn eq_obj(a: u64, b: u64) -> bool {
    cmp_obj(a, b) == std::cmp::Ordering::Equal
}
/// Element stringification shared by tags 14/15. The VM's
/// `collections::tup_to_str` / `set_to_str` map `obj::display` over the
/// elements and join with ", " — unlike `list_to_str`, they do **not** quote
/// Str elements (`("a", 1)` prints as `(a, 1)` in `un run`).
fn display_seq(p: u64, open: char, close: char) -> String {
    let parts: Vec<String> = unsafe { seq_items(p) }.iter().map(|v| display(*v)).collect();
    format!("{}{}{}", open, parts.join(", "), close)
}

pub(crate) fn display(p: u64) -> String {
    unsafe {
        match obj(p).tag {
            0 => obj(p).a.to_string(),
            1 => fmt_double(f64::from_bits(obj(p).a as u64)),
            2 => {
                if obj(p).a != 0 {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            3 => {
                let parts: Vec<String> = crate::list::items(p).iter().map(|v| {
                    if obj(*v).tag == 6 {
                        format!("'{}'", display(*v))
                    } else {
                        display(*v)
                    }
                }).collect();
                format!("[{}]", parts.join(", "))
            }
            4 => "none".into(),
            5 | 10 => {
                let (ptr, len) = (obj(p).a as *const u8, obj(p).b as usize);
                String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).into_owned()
            }
            6 => crate::strings::str_of(p).clone(),
            7 => crate::dict::display_dict(p),
            8 => crate::closure::display_closure(p),
            9 => crate::class::display_instance(p),
            11 => "http_client".into(),
            12 => "web_socket".into(),
            13 => crate::bytes_native::display_bytes(p),
            14 => display_seq(p, '(', ')'),
            15 => display_seq(p, '{', '}'),
            _ => "value".into(),
        }
    }
}
/// Deliberately duplicated from the VM's `runtime::prims::fmt_double` so this
/// runtime stays independent of `libunvm`. Keep both implementations — and
/// both parity tests — in lockstep; this is a replica, not a shared crate.
pub(crate) fn fmt_double(v: f64) -> String {
    if v.is_nan() { return "NaN".to_string(); }
    if v.is_infinite() { return if v > 0.0 { "∞".to_string() } else { "-∞".to_string() }; }
    if v == 0.0 { return if v.is_sign_negative() { "-0".to_string() } else { "0".to_string() }; }
    let neg = v < 0.0;
    let abs = v.abs();
    let sci = format!("{abs:e}");
    let (mant, exp) = match sci.split_once('e') {
        Some((m, e)) => (m, e.parse::<i64>().unwrap_or(0)),
        None => (sci.as_str(), 0),
    };
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let body = if exp >= 17 || exp <= -5 {
        let sign = if exp < 0 { "-" } else { "+" };
        let ae = exp.unsigned_abs();
        let pad = if ae < 10 { format!("0{ae}") } else { ae.to_string() };
        if digits.len() <= 1 {
            format!("{digits}E{sign}{pad}")
        } else {
            format!("{}.{}E{sign}{pad}", &digits[..1], &digits[1..])
        }
    } else {
        format!("{abs}")
    };
    if neg { format!("-{body}") } else { body }
}

/// Build an error object with an owned message. Callers may pass `format!`
/// results; leaking the string is safe because the process is short-lived.
pub(crate) fn err(msg: &str) -> u64 {
    let leaked: &'static str = Box::leak(msg.to_string().into_boxed_str());
    unsafe { alloc(NObj { tag: 5, a: leaked.as_ptr() as i64, b: leaked.len() as i64 }) }
}
/// Uncatchable panic: like an error but printed without source location/trace.
pub(crate) fn panic_err(msg: &str) -> u64 {
    let leaked: &'static str = Box::leak(msg.to_string().into_boxed_str());
    unsafe { alloc(NObj { tag: 10, a: leaked.as_ptr() as i64, b: leaked.len() as i64 }) }
}

#[no_mangle]
pub extern "C" fn unrt_init() {}
#[no_mangle]
pub extern "C" fn unrt_flush_stdout() {
    let _ = std::io::stdout().flush();
}
#[no_mangle]
pub extern "C" fn unrt_obj_none() -> u64 {
    unsafe { alloc(NObj { tag: 4, a: 0, b: 0 }) }
}
#[no_mangle]
pub extern "C" fn unrt_obj_int(v: i64) -> u64 {
    // Small integers are immutable and extremely common (loop indices, list
    // fills, dict keys). Return a pointer into the pre-built static array:
    // one bounds check and a multiply-add, no synchronization or allocation.
    if (0..SMALL_INTS).contains(&v) {
        return std::ptr::addr_of!(SMALL_INT_OBJS[v as usize]) as u64;
    }
    unsafe { alloc(NObj { tag: 0, a: v, b: 0 }) }
}
#[no_mangle]
pub extern "C" fn unrt_obj_float(v: f64) -> u64 {
    unsafe { alloc(NObj { tag: 1, a: v.to_bits() as i64, b: 0 }) }
}
#[no_mangle]
pub extern "C" fn unrt_obj_bool(v: i64) -> u64 {
    unsafe { alloc(NObj { tag: 2, a: if v != 0 { 1 } else { 0 }, b: 0 }) }
}
#[no_mangle]
pub extern "C" fn unrt_int_from_obj(p: u64) -> i64 {
    int_of(p)
}
#[no_mangle]
pub extern "C" fn unrt_float_from_obj(p: u64) -> f64 {
    float_of(p)
}
#[no_mangle]
pub extern "C" fn unrt_bool_from_obj(p: u64) -> i64 {
    if bool_of(p) { 1 } else { 0 }
}
#[no_mangle]
pub extern "C" fn unrt_obj_eq(a: u64, b: u64) -> i64 {
    if eq_obj(a, b) { 1 } else { 0 }
}
#[no_mangle]
pub extern "C" fn unrt_obj_is_panic(p: u64) -> i64 {
    if unsafe { obj(p).tag == 10 } { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn unrt_obj_is_err(p: u64) -> i64 {
    // Panics (tag 10) are uncatchable, matching the VM's `obj::is_err`.
    if unsafe { obj(p).tag == 5 } { 1 } else { 0 }
}

/// ARC-00: one new ABI symbol for bare type-name patterns (`Pattern::Type`).
/// Mirrors the VM's `is_obj` for the runtime types NRT can represent: scalar
/// tags map to their primitive type names, `any`/`obj` accept every value, and
/// tag-9 instances match their registered class name. Inheritance chains and a
/// custom `__is__` have no NRT representation; programs depending on those
/// should stay on the `un build --embed` path.
#[no_mangle]
pub extern "C" fn unrt_type_is(value: u64, name_ptr: *const u8, name_len: usize) -> i64 {
    if value == 0 {
        return 0;
    }
    let name = if name_ptr.is_null() || name_len == 0 {
        ""
    } else {
        let bytes = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
        std::str::from_utf8(bytes).unwrap_or("")
    };
    if name == "any" || name == "obj" {
        return 1;
    }
    let tag = unsafe { obj(value).tag };
    let matched = match name {
        "int" => tag == 0,
        "float" => tag == 1,
        "bool" => tag == 2,
        "list" => tag == 3,
        "none" => tag == 4,
        "str" => tag == 6,
        "dict" => tag == 7,
        "func" => tag == 8,
        "bytes" => tag == 13,
        "tuple" => tag == 14,
        "set" => tag == 15,
        // NRT has no first-class iterator object.
        "iter" => false,
        _ => {
            if tag == 9 {
                crate::class::class_name(unsafe { obj(value).a as usize }) == Some(name)
            } else {
                false
            }
        }
    };
    if matched { 1 } else { 0 }
}
#[no_mangle]
pub extern "C" fn unrt_take_error() -> u64 {
    crate::trace::nrt_clear_trace();
    crate::trace::clear_pending_site();
    PENDING.swap(0, Ordering::Relaxed)
}
#[no_mangle]
pub extern "C" fn unrt_write_error(e: u64) -> u64 {
    PENDING.store(e, Ordering::Relaxed);
    0
}
#[no_mangle]
pub extern "C" fn unrt_dump_uncaught() -> ! {
    let e = PENDING.swap(0, Ordering::Relaxed);
    let msg = if e == 0 {
        "unknown error".to_string()
    } else {
        crate::trace::error_message(e)
    };
    let is_panic = e != 0 && unsafe { obj(e).tag == 10 };
    if is_panic {
        // Panics carry no source caret in the VM, but still print the call trace.
        crate::trace::clear_pending_site();
    }
    crate::trace::traceback(&msg);
    std::process::exit(1)
}
#[no_mangle]
pub extern "C" fn unrt_make_int_div_zero() -> u64 {
    err("int division by zero")
}
#[no_mangle]
pub extern "C" fn unrt_make_mod_zero_panic() -> u64 {
    panic_err("Attempted to divide by zero.")
}
#[no_mangle]
pub extern "C" fn unrt_make_math_expected_pos() -> u64 {
    err("expected value greater than 0")
}
#[no_mangle]
pub extern "C" fn unrt_make_math_sqrt_neg() -> u64 {
    err("cannot compute sqrt of a negative number")
}

/// Replica of `collections::tup_get_item`: negative indices wrap once, then
/// out-of-range is the VM's exact error.
pub(crate) fn tuple_get(tup: u64, idx: i64) -> u64 {
    unsafe {
        if obj(tup).tag != TAG_TUP { return err("tuple get expects a tuple"); }
        let items = seq_items(tup);
        let i = if idx < 0 { items.len() as i64 + idx } else { idx };
        if i >= 0 && (i as usize) < items.len() { items[i as usize] }
        else { err("tuple index out of range") }
    }
}

/// Replica of `collections::set_get_item`: returns the stored key handle or
/// the VM's `"key {k} not found in set"` error.
pub(crate) fn set_get(set: u64, key: u64) -> u64 {
    unsafe {
        if obj(set).tag != TAG_SET { return err("set get expects a set"); }
        match seq_items(set).iter().find(|p| set_key_eq(**p, key)) {
            Some(found) => *found,
            None => err(&format!("key {} not found in set", display(key))),
        }
    }
}

/// Minimal existing-ABI surface for tuple/set literals. Reached through
/// `unrt_call_native` — **no new `unrt_*` C symbol** is introduced:
///
///   ("tuple", "new", [list])        -> tuple handle (tag 14)
///   ("tuple", "get", [tuple, int])  -> element handle / error
///   ("tuple", "len", [tuple])       -> int handle
///   ("set",   "new", [list])        -> set handle (tag 15, deduped)
///   ("set",   "get", [set, key])    -> stored key handle / error
///   ("set",   "len", [set])         -> int handle
///
/// `new` takes an ordinary list handle (tag 3) produced by the existing
/// `unrt_list_from_elems` / `unrt_list_new` + `unrt_list_push` ABI, so CMP-05
/// can lower a tuple/set literal without any extra extern declaration.
pub(crate) fn collections_dispatch(module: &str, member: &str, args: &[u64]) -> Option<u64> {
    let list_elems = |p: u64| -> Option<Vec<u64>> {
        if unsafe { obj(p).tag } == 3 {
            Some(unsafe { crate::list::items(p) }.clone())
        } else {
            None
        }
    };
    let seq_len_of = |tag: i64, p: u64| -> Option<u64> {
        if unsafe { obj(p).tag } == tag {
            Some(unrt_obj_int(unsafe { seq_len(p) }))
        } else {
            None
        }
    };
    let as_index = |p: u64| -> Option<i64> {
        unsafe { if obj(p).tag == 0 { Some(obj(p).a) } else { None } }
    };
    match (module, member, args) {
        ("tuple", "new", [src]) => Some(match list_elems(*src) {
            Some(items) => tuple_new(items),
            None => err("tuple new expects a list"),
        }),
        ("tuple", "get", [tup, idx]) => Some(match as_index(*idx) {
            Some(i) if unsafe { obj(*tup).tag } == TAG_TUP => tuple_get(*tup, i),
            Some(_) => err("tuple get expects a tuple"),
            None => err("tuple index must be int"),
        }),
        ("tuple", "len", [tup]) => Some(match seq_len_of(TAG_TUP, *tup) {
            Some(n) => n,
            None => err("tuple len expects a tuple"),
        }),
        ("set", "new", [src]) => Some(match list_elems(*src) {
            Some(items) => set_new(items),
            None => err("set new expects a list"),
        }),
        ("set", "get", [set, key]) => Some(if unsafe { obj(*set).tag } == TAG_SET {
            set_get(*set, *key)
        } else {
            err("set get expects a set")
        }),
        ("set", "len", [set]) => Some(match seq_len_of(TAG_SET, *set) {
            Some(n) => n,
            None => err("set len expects a set"),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        display, fmt_double, obj, set_new, tuple_new, unrt_obj_float, unrt_obj_int, TAG_SET,
        TAG_TUP,
    };

    fn case(v: f64, expected: &str) {
        assert_eq!(fmt_double(v), expected, "{v}");
    }

    /// Mirror of `rust/src/runtime/prims.rs::tests::double_display_matches_reference`.
    /// If the VM's expected output changes, update both tests and both copies of
    /// `fmt_double` together — plain builds must stay byte-identical with `un run`.
    #[test]
    fn double_display_matches_vm_reference() {
        case(1.0, "1");
        case(1.5, "1.5");
        case(0.0, "0");
        case(-0.0, "-0");
        case(3.0, "3");
        case(1e14, "100000000000000");
        case(1e15, "1000000000000000");
        case(1e16, "10000000000000000");
        case(9.9e15, "9900000000000000");
        case(1.2345678901234567e16, "12345678901234568");
        case(1e17, "1E+17");
        case(1.2345678901234568e20, "1.2345678901234568E+20");
        case(1e-4, "0.0001");
        case(9.9e-5, "9.9E-05");
        case(1e-5, "1E-05");
        case(1.5e-10, "1.5E-10");
        case(1.0000000000000002, "1.0000000000000002");
        case(f64::INFINITY, "∞");
        case(f64::NEG_INFINITY, "-∞");
        case(f64::NAN, "NaN");
    }

    /// Mirrors `collections::tup_to_str` / `set_to_str` and the VM tests at
    /// `rust/src/runtime/collections/tests.rs:140,248` and
    /// `rust/src/runtime/iters/tests.rs:310` (Str elements are unquoted).
    #[test]
    fn tuple_set_display_matches_vm() {
        let ints = |v: &[i64]| v.iter().map(|n| unrt_obj_int(*n)).collect::<Vec<_>>();
        assert_eq!(display(tuple_new(ints(&[1, 2, 3]))), "(1, 2, 3)");
        assert_eq!(display(set_new(ints(&[1, 2, 3]))), "{1, 2, 3}");
        assert_eq!(
            display(tuple_new(vec![crate::strings::str_new("a".into()), unrt_obj_int(1)])),
            "(a, 1)",
        );
        assert_eq!(display(set_new(vec![crate::strings::str_new("a".into())])), "{a}");
        // KeyMap::try_insert semantics: first occurrence wins, order preserved.
        assert_eq!(display(set_new(ints(&[3, 1, 3, 2]))), "{3, 1, 2}");
        // VM `key_eq`: unrelated types stay distinct, numeric types compare
        // across Int/Float (`obj.rs:304` + `base_binop` fallback).
        assert_eq!(
            display(set_new(vec![unrt_obj_int(1), crate::strings::str_new("1".into())])),
            "{1, 1}",
        );
        assert_eq!(display(set_new(vec![unrt_obj_int(1), unrt_obj_float(1.0)])), "{1}");
        // VM prints a singleton tuple without a trailing comma.
        assert_eq!(display(tuple_new(vec![])), "()");
        assert_eq!(display(tuple_new(ints(&[7]))), "(7)");
        assert_eq!(display(set_new(vec![])), "{}");
        let inner = tuple_new(ints(&[1, 2]));
        assert_eq!(display(tuple_new(vec![inner])), "((1, 2))");
    }

    #[test]
    fn tuple_set_dispatch_via_existing_abi() {
        let src = crate::list::list_new(
            [1, 2, 3].iter().map(|n| unrt_obj_int(*n)).collect::<Vec<_>>(),
        );
        let tup = crate::native::unrt_call_native(
            b"tuple\0".as_ptr(), 5, b"new\0".as_ptr(), 3, [src].as_ptr(), 1,
        );
        assert_eq!(unsafe { obj(tup).tag }, TAG_TUP);
        assert_eq!(display(tup), "(1, 2, 3)");
        let set = crate::native::unrt_call_native(
            b"set\0".as_ptr(), 3, b"new\0".as_ptr(), 3, [src].as_ptr(), 1,
        );
        assert_eq!(unsafe { obj(set).tag }, TAG_SET);
        assert_eq!(display(set), "{1, 2, 3}");

        let got = crate::native::unrt_call_native(
            b"tuple\0".as_ptr(), 5, b"get\0".as_ptr(), 3,
            [tup, unrt_obj_int(-1)].as_ptr(), 2,
        );
        assert_eq!(display(got), "3");
        let got = crate::native::unrt_call_native(
            b"tuple\0".as_ptr(), 5, b"len\0".as_ptr(), 3, [tup].as_ptr(), 1,
        );
        assert_eq!(display(got), "3");
        let got = crate::native::unrt_call_native(
            b"set\0".as_ptr(), 3, b"len\0".as_ptr(), 3, [set].as_ptr(), 1,
        );
        assert_eq!(display(got), "3");
        let got = crate::native::unrt_call_native(
            b"set\0".as_ptr(), 3, b"get\0".as_ptr(), 3,
            [set, unrt_obj_int(2)].as_ptr(), 2,
        );
        assert_eq!(display(got), "2");
    }
}
