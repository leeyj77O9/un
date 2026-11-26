//! Native-module dispatch. Reimplements the `unrt_call_native` bridge without
//! the VM: each module/member pair receives boxed argument handles and returns
//! a boxed handle, matching `un run` byte-for-byte for the supported surface.

use std::cell::RefCell;
use std::fs;
use std::io::Write;

use crate::obj::{
    display, err, obj, obj_mut, unrt_obj_bool, unrt_obj_float, unrt_obj_int, unrt_obj_none, NObj,
};

/// Format shared by `sys.version()` and `os.version()`. The VM builds these
/// with the same pieces (`rust/src/native/sys.rs` and `native/os.rs`). Both
/// crates currently declare version 0.1.0, so plain-build output stays
/// byte-identical with `un run`; keep the two Cargo.toml versions in lockstep.
const UN_VERSION: &str = concat!("Un ", env!("CARGO_PKG_VERSION"));

// Plain builds are shipped for x86_64-pc-windows-gnu today. These constants
// name the values the Windows branches already produced; using them does not
// change any non-Windows branch. Widening platform coverage is a separate
// decision (see the NRT-06 report) because it would change non-Windows output.
const OS_NAME_WINDOWS: &str = "windows";
const ARCH_X64: &str = "X64";
const SEP_WINDOWS: &str = "\\";
const PATHSEP_WINDOWS: &str = ";";
const LINESEP_WINDOWS: &str = "\r\n";

fn as_i64(p: u64) -> Option<i64> {
    unsafe { if obj(p).tag == 0 { Some(obj(p).a) } else { None } }
}
fn as_f64(p: u64) -> Option<f64> {
    unsafe {
        match obj(p).tag {
            0 => Some(obj(p).a as f64),
            1 => Some(f64::from_bits(obj(p).a as u64)),
            _ => None,
        }
    }
}
fn as_number(p: u64) -> Option<f64> {
    as_f64(p)
}
fn as_str(p: u64) -> Option<String> {
    unsafe { if obj(p).tag == 6 { Some(crate::strings::str_of(p).clone()) } else { None } }
}
fn int(v: i64) -> u64 {
    unrt_obj_int(v)
}
fn float(v: f64) -> u64 {
    unrt_obj_float(v)
}
fn boolean(v: bool) -> u64 {
    unrt_obj_bool(v as i64)
}
fn string(v: &str) -> u64 {
    crate::strings::str_new(v.to_string())
}
fn none() -> u64 {
    unrt_obj_none()
}
fn list(v: Vec<u64>) -> u64 {
    crate::list::list_new(v)
}
fn str_arg(args: &[u64], i: usize) -> Result<String, u64> {
    args.get(i).and_then(|p| as_str(*p)).ok_or_else(|| err("expected str"))
}
fn int_arg(args: &[u64], i: usize) -> Result<i64, u64> {
    args.get(i).and_then(|p| as_i64(*p)).ok_or_else(|| err("expected int"))
}
fn num_arg(args: &[u64], i: usize) -> Result<f64, u64> {
    args.get(i).and_then(|p| as_number(*p)).ok_or_else(|| err("expected number"))
}

// A `type(x)` result is represented as tag-6 Str whose text is the VM's
// `<type: NAME>` rendering, with `b` carrying a private marker so `repr()` and
// a nested `type()` can tell it apart from an ordinary user string. This adds
// no `unrt_*`/`nrt_*` symbol and keeps the existing `unrt_call_native` ABI.
const TYPE_OBJ_MARKER: i64 = i64::MIN; // ordinary Str objects always keep `b == 0`

fn type_obj(name: &str) -> u64 {
    let p = crate::strings::str_new(format!("<type: {name}>"));
    unsafe { obj_mut(p).b = TYPE_OBJ_MARKER };
    p
}

fn is_type_obj(p: u64) -> bool {
    unsafe { obj(p).tag == 6 && obj(p).b == TYPE_OBJ_MARKER }
}

/// W-2026-11-26-FUTURETYPE: recognise the closure handle that ASYNCDEFER's
/// `tir::synthesize_go` produces for `go <expr>`.
///
/// `go` lowers to a zero-argument closure whose first two captures are the
/// memo cells created at fork time:
///   `captures[0]` — `flag: list[int]`, exactly `[0]` (pending) or `[1]` (done)
///   `captures[1]` — `value: list[any]`, exactly one boxed result handle
/// with registered kinds `[obj, obj, ...]` and return kind `obj`. Ordinary
/// lambdas/functions never carry this capture/kind/return shape, so they keep
/// the `"func"` type name.
///
/// NRT has no dedicated future tag (adding one would change the C ABI), so the
/// probe reads the private `ClosureData` behind `NObj.b`. The raw layout is
/// cross-checked against the public `unrt_arg_count` bridge: if the layout
/// assumption ever breaks the probe returns false and the value falls back to
/// `"func"` rather than mislabelling a lambda.
fn go_future_of(p: u64) -> bool {
    unsafe {
        if obj(p).tag != 8 {
            return false;
        }
        // ClosureData { captures: Vec<u64>, kinds: Vec<u8>, ret_kind: u8 }.
        // On the pinned GNU toolchain each Vec is three usize words
        // `[cap, ptr, len]` and the struct keeps declaration order. The
        // unit tests below construct a real closure to pin this down.
        let words = obj(p).b as *const usize;
        let cap_c = *words;
        let ptr_c = *words.add(1) as *const u64;
        let len_c = *words.add(2);
        let cap_k = *words.add(3);
        let ptr_k = *words.add(4) as *const u8;
        let len_k = *words.add(5);
        let ret_kind = *words.add(6) as u8;
        if ptr_c.is_null() || ptr_k.is_null()
            || len_c > cap_c || len_k > cap_k
            || cap_c > 4096 || cap_k > 4096
        {
            return false;
        }
        // Cross-check the raw layout against the real ClosureData.
        let arg_count = crate::closure::unrt_arg_count(p);
        if arg_count != (len_k as i64).saturating_sub(len_c as i64) {
            return false;
        }
        if len_c < 2 || len_c != len_k || ret_kind != 3 {
            return false;
        }
        let kinds = std::slice::from_raw_parts(ptr_k, len_k);
        if kinds[0] != 3 || kinds[1] != 3 {
            return false;
        }
        let captures = std::slice::from_raw_parts(ptr_c, len_c);
        let flag = captures[0];
        let value = captures[1];
        // Both memo cells are one-element lists at `go` time and stay so
        // after `wait` (the thunk overwrites element 0 in place).
        let is_cell = |cell: u64| obj(cell).tag == 3 && obj(cell).a == 1;
        if !is_cell(flag) || !is_cell(value) {
            return false;
        }
        let f = crate::list::items(flag)[0];
        obj(f).tag == 0 && (obj(f).a == 0 || obj(f).a == 1)
    }
}

