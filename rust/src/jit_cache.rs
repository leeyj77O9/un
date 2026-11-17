//! Global registry of JIT-compiled Un functions.
//!
//! Entries are keyed by `(program_id, proto_index)` — `program_id` is the
//! per-`Rc<Program>` opaque id assigned at compile/read time (see
//! `Program::id`). Entries are scoped per program: the registry is a
//! two-level map, so every per-program state can be enumerated and freed
//! independently of every other program's, without pointer-identity tricks.
//!
//! Each entry records the native function pointer and its arity so the caller
//! can dispatch with the right signature. Per-program `clear_program(id)`
//! removes the entries and frees that program's JIT modules; `clear()`
//! removes everything for every program at once.

use cranelift_jit::JITModule;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::LazyLock;
use std::sync::Mutex;

/// JIT result tag: the value is an Int.
pub const TAG_INT: i64 = 0;
/// JIT result tag: the value is the IEEE-754 bits of a Float.
pub const TAG_FLOAT: i64 = 1;
/// JIT result tag: the JIT declined at runtime; the caller must fall back to
/// the interpreter exactly once.
pub const TAG_BAIL: i64 = 2;
/// JIT result tag: the value is a Bool (dispatched to `boolean()`).
pub const TAG_BOOL: i64 = 3;
/// JIT result tag: the callee produced `Un`'s `None` singleton (written by
/// class `__init__`s and other implicit-None functions). The value cell is
/// ignored; the caller returns `world::none_obj()`.
pub const TAG_NONE: i64 = 5;
/// JIT result tag: the callee returned the handle of a freshly-materialised
/// `Obj::List` (an all-Int/all-Bool IntVec). The value cell carries the raw
/// handle; the caller materialises it back into an Un list and frees the
/// handle.
pub const TAG_INTLIST: i64 = 6;

/// A JIT-compiled function. The `ptr` points to the Cranelift-emitted native
/// function with `arity` `i64` parameters plus a hidden result pointer,
/// returning via that hidden pointer: the first i64 cell carries the result
/// value (sent as i64 bits for Float too), the second cell carries one of
/// `TAG_INT`, `TAG_FLOAT`, `TAG_BAIL`, `TAG_BOOL` or `TAG_NONE`.
#[derive(Clone, Copy)]
pub struct JitEntry {
    pub ptr: *const u8,
    pub arity: u32,
}

// The JIT increments this counter on entry and decrements on exit. The
// process is single-threaded and the runtime is cooperative, so Relaxed
// order is sufficient — an RMW per call is about as cheap as it gets for
// shared memory, and there is no contention.
pub static JIT_DEPTH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
// Cached copy of `world::max_recursion_depth()`. Refreshed by
// `sync_max_jit_depth()` so the hot path never borrows `WORLD`.
pub static JIT_MAX_DEPTH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Refresh the JITed bound after a `set_max_recursion_depth()` call.
pub(crate) fn sync_max_jit_depth(depth: usize) {
    JIT_MAX_DEPTH.store(depth, std::sync::atomic::Ordering::Relaxed);
}

/// Cranelift-imported helper for entry/depth check.
/// Returns 1 when the JIT function must bail (too deep), 0 otherwise.
#[no_mangle]
pub extern "C" fn un_jit_depth_enter() -> i64 {
    let depth = JIT_DEPTH.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let max = JIT_MAX_DEPTH.load(std::sync::atomic::Ordering::Relaxed);
    if max == 0 || depth < max {
        0
    } else {
        // Revert the increment so the caller's own bail path doesn't double
        // count; the normal return will fetch_sub anyway.
        JIT_DEPTH.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        1
    }
}

/// Counter-side of depth accounting. Called at every normal exit path AND
/// on the bail path so the VM's own depth tracking never leaks across
/// nested calls.
#[no_mangle]
pub extern "C" fn un_jit_depth_exit() -> i64 {
    JIT_DEPTH.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    0
}

unsafe impl Send for JitEntry {}

/// Native modules emitted per program, keyed by the same per-program id.
/// Kept around so `clear_program` can free exactly one program's native
/// code without touching the others'.
static JIT_MODULES: LazyLock<Mutex<HashMap<u64, Vec<JITModule>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Stash a finished JIT module for `program_id`. Called by the emitter after
/// all of its function pointers have been extracted.
pub(crate) fn stash_module(program_id: u64, module: JITModule) {
    JIT_MODULES
        .lock()
        .expect("jit_cache")
        .entry(program_id)
        .or_default()
        .push(module);
}

