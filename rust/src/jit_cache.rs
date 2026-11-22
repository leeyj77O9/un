
use cranelift_jit::JITModule;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::LazyLock;
use std::sync::Mutex;

pub const TAG_INT: i64 = 0;
pub const TAG_FLOAT: i64 = 1;
pub const TAG_BAIL: i64 = 2;
pub const TAG_BOOL: i64 = 3;
pub const TAG_NONE: i64 = 5;
pub const TAG_INTLIST: i64 = 6;

/// Layout of the 16-byte hidden-pointer result slot every JIT entry writes:
/// `[ value @ RESULT_VALUE_OFFSET, tag @ RESULT_TAG_OFFSET ]`.
/// Kept in lockstep with `vm::jit_call`'s `[i64; 2]` out buffer.
pub const RESULT_VALUE_OFFSET: i32 = 0;
pub const RESULT_TAG_OFFSET: i32 = 8;

/// Return-kind *bits* used by `jit_bc::ret_kinds` to describe which runtime
/// tags a callee may produce. These are bit flags, not tags: a proto that can
/// return either an Int or an IntList carries `RET_INT | RET_INTLIST`.
pub const RET_INT: u8 = 0x1;
pub const RET_FLOAT: u8 = 0x2;
pub const RET_BOOL: u8 = 0x4;
pub const RET_INTLIST: u8 = 0x8;
const RET_INT_OR_INTLIST: u8 = RET_INT | RET_INTLIST;

/// Map a pure `RET_*` kind to the single tag the JIT expects in the hidden
/// pointer. The heterogeneous `RET_INT | RET_INTLIST` case normalizes to
/// `TAG_INTLIST` because such a callee materializes its Ints as list handles
/// before returning. `None` means no single tag compare can validate it, so
/// the caller must reject the proto.
pub fn ret_kind_to_tag(kind: u8) -> Option<i64> {
    match kind {
        RET_INT => Some(TAG_INT),
        RET_FLOAT => Some(TAG_FLOAT),
        RET_BOOL => Some(TAG_BOOL),
        RET_INTLIST => Some(TAG_INTLIST),
        RET_INT_OR_INTLIST => Some(TAG_INTLIST),
        _ => None,
    }
}

/// Bit positions inside the second JIT argument (`param_tags`, host -> JIT).
///
/// **Two disjoint bit spaces — do not merge them:**
/// * low bits `1 << pos` (plus [`PARAM_MASK_UNSAFE_SHIFT`] for class methods)
///   are the bytecode JIT's *unsafe-param mask*: they mark parameters whose
///   handle/foreign value must bail rather than reach arithmetic;
/// * [`PARAM_MASK_FLOAT_BASE`] / [`PARAM_MASK_BOOL_BASE`] are the high bits
///   `jit_fast` sets for Float/Bool arguments. Those live in the host->JIT
///   `param_tags` word but are never consumed by the unsafe-param check.
pub const PARAM_MASK_FLOAT_BASE: usize = 32;
pub const PARAM_MASK_BOOL_BASE: usize = 40;
/// Class-method calls reserve argument slot 0 for the receiver, so the
/// unsafe-param bit for user argument `k` is `1 << (k + PARAM_MASK_UNSAFE_SHIFT)`.
pub const PARAM_MASK_UNSAFE_SHIFT: usize = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    Unbc,
    Int,
    Float,
    Bool,
}

#[derive(Clone)]
pub struct JitEntry {
    pub ptr: *const u8,
    pub arity: u32,
    pub arg_kinds: Vec<ArgKind>,
}

pub static JIT_DEPTH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub static JIT_MAX_DEPTH: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(crate::runtime::function::MAX_RECURSION_DEPTH);

pub(crate) fn sync_max_jit_depth(depth: usize) {
    JIT_MAX_DEPTH.store(depth, std::sync::atomic::Ordering::Relaxed);
}

#[no_mangle]
pub extern "C" fn un_jit_depth_enter() -> i64 {
    let depth = JIT_DEPTH.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let max = JIT_MAX_DEPTH.load(std::sync::atomic::Ordering::Relaxed);
    if max == 0 || depth < max {
        0
    } else {
        JIT_DEPTH.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        1
    }
}

#[no_mangle]
pub extern "C" fn un_jit_depth_exit() -> i64 {
    JIT_DEPTH.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    0
}

unsafe impl Send for JitEntry {}

static JIT_MODULES: LazyLock<Mutex<HashMap<u64, Vec<JITModule>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn stash_module(program_id: u64, module: JITModule) {
    JIT_MODULES
        .lock()
        .expect("jit_cache")
        .entry(program_id)
        .or_default()
        .push(module);
}