/// VM `obj::type_name` for the NRT-representable tags. The names are the same
/// ones `unrt_obj_type_name` matches, so `type()` and `repr()` stay aligned
/// with class/variant pattern checks and with `un run` output.
fn type_name_of(p: u64) -> &'static str {
    let o = unsafe { obj(p) };
    match o.tag {
        0 => "int",
        1 => "float",
        2 => "bool",
        3 | 13 => {
            // D-098: a byte-string literal still carries a List<Int> payload,
            // so `type(b"...")` is `list` in both VM and plain build.
            if o.tag == 3 { "list" } else { "bytes" }
        }
        4 => "none",
        // `obj::err` / `obj::panic` both attach type_name "obj" in the VM.
        5 | 10 => "obj",
        6 => if is_type_obj(p) { "type" } else { "str" },
        7 => "dict",
        // W-2026-11-26-FUTURETYPE: a `go` future is a closure handle, but the
        // VM gives it type_name "future" (native/flow.rs::future_of). Normal
        // lambda/function values remain "func".
        8 => if go_future_of(p) { "future" } else { "func" },
        9 => crate::class::class_name(o.a as usize).unwrap_or("obj"),
        11 => "http_client",
        12 => "web_socket",
        14 => "tuple",
        15 => "set",
        // Cooperative flow handles; names match the VM's native object types.
        16 => flow_type_name(p).unwrap_or("obj"),
        _ => "obj",
    }
}

fn is_pending(p: u64) -> bool {
    unsafe { matches!(obj(p).tag, 5 | 10) }
}

fn regex_compile(pattern: &str) -> Result<crate::regex::Regex, u64> {
    crate::regex::Regex::new(pattern).map_err(|m| err(&format!("invalid regex pattern: {m}")))
}

fn fs_path(s: &str) -> std::path::PathBuf {
    std::path::Path::new(s).to_path_buf()
}

fn platform_path(s: &str) -> String {
    if cfg!(windows) { s.replace('/', "\\") } else { s.to_string() }
}

/// ISO-8601 timestamp matching the VM's `runtime::date` formatting.
fn iso_time(time: std::time::SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    const EPOCH_TICKS: i128 = 621_355_968_000_000_000i128;
    const TICKS_PER_DAY: i128 = 864_000_000_000;
    let ticks = match time.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128 / 100 + EPOCH_TICKS,
        Err(d) => EPOCH_TICKS - d.duration().as_nanos() as i128 / 100,
    };
    // Howard Hinnant's civil_from_days algorithm.
    let days = ticks.div_euclid(TICKS_PER_DAY);
    let rem = ticks.rem_euclid(TICKS_PER_DAY);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    let hh = rem / 36_000_000_000;
    let mm = (rem / 600_000_000) % 60;
    let ss = (rem / 10_000_000) % 60;
    let ms = (rem / 10_000) % 1000;
    format!("{year:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{ms:03}Z")
}

// ---- cooperative `flow` shim (W-2026-11-26-ASYNCflow) ---------------------
//
// D-012/D-054/D-061: Un never creates OS threads. `flow.lock`, `flow.spawn`
// and `flow.atomic` are cooperative single-thread handles backed by RefCell
// cells; `pool.map`/`pool.submit` invoke the supplied closure inline on the
// calling thread. `rust/src/native/flow.rs` and `native/atomic.rs` are the
// semantic reference — error strings and return values below mirror them.
//
// Everything is reached through the existing `unrt_call_native` bridge, so no
// new `unrt_*`/`nrt_*` C ABI symbol is introduced (AGENTS.md §4 / §5.8).

const TAG_FLOW: i64 = 16;
const FLOW_LOCK: i64 = 0;
const FLOW_POOL: i64 = 1;
const FLOW_FUTURE: i64 = 2;
const FLOW_ATOMIC_INT: i64 = 3;
const FLOW_ATOMIC_BOOL: i64 = 4;

// The VM's `PoolState` also stores a worker count; NRT is cooperative
// single-thread (D-054), so only the `closed` flag has observable behaviour.
struct FlowPool {
    closed: bool,
}

enum FlowState {
    Lock(RefCell<bool>),
    Pool(RefCell<FlowPool>),
    Future(RefCell<Option<u64>>),
    AtomicInt(RefCell<i64>),
    AtomicBool(RefCell<bool>),
}

fn flow_new(kind: i64, state: FlowState) -> u64 {
    let ptr = Box::into_raw(Box::new(state));
    unsafe { crate::obj::alloc(NObj { tag: TAG_FLOW, a: kind, b: ptr as i64 }) }
}

fn flow_kind(p: u64) -> Option<i64> {
    unsafe {
        let o = obj(p);
        if o.tag == TAG_FLOW { Some(o.a) } else { None }
    }
}

fn flow_type_name(p: u64) -> Option<&'static str> {
    match flow_kind(p) {
        Some(FLOW_LOCK) => Some("lock"),
        Some(FLOW_POOL) => Some("pool"),
        Some(FLOW_FUTURE) => Some("future"),
        Some(FLOW_ATOMIC_INT) => Some("atomic_int"),
        Some(FLOW_ATOMIC_BOOL) => Some("atomic_bool"),
        _ => None,
    }
}

/// The VM's rest argument arrives either as a tuple handle (`vargs`) or as
/// plain trailing handles. Flatten both shapes before invoking the closure.
fn flow_values(args: &[u64]) -> Vec<u64> {
    if args.len() == 1 && unsafe { obj(args[0]).tag == 14 } {
        unsafe { crate::obj::seq_items(args[0]).clone() }
    } else {
        args.to_vec()
    }
}

fn flow_call(f: u64, values: &[u64]) -> u64 {
    crate::closure::unrt_call_obj(f, values.as_ptr(), values.len())
}

fn atomic_int_arg(args: &[u64], i: usize, label: &str) -> Result<i64, u64> {
    args.get(i).copied().and_then(as_i64)
        .ok_or_else(|| err(&format!("expected '{label}' argument to be of type 'int'")))
}

fn atomic_bool_arg(args: &[u64], i: usize) -> Result<bool, u64> {
    match args.get(i).copied() {
        Some(p) if unsafe { obj(p).tag == 2 } => Ok(unsafe { obj(p).a != 0 }),
        _ => Err(err("expected 'value' argument to be of type 'bool'")),
    }
}

fn flow_atomic_int(state: &FlowState, member: &str, args: &[u64]) -> Option<u64> {
    let FlowState::AtomicInt(cell) = state else { return None; };
    let mut value = cell.borrow_mut();
    match member {
        "get" => Some(int(*value)),
        "set" => match atomic_int_arg(args, 0, "value") {
            Ok(a) => { *value = a; Some(none()) }
            Err(e) => Some(e),
        },
        "add" => match atomic_int_arg(args, 0, "value") {
            Ok(a) => { *value = value.wrapping_add(a); Some(int(*value)) }
            Err(e) => Some(e),
        },
        "inc" => { *value = value.wrapping_add(1); Some(int(*value)) }
        "dec" => { *value = value.wrapping_sub(1); Some(int(*value)) }
        "exchange" => match atomic_int_arg(args, 0, "value") {
            Ok(a) => {
                let previous = *value;
                *value = a;
                Some(int(previous))
            }
            Err(e) => Some(e),
        },
        "compare_and_swap" => {
            let expected = match atomic_int_arg(args, 0, "expected") { Ok(v) => v, Err(e) => return Some(e) };
            let new_value = match atomic_int_arg(args, 1, "new_value") { Ok(v) => v, Err(e) => return Some(e) };
            if *value == expected {
                *value = new_value;
                Some(boolean(true))
            } else {
                Some(boolean(false))
            }
        }
        _ => None,
    }
}

