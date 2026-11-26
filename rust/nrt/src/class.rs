//! Native class objects. Class descriptors are registered from the generated
//! driver (`nrt_register_*`), so no bytecode or VM class registry is needed.

use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
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

pub(crate) fn classes() -> Option<&'static Vec<ClassData>> {
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
#[derive(Clone)]
pub(crate) struct Method {
    pub(crate) name: String,
    pub(crate) ptr: usize,
    /// Parameter kinds excluding `self`, stored inline to avoid a heap clone
    /// on every method call. MAX_CORE_ARITY is 8, plus one `self` slot.
    pub(crate) param_kinds: [Kind; 9],
    pub(crate) kinds_len: usize,
    pub(crate) ret_kind: Kind,
}

pub(crate) struct ClassData {
    pub(crate) name: String,
    pub(crate) fields: Vec<String>,
    pub(crate) field_names: Vec<String>,
    pub(crate) methods: Vec<Method>,
    /// Registered base class names (for type patterns with inheritance).
    pub(crate) parents: Vec<String>,
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
// P02: the three former `static mut` arrays are thread-local `RefCell`s. A
// plain native exe is single-threaded, so the hit/write sequence is unchanged
// byte-for-byte; an opt-in worker gets its own cache and can never race
// another worker on a slot. A hit is still validated against the frozen
// registry before use, so a stale slot can only fall back to the original
// linear search — it can never return a wrong index or change an error
// message.
const NAME_CACHE_SLOTS: usize = 16;
const NO_OWNER: usize = usize::MAX;
/// (name pointer, name length, owner id, resolved index)
type NameCacheEntry = (u64, usize, usize, usize);
type NameCache = RefCell<[NameCacheEntry; NAME_CACHE_SLOTS]>;

thread_local! {
    static CLASS_ID_CACHE: NameCache =
        RefCell::new([(0, 0, NO_OWNER, NO_OWNER); NAME_CACHE_SLOTS]);
    static FIELD_IDX_CACHE: NameCache =
        RefCell::new([(0, 0, NO_OWNER, NO_OWNER); NAME_CACHE_SLOTS]);
    static METHOD_IDX_CACHE: NameCache =
        RefCell::new([(0, 0, NO_OWNER, NO_OWNER); NAME_CACHE_SLOTS]);
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
fn cached_index<S>(
    cache: &'static std::thread::LocalKey<NameCache>,
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
    let e = cache.with(|c| c.borrow()[slot]);
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
        cache.with(|c| c.borrow_mut()[slot] = (name_ptr as u64, name.len(), owner, i));
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
        parents: Vec::new(),
    });
    id as i64
}

/// Register a base class of an already-registered class (inheritance data for
/// type patterns). Repeated registrations of the same parent are ignored.
#[no_mangle]
pub extern "C" fn nrt_register_class_parent(
    class_id: i64,
    parent_name_ptr: *const u8,
    parent_len: usize,
) {
    if class_id < 0 {
        return;
    }
    let parent = name_of(parent_name_ptr, parent_len);
    if parent.is_empty() {
        return;
    }
    let mut classes = CLASSES.lock().unwrap();
    if let Some(class) = classes.get_mut(class_id as usize) {
        if !class.parents.iter().any(|p| *p == parent) {
            class.parents.push(parent);
        }
    }
}

/// True when `id` itself or any registered ancestor is named `name`.
pub(crate) fn class_chain_accepts(id: usize, name: &str) -> bool {
    let Some(cs) = classes() else {
        return false;
    };
    let mut seen = vec![id];
    let mut stack = vec![id];
    while let Some(cur) = stack.pop() {
        let Some(class) = cs.get(cur) else {
            continue;
        };
        if class.name == name {
            return true;
        }
        for parent in &class.parents {
            if let Some(pid) = cs.iter().position(|c| c.name == *parent) {
                if seen.contains(&pid) {
                    continue;
                }
                seen.push(pid);
                stack.push(pid);
            }
        }
    }
    false
}

