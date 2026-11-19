//! `unrt` ??the C ABI for the embedded runtime library.
//!
//! This is the foundation of stage-3 (VM static link ABI): it makes the
//! whole VM runtime callable from code that does not run inside the Un VM,
//! so object files produced by `un obj` can be linked against a static
//! library instead of duplicating the interpreter, and executables produced
//! by `un build --embed` run full Un programs with no runtime files.
//!
//! The ABI follows the same raw-pointer convention as the VM's JIT:
//! objects are passed around as raw pointers to the VM's internal object
//! representation; the ABI never exposes the Rust types.
//!
//! Currently provided:
//!
//! - `unvm_run_embedded` ??parse a `.unbc` image held in memory and run it
//!   exactly like `un run <file.unbc>` would, returning a process exit code
//!   (0 on success, 1 on any error). Used by `un build --embed`, which bakes
//!   the program's bytecode into the generated driver.
//!
//! More entries (native calls, list/dict/class/exception construction) are
//! follow-ups; see `docs/compile/OBJ_MILESTONES.md`.

use std::process;
use std::sync::atomic::AtomicPtr;

use crate::bytecode::unbc;
use crate::native;
use crate::runtime::world;
use crate::source::Source;
use crate::vm::{err_header, err_message, panic_display, Outcome, VM};

// ---------------------------------------------------------------------------
// Pending-error slot for the stage-3b native bridge (Obj mode only).
//
// The interpreter's try/catch/finally machinery works by signalling failures
// as `Outcome::Error`/`Outcome::Propagate` across frames; the native object
// path needs an equivalent channel between *separately compiled* protos and
// between generated code and the VM runtime. The convention:
//
// - `unrt_write_error(obj)` parks an error Obj pointer in the slot.
// - Generated code checks `unrt_is_pending()` after any call that may raise;
//   non-zero jumps to the nearest enclosing catch label.
// - `unrt_take_error()` takes the error and clears the slot (catch entries).
// - `unrt_dump_uncaught()` prints the pending error and exit(1) (`un_main`
//   boundary; matches the interpreter's uncaught-error behavior).

use std::sync::atomic::{AtomicU8, Ordering};

static PENDING_ERROR: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static PENDING_TAG: AtomicU8 = AtomicU8::new(0); // 0 = none, 1 = err obj

/// Park `err` (an Obj pointer) as the current pending error. The pointer is
/// owned by the caller (`err_ptr` came from `<unw opt.into_raw`), this call
/// leaks it into the slot; a subsequent `unrt_take_error` reclaims it.
#[no_mangle]
pub extern "C" fn unrt_write_error(err: u64) -> u64 {
    // Park the Obj pointer without an extra retain/release: the slot holds it
    // until `unrt_take_error` takes it over.
    let _ = store_pending(err);
    0 // always success; caller-side code must not treat this as a value
}

fn store_pending(err: u64) {
    // One pending error at a time: drop the previous one if present. A
    // caller that writes a new error before clearing the old one loses the
    // old error object (it is dropped, not double-freed).
    if let Some(prev) = take_pending_inner() {
        drop(prev);
    }
    PENDING_ERROR.store(err as *mut u8, Ordering::Relaxed);
    PENDING_TAG.store(1, Ordering::Release);
}

fn take_pending_inner() -> Option<crate::runtime::obj::Obj> {
    if PENDING_TAG.swap(0, Ordering::Acquire) != 1 {
        return None;
    }
    let ptr = PENDING_ERROR.swap(std::ptr::null_mut(), Ordering::Relaxed);
    if ptr.is_null() {
        return None;
    }
    let raw = ptr as *const std::cell::RefCell<crate::runtime::obj::ObjData>;
    unsafe { Some(std::rc::Rc::from_raw(raw)) }
}

/// Returns the pending error pointer (0 = no pending error), WITHOUT clearing it.
#[no_mangle]
pub extern "C" fn unrt_is_pending() -> u64 {
    if PENDING_TAG.load(Ordering::Acquire) == 1 {
        PENDING_ERROR.load(Ordering::Relaxed) as u64
    } else {
        0
    }
}

/// Returns a fresh VM object carrying the payload `Error { header: "error",
/// message: "int division by zero" }`, exactly matching the interpreter's
/// `div_zero("int")` error. Used by the native Int/Int `//` and `/` paths to
/// park a catchable error into the pending slot.
#[no_mangle]
pub extern "C" fn unrt_make_int_div_zero() -> u64 {
    to_raw(crate::runtime::obj::err("int division by zero"))
}

/// Returns a fresh VM object carrying the `Panic { name: "error",
/// message: "Attempted to divide by zero." }` payload, exactly matching the
/// interpreter's `divide_by_zero()`. The interpreter never catches Panic
/// payloads with `try`, so Obj-mode `%` MUST NOT park this object into the
/// catch path — the emitted code must call `unrt_dump_uncaught` directly,
/// unconditionally.
/// Returns a fresh VM error Obj carrying `"expected value greater than 0"`,
/// exactly matching the interpreter's `math.log*` boundary error. Used by
/// Obj-mode Float math emission to park a catchable error into pending.
#[no_mangle]
pub extern "C" fn unrt_make_math_expected_pos() -> u64 {
    to_raw(crate::runtime::obj::err("expected value greater than 0"))
}