fn flow_atomic_bool(state: &FlowState, member: &str, args: &[u64]) -> Option<u64> {
    let FlowState::AtomicBool(cell) = state else { return None; };
    let mut value = cell.borrow_mut();
    match member {
        "get" => Some(boolean(*value)),
        "set" => match atomic_bool_arg(args, 0) {
            Ok(a) => { *value = a; Some(none()) }
            Err(e) => Some(e),
        },
        "exchange" => match atomic_bool_arg(args, 0) {
            Ok(a) => {
                let previous = *value;
                *value = a;
                Some(boolean(previous))
            }
            Err(e) => Some(e),
        },
        "compare_and_swap" => {
            let expected = match atomic_bool_arg(args, 0) { Ok(v) => v, Err(e) => return Some(e) };
            let new_value = match atomic_bool_arg(args, 1) { Ok(v) => v, Err(e) => return Some(e) };
            if *value == expected {
                *value = new_value;
                Some(boolean(true))
            } else {
                Some(boolean(false))
            }
        }
        _ => None,
    }
}

/// Dispatch a method on a `flow` handle. `None` means the handle/member is not
/// part of this shim (the caller then reports the existing unsupported-module
/// error); failures that the VM surfaces as uncaught `Obj` errors are returned
/// as `Some(err(..))`.
pub(crate) fn flow_method(handle: u64, member: &str, args: &[u64]) -> Option<u64> {
    let (kind, ptr) = unsafe {
        let o = obj(handle);
        if o.tag != TAG_FLOW { return None; }
        (o.a, o.b as *const FlowState)
    };
    let state = unsafe { &*ptr };
    match (kind, member) {
        (FLOW_LOCK, "acquire") => match state {
            FlowState::Lock(held) => { *held.borrow_mut() = true; Some(handle) }
            _ => None,
        },
        (FLOW_LOCK, "try_acquire") => match state {
            FlowState::Lock(held) => {
                let mut h = held.borrow_mut();
                if *h { Some(boolean(false)) } else { *h = true; Some(boolean(true)) }
            }
            _ => None,
        },
        (FLOW_LOCK, "release") => match state {
            FlowState::Lock(held) => {
                let mut h = held.borrow_mut();
                if !*h { Some(err("lock not held")) } else { *h = false; Some(handle) }
            }
            _ => None,
        },
        (FLOW_LOCK, "dispose") => match state {
            FlowState::Lock(held) => { *held.borrow_mut() = false; Some(none()) }
            _ => None,
        },
        (FLOW_POOL, "submit") | (FLOW_POOL, "map") => match state {
            FlowState::Pool(pool) => {
                if pool.borrow().closed { return Some(err("pool is closed")); }
                let Some(&f) = args.first() else {
                    return Some(err("expected 'fn' argument to be of type 'func'"));
                };
                if crate::closure::unrt_is_callable(f) == 0 {
                    return Some(err("expected 'fn' argument to be of type 'func'"));
                }
                let values = flow_values(&args[1..]);
                if member == "submit" {
                    let v = flow_call(f, &values);
                    if crate::obj::unrt_obj_is_err(v) != 0 { return Some(v); }
                    Some(flow_new(FLOW_FUTURE, FlowState::Future(RefCell::new(Some(v)))))
                } else {
                    let mut out = Vec::with_capacity(values.len());
                    for value in values {
                        let call_args = if unsafe { obj(value).tag == 14 } {
                            unsafe { crate::obj::seq_items(value).clone() }
                        } else { vec![value] };
                        let r = flow_call(f, &call_args);
                        if crate::obj::unrt_obj_is_err(r) != 0 { return Some(r); }
                        out.push(r);
                    }
                    Some(list(out))
                }
            }
            _ => None,
        },
        (FLOW_POOL, "close") => match state {
            FlowState::Pool(pool) => { pool.borrow_mut().closed = true; Some(none()) }
            _ => None,
        },
        (FLOW_FUTURE, "value") | (FLOW_FUTURE, "wait") => match state {
            FlowState::Future(cell) => Some(cell.borrow().clone().unwrap_or_else(none)),
            _ => None,
        },
        (FLOW_ATOMIC_INT, _) => flow_atomic_int(state, member, args),
        (FLOW_ATOMIC_BOOL, _) => flow_atomic_bool(state, member, args),
        _ => None,
    }
}

/// Module-level constructors plus method-shaped calls routed through the
/// existing `unrt_call_native` bridge (e.g. `flow.lock.try_acquire(handle)`).
fn flow_module(m: &str, name: &str, args: &[u64]) -> Option<u64> {
    match (m, name) {
        ("flow", "lock") => Some(flow_new(FLOW_LOCK, FlowState::Lock(RefCell::new(false)))),
        ("flow", "spawn") => {
            // Validate the worker-count argument like the VM, but do not spawn
            // anything: D-054 requires cooperative execution on this thread.
            match args.first().copied() {
                None => {}
                Some(p) if as_i64(p).is_some() => {}
                Some(_) => return Some(err("argument 'worker' must be of type int")),
            }
            Some(flow_new(FLOW_POOL, FlowState::Pool(RefCell::new(FlowPool { closed: false }))))
        }
        ("flow", "atomic") => {
            match args.first().copied() {
                None => Some(flow_new(FLOW_ATOMIC_INT, FlowState::AtomicInt(RefCell::new(0)))),
                Some(p) if unsafe { obj(p).tag == 2 } => Some(flow_new(
                    FLOW_ATOMIC_BOOL,
                    FlowState::AtomicBool(RefCell::new(unsafe { obj(p).a != 0 })),
                )),
                Some(p) => match as_i64(p) {
                    Some(v) => Some(flow_new(FLOW_ATOMIC_INT, FlowState::AtomicInt(RefCell::new(v)))),
                    None => Some(err("expected 'initial' argument to be of type 'int'")),
                },
            }
        }
        ("atomic", "int") | ("atomic", "bool") => {
            let want_bool = name == "bool";
            match args.first().copied() {
                None => Some(flow_new(
                    if want_bool { FLOW_ATOMIC_BOOL } else { FLOW_ATOMIC_INT },
                    if want_bool { FlowState::AtomicBool(RefCell::new(false)) }
                    else { FlowState::AtomicInt(RefCell::new(0)) },
                )),
                Some(p) if want_bool && unsafe { obj(p).tag == 2 } => Some(flow_new(
                    FLOW_ATOMIC_BOOL,
                    FlowState::AtomicBool(RefCell::new(unsafe { obj(p).a != 0 })),
                )),
                Some(p) => match as_i64(p) {
                    Some(v) => Some(flow_new(FLOW_ATOMIC_INT, FlowState::AtomicInt(RefCell::new(v)))),
                    None => Some(err("expected 'initial' argument to be of type 'int'")),
                },
            }
        }
        // Method-shaped call: first argument is the receiver handle; an
        // optional "<kind>." prefix is accepted for self-describing dispatch.
        ("flow" | "atomic", _) => {
            let handle = *args.first()?;
            if flow_kind(handle).is_none() { return None; }
            let member = name.split_once('.').map(|(_, rest)| rest).unwrap_or(name);
            flow_method(handle, member, &args[1..])
        }
        _ => None,
    }
}

