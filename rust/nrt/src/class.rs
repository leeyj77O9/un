//! Native class objects. Class descriptors are registered from the generated
//! driver (`nrt_register_*`), so no bytecode or VM class registry is needed.

use std::sync::Mutex;

use crate::obj::{alloc, err, obj, NObj};

static CLASSES: Mutex<Vec<ClassData>> = Mutex::new(Vec::new());
/// Frozen registry used on the hot path: no lock, populated by the generated
/// driver after all `nrt_register_*` calls.
static FROZEN: std::sync::OnceLock<Vec<ClassData>> = std::sync::OnceLock::new();

fn classes() -> Option<&'static Vec<ClassData>> {
    FROZEN.get().or_else(|| {
        // Fallback for callers that did not freeze (e.g. a dynamically built
        // class); registration is complete in plain `un build` executables.
        None
    })
}

#[no_mangle]
pub extern "C" fn nrt_freeze_classes() {
    if FROZEN.get().is_none() {
        let mut guard = CLASSES.lock().unwrap();
        let data = std::mem::take(&mut *guard);
        let _ = FROZEN.set(data);
    }
}

/// 0 Int, 1 Float, 2 Bool, 3 object handle (also used for `self`).
pub(crate) type Kind = u8;
struct Method {
    name: String,
    ptr: usize,
    /// Parameter kinds excluding `self`, stored inline to avoid a heap clone
    /// on every method call. MAX_CORE_ARITY is 8, plus one `self` slot.
    param_kinds: [Kind; 9],
    kinds_len: usize,
    ret_kind: Kind,
}

struct ClassData {
    name: String,
    fields: Vec<String>,
    field_names: Vec<String>,
    methods: Vec<Method>,
}

fn name_of(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf8_lossy(bytes).into_owned()
}

/// Borrow the static name bytes embedded by codegen. Runtime lookups compare
/// raw bytes directly, avoiding a `String` allocation per member access.
fn bytes_of(ptr: *const u8, len: usize) -> &'static [u8] {
    if ptr.is_null() || len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }
}

#[no_mangle]
pub extern "C" fn nrt_register_class(name_ptr: *const u8, name_len: usize) -> i64 {
    let mut classes = CLASSES.lock().unwrap();
    let id = classes.len();
    classes.push(ClassData {
        name: name_of(name_ptr, name_len),
        fields: Vec::new(),
        field_names: Vec::new(),
        methods: Vec::new(),
    });
    id as i64
}

#[no_mangle]
pub extern "C" fn nrt_register_field(class_id: i64, name_ptr: *const u8, name_len: usize) {
    if let Some(class) = CLASSES.lock().unwrap().get_mut(class_id as usize) {
        let n = name_of(name_ptr, name_len);
        class.field_names.push(n.clone());
        class.fields.push(n);
    }
}

#[no_mangle]
pub extern "C" fn nrt_register_method(
    class_id: i64,
    name_ptr: *const u8,
    name_len: usize,
    fn_ptr: usize,
    kinds_ptr: *const u8,
    kinds_len: usize,
    ret_kind: u8,
) {
    let mut param_kinds = [0u8; 9];
    let kinds = if kinds_ptr.is_null() {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(kinds_ptr, kinds_len.min(9)) }
    };
    param_kinds[..kinds.len()].copy_from_slice(kinds);
    if let Some(class) = CLASSES.lock().unwrap().get_mut(class_id as usize) {
        class.methods.push(Method {
            name: name_of(name_ptr, name_len),
            ptr: fn_ptr,
            param_kinds,
            kinds_len: kinds.len(),
            ret_kind,
        });
    }
}
pub(crate) fn box_return(raw: u64, kind: u8) -> u64 {
    match kind {
        0 => crate::obj::unrt_obj_int(raw as i64),
        1 => crate::obj::unrt_obj_float(f64::from_bits(raw)),
        2 => crate::obj::unrt_obj_bool((raw & 0xff) as i64),
        _ => raw,
    }
}