/// Registered name of a class instance's class id (tag-9 `a` field).
/// Used by type-name patterns to compare a receiver against a class name.
pub(crate) fn class_name(id: usize) -> Option<&'static str> {
    classes().and_then(|cs| cs.get(id)).map(|c| c.name.as_str())
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

// Indirect calls now go through `abi::call_native_typed`, which reconstructs
// the Cranelift C signature (including XMM Float arguments/returns) from the
// registered parameter kinds. The former all-`u64` shim is intentionally gone:
// it silently passed Float values in general-purpose registers.

// Instance fields never need deallocation (plain executables are short-lived),
// so they are packed into a bump arena instead of one heap allocation per
// object. P02: the arena is a `Mutex<Vec<u64>>` with an `AtomicUsize` bump
// cursor, and `NObj.b` stores the *offset* of the block's count word (an opaque
// i64 handle at the C ABI), not a raw pointer. Indices stay stable when the
// `Vec` grows, removing the old reallocation-dangling hazard, and no
// `&'static mut` slice ever escapes the lock.
static FIELD_ARENA: Mutex<Vec<u64>> = Mutex::new(Vec::new());
static FIELD_BUMP: AtomicUsize = AtomicUsize::new(0);

/// Reserve `[count, field0, ...]` and return the block's arena offset.
fn alloc_field_block(n: usize) -> usize {
    let off = FIELD_BUMP.fetch_add(n + 1, Ordering::Relaxed);
    let needed = off + n + 1;
    let mut arena = FIELD_ARENA.lock().unwrap();
    if needed > arena.len() {
        arena.resize(needed, 0);
    }
    arena[off] = n as u64;
    off
}

/// Fill a freshly reserved block. Used before the instance handle exists.
fn fill_field_block(off: usize, n: usize, value: u64) {
    let mut arena = FIELD_ARENA.lock().unwrap();
    arena[off + 1..off + 1 + n].fill(value);
}

/// Snapshot an instance's field words. Read-only callers use this so the lock
/// is released before any nested `display` / `eq_obj` call can re-enter.
pub(crate) fn instance_fields(p: u64) -> Vec<u64> {
    let off = unsafe { obj(p).b as usize };
    let arena = FIELD_ARENA.lock().unwrap();
    let len = arena[off] as usize;
    arena[off + 1..off + 1 + len].to_vec()
}

/// Mutate an instance's field words under the arena lock. The closure cannot
/// escape, so no `&'static mut` is exposed to callers.
pub(crate) fn with_instance_fields<R>(p: u64, f: impl FnOnce(&mut [u64]) -> R) -> R {
    let off = unsafe { obj(p).b as usize };
    let mut arena = FIELD_ARENA.lock().unwrap();
    let len = arena[off] as usize;
    f(&mut arena[off + 1..off + 1 + len])
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
    let id = cached_index(
        &CLASS_ID_CACHE,
        name,
        name_ptr,
        NO_OWNER,
        |i| i < registry.len(),
        || registry.iter().position(|c| c.name.as_bytes() == name),
    );
    let Some(id) = id else {
        return err(&format!("class '{}' not registered", String::from_utf8_lossy(name)));
    };
    // `none` is immutable; share one boxed object instead of allocating one
    // per field per instance.
    static NONE_OBJ: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let none = *NONE_OBJ.get_or_init(|| crate::obj::unrt_obj_none());
    let nfields = registry[id].fields.len();
    let fields_off = alloc_field_block(nfields);
    fill_field_block(fields_off, nfields, none);
    // The arena owns the field words; `b` is the offset of the count header.
    let instance = unsafe { alloc(NObj { tag: 9, a: id as i64, b: fields_off as i64 }) };

    // Copy the descriptor out before calling: `__init__` writes fields via
    // `unrt_member_set`, which locks the same registry (Mutex is not reentrant).
    let init = registry[id]
        .methods
        .iter()
        .find(|m| m.name == "__init__")
        .map(|m| (m.ptr, m.param_kinds, m.kinds_len, m.ret_kind));
    if let Some((f, kinds, kinds_len, ret_kind)) = init {
        if class_debug() {
            eprintln!("NRT class_new id={id} argc={argc} kinds_len={kinds_len}");
        }
        let supplied = unsafe { std::slice::from_raw_parts(args, argc) };
        let n = supplied.len().min(8);
        let mut call_args = [0u64; 9];
        call_args[0] = instance;
        call_args[1..=n].copy_from_slice(&supplied[..n]);
        let _ = call_native_at(f, &kinds[..kinds_len], &call_args[..n + 1], ret_kind);
    } else {
        let call_args = unsafe { std::slice::from_raw_parts(args, argc) };
        with_instance_fields(instance, |f| {
            for (i, v) in call_args.iter().enumerate().take(f.len()) {
                f[i] = *v;
            }
        });
    }
    instance
}