fn dispatch(m: &str, name: &str, args: &[u64]) -> u64 {
    if let Some(result) = flow_module(m, name, args) {
        return result;
    }
    if let Some(result) = crate::bytes_native::dispatch(m, name, args) {
        return result;
    }
    if let Some(result) = crate::net_native::dispatch(m, name, args) {
        return result;
    }
    // List slice/clear reuse the existing `unrt_call_native` ABI; no new C ABI
    // symbol is introduced (ARC-00 approval is required for new `unrt_*` names).
    if let Some(result) = crate::list::dispatch(m, name, args) {
        return result;
    }
    // Tuple/Set literal construction and lookup reuse `unrt_call_native` as
    // well: no new `unrt_*` C symbol (see `obj::collections_dispatch`).
    if let Some(result) = crate::obj::collections_dispatch(m, name, args) {
        return result;
    }
    match (m, name) {
        ("io", "write") => {
            let parts: Vec<String> = args.iter().map(|p| {
                let tag = unsafe { obj(*p).tag };
                if tag == 5 || tag == 10 { format!("error: {}", display(*p)) } else { display(*p) }
            }).collect();
            let line = format!("{}\n", parts.join(" "));
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(line.as_bytes());
            none()
        }
        ("io", "read") => {
            let mut line = String::new();
            match std::io::stdin().read_line(&mut line) {
                Ok(_) => string(line.trim_end_matches(['\r', '\n'])),
                Err(e) => err(&e.to_string()),
            }
        }

        ("math", "sign") => match num_arg(args, 0) {
            Ok(v) => int(if v.is_nan() || v == 0.0 { 0 } else if v > 0.0 { 1 } else { -1 }),
            Err(e) => e,
        },
        ("math", "isqrt") => match int_arg(args, 0) {
            Ok(v) if v >= 0 => int(v.isqrt()),
            Ok(_) => err("cannot compute isqrt of a negative number"),
            Err(e) => e,
        },
        ("math", "is_nan") => match num_arg(args, 0) {
            Ok(v) => boolean(v.is_nan()),
            Err(e) => e,
        },
        ("math", "is_infinite") => match num_arg(args, 0) {
            Ok(v) => boolean(v.is_infinite()),
            Err(e) => e,
        },
        ("math", "is_finite") => match num_arg(args, 0) {
            Ok(v) => boolean(v.is_finite()),
            Err(e) => e,
        },
        ("math", "pi") => float(std::f64::consts::PI),
        ("math", "e") => float(std::f64::consts::E),
        ("math", "tau") => float(std::f64::consts::TAU),
        ("math", "abs") => match args.first().copied() {
            Some(v) if unsafe { obj(v).tag == 0 } => {
                let i = unsafe { obj(v).a };
                if i == i64::MIN { float(9223372036854775808.0) } else { int(i.unsigned_abs() as i64) }
            }
            _ => match num_arg(args, 0) { Ok(v) => float(v.abs()), Err(e) => e },
        },
        ("math", "round") => match num_arg(args, 0) {
            Ok(v) => {
                let r = v.round_ties_even();
                if r.fract() == 0.0 && r >= i64::MIN as f64 && r <= i64::MAX as f64 {
                    int(r as i64)
                } else {
                    float(r)
                }
            }
            Err(e) => e,
        },
        ("math", "sqrt") => match num_arg(args, 0) {
            Ok(v) if v < 0.0 => err("cannot compute sqrt of a negative number"),
            Ok(v) => float(v.sqrt()),
            Err(e) => e,
        },
        ("math", "pow") | ("math", "atan2") | ("math", "hypot") | ("math", "log") => {
            if name == "log" {
                match num_arg(args, 0) {
                    Ok(v) if v <= 0.0 => return err("expected value greater than 0"),
                    _ => {}
                }
            }
            match (num_arg(args, 0), num_arg(args, 1)) {
                (Ok(a), Ok(b)) => {
                    let r = match name {
                        "pow" => a.powf(b),
                        "atan2" => a.atan2(b),
                        "hypot" => a.hypot(b),
                        _ => a.ln() / b.ln(),
                    };
                    if name == "pow" && args.len() >= 2
                        && unsafe { obj(args[0]).tag == 0 && obj(args[1]).tag == 0 }
                        && r.fract() == 0.0 && r >= i64::MIN as f64 && r <= i64::MAX as f64
                    {
                        int(r as i64)
                    } else { float(r) }
                }
                (Err(e), _) | (_, Err(e)) => e,
            }
        }
        ("math", "gcd") | ("math", "lcm") => match (int_arg(args, 0), int_arg(args, 1)) {
            (Ok(a), Ok(b)) => {
                let mut x = a.unsigned_abs();
                let mut y = b.unsigned_abs();
                if name == "gcd" {
                    while y != 0 { let t = x % y; x = y; y = t; }
                    int(x as i64)
                } else {
                    let g = { let (mut p, mut q) = (x, y); while q != 0 { let t = p % q; p = q; q = t; } p };
                    int((x / g * y) as i64)
                }
            }
            (Err(e), _) | (_, Err(e)) => e,
        },
        ("math", "clamp") => match (num_arg(args, 0), num_arg(args, 1), num_arg(args, 2)) {
            (Ok(v), Ok(lo), Ok(hi)) => float(v.clamp(lo, hi)),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => e,
        },

        ("iter", "map") | ("iter", "filter") => {
            if args.len() == 2 && crate::closure::unrt_is_callable(args[0]) == 1
                && unsafe { obj(args[1]).tag == 3 }
            {
                let f = args[0];
                let items = unsafe { crate::list::items(args[1]).clone() };
                let mut out = Vec::new();
                for item in items {
                    let r = crate::closure::unrt_call_obj(f, &item, 1);
                    if crate::obj::unrt_obj_is_err(r) != 0 {
                        return r;
                    }
                    if name == "map" {
                        out.push(r);
                    } else if crate::obj::unrt_bool_from_obj(r) != 0 {
                        out.push(item);
                    }
                }
                return list(out);
            }
            err(&format!("native module 'iter.{name}' requires a callable and a list"))
        }
        ("iter", "min") | ("iter", "max") => {
            let items: Vec<u64> = if args.len() == 1 && unsafe { obj(args[0]).tag == 3 } {
                unsafe { crate::list::items(args[0]).clone() }
            } else {
                args.to_vec()
            };
            if items.is_empty() { return err("expected 'iterable' argument to have at least one value"); }
            let mut best = items[0];
            for item in items.iter().skip(1) {
                let ord = crate::obj::cmp_obj(best, *item);
                if (name == "min" && ord == std::cmp::Ordering::Greater)
                    || (name == "max" && ord == std::cmp::Ordering::Less) { best = *item; }
            }
            best
        }
        ("iter", "sum") => {
            let items: Vec<u64> = if args.len() == 1 && unsafe { obj(args[0]).tag == 3 } {
                unsafe { crate::list::items(args[0]).clone() }
            } else { args.to_vec() };
            let mut total = 0.0;
            let mut is_int = true;
            for p in &items {
                match as_i64(*p) { Some(v) => total += v as f64, None => { is_int = false; total += as_f64(*p).unwrap_or(0.0); } }
            }
            if is_int { int(total as i64) } else { float(total) }
        }
        ("iter", "sorted") => {
            let items: Vec<u64> = if args.len() == 1 && unsafe { obj(args[0]).tag == 3 } {
                unsafe { crate::list::items(args[0]).clone() }
            } else { args.to_vec() };
            let mut v = items;
            v.sort_by(|a, b| crate::obj::cmp_obj(*a, *b));
            list(v)
        }
        ("iter", "reverse") => {
            let items: Vec<u64> = if args.len() == 1 && unsafe { obj(args[0]).tag == 3 } {
                unsafe { crate::list::items(args[0]).clone() }
            } else { args.to_vec() };
            let mut v = items;
            v.reverse();
            list(v)
        }

        ("codes", "bin") => match args.first().copied().and_then(as_i64) {
            None => err("bin: expected int"),
            Some(v) if v < 0 => string(&format!("-0b{:b}", v.wrapping_neg())),
            Some(v) => string(&format!("0b{v:b}")),
        },
        ("codes", "oct") => match args.first().copied().and_then(as_i64) {
            None => err("oct: expected int"),
            Some(v) if v < 0 => string(&format!("-0o{:o}", v.wrapping_neg())),
            Some(v) => string(&format!("0o{v:o}")),
        },
        ("codes", "hex") => match args.first().copied().and_then(as_i64) {
            None => err("hex: expected int"),
            Some(v) if v < 0 => string(&format!("-0x{:x}", v.wrapping_neg())),
            Some(v) => string(&format!("0x{v:x}")),
        },
        ("codes", "chr") => match args.first().copied().and_then(as_i64) {
            None => err("chr: expected int code"),
            Some(v) if !(0..=0x10FFFF).contains(&v) => err("chr: code point out of range (0..1114111)"),
            Some(v) => char::from_u32(v as u32)
                .map(|c| string(&c.to_string()))
                .unwrap_or_else(|| crate::obj::panic_err("A valid UTF32 value is between 0x000000 and 0x10ffff, inclusive, and should not include surrogate codepoint values (0x00d800 ~ 0x00dfff). (Parameter 'utf32')")),
        },
        ("codes", "ord") => match args.first().copied().and_then(as_str) {
            None => err("ord: expected str"),
            Some(s) => {
                let mut it = s.chars();
                match (it.next(), it.next()) {
                    (Some(c), None) => int(c as u32 as i64),
                    (None, _) => err("ord: expected a single character, got empty string"),
                    _ => err("ord: expected a single character, got multiple characters"),
                }
            }
        },
        ("codes", "ascii") => match args.first().copied().and_then(as_str) {
            None => err("ascii: expected str"),
            Some(s) => {
                let mut it = s.encode_utf16();
                match (it.next(), it.next()) {
                    (Some(u), None) => int(u as i64),
                    _ => err("ascii: expected str"),
                }
            }
        },

        ("os", "pid") | ("sys", "pid") => int(std::process::id() as i64),
        ("os", "cpuCount") | ("sys", "cpuCount") => {
            int(std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1))
        }
        ("os", "is64bit") | ("sys", "is64bit") => boolean(cfg!(target_pointer_width = "64")),
        ("os", "name") | ("sys", "name") => string(if cfg!(windows) { OS_NAME_WINDOWS } else { "unknown" }),
        ("os", "arch") | ("sys", "arch") => string(ARCH_X64),
        ("os", "hostname") | ("sys", "hostname") => string(
            &std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default(),
        ),
        ("os", "username") | ("sys", "username") => string(
            &std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default(),
        ),
        ("os", "version") => string(&format!("{} ({UN_VERSION})", std::env::consts::OS)),
        ("sys", "version") => string(UN_VERSION),
        ("os", "tickCount") | ("sys", "tickCount") => int(
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64).unwrap_or(0),
        ),
        ("sys", "getcalldepth") | ("sys", "getrecursionlimit") => int(1000),
        ("os", "sep") | ("sys", "sep") => string(SEP_WINDOWS),
        ("os", "pathsep") | ("sys", "pathsep") => string(PATHSEP_WINDOWS),
        ("os", "linesep") | ("sys", "linesep") => string(LINESEP_WINDOWS),

        ("fs", "exists") => match str_arg(args, 0) { Ok(s) => boolean(fs_path(&s).exists()), Err(e) => e },
        ("fs", "file") => match str_arg(args, 0) { Ok(s) => boolean(fs_path(&s).is_file()), Err(e) => e },
        ("fs", "dir") => match str_arg(args, 0) { Ok(s) => boolean(fs_path(&s).is_dir()), Err(e) => e },
        ("fs", "readonly") => match str_arg(args, 0) {
            Ok(s) => match fs::metadata(&s) {
                Ok(m) => boolean(m.permissions().readonly()),
                Err(_) => err("path not found"),
            },
            Err(e) => e,
        },
        ("fs", "hidden") => match str_arg(args, 0) {
            Ok(s) => {
                if !fs_path(&s).exists() { return err("file not found"); }
                boolean(fs_path(&s).file_name()
                    .and_then(|n| n.to_str()).map(|n| n.starts_with('.')).unwrap_or(false))
            }
            Err(e) => e,
        },
        ("fs", "size") => match str_arg(args, 0) {
            Ok(s) => match fs::metadata(&s) {
                Ok(m) if m.is_file() => int(m.len() as i64),
                _ => err("file not found"),
            },
            Err(e) => e,
        },
        ("fs", "join") => {
            let mut it = args.iter();
            let mut p = match it.next().and_then(|p| as_str(*p)) { Some(s) => fs_path(&s), None => return err("fs.join: expected str") };
            for part in it {
                if let Some(s) = as_str(*part) { p.push(s); }
            }
            string(&p.to_string_lossy())
        }
        ("fs", "parent") => match str_arg(args, 0) {
            Ok(s) => string(&platform_path(&fs_path(&s).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())),
            Err(e) => e,
        },
        ("fs", "name") => match str_arg(args, 0) {
            Ok(s) => string(&platform_path(&fs_path(&s).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())),
            Err(e) => e,
        },
        ("fs", "stem") => match str_arg(args, 0) {
            Ok(s) => string(&fs_path(&s).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
            Err(e) => e,
        },
        ("fs", "ext") => match str_arg(args, 0) {
            Ok(s) => string(&fs_path(&s).extension()
                .map(|n| format!(".{}", n.to_string_lossy())).unwrap_or_default()),
            Err(e) => e,
        },
        ("fs", "cwd") => string(&std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()),
        ("fs", "temp") => string(&std::env::temp_dir().to_string_lossy()),
        ("fs", "home") => string(&std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default()),
        ("fs", "exe") => string(&std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()),
        ("fs", "app") => string(&std::env::current_exe().map(|p| p.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()).unwrap_or_default()),
        ("fs", "absolute") => match str_arg(args, 0) {
            Ok(s) => {
                let p = fs_path(&s);
                let full = if p.is_absolute() { p } else {
                    std::env::current_dir().unwrap_or_default().join(p)
                };
                string(&platform_path(&std::fs::canonicalize(&full).unwrap_or(full).to_string_lossy()))
            }
            Err(e) => e,
        },
        ("fs", "normalize") => match str_arg(args, 0) {
            Ok(s) => {
                let p = fs_path(&s);
                let full = if p.is_absolute() { p } else { std::env::current_dir().unwrap_or_default().join(p) };
                let mut parts: Vec<String> = Vec::new();
                use std::path::Component;
                for c in full.components() {
                    match c {
                        Component::ParentDir => { parts.pop(); }
                        Component::CurDir => {}
                        other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
                    }
                }
                let sep = if cfg!(windows) { "\\" } else { "/" };
                string(&parts.join(sep))
            }
            Err(e) => e,
        },
        ("fs", "relative") => {
            let a = match str_arg(args, 0) { Ok(s) => s, Err(e) => return e };
            let b = match str_arg(args, 1) { Ok(s) => s, Err(e) => return e };
            let from = std::fs::canonicalize(&a).unwrap_or_else(|_| fs_path(&a));
            let to = std::fs::canonicalize(&b).unwrap_or_else(|_| fs_path(&b));
            let rest = to.strip_prefix(&from).unwrap_or(&to);
            string(&platform_path(&rest.to_string_lossy()))
        }
        ("fs", "created") | ("fs", "modified") | ("fs", "accessed") => match str_arg(args, 0) {
            Ok(s) => {
                let mt = match fs::metadata(&s) {
                    Ok(m) => m,
                    Err(_) => return err("file not found"),
                };
                let t = if name == "created" { mt.created() }
                    else if name == "modified" { mt.modified() }
                    else { mt.modified() };
                match t {
                    Ok(time) => string(&iso_time(time)),
                    Err(_) => err("file not found"),
                }
            }
            Err(e) => e,
        },

        ("re", "escape") => match str_arg(args, 0) { Ok(s) => string(&crate::regex::escape(&s)), Err(e) => e },
        ("re", "test") | ("re", "search") | ("re", "match") => {
            match (str_arg(args, 0), str_arg(args, 1)) {
                (Ok(p), Ok(t)) => match regex_compile(&p) {
                    Ok(re) => {
                        let chars: Vec<char> = t.chars().collect();
                        match re.find_at(&chars, 0) {
                            Ok(found) => match found {
                                None => {
                                    if name == "test" { boolean(false) }
                                    else { crate::obj::unrt_obj_none() }
                                }
                                Some(m) => {
                                    if name == "test" {
                                        boolean(true)
                                    } else if name == "match" && m.start != 0 {
                                        crate::obj::unrt_obj_none()
                                    } else {
                                        string(&chars[m.start..m.end].iter().collect::<String>())
                                    }
                                }
                            },
                            Err(_) => err("regex backtracking limit exceeded"),
                        }
                    }
                    Err(e) => e,
                },
                (Err(e), _) | (_, Err(e)) => e,
            }
        }
        ("re", "find_all") => match (str_arg(args, 0), str_arg(args, 1)) {
            (Ok(p), Ok(t)) => match regex_compile(&p) {
                Ok(re) => {
                    let chars: Vec<char> = t.chars().collect();
                    match re.find_all(&chars) {
                        Ok(ms) => list(ms.iter().map(|m| string(&chars[m.start..m.end].iter().collect::<String>())).collect()),
                        Err(_) => err("regex backtracking limit exceeded"),
                    }
                }
                Err(e) => e,
            },
            (Err(e), _) | (_, Err(e)) => e,
        },
        ("re", "replace") | ("re", "replace_all") => {
            match (str_arg(args, 0), str_arg(args, 1), str_arg(args, 2)) {
                (Ok(p), Ok(t), Ok(r)) => match regex_compile(&p) {
                    Ok(re) => {
                        let chars: Vec<char> = t.chars().collect();
                        match re.find_all(&chars) {
                            Ok(ms) => {
                                let mut out = String::new();
                                let mut pos = 0;
                                let limit = if name == "replace" { 1 } else { usize::MAX };
                                for m in ms.iter().take(limit) {
                                    out.extend(&chars[pos..m.start]);
                                    out.push_str(&r);
                                    pos = m.end;
                                }
                                out.extend(&chars[pos..]);
                                string(&out)
                            }
                            Err(_) => err("regex backtracking limit exceeded"),
                        }
                    }
                    Err(e) => e,
                },
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => e,
            }
        }
        ("re", "split") => match (str_arg(args, 0), str_arg(args, 1)) {
            (Ok(p), Ok(t)) => match regex_compile(&p) {
                Ok(re) => {
                    let chars: Vec<char> = t.chars().collect();
                    match re.find_all(&chars) {
                        Ok(ms) => {
                            let mut out = Vec::new();
                            let mut pos = 0;
                            for m in &ms {
                                out.push(string(&chars[pos..m.start].iter().collect::<String>()));
                                pos = m.end;
                            }
                            out.push(string(&chars[pos..].iter().collect::<String>()));
                            list(out)
                        }
                        Err(_) => err("regex backtracking limit exceeded"),
                    }
                }
                Err(e) => e,
            },
            (Err(e), _) | (_, Err(e)) => e,
        },

        ("time", "monotonic") | ("time", "monotonic_ms") | ("time", "epoch") | ("time", "epoch_ms")
        | ("time", "now") => {
            static START: std::sync::LazyLock<std::time::Instant> = std::sync::LazyLock::new(std::time::Instant::now);
            let mono_ms = START.elapsed().as_millis() as i64;
            let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64).unwrap_or(0);
            match name {
                "monotonic" => float(START.elapsed().as_micros() as f64 / 1_000_000.0),
                "monotonic_ms" => int(mono_ms),
                "epoch_ms" => int(ms),
                _ => int(ms / 1000),
            }
        }

        // `type`/`repr`/`str` are global builtins in the VM, but the plain
        // build reaches them through the existing `types` native module (no
        // new C ABI symbol). `type()` renders like the VM's `TObj` (`<type:
        // NAME>`) and `repr()` returns the type-name string, byte-identical
        // to `un run` for every NRT-representable tag.
        ("types", "type") => match args.first().copied() {
            Some(v) if is_pending(v) => v,
            Some(v) => type_obj(type_name_of(v)),
            None => err("type: expected an argument"),
        },
        ("types", "repr") => match args.first().copied() {
            Some(v) if is_pending(v) => v,
            // VM `repr` on a function uses `fn_repr`, which plain builds do
            // not expose; keep the documented type-name contract here.
            Some(v) => string(type_name_of(v)),
            None => err("repr: expected an argument"),
        },
        ("types", "str") => match args.first().copied() {
            Some(v) if is_pending(v) => v,
            // VM refuses `str()` on native flow handles and on `go` futures
            // (`test/unit/runtime/async.un` asserts this for pool/future).
            // No new ABI is needed: the error comes from the existing bridge.
            // W-2026-11-26-FUTURETYPE: a future is a tag-8 closure, so add the
            // same guard here; ordinary funcs keep the `display` path below.
            Some(v) if flow_type_name(v).is_some() || go_future_of(v) => {
                let name = if go_future_of(v) {
                    "future"
                } else {
                    flow_type_name(v).unwrap_or("obj")
                };
                err(&format!("unsupported operand type(s) for str(): '{name}'"))
            }
            // VM `str()` delegates to `obj::to_str`, whose rendering is the
            // same `display` this runtime already uses for `io.write`.
            Some(v) => string(&display(v)),
            None => err("str: expected an argument"),
        },

        ("types", n @ ("u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64")) => {
            match int_arg(args, 0) {
                Ok(v) => {
                    let ok = match n {
                        "u8" => (0..=u8::MAX as i64).contains(&v),
                        "u16" => (0..=u16::MAX as i64).contains(&v),
                        "u32" => (0..=u32::MAX as i64).contains(&v),
                        "u64" => v >= 0,
                        "i8" => (i8::MIN as i64..=i8::MAX as i64).contains(&v),
                        "i16" => (i16::MIN as i64..=i16::MAX as i64).contains(&v),
                        "i32" => (i32::MIN as i64..=i32::MAX as i64).contains(&v),
                        _ => true,
                    };
                    if ok { int(v) } else {
                        let (min, max) = match n {
                            "u8" => (0, u8::MAX as i64), "u16" => (0, u16::MAX as i64),
                            "u32" => (0, u32::MAX as i64), "u64" => (0, i64::MAX),
                            "i8" => (i8::MIN as i64, i8::MAX as i64),
                            "i16" => (i16::MIN as i64, i16::MAX as i64),
                            "i32" => (i32::MIN as i64, i32::MAX as i64),
                            _ => (i64::MIN, i64::MAX),
                        };
                        crate::obj::panic_err(&format!("{n}: value {v} out of range ({min}..={max})"))
                    }
                }
                Err(e) => e,
            }
        }
        ("types", "f32") => match num_arg(args, 0) { Ok(v) => float(v as f32 as f64), Err(e) => e },
        ("types", "f64") => match num_arg(args, 0) { Ok(v) => float(v), Err(e) => e },

        _ => err(&format!("native module '{m}.{name}' is not supported in a no-VM executable")),
    }
}