/// Returns a fresh VM error Obj carrying `"cannot compute sqrt of a negative
/// number"`, exactly matching the interpreter's `math.sqrt` boundary error.
#[no_mangle]
pub extern "C" fn unrt_make_math_sqrt_neg() -> u64 {
    to_raw(crate::runtime::obj::err("cannot compute sqrt of a negative number"))
}

#[no_mangle]
pub extern "C" fn unrt_make_mod_zero_panic() -> u64 {
    to_raw(crate::runtime::obj::divide_by_zero())
}

/// Takes the pending error and clears the slot. Returns 0 when none.
#[no_mangle]
pub extern "C" fn unrt_take_error() -> u64 {
    match take_pending_inner() {
        Some(o) => to_raw(o),
        None => 0,
    }
}

/// Dump the pending error to stdout in the same shape the interpreter reports
/// uncaught errors (`report()` in main.rs prints via `print!`) and terminate
/// with exit code 1. Used by the unguarded Obj-mode entry (`un_main` driver)
/// for programs that end with an uncaught error.
#[no_mangle]
pub extern "C" fn unrt_dump_uncaught() -> ! {
    // Interpreter's report() uses print! on stdout and exit(1). Mirror that
    // shape (the source-context traceback is not available in a frozen
    // exe): stdout line + exit 1.
    let err = take_pending_inner();
    let mut message = String::from("<no pending error>");
    if let Some(err) = err {
        let payload = crate::runtime::obj::payload_ref(&err);
        match &*payload {
            crate::runtime::obj::Payload::Error { header, message: m } => {
                if header == "error" || header == "Error" {
                    message = format!("error: {m}");
                } else {
                    message = format!("error: {header}: {m}");
                }
            }
            crate::runtime::obj::Payload::Panic { name, message: m } => {
                message = format!("{name}: {m}");
            }
            other => {
                message = format!("error: {other:?}");
            }
        }
    }
    println!("{message}");
    std::io::Write::flush(&mut std::io::stdout()).ok();
    process::exit(1);
}

/// Construct a class instance, mirroring the interpreter: find the class
/// template in the process-wide world (registered via
/// `unrt_register_classes`), clone it, and run `__init__` through the VM.
/// Returns an error object when the class is not registered.
#[no_mangle]
pub extern "C" fn unrt_class_new(
    name_ptr: *const u8,
    name_len: usize,
    args: *const u64,
    argc: usize,
) -> u64 {
    if name_ptr.is_null() {
        return to_raw(crate::runtime::obj::err("unrt_class_new: null class name"));
    }
    let name_bytes = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
    let Ok(name) = std::str::from_utf8(name_bytes) else {
        return to_raw(crate::runtime::obj::err("unrt_class_new: class name is not utf-8"));
    };
    let Some(class) = crate::runtime::world::get_class(name) else {
        return to_raw(crate::runtime::obj::err(format!(
            "class '{name}' not registered"
        )));
    };
    let values: Vec<crate::runtime::obj::Obj> = (0..argc)
        .map(|i| {
            borrow_obj(unsafe { *args.add(i) }).unwrap_or_else(crate::runtime::obj::none)
        })
        .collect();
    let instance = crate::runtime::obj::clone_obj(&class);
    to_raw(crate::runtime::obj::init(&instance, &values))
}

/// Call a bound method on `obj` by name: full lookup (properties, parents,
/// `__getattr__`), bind `self`, and call with positional Obj args. The
/// result is a raw Obj pointer; errors come back as error objects.
#[no_mangle]
pub extern "C" fn unrt_method_call(
    obj: u64,
    name_ptr: *const u8,
    name_len: usize,
    args: *const u64,
    argc: usize,
) -> u64 {
    if name_ptr.is_null() {
        return to_raw(crate::runtime::obj::err("unrt_method_call: null member name"));
    }
    let Some(o) = borrow_obj(obj) else {
        return to_raw(crate::runtime::obj::err("unrt_method_call: null receiver"));
    };
    let name_bytes = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
    let Ok(name) = std::str::from_utf8(name_bytes) else {
        return to_raw(crate::runtime::obj::err("unrt_method_call: member name is not utf-8"));
    };
    let member = crate::runtime::obj::get_attr(&o, name);
    let values: Vec<crate::runtime::obj::Obj> = (0..argc)
        .map(|i| {
            borrow_obj(unsafe { *args.add(i) }).unwrap_or_else(crate::runtime::obj::none)
        })
        .collect();
    to_raw(crate::runtime::obj::call_args(
        &member,
        &crate::runtime::function::Args::positional(values),
    ))
}

/// Unbox an Obj to a bool (1/0). Returns 0 for non-Bool payloads.
#[no_mangle]
pub extern "C" fn unrt_bool_from_obj(o: u64) -> i64 {
    let Some(o) = borrow_obj(o) else {
        return 0;
    };
    match crate::runtime::obj::payload(&o) {
        crate::runtime::obj::Payload::Bool(b) => b as i64,
        _ => 0,
    }
}