pub(crate) fn call_native_at(ptr: usize, kinds: &[Kind], args: &[u64], ret_kind: Kind) -> u64 {
    // `args` starts with `self` (an object handle). Remaining values arrive as
    // boxed handles from the bridge but the emitted function expects raw
    // scalars for Int/Float/Bool parameters, so unbox according to `kinds`.
    // MAX_CORE_ARITY is 8, plus one `self` handle.
    let mut a = [0u64; 9];
    let mut k = [3u8; 9]; // self is an object handle in a GPR
    a[0] = args[0];
    for (i, kind) in kinds.iter().enumerate() {
        let p = args.get(i + 1).copied().unwrap_or_else(|| crate::obj::unrt_obj_int(0));
        a[i + 1] = match kind {
            0 => crate::obj::unrt_int_from_obj(p) as u64,
            1 => crate::obj::float_of(p).to_bits(),
            2 => crate::obj::unrt_bool_from_obj(p) as u64,
            _ => p,
        };
        k[i + 1] = *kind;
    }
    let n = kinds.len() + 1;
    crate::abi::call_native_typed(ptr, &a[..n], &k[..n], ret_kind)
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
        let tag = obj(obj_handle).tag;
        if tag == 11 || tag == 12 {
            // Native opaque handles (`http_client` / `web_socket`) dispatch
            // through the same table as their module-level constructors.
            let supplied = std::slice::from_raw_parts(args, argc);
            let mut call_args = [0u64; 9];
            call_args[0] = obj_handle;
            let n = supplied.len().min(8);
            call_args[1..=n].copy_from_slice(&supplied[..n]);
            let module = if tag == 11 { "http_client" } else { "web_socket" };
            let member = std::str::from_utf8(name).unwrap_or("");
            return crate::net_native::dispatch(module, member, &call_args[..=n])
                .unwrap_or_else(|| err("unrt_method_call: unknown native method"));
        }
        if tag != 9 {
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
    let method_idx = cached_index(
        &METHOD_IDX_CACHE,
        name,
        name_ptr,
        id,
        |i| registry.get(id).map_or(false, |c| i < c.methods.len()),
        || registry.get(id).and_then(|c| c.methods.iter().position(|m| m.name.as_bytes() == name)),
    );
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
    let raw = call_native_at(f, &kinds[..kinds_len], &call_args[..n + 1], ret_kind);
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
    let idx = cached_index(
        &FIELD_IDX_CACHE,
        name,
        name_ptr,
        id,
        |i| registry.get(id).map_or(false, |c| i < c.fields.len()),
        || registry.get(id).and_then(|c| c.fields.iter().position(|f| f.as_bytes() == name)),
    );
    match idx {
        Some(i) => with_instance_fields(obj_handle, |fields| fields[i]),
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
    let idx = cached_index(
        &FIELD_IDX_CACHE,
        name,
        name_ptr,
        id,
        |i| registry.get(id).map_or(false, |c| i < c.fields.len()),
        || registry.get(id).and_then(|c| c.fields.iter().position(|f| f.as_bytes() == name)),
    );
    match idx {
        Some(i) => with_instance_fields(obj_handle, |fields| fields[i] = value),
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
    // CMP-05 lowers data-carrying enum variants to a synthetic class with the
    // `__un_variant` (Str) marker and a `value` payload field. Mirror the VM's
    // `collections::enu_to_str`: `Enum.variant(payload)`. Ordinary classes keep
    // the existing `Name(field: value, ...)` display below.
    if class.field_names.len() == 2
        && class.field_names[0] == "__un_variant"
        && class.field_names[1] == "value"
    {
        if let Some(&marker) = fields.get(0) {
            if unsafe { obj(marker).tag } == 6 {
                let variant = unsafe { crate::strings::str_of(marker) };
                let payload = fields
                    .get(1)
                    .copied()
                    .unwrap_or_else(|| crate::obj::unrt_obj_none());
                return format!(
                    "{}.{}({})",
                    class.name,
                    variant,
                    crate::obj::display(payload)
                );
            }
        }
    }
    let parts: Vec<String> = class.field_names.iter().enumerate()
        .filter_map(|(i, name)| fields.get(i).map(|v| format!("{}: {}", name, crate::obj::display(*v))))
        .collect();
    if parts.is_empty() { class.name.clone() }
    else { format!("{}({})", class.name, parts.join(", ")) }
}

#[cfg(test)]
mod p02_tests {
    use super::*;

    /// Build a tag-9 handle directly over an arena offset. This keeps these
    /// tests independent of the process-global class registry (which other
    /// tests freeze), while exercising exactly the same field-block path as
    /// `unrt_class_new` / `unrt_member_get` / `unrt_member_set`.
    fn instance_at(off: usize) -> u64 {
        unsafe { alloc(NObj { tag: 9, a: 0, b: off as i64 }) }
    }

    #[test]
    fn field_block_roundtrip_via_offset_handle() {
        let n = 3;
        let off = alloc_field_block(n);
        let sentinel = crate::obj::unrt_obj_int(42);
        fill_field_block(off, n, sentinel);
        let p = instance_at(off);
        assert_eq!(instance_fields(p), vec![sentinel; n]);

        let other = crate::obj::unrt_obj_int(7);
        with_instance_fields(p, |fields| fields[1] = other);
        assert_eq!(instance_fields(p), vec![sentinel, other, sentinel]);
    }

    #[test]
    fn field_handles_survive_arena_growth() {
        // Grow the backing Vec well past its initial size. The old
        // pointer-in-`b` representation dangled after a realloc; offset
        // handles stay valid and must return the identical field words.
        let mut handles = Vec::new();
        for i in 0..3000i64 {
            let off = alloc_field_block(2);
            let v = crate::obj::unrt_obj_int(i);
            fill_field_block(off, 2, v);
            handles.push(instance_at(off));
        }
        for (i, p) in handles.iter().enumerate() {
            let expected = crate::obj::unrt_obj_int(i as i64);
            assert_eq!(instance_fields(*p), vec![expected, expected]);
        }
    }

    #[test]
    fn name_cache_hit_miss_and_bounds_guard() {
        let name = b"p02-cached-name\0";
        let ptr = name.as_ptr();
        let probe = || {
            cached_index(&FIELD_IDX_CACHE, name, ptr, 7, |i| i < 8, || Some(5))
        };
        assert_eq!(probe(), Some(5));
        assert_eq!(probe(), Some(5));
        assert_eq!(probe(), Some(5));

        // A different name misses the slot and caches its own result.
        let other = b"p02-other-name\0";
        assert_eq!(
            cached_index(
                &FIELD_IDX_CACHE,
                other,
                other.as_ptr(),
                7,
                |i| i < 8,
                || Some(3),
            ),
            Some(3),
        );

        // A cached index outside the registry's bounds must be treated as a
        // miss, falling back to `search` (None here) — never returned stale.
        assert_eq!(
            cached_index(&FIELD_IDX_CACHE, name, ptr, 7, |i| i < 3, || None),
            None,
        );
    }
}
