//! Core object model and scalar/error bridges.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct NObj {
    pub(crate) tag: i64, // 0 Int, 1 Float, 2 Bool, 3 List, 4 None, 5 Error, 6 Str, 7 Dict
    pub(crate) a: i64,
    pub(crate) b: i64,
}

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

#[cfg(test)]
mod tests {
    use super::fmt_double;

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
}
