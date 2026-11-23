
use std::process;
use std::sync::atomic::AtomicPtr;

use crate::bytecode::unbc;
use crate::native;
use crate::runtime::world;
use crate::source::Source;
use crate::vm::{err_header, err_message, panic_display, Outcome, VM};


use std::sync::atomic::{AtomicU8, Ordering};

static PENDING_ERROR: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static PENDING_TAG: AtomicU8 = AtomicU8::new(0);
#[no_mangle]
pub extern "C" fn unrt_write_error(err: u64) -> u64 {
    let _ = store_pending(err);
    0}

fn store_pending(err: u64) {
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

#[no_mangle]
pub extern "C" fn unrt_is_pending() -> u64 {
    if PENDING_TAG.load(Ordering::Acquire) == 1 {
        PENDING_ERROR.load(Ordering::Relaxed) as u64
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn unrt_make_int_div_zero() -> u64 {
    to_raw(crate::runtime::obj::err("int division by zero"))
}

#[no_mangle]
pub extern "C" fn unrt_make_math_expected_pos() -> u64 {
    to_raw(crate::runtime::obj::err("expected value greater than 0"))
}

#[no_mangle]
pub extern "C" fn unrt_make_math_sqrt_neg() -> u64 {
    to_raw(crate::runtime::obj::err("cannot compute sqrt of a negative number"))
}

#[no_mangle]
pub extern "C" fn unrt_make_mod_zero_panic() -> u64 {
    to_raw(crate::runtime::obj::divide_by_zero())
}

#[no_mangle]
pub extern "C" fn unrt_take_error() -> u64 {
    match take_pending_inner() {
        Some(o) => to_raw(o),
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn unrt_dump_uncaught() -> ! {
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

#[no_mangle]
pub extern "C" fn unrt_list_new() -> u64 {
    to_raw(crate::runtime::collections::list(Vec::new()))
}

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

#[no_mangle]
pub extern "C" fn unrt_list_contains(list: u64, item: u64) -> i64 {
    (unrt_list_index_of(list, item) >= 0) as i64
}

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

#[no_mangle]
pub extern "C" fn unrt_list_from_elems(ptr: *const u64, len: i64) -> u64 {
    if ptr.is_null() || len < 0 {
        return to_raw(crate::runtime::obj::err("unrt_list_from_elems: bad pointer"));
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let items: Vec<crate::runtime::obj::Obj> = slice.iter().map(|&o| borrow_obj(o).unwrap_or_else(crate::runtime::obj::none)).collect();
    to_raw(crate::runtime::collections::list(items))
}

#[no_mangle]
pub extern "C" fn unrt_list_repeat_int(value: i64, len: i64) -> u64 {
    if len < 0 {
        return to_raw(crate::runtime::obj::err("expected all items in 'size' list to be non-negative integers"));
    }
    let items: Vec<crate::runtime::obj::Obj> =
        (0..len).map(|_| crate::runtime::obj::int(value)).collect();
    to_raw(crate::runtime::collections::list(items))
}

#[no_mangle]
pub extern "C" fn unrt_list_repeat_float(value: f64, len: i64) -> u64 {
    if len < 0 {
        return to_raw(crate::runtime::obj::err("expected all items in 'size' list to be non-negative integers"));
    }
    let items: Vec<crate::runtime::obj::Obj> =
        (0..len).map(|_| crate::runtime::obj::float(value)).collect();
    to_raw(crate::runtime::collections::list(items))
}

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

#[no_mangle]
pub extern "C" fn unrt_len(o: u64) -> u64 {
    let Some(o) = borrow_obj(o) else {
        return to_raw(crate::runtime::obj::err("unrt: len() argument is not an object"));
    };
    to_raw(crate::runtime::obj::len(&o))
}

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

/// Fused `list[int]` write for the standalone native path (matching the NRT
/// ABI). Takes the value as a raw i64 so the caller does not need a boxing FFI,
/// and returns the `list` handle on success (or an error Obj for an
/// out-of-range index). The generated code compares the result against the
/// input list handle via `emit_self_return_gate`, preserving the same error,
/// catch and traceback behaviour as the boxed path.
#[no_mangle]
pub extern "C" fn unrt_list_set_int(list: u64, idx: i64, value: i64) -> u64 {
    let Some(list) = borrow_obj(list) else {
        return to_raw(crate::runtime::obj::err("unrt: list argument is not a list"));
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
            let boxed = crate::runtime::obj::int(value);
            vec.borrow_mut()[i as usize] = boxed;
            to_raw(list)
        }
        _ => to_raw(crate::runtime::obj::err("unrt: set target is not a list")),
    }
}

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

/// Fused `dict[int, int]` lookup for the standalone native path (matching the
/// NRT ABI). On success returns the raw i64 value and writes 0 to `err_out`.
/// On failure writes the error Obj handle to `err_out` and returns 0. The
/// generated code checks `err_out` and routes the handle through the regular
/// error gate, preserving source span, message and catch semantics.
#[no_mangle]
pub extern "C" fn unrt_dict_get_int(dict: u64, key: i64, err_out: *mut u64) -> i64 {
    unsafe { *err_out = 0 };
    let Some(dict) = borrow_obj(dict) else {
        unsafe {
            *err_out = to_raw(crate::runtime::obj::err("unrt: dict argument is not a dict"));
        }
        return 0;
    };
    let key_obj = crate::runtime::obj::int(key);
    let value = match crate::runtime::obj::payload(&dict) {
        crate::runtime::obj::Payload::Dict(km) => {
            crate::runtime::collections::dict_get_item(&km, &key_obj)
        }
        _ => {
            unsafe {
                *err_out = to_raw(crate::runtime::obj::err("unrt: lookup target is not a dict"));
            }
            return 0;
        }
    };
    if crate::runtime::obj::is_err(&value) || crate::runtime::obj::is_panic(&value) {
        unsafe { *err_out = to_raw(value) };
        return 0;
    }
    match crate::runtime::obj::payload(&value) {
        crate::runtime::obj::Payload::Int(v) => v,
        _ => 0,
    }
}


#[no_mangle]
pub extern "C" fn unvm_exit(code: i32) -> ! {
    process::exit(code)
}


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

#[no_mangle]
pub extern "C" fn unrt_init() {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    crate::native::init(cwd.to_str().unwrap_or("."));
}

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

#[no_mangle]
pub extern "C" fn unrt_str_from_bytes(ptr: *const u8, len: usize) -> u64 {
    if ptr.is_null() {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let s = String::from_utf8_lossy(bytes).into_owned();
    to_raw(crate::runtime::obj::str(&s))
}

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