#[no_mangle]
pub extern "C" fn unrt_call_native(
    module: *const u8,
    module_len: usize,
    member: *const u8,
    member_len: usize,
    args: *const u64,
    argc: usize,
) -> u64 {
    let (m, name) = unsafe {
        (
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(module, module_len)),
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(member, member_len)),
        )
    };
    let vals = unsafe { std::slice::from_raw_parts(args, argc) };
    dispatch(m, name, vals)
}

#[cfg(test)]
mod tests {
    use super::{boolean, dispatch, display, int, string};
    use crate::list::list_new;

    /// The exported C ABI itself (not just the internal `dispatch` helper)
    /// must reach the new member. This is the exact symbol the plain build
    /// calls, so it is the strongest check available while TIR is untouched.
    #[test]
    fn unrt_call_native_abi_reaches_types_type() {
        let module = b"types\0";
        let member = b"type\0";
        let args = [int(1)];
        let got = super::unrt_call_native(
            module.as_ptr(), 5,
            member.as_ptr(), 4,
            args.as_ptr(), 1,
        );
        assert_eq!(display(got), "<type: int>");
    }

    /// Direct dispatch check: the `types` members must reproduce the VM's
    /// rendering without any TIR/natcode change. VM reference (`un run`):
    /// `type(1)` -> `<type: int>`, `repr(1)` -> `int`, `str(1)` -> `1`.
    #[test]
    fn types_type_repr_str_match_vm_scalars() {
        assert_eq!(display(dispatch("types", "type", &[int(1)])), "<type: int>");
        assert_eq!(display(dispatch("types", "repr", &[int(1)])), "int");
        assert_eq!(display(dispatch("types", "str", &[int(1)])), "1");
        assert_eq!(display(dispatch("types", "type", &[string("abc")])), "<type: str>");
        assert_eq!(display(dispatch("types", "repr", &[string("abc")])), "str");
    }

