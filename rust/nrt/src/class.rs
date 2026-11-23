//! Native class objects. Class descriptors are registered from the generated
//! driver (`nrt_register_*`), so no bytecode or VM class registry is needed.

use std::sync::{Mutex, OnceLock};

use crate::obj::{alloc, err, obj, NObj};

// `std::env::var_os` on Windows performs an OS call; `unrt_class_new` sits on
// the hot path of class benchmarks and must not pay for it per instance.
static CLASS_DEBUG: OnceLock<bool> = OnceLock::new();
#[inline(always)]
fn class_debug() -> bool {
    *CLASS_DEBUG.get_or_init(|| std::env::var_os("NRT_CLASS_DEBUG").is_some())
}

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

// --- Internal name -> registry-index caches ---------------------------------
// Codegen embeds one static byte string per name reference, so the same
// `(pointer, length)` pair repeats on every call. A direct-mapped cache turns
// the repeated linear scans in `unrt_class_new`, `unrt_member_get/set` and
// `unrt_method_call` into a single indexed probe.
//
// SAFETY: plain `un build` executables are single-threaded (the NRT subset has
// no `go`/`wait`), so these `static mut` slots have one reader/writer. Every
// access goes through a raw pointer obtained with `addr_of_mut!`; no reference
// to the static itself is created. A hit is validated against the frozen
// registry before use, so a stale or aliased slot can only fall back to the
// original linear search — it can never return a wrong index or change an
// error message.
const NAME_CACHE_SLOTS: usize = 16;
const NO_OWNER: usize = usize::MAX;
/// (name pointer, name length, owner id, resolved index)
type NameCacheEntry = (u64, usize, usize, usize);
static mut CLASS_ID_CACHE: [NameCacheEntry; NAME_CACHE_SLOTS] =
    [(0, 0, NO_OWNER, NO_OWNER); NAME_CACHE_SLOTS];
static mut FIELD_IDX_CACHE: [NameCacheEntry; NAME_CACHE_SLOTS] =
    [(0, 0, NO_OWNER, NO_OWNER); NAME_CACHE_SLOTS];
static mut METHOD_IDX_CACHE: [NameCacheEntry; NAME_CACHE_SLOTS] =
    [(0, 0, NO_OWNER, NO_OWNER); NAME_CACHE_SLOTS];

#[inline(always)]
fn class_id_cache() -> *mut NameCacheEntry {
    std::ptr::addr_of_mut!(CLASS_ID_CACHE) as *mut NameCacheEntry
}
#[inline(always)]
fn field_idx_cache() -> *mut NameCacheEntry {
    std::ptr::addr_of_mut!(FIELD_IDX_CACHE) as *mut NameCacheEntry
}
#[inline(always)]
fn method_idx_cache() -> *mut NameCacheEntry {
    std::ptr::addr_of_mut!(METHOD_IDX_CACHE) as *mut NameCacheEntry
}

#[inline(always)]
fn name_cache_slot(name_ptr: *const u8, name_len: usize, owner: usize) -> usize {
    ((name_ptr as usize >> 4) ^ name_len.wrapping_mul(31) ^ owner.wrapping_mul(131))
        & (NAME_CACHE_SLOTS - 1)
}