/// Register the classes baked into embedded `.unbc` bytes into the current
/// process-wide VM world, then return. The class-registration spine is the
/// dedicated `<classes_init>` proto emitted by the compiler; it re-compiles
/// each top-level `class` declaration into MakeClass/AddMethod/AddMember/
/// RegisterClass ops. Programs without any classes (or programs using the
/// interpreter entry path, which is `unvm_run_embedded`) do not call this.
/// Returns 0 on success, 1 on a fatal class-initialization error (already
/// reported on stdout).
#[no_mangle]
pub extern "C" fn unrt_register_classes(bytes: *const u8, len: usize) -> i32 {
    if bytes.is_null() || len == 0 {
        return 0;
    }
    let slice = unsafe { std::slice::from_raw_parts(bytes, len) };
    let program = match crate::bytecode::unbc::read(slice) {
        Ok(program) => program,
        Err(message) => {
            eprintln!("unrt_register_classes: {message}");
            return 1;
        }
    };
    let entry = program
        .protos
        .iter()
        .position(|p| program.names[p.name] == "<classes_init>");
    let Some(entry) = entry else {
        return 0;
    };
    let source = crate::source::Source::new(
        program.source_path.clone(),
        program.source.clone().unwrap_or_default().as_str(),
    );
    let flow = match crate::vm::VM::run_proto(std::rc::Rc::new(program), std::rc::Rc::new(source), entry) {
        Ok(outcome) => outcome,
        Err(message) => {
            eprintln!("un: class initialization failed: {message}");
            return 1;
        }
    };
    match flow {
        crate::vm::Outcome::Value(_) | crate::vm::Outcome::Return(..) => 0,
        crate::vm::Outcome::Error(e) => {
            eprintln!("un: class initialization failed: {}", e.message);
            1
        }
        crate::vm::Outcome::Propagate(obj, _, _) => {
            eprintln!(
                "un: class initialization failed: {}",
                crate::runtime::obj::display(&obj)
            );
            1
        }
        crate::vm::Outcome::Panic(obj) => {
            eprintln!(
                "un: class initialization failed: {}",
                crate::vm::panic_display(&obj)
            );
            1
        }
        crate::vm::Outcome::Break(_, _) => {
            eprintln!("un: class initialization failed: 'break' outside loop");
            1
        }
        crate::vm::Outcome::Skip(_, _) => {
            eprintln!("un: class initialization failed: 'skip' outside loop");
            1
        }
    }
}

