//! Native class objects. Class descriptors are registered from the generated
//! driver (`nrt_register_*`), so no bytecode or VM class registry is needed.

use std::sync::Mutex;

use crate::obj::{alloc, err, obj, NObj};

static CLASSES: Mutex<Vec<ClassData>> = Mutex::new(Vec::new());

/// 0 Int, 1 Float, 2 Bool, 3 object handle (also used for `self`).
pub(crate) type Kind = u8;
struct Method {
    name: String,
    ptr: usize,
    /// Parameter kinds excluding `self`.
    param_kinds: Vec<Kind>,
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
    let param_kinds = if kinds_ptr.is_null() || kinds_len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(kinds_ptr, kinds_len) }.to_vec()
    };
    if let Some(class) = CLASSES.lock().unwrap().get_mut(class_id as usize) {
        class.methods.push(Method {
            name: name_of(name_ptr, name_len),
            ptr: fn_ptr,
            param_kinds,
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
    let name = name_of(name_ptr, name_len);
    let id = CLASSES.lock().unwrap().iter().position(|c| c.name == name);
    let Some(id) = id else {
        return err(&format!("class '{name}' not registered"));
    };
    let mut fields: Vec<u64> = Vec::new();
    {
        let classes = CLASSES.lock().unwrap();
        fields.resize(classes[id].fields.len(), crate::obj::unrt_obj_none());
    }
    let p = Box::into_raw(Box::new(fields));
    let instance = unsafe { alloc(NObj { tag: 9, a: id as i64, b: p as i64 }) };

    let init = {
        let classes = CLASSES.lock().unwrap();
        classes[id]
            .methods
            .iter()
            .find(|m| m.name == "__init__")
            .map(|m| (m.ptr, m.param_kinds.clone()))
    };
    if let Some((f, kinds)) = init {
        let mut call_args = vec![instance];
        call_args.extend_from_slice(unsafe { std::slice::from_raw_parts(args, argc) });
        let _ = call_native_at(f, &kinds, &call_args);
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
    let mut a = Vec::with_capacity(kinds.len() + 1);
    a.push(args[0]);
    for (i, kind) in kinds.iter().enumerate() {
        let p = args.get(i + 1).copied().unwrap_or_else(|| crate::obj::unrt_obj_int(0));
        a.push(match kind {
            0 => crate::obj::unrt_int_from_obj(p) as u64,
            1 => crate::obj::float_of(p).to_bits(),
            2 => crate::obj::unrt_bool_from_obj(p) as u64,
            _ => p,
        });
    }
    call_native(ptr, &a)
}

#[no_mangle]
pub extern "C" fn unrt_method_call(
    obj_handle: u64,
    name_ptr: *const u8,
    name_len: usize,
    args: *const u64,
    argc: usize,
) -> u64 {
    let name = name_of(name_ptr, name_len);
    unsafe {
        if obj(obj_handle).tag != 9 {
            return err("unrt_method_call: receiver is not a class instance");
        }
    }
    let id = unsafe { obj(obj_handle).a as usize };
    let method = CLASSES.lock().unwrap().get(id).and_then(|c| {
        c.methods.iter().find(|m| m.name == name).map(|m| (m.ptr, m.param_kinds.clone(), m.ret_kind))
    });
    let Some((f, kinds, ret_kind)) = method else {
        return err(&format!("class '{}' has no method '{name}'", CLASSES.lock().unwrap()[id].name));
    };
    let mut call_args = vec![obj_handle];
    call_args.extend_from_slice(unsafe { std::slice::from_raw_parts(args, argc) });
    let raw = call_native_at(f, &kinds, &call_args);
    box_return(raw, ret_kind)
}

#[no_mangle]
pub extern "C" fn unrt_member_get(obj_handle: u64, name_ptr: *const u8, name_len: usize) -> u64 {
    let name = name_of(name_ptr, name_len);
    unsafe {
        if obj(obj_handle).tag != 9 {
            return err("unrt: member access on a non-instance");
        }
    }
    let id = unsafe { obj(obj_handle).a as usize };
    let idx = CLASSES.lock().unwrap().get(id).and_then(|c| c.fields.iter().position(|f| *f == name));
    match idx {
        Some(i) => instance_fields(obj_handle)[i],
        None => err(&format!("class '{}' has no field '{name}'", CLASSES.lock().unwrap()[id].name)),
    }
}

#[no_mangle]
pub extern "C" fn unrt_member_set(
    obj_handle: u64,
    name_ptr: *const u8,
    name_len: usize,
    value: u64,
) -> u64 {
    let name = name_of(name_ptr, name_len);
    unsafe {
        if obj(obj_handle).tag != 9 {
            return err("unrt: member set on a non-instance");
        }
    }
    let id = unsafe { obj(obj_handle).a as usize };
    let idx = CLASSES.lock().unwrap().get(id).and_then(|c| c.fields.iter().position(|f| *f == name));
    match idx {
        Some(i) => instance_fields(obj_handle)[i] = value,
        None => return err(&format!("class '{}' has no field '{name}'", CLASSES.lock().unwrap()[id].name)),
    }
    obj_handle
}

pub(crate) fn display_instance(p: u64) -> String {
    let classes = CLASSES.lock().unwrap();
    let id = unsafe { obj(p).a as usize };
    let Some(class) = classes.get(id) else { return String::new() };
    let fields = instance_fields(p);
    let parts: Vec<String> = class.field_names.iter().enumerate()
        .filter_map(|(i, name)| fields.get(i).map(|v| format!("{}: {}", name, crate::obj::display(*v))))
        .collect();
    if parts.is_empty() { class.name.clone() }
    else { format!("{}({})", class.name, parts.join(", ")) }
}