/// Two-level registry: program_id -> proto_index -> entry.
static JIT_FNS: LazyLock<Mutex<HashMap<u64, HashMap<u32, JitEntry>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// JIT is enabled unless `UV_JIT` is set to `0` (or `false`) in the environment.
/// Read once per process.
static JIT_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("UV_JIT")
        .map(|v| {
            let v = v.trim();
            v != "0" && !v.eq_ignore_ascii_case("false") && !v.eq_ignore_ascii_case("off")
        })
        .unwrap_or(true)
});

/// Whether the JIT fast-path is enabled (default: yes, unless `UV_JIT=0`).
pub fn jit_enabled() -> bool {
    *JIT_ENABLED
}

/// Whether the JIT emits diagnostic output (`UV_JIT_DEBUG` is set).
///
/// The flag is read once per process: the emitters ask for it per emitted
/// instruction, and an environment lookup per opcode is expensive enough to
/// show up in the JIT compile cost.
pub fn jit_debug() -> bool {
    static JIT_DEBUG: LazyLock<bool> =
        LazyLock::new(|| std::env::var_os("UV_JIT_DEBUG").is_some());
    *JIT_DEBUG
}

/// Programs already attempted a lazy compile. On a lookup miss we attempt
/// compilation once per program instead of on every call (ineligible
/// programs stay ineligible; the check is just a hash-set probe).
static JIT_LAZY_ATTEMPTED: LazyLock<Mutex<HashSet<u64>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Record that this program instance had a lazy compile attempt.
pub fn mark_jit_lazy_attempted(program_id: u64) {
    JIT_LAZY_ATTEMPTED.lock().expect("jit_cache").insert(program_id);
}

/// Whether a lazy compile attempt already ran (or was decided) for `program_id`.
pub fn jit_lazy_attempted(program_id: u64) -> bool {
    JIT_LAZY_ATTEMPTED
        .lock()
        .map(|s| s.contains(&program_id))
        .unwrap_or(true)
}

/// Register a JIT-compiled entry for a proto of `program_id`.
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

/// Look up a previously registered JIT entry for `program_id`.
pub fn get_jit_entry(program_id: u64, proto_index: u32) -> Option<JitEntry> {
    if !jit_enabled() {
        return None;
    }
    JIT_FNS.lock().ok()?.get(&program_id)?.get(&proto_index).copied()
}

/// Free exactly one program's JIT state: remove its entries, drop its
/// native modules (`free_memory`), and forget its lazy-compile marker so the
/// next attempt compiles fresh. Use `clear()` only for whole-process
/// teardown; `clear_program()` is safe point-in-time per program.
///
/// Returns the number of native functions freed.
pub fn clear_program(program_id: u64) -> usize {
    let (entries, modules) = {
        let mut fns = JIT_FNS.lock().expect("jit_cache");
        let mut modules = JIT_MODULES.lock().expect("jit_cache");
        let entries = fns.remove(&program_id).map(|m| m.len()).unwrap_or(0);
        let modules = modules.remove(&program_id).unwrap_or_default();
        (entries, modules)
    };
    JIT_LAZY_ATTEMPTED
        .lock()
        .expect("jit_cache")
        .remove(&program_id);
    for module in modules {
        // SAFETY: free_memory invalidates pointers into the module; no code
        // in the module may be entered afterwards and all related entries
        // were removed from JIT_FNS above.
        unsafe { module.free_memory() };
    }
    entries
}

/// Free every native module the JIT emitted for this process and wipe the
/// registry. Also drops every IntDict created by JIT code (the arena is
/// process-lifetime otherwise, and repeated `clear()` cycles in tests would
/// keep every table allocated). Any code entered afterwards must re-JIT
/// against a fresh registry; the IntDict ids vanish with the native code,
/// so no stale id is ever observable.
///
/// # Safety
/// Calling this invalidates all previously returned `JitEntry.ptr` values:
/// every native code pointer from every compiled module is freed. The caller
/// must ensure no JIT-compiled code is executing anymore nor will be entered
/// afterwards. Intended for whole-process shutdown (e.g. benchmarks /
/// playground teardown), never for in-flight re-entry. Tests or other code
/// that keep running Un programs after a `clear()` would crash or corrupt.
pub fn clear() {
    let modules = std::mem::take(&mut *JIT_MODULES.lock().expect("jit_cache"));
    crate::runtime::int_dict::clear();
    crate::runtime::int_list::clear();
    crate::runtime::int_class::clear();
    JIT_LAZY_ATTEMPTED.lock().expect("jit_cache").clear();
    JIT_FNS.lock().expect("jit_cache").clear();
    for (_id, modules) in modules.into_iter() {
        for module in modules {
            // SAFETY: free_memory invalidates all pointers into the module; the
            // caller must not enter any of them afterwards. All entries from
            // those modules have been removed from JIT_FNS, so no lookup can
            // return a dangling pointer.
            unsafe { module.free_memory() };
        }
    }
}