/// Probe the cache; on miss run `search`, then cache a successful result.
/// `in_bounds` guards the cached index against the frozen registry.
///
/// The registry is immutable after `nrt_freeze_classes` (the only path that
/// makes `classes()` return `Some`), and codegen names are static byte strings,
/// so `(pointer, length)` identity plus an index bounds check is sufficient —
/// re-comparing the name bytes on every hit would cost as much as the scan.
#[inline(always)]
unsafe fn cached_index<S>(
    cache: *mut NameCacheEntry,
    name: &[u8],
    name_ptr: *const u8,
    owner: usize,
    in_bounds: impl Fn(usize) -> bool,
    search: S,
) -> Option<usize>
where
    S: FnOnce() -> Option<usize>,
{
    let slot = name_cache_slot(name_ptr, name.len(), owner);
    let e = *cache.add(slot);
    if e.0 == name_ptr as u64
        && e.1 == name.len()
        && e.2 == owner
        && e.3 != NO_OWNER
        && in_bounds(e.3)
    {
        return Some(e.3);
    }
    let found = search();
    if let Some(i) = found {
        *cache.add(slot) = (name_ptr as u64, name.len(), owner, i);
    }
    found
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

// Instance fields never need deallocation (plain executables are short-lived
// and single-threaded), so pack them into a bump arena instead of doing one
// heap allocation per object. `NObj.b` is an opaque i64 handle at the C ABI,
// so the internal pointer representation can change freely.
static mut FIELD_ARENA: Vec<u64> = Vec::new();
static mut FIELD_BUMP: usize = 0;

fn alloc_field_block(n: usize) -> &'static mut [u64] {
    unsafe {
        let bump = std::ptr::addr_of_mut!(FIELD_BUMP);
        let off = *bump;
        let needed = off + n + 1; // first word stores the field count
        let arena = std::ptr::addr_of_mut!(FIELD_ARENA);
        if needed > (*arena).capacity() {
            let target = needed.max((*arena).len() * 2).max(4096);
            (*arena).reserve(target);
        }
        if needed > (*arena).len() {
            (*arena).set_len(needed);
        }
        let base = (*arena).as_mut_ptr().add(off);
        *base = n as u64;
        *bump = needed;
        std::slice::from_raw_parts_mut(base.add(1), n)
    }
}

fn instance_fields(p: u64) -> &'static mut [u64] {
    unsafe {
        let base = obj(p).b as *mut u64;
        let len = *base as usize;
        std::slice::from_raw_parts_mut(base.add(1), len)
    }
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
    let id = unsafe {
        cached_index(
            class_id_cache(),
            name,
            name_ptr,
            NO_OWNER,
            |i| i < registry.len(),
            || registry.iter().position(|c| c.name.as_bytes() == name),
        )
    };
    let Some(id) = id else {
        return err(&format!("class '{}' not registered", String::from_utf8_lossy(name)));
    };
    // `none` is immutable; share one boxed object instead of allocating one
    // per field per instance.
    static NONE_OBJ: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let none = *NONE_OBJ.get_or_init(|| crate::obj::unrt_obj_none());
    let fields = alloc_field_block(registry[id].fields.len());
    fields.fill(none);
    // The arena owns the field words; `b` points at the count header.
    let fields_ptr = unsafe { fields.as_mut_ptr().sub(1) };
    let instance = unsafe { alloc(NObj { tag: 9, a: id as i64, b: fields_ptr as i64 }) };

    // Copy the descriptor out before calling: `__init__` writes fields via
    // `unrt_member_set`, which locks the same registry (Mutex is not reentrant).
    let init = registry[id]
        .methods
        .iter()
        .find(|m| m.name == "__init__")
        .map(|m| (m.ptr, m.param_kinds, m.kinds_len));
    if let Some((f, kinds, kinds_len)) = init {
        if class_debug() {
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
    let method_idx = unsafe {
        cached_index(
            method_idx_cache(),
            name,
            name_ptr,
            id,
            |i| registry.get(id).map_or(false, |c| i < c.methods.len()),
            || registry.get(id).and_then(|c| c.methods.iter().position(|m| m.name.as_bytes() == name)),
        )
    };
    let method = method_idx.and_then(|i| {
        registry
            .get(id)
            .and_then(|c| c.methods.get(i))
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
    let idx = unsafe {
        cached_index(
            field_idx_cache(),
            name,
            name_ptr,
            id,
            |i| registry.get(id).map_or(false, |c| i < c.fields.len()),
            || registry.get(id).and_then(|c| c.fields.iter().position(|f| f.as_bytes() == name)),
        )
    };
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
    let idx = unsafe {
        cached_index(
            field_idx_cache(),
            name,
            name_ptr,
            id,
            |i| registry.get(id).map_or(false, |c| i < c.fields.len()),
            || registry.get(id).and_then(|c| c.fields.iter().position(|f| f.as_bytes() == name)),
        )
    };
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