pub(crate) fn call_native(ptr: usize, args: &[u64]) -> u64 {
    unsafe {
        match args.len() {
            0 => std::mem::transmute::<usize, extern "C" fn() -> u64>(ptr)(),
            1 => std::mem::transmute::<usize, extern "C" fn(u64) -> u64>(ptr)(args[0]),
            2 => std::mem::transmute::<usize, extern "C" fn(u64, u64) -> u64>(ptr)(args[0], args[1]),
            3 => std::mem::transmute::<usize, extern "C" fn(u64, u64, u64) -> u64>(ptr)(
                args[0], args[1], args[2],
            ),
            4 => std::mem::transmute::<usize, extern "C" fn(u64, u64, u64, u64) -> u64>(ptr)(
                args[0], args[1], args[2], args[3],
            ),
            5 => std::mem::transmute::<usize, extern "C" fn(u64, u64, u64, u64, u64) -> u64>(ptr)(
                args[0], args[1], args[2], args[3], args[4],
            ),
            6 => std::mem::transmute::<usize, extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64>(ptr)(
                args[0], args[1], args[2], args[3], args[4], args[5],
            ),
            7 => std::mem::transmute::<usize, extern "C" fn(u64, u64, u64, u64, u64, u64, u64) -> u64>(
                ptr,
            )(args[0], args[1], args[2], args[3], args[4], args[5], args[6]),
            _ => std::mem::transmute::<usize, extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64>(
                ptr,
            )(args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7]),
        }
    }
}

fn instance_fields(p: u64) -> &'static mut Vec<u64> {
    unsafe { &mut *(obj(p).b as *mut Vec<u64>) }
}

#[no_mangle]
pub extern "C" fn unrt_class_new(
    name_ptr: *const u8,
    name_len: usize,
    args: *const u64,
    argc: usize,
) -> u64 {
    let name = bytes_of(name_ptr, name_len);
    let registry = match classes() {
        Some(c) => c,
        None => return err("class registry is not frozen"),
    };
    let id = registry.iter().position(|c| c.name.as_bytes() == name);
    let Some(id) = id else {
        return err(&format!("class '{}' not registered", String::from_utf8_lossy(name)));
    };
    // `none` is immutable; share one boxed object instead of allocating one
    // per field per instance.
    static NONE_OBJ: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let none = *NONE_OBJ.get_or_init(|| crate::obj::unrt_obj_none());
    let mut fields: Vec<u64> = Vec::new();
    fields.resize(registry[id].fields.len(), none);
    let p = Box::into_raw(Box::new(fields));
    let instance = unsafe { alloc(NObj { tag: 9, a: id as i64, b: p as i64 }) };

    // Copy the descriptor out before calling: `__init__` writes fields via
    // `unrt_member_set`, which locks the same registry (Mutex is not reentrant).
    let init = registry[id]
        .methods
        .iter()
        .find(|m| m.name == "__init__")
        .map(|m| (m.ptr, m.param_kinds, m.kinds_len));
    if let Some((f, kinds, kinds_len)) = init {
        if std::env::var_os("NRT_CLASS_DEBUG").is_some() {
            eprintln!("NRT class_new id={id} argc={argc} kinds_len={kinds_len}");
        }
        let supplied = unsafe { std::slice::from_raw_parts(args, argc) };
        let n = supplied.len().min(8);
        let mut call_args = [0u64; 9];
        call_args[0] = instance;
        call_args[1..=n].copy_from_slice(&supplied[..n]);
        let _ = call_native_at(f, &kinds[..kinds_len], &call_args[..n + 1]);
    } else {
        let call_args = unsafe { std::slice::from_raw_parts(args, argc) };
        let f = instance_fields(instance);
        for (i, v) in call_args.iter().enumerate().take(f.len()) {
            f[i] = *v;
        }
    }
    instance
}

fn call_native_at(ptr: usize, kinds: &[Kind], args: &[u64]) -> u64 {
    // `args` starts with `self` (an object handle). Remaining values arrive as
    // boxed handles from the bridge but the emitted function expects raw
    // scalars for Int/Float/Bool parameters, so unbox according to `kinds`.
    // MAX_CORE_ARITY is 8, plus one `self` handle.
    let mut a = [0u64; 9];
    a[0] = args[0];
    for (i, kind) in kinds.iter().enumerate() {
        let p = args.get(i + 1).copied().unwrap_or_else(|| crate::obj::unrt_obj_int(0));
        a[i + 1] = match kind {
            0 => crate::obj::unrt_int_from_obj(p) as u64,
            1 => crate::obj::float_of(p).to_bits(),
            2 => crate::obj::unrt_bool_from_obj(p) as u64,
            _ => p,
        };
    }
    call_native(ptr, &a[..kinds.len() + 1])
}