    #[test]
    fn types_type_distinguishes_byte_representations() {
        // D-098: a byte-string literal is still a List<Int> (tag 3) in both
        // engines, while `bytes.from_str` produces the dedicated tag.
        let literal = list_new(vec![int(97), int(98), int(99)]);
        assert_eq!(display(dispatch("types", "type", &[literal])), "<type: list>");
        let dedicated = dispatch("bytes", "from_str", &[string("abc")]);
        assert_eq!(display(dispatch("types", "type", &[dedicated])), "<type: bytes>");
        assert_eq!(display(dispatch("types", "repr", &[dedicated])), "bytes");
    }

    #[test]
    fn types_type_of_type_is_type() {
        let t = dispatch("types", "type", &[int(1)]);
        assert_eq!(display(t), "<type: int>");
        assert_eq!(display(dispatch("types", "type", &[t])), "<type: type>");
        assert_eq!(display(dispatch("types", "repr", &[t])), "type");
        assert_eq!(display(dispatch("types", "str", &[t])), "<type: int>");
    }

    #[test]
    fn types_cast_members_still_dispatch() {
        assert_eq!(display(dispatch("types", "f64", &[int(7)])), "7");
        assert!(dispatch("types", "u8", &[int(-1)]) != 0);
    }

    // ---- W-2026-11-26-FUTURETYPE: go future vs ordinary function ----------
    // The go thunk's ABI is `[obj, obj] -> obj` with the two memo cells as
    // captures (see tir::synthesize_go). `unrt_call_obj` prepends captures, so
    // the thunk function itself just returns the value cell.
    extern "C" fn go_thunk(_flag: u64, value: u64) -> u64 { value }

