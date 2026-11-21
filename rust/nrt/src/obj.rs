//! Core object model and scalar/error bridges.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

#[repr(C)]
pub(crate) struct NObj {
    pub(crate) tag: i64, // 0 Int, 1 Float, 2 Bool, 3 List, 4 None, 5 Error, 6 Str, 7 Dict
    pub(crate) a: i64,
    pub(crate) b: i64,
}

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
            _ => "value".into(),
        }
    }
}
/// Build an error object with an owned message. Callers may pass `format!`
/// results; leaking the string is safe because the process is short-lived.
/// Match the VM's `runtime::prims::fmt_double` exactly.
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