#[no_mangle]
pub extern "C" fn unrt_method_call(
    obj_handle: u64,
    name_ptr: *const u8,
    name_len: usize,
    args: *const u64,
    argc: usize,
) -> u64 {
    let name = bytes_of(name_ptr, name_len);
    unsafe {
        if obj(obj_handle).tag != 9 {
            return err("unrt_method_call: receiver is not a class instance");
        }
    }
    let id = unsafe { obj(obj_handle).a as usize };
    let mut call_args = [0u64; 9];
    call_args[0] = obj_handle;
    let supplied = unsafe { std::slice::from_raw_parts(args, argc) };
    let n = supplied.len().min(8);
    call_args[1..=n].copy_from_slice(&supplied[..n]);
    // Copy the descriptor out before invoking: the method body calls back into
    // `unrt_member_get/set`, which locks the same registry mutex.
    let registry = match classes() {
        Some(c) => c,
        None => return err("class registry is not frozen"),
    };
    let method = registry.get(id).and_then(|c| {
        c.methods
            .iter()
            .find(|m| m.name.as_bytes() == name)
            .map(|m| (m.ptr, m.param_kinds, m.kinds_len, m.ret_kind))
    });
    let Some((f, kinds, kinds_len, ret_kind)) = method else {
        return err(&format!(
            "class '{}' has no method '{}'",
            registry.get(id).map(|c| c.name.as_str()).unwrap_or("?"),
            String::from_utf8_lossy(name)
        ));
    };
    let raw = call_native_at(f, &kinds[..kinds_len], &call_args[..n + 1]);
    box_return(raw, ret_kind)
}

#[no_mangle]
pub extern "C" fn unrt_member_get(obj_handle: u64, name_ptr: *const u8, name_len: usize) -> u64 {
    let name = bytes_of(name_ptr, name_len);
    unsafe {
        if obj(obj_handle).tag != 9 {
            return err("unrt: member access on a non-instance");
        }
    }
    let id = unsafe { obj(obj_handle).a as usize };
    let registry = match classes() {
        Some(c) => c,
        None => return err("class registry is not frozen"),
    };
    let idx = registry.get(id).and_then(|c| c.fields.iter().position(|f| f.as_bytes() == name));
    match idx {
        Some(i) => instance_fields(obj_handle)[i],
        None => err(&format!("class '{}' has no field '{}'", registry.get(id).map(|c| c.name.as_str()).unwrap_or("?"), String::from_utf8_lossy(name))),
    }
}

#[no_mangle]
pub extern "C" fn unrt_member_set(
    obj_handle: u64,
    name_ptr: *const u8,
    name_len: usize,
    value: u64,
) -> u64 {
    let name = bytes_of(name_ptr, name_len);
    unsafe {
        if obj(obj_handle).tag != 9 {
            return err("unrt: member set on a non-instance");
        }
    }
    let id = unsafe { obj(obj_handle).a as usize };
    let registry = match classes() {
        Some(c) => c,
        None => return err("class registry is not frozen"),
    };
    let idx = registry.get(id).and_then(|c| c.fields.iter().position(|f| f.as_bytes() == name));
    match idx {
        Some(i) => instance_fields(obj_handle)[i] = value,
        None => return err(&format!("class '{}' has no field '{}'", registry.get(id).map(|c| c.name.as_str()).unwrap_or("?"), String::from_utf8_lossy(name))),
    }
    obj_handle
}

pub(crate) fn display_instance(p: u64) -> String {
    let classes = match classes() {
        Some(c) => c,
        None => return String::new(),
    };
    let id = unsafe { obj(p).a as usize };
    let Some(class) = classes.get(id) else { return String::new() };
    let fields = instance_fields(p);
    let parts: Vec<String> = class.field_names.iter().enumerate()
        .filter_map(|(i, name)| fields.get(i).map(|v| format!("{}: {}", name, crate::obj::display(*v))))
        .collect();
    if parts.is_empty() { class.name.clone() }
    else { format!("{}({})", class.name, parts.join(", ")) }
}