    fn go_like_future(index: i64) -> u64 {
        let kinds: [u8; 2] = [3, 3];
        crate::closure::nrt_register_lambda(
            index,
            go_thunk as *const () as usize,
            kinds.as_ptr(),
            kinds.len(),
            3,
        );
        let flag = crate::list::list_new(vec![int(0)]);
        let value = crate::list::list_new(vec![int(0)]);
        let captures = [flag, value];
        crate::closure::unrt_obj_closure(index, captures.as_ptr(), captures.len())
    }

    #[test]
    fn go_future_type_repr_str_match_vm_and_wait_still_works() {
        let fut = go_like_future(9101);
        assert_eq!(display(dispatch("types", "type", &[fut])), "<type: future>");
        assert_eq!(display(dispatch("types", "repr", &[fut])), "future");
        assert_eq!(
            display(dispatch("types", "str", &[fut])),
            "unsupported operand type(s) for str(): 'future'"
        );
        // WAITGUARD's happy path (`unrt_call_obj` on the same handle) is intact.
        let empty = [0u64];
        let got = crate::closure::unrt_call_obj(fut, empty.as_ptr(), 0);
        assert_eq!(display(got), "[0]");
    }

    #[test]
    fn ordinary_lambdas_keep_func_type_name() {
        extern "C" fn zero() -> u64 { 0 }
        let none: [u8; 0] = [];
        crate::closure::nrt_register_lambda(9102, zero as *const () as usize, none.as_ptr(), 0, 3);
        // A zero-arg, zero-capture lambda returning an object handle matches
        // the thunk's `fn() -> any` display but has no memo cells.
        let plain = crate::closure::unrt_obj_closure(9102, std::ptr::null(), 0);
        assert_eq!(display(dispatch("types", "type", &[plain])), "<type: func>");
        assert_eq!(display(dispatch("types", "repr", &[plain])), "func");

        extern "C" fn inc(x: i64) -> i64 { x + 1 }
        let kinds: [u8; 2] = [0, 0];
        crate::closure::nrt_register_lambda(9103, inc as *const () as usize, kinds.as_ptr(), 2, 0);
        let captures = [int(5)];
        let lambda = crate::closure::unrt_obj_closure(9103, captures.as_ptr(), 1);
        assert_eq!(display(dispatch("types", "type", &[lambda])), "<type: func>");
    }

    #[test]
    fn lookalike_lambda_with_params_stays_func() {
        // Same first two captures as a future, but it takes a real third
        // argument: the `len(kinds) == len(captures)` guard rules it out.
        extern "C" fn pick(_flag: u64, _value: u64, extra: u64) -> u64 { extra }
        let kinds: [u8; 3] = [3, 3, 3];
        crate::closure::nrt_register_lambda(9104, pick as *const () as usize, kinds.as_ptr(), 3, 3);
        let flag = crate::list::list_new(vec![int(0)]);
        let value = crate::list::list_new(vec![int(7)]);
        let captures = [flag, value];
        let lam = crate::closure::unrt_obj_closure(9104, captures.as_ptr(), 2);
        assert_eq!(display(dispatch("types", "type", &[lam])), "<type: func>");
    }

    // ---- cooperative `flow` shim (W-2026-11-26-ASYNCflow) ------------------
    //
    // Reference values are from `test/unit/runtime/async.un` (VM) and D-062.
    // These exercise the exact `unrt_call_native` surface the plain build uses;
    // the TIR-side allowance for `flow.*` is tracked separately (see report).

