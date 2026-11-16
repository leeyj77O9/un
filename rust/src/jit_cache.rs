//! Global registry of JIT-compiled Un functions.
//!
//! Entries are keyed `"unbc:<ptr>:<proto_index>"` -> the `.unbc` translation of
//! a specific proto of a specific compiled program. `<ptr>` is the
//! `Rc::as_ptr` of the `Rc<Program>` used by the VM, which makes entries
//! precise per program.
//!
//! Each entry records the native function pointer and its arity so the caller
//! can dispatch with the right signature.

use cranelift_jit::JITModule;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
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

/// A JIT-compiled function. The `ptr` points to the Cranelift-emitted native
/// function with `arity` `i64` parameters plus a hidden result pointer,
/// returning via that hidden pointer: the first i64 cell carries the result
/// value (sent as i64 bits for Float too), the second cell carries one of
/// `TAG_INT`, `TAG_FLOAT`, `TAG_BAIL`, `TAG_BOOL`.
#[derive(Clone, Copy)]
pub struct JitEntry {
    pub ptr: *const u8,
    pub arity: u32,
}

/// Thread of execution here; JIT functions count call depth using this
/// counter (both from VM dispatch — no, VM increments its own limit — and
/// from JIT-internal call sites). Capped at world::max_recursion_depth().
pub static JIT_DEPTH: AtomicUsize = AtomicUsize::new(0);

/// Cranelift-imported helper for entry/depth check.
/// Returns 1 when the JIT function must bail (too deep), 0 otherwise.
#[no_mangle]
pub extern "C" fn un_jit_depth_enter() -> i64 {
    let depth = JIT_DEPTH.fetch_add(1, Ordering::Relaxed) + 1;
    if depth >= crate::runtime::world::max_recursion_depth() {
        // Revert the increment so the VM doesn't double-count on bail.
        JIT_DEPTH.fetch_sub(1, Ordering::Relaxed);
        1
    } else {
        0
    }
}

/// Counter-side of depth accounting. Called at every normal exit path AND
/// on the bail path so the VM's own depth tracking never leaks across
/// nested calls.
#[no_mangle]
pub extern "C" fn un_jit_depth_exit() -> i64 {
    JIT_DEPTH.fetch_sub(1, Ordering::Relaxed);
    0
}

unsafe impl Send for JitEntry {}

/// JIT modules that have been emitted for this process. The vendored
/// Cranelift backend leaks their native allocations by design (so
/// `unsafe impl Send for JitEntry` sound); we keep the modules here so
/// `clear()` can reclaim that memory on demand. Native code is never freed
/// unless the caller invokes `clear()` after all execution has stopped.
static JIT_MODULES: LazyLock<Mutex<Vec<JITModule>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Stash a finished JIT module. Called by the emitter after all of its
/// function pointers have been extracted.
pub(crate) fn stash_module(module: JITModule) {
    JIT_MODULES.lock().expect("jit_cache").push(module);
}

static JIT_FNS: LazyLock<Mutex<HashMap<String, JitEntry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

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

/// Register a JIT-compiled entry under `key`.
pub fn register_jit_entry(key: &str, entry: JitEntry) {
    if !jit_enabled() {
        return;
    }
    JIT_FNS.lock().expect("jit_cache").insert(key.to_owned(), entry);
}

/// Convenience key for a compiled `.unbc` proto of `program`.
pub fn unbc_key(program_ptr: usize, proto_index: usize) -> String {
    format!("unbc:{program_ptr:x}:{proto_index}")
}

/// Look up a previously registered JIT entry.
pub fn get_jit_entry(key: &str) -> Option<JitEntry> {
    if !jit_enabled() {
        return None;
    }
    JIT_FNS.lock().ok()?.get(key).copied()
}

/// Free every native module the JIT emitted for this process and wipe the
/// registry.
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
    JIT_FNS.lock().expect("jit_cache").clear();
    for module in modules {
        // SAFETY: free_memory invalidates all pointers into the module; the
        // caller must not enter any of them afterwards. All entries from
        // those modules have been removed from JIT_FNS, so no lookup can
        // return a dangling pointer.
        unsafe { module.free_memory() };
    }
}