/// Parse a `.unbc` image from memory and execute it with the default
/// interpreter configuration (same as `un run`). Stdout is flushed; errors
/// are printed to stdout. Returns 0 on success, 1 otherwise. Must be called
/// from a fresh thread or after any previous VM has finished ??the VM keeps
/// process-global state.
#[no_mangle]
pub extern "C" fn unvm_run_embedded(bytes: *const u8, len: usize) -> i32 {
    if bytes.is_null() || len == 0 {
        return 1;
    }
    let data = unsafe { std::slice::from_raw_parts(bytes, len) }.to_vec();
    let program = match unbc::read(&data) {
        Ok(program) => program,
        Err(message) => {
            eprintln!("unvm_run_embedded: {message}");
            return 1;
        }
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    native::init(cwd.to_str().unwrap_or("."));

    let text = program.source.clone().unwrap_or_default();
    let source = std::rc::Rc::new(Source::new(&program.source_path, &text));
    let mut vm = match VM::new(std::rc::Rc::new(program), source.clone()) {
        Ok(vm) => vm,
        Err(message) => {
            eprintln!("unvm_run_embedded: vm: {message}");
            return 1;
        }
    };
    match vm.run() {
        Outcome::Value(_) => {
            world::flush_stdout();
            0
        }
        Outcome::Error(e) => {
            world::flush_stdout();
            let root = Some((
                e.source.clone().unwrap_or_else(|| source.clone()),
                e.start,
            ));
            if let Some(trace) = crate::runtime::context::trace_string(&vm.ctx, root.as_ref().map(|(s, st)| (s.as_ref(), *st))) {
                print!("{trace}");
            }
            print!("{}\n", e.display(&source));
            1
        }
        Outcome::Propagate(_, _, _) | Outcome::Return(_, _, _) | Outcome::Break(_, _) | Outcome::Skip(_, _) => {
            world::flush_stdout();
            print!("unvm_run_embedded: uncaught error\n");
            let _ = err_header;
            let _ = err_message;
            1
        }
        Outcome::Panic(o) => {
            world::flush_stdout();
            print!("{}\n", panic_display(&o));
            1
        }
    }
}

/// Convenience wrapper for embedding from a file path instead of raw bytes ??/// reads `.unbc` from disk like `un run` does. Provided for callers that
/// don't want to embed the bytes in the binary.
#[no_mangle]
pub extern "C" fn unvm_run_file(path_ptr: *const u8, path_len: usize) -> i32 {
    if path_ptr.is_null() || path_len == 0 {
        return 1;
    }
    let path = unsafe { std::slice::from_raw_parts(path_ptr, path_len) };
    let Ok(path) = std::str::from_utf8(path) else {
        return 1;
    };
    let Ok(bytes) = std::fs::read(path) else {
        return 1;
    };
    unvm_run_embedded(bytes.as_ptr(), bytes.len())
}

/// Current capacity growth strategy for lists ??freshly-created lists start
/// with the capacity hint requested from the caller (default 8).
#[no_mangle]
pub extern "C" fn unrt_list_new() -> u64 {
    to_raw(crate::runtime::collections::list(Vec::new()))
}

/// Append `item` to `list`. Returns the list (like all bridge calls,
/// callers treat it as opaque).
#[no_mangle]
pub extern "C" fn unrt_list_push(list: u64, item: u64) -> u64 {
    let Some(list) = borrow_obj(list) else {
        return to_raw(crate::runtime::obj::err("unrt: list argument is not a list"));
    };
    let Some(item) = borrow_obj(item) else {
        return to_raw(crate::runtime::obj::err("unrt: item is not an object"));
    };
    match crate::runtime::obj::payload(&list) {
        crate::runtime::obj::Payload::List(vec) => {
            vec.borrow_mut().push(item);
            to_raw(list)
        }
        _ => to_raw(crate::runtime::obj::err("unrt: push target is not a list")),
    }
}

/// Pop the last element of `list`. Returns an error object (which generated
/// code detects) when the list is empty.
#[no_mangle]
pub extern "C" fn unrt_list_pop(list: u64) -> u64 {
    let Some(list) = borrow_obj(list) else {
        return to_raw(crate::runtime::obj::err("unrt: list argument is not a list"));
    };
    match crate::runtime::obj::payload(&list) {
        crate::runtime::obj::Payload::List(vec) => {
            match vec.borrow_mut().pop() {
                Some(v) => to_raw(v),
                None => to_raw(crate::runtime::obj::err("list index out of range")),
            }
        }
        _ => to_raw(crate::runtime::obj::err("unrt: pop target is not a list")),
    }
}

/// Pop the element at `idx` of `list` (negative indices wrap, like the
/// interpreter). Returns an error object on out-of-range.
#[no_mangle]
pub extern "C" fn unrt_list_pop_at(list: u64, idx: i64) -> u64 {
    let Some(list) = borrow_obj(list) else {
        return to_raw(crate::runtime::obj::err("unrt: list argument is not a list"));
    };
    match crate::runtime::obj::payload(&list) {
        crate::runtime::obj::Payload::List(vec) => {
            let count = vec.borrow().len() as i64;
            let mut i = idx;
            if i < 0 {
                i = i.saturating_add(count);
            }
            if i < 0 || i >= count {
                return to_raw(crate::runtime::obj::err("list index out of range"));
            }
            let value = vec.borrow_mut().remove(i as usize);
            to_raw(value)
        }
        _ => to_raw(crate::runtime::obj::err("unrt: pop target is not a list")),
    }
}

/// In-place sort of `list` by `obj::compare_to`; the interpreter's
/// order is preserved (uncomparable pairs compare Equal).
#[no_mangle]
pub extern "C" fn unrt_list_sort(list: u64) -> u64 {
    let Some(list) = borrow_obj(list) else {
        return to_raw(crate::runtime::obj::err("unrt: list argument is not a list"));
    };
    match crate::runtime::obj::payload(&list) {
        crate::runtime::obj::Payload::List(vec) => {
            let mut items = vec.borrow().clone();
            items.sort_by(|a, b| match crate::runtime::obj::compare_to(a, b) {
                Ok(order) => order.cmp(&0),
                Err(_) => std::cmp::Ordering::Equal,
            });
            *vec.borrow_mut() = items;
            to_raw(crate::runtime::obj::none())
        }
        _ => to_raw(crate::runtime::obj::err("unrt: sort target is not a list")),
    }
}

/// In-place reverse of `list`.
#[no_mangle]
pub extern "C" fn unrt_list_reverse(list: u64) -> u64 {
    let Some(list) = borrow_obj(list) else {
        return to_raw(crate::runtime::obj::err("unrt: list argument is not a list"));
    };
    match crate::runtime::obj::payload(&list) {
        crate::runtime::obj::Payload::List(vec) => {
            vec.borrow_mut().reverse();
            to_raw(crate::runtime::obj::none())
        }
        _ => to_raw(crate::runtime::obj::err("unrt: reverse target is not a list")),
    }
}

/// Insert `item` at `idx` in `list`. `idx` is clamped to `[0, len]`,
/// matching the interpreter's `insert` (`index.max(0).min(len)`).
#[no_mangle]
pub extern "C" fn unrt_list_insert(list: u64, item: u64, idx: i64) -> u64 {
    let Some(list) = borrow_obj(list) else {
        return to_raw(crate::runtime::obj::err("unrt: list argument is not a list"));
    };
    let Some(item) = borrow_obj(item) else {
        return to_raw(crate::runtime::obj::err("unrt: item is not an object"));
    };
    match crate::runtime::obj::payload(&list) {
        crate::runtime::obj::Payload::List(vec) => {
            let mut items = vec.borrow_mut();
            let base = idx.max(0).min(items.len() as i64) as usize;
            items.insert(base, item);
            to_raw(crate::runtime::obj::none())
        }
        _ => to_raw(crate::runtime::obj::err("unrt: insert target is not a list")),
    }
}

/// Position of `item` in `list` (element-wise key equality), -1 when absent.
#[no_mangle]
pub extern "C" fn unrt_list_index_of(list: u64, item: u64) -> i64 {
    let Some(list) = borrow_obj(list) else {
        return -1;
    };
    let Some(item) = borrow_obj(item) else {
        return -1;
    };
    match crate::runtime::obj::payload(&list) {
        crate::runtime::obj::Payload::List(vec) => {
            for (i, entry) in vec.borrow().iter().enumerate() {
                if crate::runtime::obj::key_eq(entry, &item) {
                    return i as i64;
                }
            }
            -1
        }
        _ => -1,
    }
}

/// 1 when `item` is in `list` (element-wise key equality), 0 otherwise.
#[no_mangle]
pub extern "C" fn unrt_list_contains(list: u64, item: u64) -> i64 {
    (unrt_list_index_of(list, item) >= 0) as i64
}

/// Length of a list (in elements). Returns -1 for non-lists.
#[no_mangle]
pub extern "C" fn unrt_list_len(list: u64) -> i64 {
    let Some(list) = borrow_obj(list) else {
        return -1;
    };
    match crate::runtime::obj::payload(&list) {
        crate::runtime::obj::Payload::List(vec) => vec.borrow().len() as i64,
        _ => -1,
    }
}

/// Get the element at `idx` as an Obj (caller treats it as Int/Float/??.
#[no_mangle]
pub extern "C" fn unrt_list_get(list: u64, idx: i64) -> u64 {
    let Some(list) = borrow_obj(list) else {
        return to_raw(crate::runtime::obj::err("unrt: list argument is not a list"));
    };
    match crate::runtime::obj::payload(&list) {
        crate::runtime::obj::Payload::List(vec) => {
            let i = if idx < 0 {
                vec.borrow().len() as i64 + idx
            } else {
                idx
            };
            if i < 0 || i >= vec.borrow().len() as i64 {
                return to_raw(crate::runtime::obj::err("index out of range"));
            }
            to_raw(vec.borrow()[i as usize].clone())
        }
        _ => to_raw(crate::runtime::obj::err("unrt: get target is not a list")),
    }
}

/// Build a list from an array of raw i64 elements.
#[no_mangle]
pub extern "C" fn unrt_list_from_ints(ptr: *const i64, len: i64) -> u64 {
    if ptr.is_null() || len < 0 {
        return to_raw(crate::runtime::obj::err("unrt: bad ints pointer"));
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let items: Vec<crate::runtime::obj::Obj> = slice
        .iter()
        .map(|&v| crate::runtime::obj::int(v))
        .collect();
    to_raw(crate::runtime::collections::list(items))
}

/// Build a list from an array of raw f64 elements.
#[no_mangle]
pub extern "C" fn unrt_list_from_floats(ptr: *const f64, len: i64) -> u64 {
    if ptr.is_null() || len < 0 {
        return to_raw(crate::runtime::obj::err("unrt: bad floats pointer"));
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let items: Vec<crate::runtime::obj::Obj> = slice
        .iter()
        .map(|&v| crate::runtime::obj::float(v))
        .collect();
    to_raw(crate::runtime::collections::list(items))
}

/// Build a list from an array of raw Obj pointers, all expected to belong to
/// a common type (e.g. class instances). Elements cross the FFI boundary as
/// raw pointers; this bridge re-wraps them as VM Obj values.
#[no_mangle]
pub extern "C" fn unrt_list_from_elems(ptr: *const u64, len: i64) -> u64 {
    if ptr.is_null() || len < 0 {
        return to_raw(crate::runtime::obj::err("unrt_list_from_elems: bad pointer"));
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let items: Vec<crate::runtime::obj::Obj> = slice.iter().map(|&o| borrow_obj(o).unwrap_or_else(crate::runtime::obj::none)).collect();
    to_raw(crate::runtime::collections::list(items))
}

/// Build a list of `len` copies of `value`, int elements. Used by the
/// native exe (Obj mode) path for `iter.array(v, n)`.
#[no_mangle]
pub extern "C" fn unrt_list_repeat_int(value: i64, len: i64) -> u64 {
    if len < 0 {
        return to_raw(crate::runtime::obj::err("expected all items in 'size' list to be non-negative integers"));
    }
    let items: Vec<crate::runtime::obj::Obj> =
        (0..len).map(|_| crate::runtime::obj::int(value)).collect();
    to_raw(crate::runtime::collections::list(items))
}

/// Build a list of `len` copies of `value`, float elements. Used by the
/// native exe (Obj mode) path for `iter.array(v, n)`.
#[no_mangle]
pub extern "C" fn unrt_list_repeat_float(value: f64, len: i64) -> u64 {
    if len < 0 {
        return to_raw(crate::runtime::obj::err("expected all items in 'size' list to be non-negative integers"));
    }
    let items: Vec<crate::runtime::obj::Obj> =
        (0..len).map(|_| crate::runtime::obj::float(value)).collect();
    to_raw(crate::runtime::collections::list(items))
}

/// Unbox an Obj to i64. Returns 0 for non-Int payloads.
#[no_mangle]
pub extern "C" fn unrt_int_from_obj(o: u64) -> i64 {
    let Some(o) = borrow_obj(o) else {
        return 0;
    };
    match crate::runtime::obj::payload(&o) {
        crate::runtime::obj::Payload::Int(v) => v,
        _ => 0,
    }
}

/// Unbox an Obj to f64. Returns NaN for non-Float payloads.
#[no_mangle]
pub extern "C" fn unrt_float_from_obj(o: u64) -> f64 {
    let Some(o) = borrow_obj(o) else {
        return f64::NAN;
    };
    match crate::runtime::obj::payload(&o) {
        crate::runtime::obj::Payload::Float(v) => v,
        _ => f64::NAN,
    }
}

/// Get the canonical `TObj` proxy for class `name` (what the interpreter's
/// global lookup yields). `unrt_call_obj` on it runs the constructor with
/// full VM semantics. Returns an error object if the class isn't registered.
#[no_mangle]
pub extern "C" fn unrt_get_class(ptr: *const u8, len: usize) -> u64 {
    if ptr.is_null() {
        return to_raw(crate::runtime::obj::err("unrt: get_class called with a null name"));
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let Ok(name) = std::str::from_utf8(slice) else {
        return to_raw(crate::runtime::obj::err("unrt: get_class called with a non-UTF-8 name"));
    };
    match crate::runtime::world::get_class(&name) {
        Some(_) => to_raw(crate::runtime::world::class_tobj(&name)),
        None => to_raw(crate::runtime::obj::err(format!("class '{name}' not found"))),
    }
}

/// `obj.member` for any Obj (classes, natives, dicts): runs properties,
/// `__getattr__`, bound-method binding, in the VM exactly as `obj.get_attr`.
/// Error returns are passed through; generated code branches on
/// `unrt_obj_is_err`.
#[no_mangle]
pub extern "C" fn unrt_member_get(obj: u64, name_ptr: *const u8, name_len: usize) -> u64 {
    let Some(obj) = borrow_obj(obj) else {
        return to_raw(crate::runtime::obj::err("unrt: member access on a null object"));
    };
    if name_ptr.is_null() {
        return to_raw(crate::runtime::obj::err("unrt: null member name"));
    }
    let slice = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
    let Ok(name) = std::str::from_utf8(slice) else {
        return to_raw(crate::runtime::obj::err("unrt: member access with a non-UTF-8 name"));
    };
    to_raw(crate::runtime::obj::get_attr(&obj, name))
}

/// `obj.member = value` for any Obj: runs VM property setters,
/// `__setattr__`, and member assignment, exactly as the interpreter.
#[no_mangle]
pub extern "C" fn unrt_member_set(obj: u64, name_ptr: *const u8, name_len: usize, value: u64) -> u64 {
    let Some(obj) = borrow_obj(obj) else {
        return to_raw(crate::runtime::obj::err("unrt: member access on a null object"));
    };
    let Some(value) = borrow_obj(value) else {
        return to_raw(crate::runtime::obj::err("unrt: member set with a null value"));
    };
    if name_ptr.is_null() {
        return to_raw(crate::runtime::obj::err("unrt: null member name"));
    }
    let slice = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
    let Ok(name) = std::str::from_utf8(slice) else {
        return to_raw(crate::runtime::obj::err("unrt: member set with a non-UTF-8 name"));
    };
    to_raw(crate::runtime::obj::set_attr(&obj, name, &value))
}

/// Structural equality as the VM sees it: `key_eq` without hash semantics.
/// Returns 1 when equal, 0 otherwise. Used by generated code for `str == str`
/// (and anything else that needs object equality in native code).
#[no_mangle]
pub extern "C" fn unrt_obj_eq(a: u64, b: u64) -> i64 {
    let Some(a) = borrow_obj(a) else {
        return 0;
    };
    let Some(b) = borrow_obj(b) else {
        return 0;
    };
    crate::runtime::obj::key_eq(&a, &b) as i64
}

/// `len(x)`: size of a string (UTF-16 code units, like the interpreter) or
/// of a list/dict. Reuses the VM's own `unary(obj::Len)` so semantics match
/// the interpreter exactly.
#[no_mangle]
pub extern "C" fn unrt_len(o: u64) -> u64 {
    let Some(o) = borrow_obj(o) else {
        return to_raw(crate::runtime::obj::err("unrt: len() argument is not an object"));
    };
    to_raw(crate::runtime::obj::len(&o))
}

/// 1 when `o` is a VM error/panic object, 0 otherwise. Lets generated code
/// turn a failing native bridge call (e.g. a missing dict key) into a trap
/// instead of silently reading an error object as a value.
#[no_mangle]
pub extern "C" fn unrt_obj_is_err(o: u64) -> i64 {
    let Some(o) = borrow_obj(o) else {
        return 1;
    };
    let payload = crate::runtime::obj::payload_ref(&o);
    matches!(
        &*payload,
        crate::runtime::obj::Payload::Error { .. } | crate::runtime::obj::Payload::Panic { .. }
    ) as i64
}

/// Build a dict from `len` interleaved (key, value) Obj pointers:
/// `pairs[0]=key0, pairs[1]=value0, pairs[2]=key1, pairs[3]=value1, ...`
/// Semantics match the interpreter's dict literal (later duplicate keys win).
#[no_mangle]
pub extern "C" fn unrt_dict_from_pairs(pairs: *const u64, len: i64) -> u64 {
    if pairs.is_null() || len < 0 {
        return to_raw(crate::runtime::obj::err("unrt: bad dict pairs pointer"));
    }
    let slice = unsafe { std::slice::from_raw_parts(pairs, (2 * len) as usize) };
    let mut entries = Vec::with_capacity(len as usize);
    for chunk in slice.chunks_exact(2) {
        let Some(key) = borrow_obj(chunk[0]) else {
            return to_raw(crate::runtime::obj::err("unrt: dict key is not an object"));
        };
        let Some(value) = borrow_obj(chunk[1]) else {
            return to_raw(crate::runtime::obj::err("unrt: dict value is not an object"));
        };
        entries.push((key, value));
    }
    to_raw(crate::runtime::collections::dict(entries))
}

/// Set (or insert) `key` in `dict`, mirroring the interpreter's
/// `d["k"] = v` (missing keys are inserted; Int/Float values accepted).
/// Returns the stored value object (or the error object, which generated
/// code detects via `unrt_obj_is_err`).
#[no_mangle]
pub extern "C" fn unrt_dict_set(dict: u64, key: u64, value: u64) -> u64 {
    let Some(dict) = borrow_obj(dict) else {
        return to_raw(crate::runtime::obj::err("unrt: dict argument is not a dict"));
    };
    let Some(key) = borrow_obj(key) else {
        return to_raw(crate::runtime::obj::err("unrt: dict key is not an object"));
    };
    let Some(value) = borrow_obj(value) else {
        return to_raw(crate::runtime::obj::err("unrt: dict value is not an object"));
    };
    match crate::runtime::obj::payload(&dict) {
        crate::runtime::obj::Payload::Dict(km) => {
            to_raw(crate::runtime::collections::dict_set_item(&km, &key, &value))
        }
        _ => to_raw(crate::runtime::obj::err("unrt: set target is not a dict")),
    }
}

/// Replace the element at `idx` in `list` with `value`. Negative indices wrap,
/// matching the interpreter (`xs[-1] = v`). Out-of-range returns an error
/// object (generated code branches on `unrt_obj_is_err`).
#[no_mangle]
pub extern "C" fn unrt_list_set(list: u64, idx: i64, value: u64) -> u64 {
    let Some(list) = borrow_obj(list) else {
        return to_raw(crate::runtime::obj::err("unrt: list argument is not a list"));
    };
    let Some(value) = borrow_obj(value) else {
        return to_raw(crate::runtime::obj::err("unrt: list value is not an object"));
    };
    match crate::runtime::obj::payload(&list) {
        crate::runtime::obj::Payload::List(vec) => {
            let count = vec.borrow().len() as i64;
            let mut i = idx;
            if i < 0 || i >= count {
                i = i.saturating_add(count);
            }
            if i < 0 || i >= count {
                return to_raw(crate::runtime::obj::err("list index out of range"));
            }
            vec.borrow_mut()[i as usize] = value.clone();
            to_raw(value)
        }
        _ => to_raw(crate::runtime::obj::err("unrt: set target is not a list")),
    }
}

/// Look up `key` in `dict`. Returns the value object, or the
/// `key '{key}' not found in dictionary` VM error object (same as the
/// interpreter's `d[key]`). Callers branch on `unrt_obj_is_err`.
#[no_mangle]
pub extern "C" fn unrt_dict_get(dict: u64, key: u64) -> u64 {
    let Some(dict) = borrow_obj(dict) else {
        return to_raw(crate::runtime::obj::err("unrt: dict argument is not a dict"));
    };
    let Some(key) = borrow_obj(key) else {
        return to_raw(crate::runtime::obj::err("unrt: dict key is not an object"));
    };
    match crate::runtime::obj::payload(&dict) {
        crate::runtime::obj::Payload::Dict(km) => {
            to_raw(crate::runtime::collections::dict_get_item(&km, &key))
        }
        _ => to_raw(crate::runtime::obj::err("unrt: lookup target is not a dict")),
    }
}


/// Never returns; used by the generated driver for `un build --embed` so the
/// process ends with the program's exit code rather than a default 0.
#[no_mangle]
pub extern "C" fn unvm_exit(code: i32) -> ! {
    process::exit(code)
}

// ---------------------------------------------------------------------------
// Stage 3b (VM static link ABI): generated code calls into the VM runtime
// instead of the interpreter. Obj values cross the ABI as raw pointers
// (`u64`), following the same raw-pointer convention as the VM's JIT.
// ---------------------------------------------------------------------------

fn borrow_obj(ptr: u64) -> Option<crate::runtime::obj::Obj> {
    let raw = ptr as *const std::cell::RefCell<crate::runtime::obj::ObjData>;
    if raw.is_null() {
        return None;
    }
    unsafe {
        let rc = std::rc::Rc::from_raw(raw);
        let cloned = rc.clone();
        let _ = std::rc::Rc::into_raw(rc);
        Some(cloned)
    }
}

fn to_raw(o: crate::runtime::obj::Obj) -> u64 {
    std::rc::Rc::into_raw(o) as u64
}

/// (Re)initialize the native module table from `.` ??same as a fresh Un
/// process. Idempotent.
#[no_mangle]
pub extern "C" fn unrt_init() {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    crate::native::init(cwd.to_str().unwrap_or("."));
}

/// Flush the buffered Un stdout (io.write writes into the buffer).
#[no_mangle]
pub extern "C" fn unrt_flush_stdout() {
    crate::runtime::world::flush_stdout();
}

#[no_mangle]
pub extern "C" fn unrt_obj_int(v: i64) -> u64 {
    to_raw(crate::runtime::obj::int(v))
}

#[no_mangle]
pub extern "C" fn unrt_obj_bool(v: i64) -> u64 {
    to_raw(crate::runtime::obj::boolean(v != 0))
}

#[no_mangle]
pub extern "C" fn unrt_obj_float(v: f64) -> u64 {
    to_raw(crate::runtime::obj::float(v))
}

#[no_mangle]
pub extern "C" fn unrt_obj_none() -> u64 {
    to_raw(crate::runtime::obj::none())
}

/// Bytes at `ptr` (not necessarily NUL-terminated) become a Str Obj.
#[no_mangle]
pub extern "C" fn unrt_str_from_bytes(ptr: *const u8, len: usize) -> u64 {
    if ptr.is_null() {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let s = String::from_utf8_lossy(bytes).into_owned();
    to_raw(crate::runtime::obj::str(&s))
}

/// Concatenate two Str Objs (panics at the VM boundary on mismatch).
#[no_mangle]
pub extern "C" fn unrt_str_concat(a: u64, b: u64) -> u64 {
    let (Some(a), Some(b)) = (borrow_obj(a), borrow_obj(b)) else {
        return to_raw(crate::runtime::obj::err("unrt: str concat needs two values"));
    };
    let ab = a.borrow();
    let bb = b.borrow();
    match (&*ab, &*bb) {
        (
            crate::runtime::obj::ObjData {
                payload: crate::runtime::obj::Payload::Str(x),
                ..
            },
            crate::runtime::obj::ObjData {
                payload: crate::runtime::obj::Payload::Str(y),
                ..
            },
        ) => to_raw(crate::runtime::obj::str(&format!("{x}{y}"))),
        _ => to_raw(crate::runtime::obj::err("unrt: str concat needs two Str")),
    }
}

/// Generic bridge into any registered native module member:
/// `io.write("hi")` becomes `unrt_call_native("io", "write", [arg])`.
/// Arguments are Obj pointers (raw), the result is a raw Obj pointer.
#[no_mangle]
pub extern "C" fn unrt_call_native(
    module: *const u8,
    module_len: usize,
    member: *const u8,
    member_len: usize,
    args: *const u64,
    argc: usize,
) -> u64 {
    let module_bytes = unsafe { std::slice::from_raw_parts(module, module_len) };
    let member_bytes = unsafe { std::slice::from_raw_parts(member, member_len) };
    let (Some(module), Some(member)) = (
        std::str::from_utf8(module_bytes).ok(),
        std::str::from_utf8(member_bytes).ok(),
    ) else {
        return to_raw(crate::runtime::obj::err("unrt: invalid utf-8 module or member name"));
    };
    let Some(members) = crate::runtime::world::native_members(module) else {
        return to_raw(crate::runtime::obj::err(format!(
            "unrt: native module '{module}' not registered"
        )));
    };
    let Some((_, callable)) = members.iter().find(|(k, _)| k == member) else {
        return to_raw(crate::runtime::obj::err(format!(
            "unrt: native '{module}.{member}' not found"
        )));
    };
    let values: Vec<crate::runtime::obj::Obj> = (0..argc)
        .map(|i| {
            borrow_obj(unsafe { *args.add(i) }).unwrap_or_else(crate::runtime::obj::none)
        })
        .collect();
    let result = crate::runtime::obj::call_args(
        callable,
        &crate::runtime::function::Args::positional(values),
    );
    to_raw(result)
}

/// Generic bridge for calls to ordinary Un values (Obj of any callable
/// kind ??typically used with closures and class receivers). Not yet wired
/// to generated code; provided for stage 3b follow-ups.
#[no_mangle]
pub extern "C" fn unrt_call_obj(callable: u64, args: *const u64, argc: usize) -> u64 {
    let Some(c) = borrow_obj(callable) else {
        return to_raw(crate::runtime::obj::err("unrt: null callable"));
    };
    let values: Vec<crate::runtime::obj::Obj> = (0..argc)
        .map(|i| {
            borrow_obj(unsafe { *args.add(i) }).unwrap_or_else(crate::runtime::obj::none)
        })
        .collect();
    let result =
        crate::runtime::obj::call_args(&c, &crate::runtime::function::Args::positional(values));
    to_raw(result)
}