    extern "C" fn flow_double(x: i64) -> i64 { x * 2 }
    extern "C" fn flow_add_one(x: i64) -> i64 { x + 1 }

    fn flow_lambda(index: i64, f: extern "C" fn(i64) -> i64) -> u64 {
        let kinds: [u8; 1] = [0]; // one Int parameter, Int return
        crate::closure::nrt_register_lambda(index, f as usize, kinds.as_ptr(), 1, 0);
        crate::closure::unrt_obj_closure(index, std::ptr::null(), 0)
    }

    #[test]
    fn unrt_call_native_abi_reaches_flow_lock() {
        let module = b"flow\0";
        let member = b"lock\0";
        // The exported bridge itself (not just the internal dispatcher) must
        // construct a lock: this is the exact symbol the plain build calls.
        let dummy = [0u64];
        let got = super::unrt_call_native(
            module.as_ptr(), 4, member.as_ptr(), 4, dummy.as_ptr(), 0,
        );
        assert_eq!(display(dispatch("types", "type", &[got])), "<type: lock>");
    }

    #[test]
    fn flow_lock_lifecycle_matches_vm() {
        let lock = dispatch("flow", "lock", &[]);
        assert_eq!(display(dispatch("types", "type", &[lock])), "<type: lock>");
        assert_eq!(display(dispatch("flow", "lock.try_acquire", &[lock])), "true");
        assert_eq!(display(dispatch("flow", "lock.try_acquire", &[lock])), "false");
        assert_eq!(dispatch("flow", "lock.release", &[lock]), lock);
        assert_eq!(dispatch("flow", "lock.acquire", &[lock]), lock);
        assert_eq!(display(dispatch("flow", "lock.try_acquire", &[lock])), "false");
        assert_eq!(dispatch("flow", "lock.release", &[lock]), lock);
        assert_eq!(display(dispatch("flow", "lock.try_acquire", &[lock])), "true");
        assert_eq!(dispatch("flow", "lock.release", &[lock]), lock);
        // async.un asserts the uncaught "lock not held" error on a second release.
        assert_eq!(display(dispatch("flow", "lock.release", &[lock])), "lock not held");
        assert_eq!(display(dispatch("flow", "lock.dispose", &[lock])), "none");
        assert_eq!(
            display(dispatch("types", "str", &[lock])),
            "unsupported operand type(s) for str(): 'lock'"
        );
    }

    #[test]
    fn flow_pool_map_matches_async_test() {
        let pool = dispatch("flow", "spawn", &[int(2)]);
        assert_eq!(display(dispatch("types", "type", &[pool])), "<type: pool>");
        let f = flow_lambda(9001, flow_double);
        // `pool.map(fn(x) -> x * 2, 10, 20, 30)` -> `[20, 40, 60]`.
        assert_eq!(
            display(dispatch("flow", "pool.map", &[pool, f, int(10), int(20), int(30)])),
            "[20, 40, 60]"
        );
        assert_eq!(display(dispatch("flow", "pool.map", &[pool, f, int(7)])), "[14]");
        // VM rest-arg shape (a tuple handle) flattens to the same results.
        let tup = crate::obj::tuple_new(vec![int(10), int(20), int(30)]);
        assert_eq!(display(dispatch("flow", "pool.map", &[pool, f, tup])), "[20, 40, 60]");
        assert_eq!(
            display(dispatch("types", "str", &[pool])),
            "unsupported operand type(s) for str(): 'pool'"
        );
    }

    #[test]
    fn flow_pool_submit_returns_future_and_close_rejects() {
        let pool = dispatch("flow", "spawn", &[]); // VM default: 4 workers
        let f = flow_lambda(9002, flow_add_one);
        let fut = dispatch("flow", "pool.submit", &[pool, f, int(5)]);
        assert_eq!(display(dispatch("types", "type", &[fut])), "<type: future>");
        // Cooperative shim memoizes the inline result (`wait submitted` -> 6).
        assert_eq!(display(dispatch("flow", "future.value", &[fut])), "6");
        assert_eq!(
            display(dispatch("types", "str", &[fut])),
            "unsupported operand type(s) for str(): 'future'"
        );
        assert_eq!(display(dispatch("flow", "pool.close", &[pool])), "none");
        assert_eq!(display(dispatch("flow", "pool.map", &[pool, f, int(1)])), "pool is closed");
        assert_eq!(display(dispatch("flow", "pool.submit", &[pool, f, int(1)])), "pool is closed");
        // Wrong argument strings match native/flow.rs (use a fresh pool:
        // the closed check runs before the callable check in the VM).
        let open_pool = dispatch("flow", "spawn", &[int(1)]);
        assert_eq!(display(dispatch("flow", "pool.map", &[open_pool, int(0), int(1)])),
                   "expected 'fn' argument to be of type 'func'");
    }

    #[test]
    fn flow_atomic_int_matches_vm() {
        // D-062: `atomic.int(5).add(3).get()` -> 8, exchange -> previous value.
        let cell = dispatch("flow", "atomic", &[int(5)]);
        assert_eq!(display(dispatch("types", "type", &[cell])), "<type: atomic_int>");
        assert_eq!(display(dispatch("flow", "atomic_int.add", &[cell, int(3)])), "8");
        assert_eq!(display(dispatch("flow", "atomic_int.get", &[cell])), "8");
        assert_eq!(display(dispatch("flow", "atomic_int.exchange", &[cell, int(2)])), "8");
        assert_eq!(display(dispatch("flow", "atomic_int.compare_and_swap", &[cell, int(2), int(9)])), "true");
        assert_eq!(display(dispatch("flow", "atomic_int.get", &[cell])), "9");
        assert_eq!(display(dispatch("flow", "atomic_int.compare_and_swap", &[cell, int(2), int(0)])), "false");
        assert_eq!(display(dispatch("flow", "atomic_int.inc", &[cell])), "10");
        assert_eq!(display(dispatch("flow", "atomic_int.dec", &[cell])), "9");
        assert_eq!(display(dispatch("flow", "atomic_int.set", &[cell, int(4)])), "none");
        assert_eq!(display(dispatch("flow", "atomic_int.get", &[cell])), "4");
        // TD-39: wrapping arithmetic matches Un Int, no debug panic.
        let max = dispatch("atomic", "int", &[int(i64::MAX)]);
        assert_eq!(display(dispatch("flow", "atomic_int.inc", &[max])), "-9223372036854775808");
    }

    #[test]
    fn flow_atomic_bool_matches_vm() {
        let cell = dispatch("flow", "atomic", &[boolean(true)]);
        assert_eq!(display(dispatch("types", "type", &[cell])), "<type: atomic_bool>");
        assert_eq!(display(dispatch("flow", "atomic_bool.get", &[cell])), "true");
        assert_eq!(display(dispatch("flow", "atomic_bool.exchange", &[cell, boolean(false)])), "true");
        assert_eq!(display(dispatch("flow", "atomic_bool.get", &[cell])), "false");
        assert_eq!(display(dispatch("flow", "atomic_bool.compare_and_swap", &[cell, boolean(false), boolean(true)])), "true");
        assert_eq!(display(dispatch("flow", "atomic_bool.get", &[cell])), "true");
        let default_bool = dispatch("atomic", "bool", &[]);
        assert_eq!(display(dispatch("flow", "atomic_bool.get", &[default_bool])), "false");
    }
}