/// Bytecode-JIT compiled entries (`.unbc` -> native, registered by `jit_bc`).
static JIT_FNS: LazyLock<Mutex<HashMap<u64, HashMap<u32, JitEntry>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Compile-Mode TIR entries (registered by `natcode`). Kept in a **separate**
/// map from `JIT_FNS`: both use `JitEntry` and the same `(program_id, proto)`
/// key, so sharing one map made `get_tir_entry()` return bytecode entries and
/// silently route every call after the first one through `try_call_tir_entry`
/// (whose empty `arg_kinds` rejects argument-taking protos) into the
/// interpreter. See AUDIT-001.
static TIR_FNS: LazyLock<Mutex<HashMap<u64, HashMap<u32, JitEntry>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Number of native JIT invocations that actually reached `vm::jit_call`.
/// Used by regression tests to prove the JIT is really executing, not merely
/// registering a stub that always falls back to the interpreter.
pub static JIT_NATIVE_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub fn jit_native_calls() -> usize {
    JIT_NATIVE_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
pub fn reset_jit_native_calls() {
    JIT_NATIVE_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
}

static JIT_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("UV_JIT")
        .map(|v| {
            let v = v.trim();
            v != "0" && !v.eq_ignore_ascii_case("false") && !v.eq_ignore_ascii_case("off")
        })
        .unwrap_or(true)
});

pub fn jit_enabled() -> bool {
    *JIT_ENABLED
}

pub fn jit_strict() -> u8 {
    static STRICT: LazyLock<u8> = LazyLock::new(|| {
        std::env::var("UV_JIT")
            .map(|v| match v.trim() {
                "2" => 2,
                v if v.eq_ignore_ascii_case("strict") => 2,
                _ => 0,
            })
            .unwrap_or(0)
    });
    *STRICT
}

pub fn jit_debug() -> bool {
    static JIT_DEBUG: LazyLock<bool> =
        LazyLock::new(|| std::env::var_os("UV_JIT_DEBUG").is_some());
    *JIT_DEBUG
}

static JIT_LAZY_ATTEMPTED: LazyLock<Mutex<HashSet<u64>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub fn mark_jit_lazy_attempted(program_id: u64) {
    JIT_LAZY_ATTEMPTED.lock().expect("jit_cache").insert(program_id);
}

pub fn jit_lazy_attempted(program_id: u64) -> bool {
    JIT_LAZY_ATTEMPTED
        .lock()
        .map(|s| s.contains(&program_id))
        .unwrap_or(true)
}

pub fn register_jit_entry(program_id: u64, proto_index: u32, entry: JitEntry) {
    if !jit_enabled() {
        return;
    }
    JIT_FNS
        .lock()
        .expect("jit_cache")
        .entry(program_id)
        .or_default()
        .insert(proto_index, entry);
}

pub fn register_tir_entry(program_id: u64, proto_index: u32, entry: JitEntry) {
    TIR_FNS
        .lock()
        .expect("jit_cache")
        .entry(program_id)
        .or_default()
        .insert(proto_index, entry);
}

pub fn get_jit_entry(program_id: u64, proto_index: u32) -> Option<JitEntry> {
    if !jit_enabled() {
        return None;
    }
    JIT_FNS.lock().ok()?.get(&program_id)?.get(&proto_index).cloned()
}

pub fn get_tir_entry(program_id: u64, proto_index: u32) -> Option<JitEntry> {
    TIR_FNS.lock().ok()?.get(&program_id)?.get(&proto_index).cloned()
}

pub fn clear_program(program_id: u64) -> usize {
    let (entries, modules) = {
        let mut fns = JIT_FNS.lock().expect("jit_cache");
        let mut tirs = TIR_FNS.lock().expect("jit_cache");
        let mut modules = JIT_MODULES.lock().expect("jit_cache");
        let entries = fns.remove(&program_id).map(|m| m.len()).unwrap_or(0);
        tirs.remove(&program_id);
        let modules = modules.remove(&program_id).unwrap_or_default();
        (entries, modules)
    };
    JIT_LAZY_ATTEMPTED
        .lock()
        .expect("jit_cache")
        .remove(&program_id);
    for module in modules {
        unsafe { module.free_memory() };
    }
    entries
}

pub fn clear() {
    let modules = std::mem::take(&mut *JIT_MODULES.lock().expect("jit_cache"));
    crate::runtime::int_dict::clear();
    crate::runtime::int_list::clear();
    crate::runtime::int_class::clear();
    JIT_LAZY_ATTEMPTED.lock().expect("jit_cache").clear();
    JIT_FNS.lock().expect("jit_cache").clear();
    TIR_FNS.lock().expect("jit_cache").clear();
    for (_id, modules) in modules.into_iter() {
        for module in modules {
            unsafe { module.free_memory() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytecode_and_tir_entries_live_in_separate_maps() {
        // Use a key that cannot collide with a real program id.
        let id = u64::MAX - 4242;
        let mk = |kinds| JitEntry {
            ptr: std::ptr::null(),
            arity: 1,
            arg_kinds: kinds,
        };
        JIT_FNS
            .lock()
            .expect("jit_cache")
            .entry(id)
            .or_default()
            .insert(0, mk(Vec::new()));
        TIR_FNS
            .lock()
            .expect("jit_cache")
            .entry(id)
            .or_default()
            .insert(1, mk(vec![ArgKind::Int]));

        let jit = JIT_FNS.lock().expect("jit_cache");
        let tir = TIR_FNS.lock().expect("jit_cache");
        assert!(jit.get(&id).unwrap().contains_key(&0));
        assert!(tir.get(&id).unwrap().contains_key(&1));
        assert!(!tir.get(&id).unwrap().contains_key(&0));
        assert!(!jit.get(&id).unwrap().contains_key(&1));
        drop(jit);
        drop(tir);
        clear_program(id);
    }
}
