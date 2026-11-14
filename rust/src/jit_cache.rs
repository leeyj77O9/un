//! Global registry of JIT-compiled Un functions.
//!
//! Two kinds of entries are registered:
//!
//! - `"unbc:<ptr>:<proto_index>"` -> the `.unbc` translation of a specific
//!   proto of a specific compiled program. `<ptr>` is the `Rc::as_ptr` of the
//!   `Rc<Program>` used by the VM, which makes entries precise per program.
//! - `"fn:<name>"` -> the AST-based i64 subset translation (single Int param).
//!
//! Each entry records the native function pointer and its arity so the caller
//! can dispatch with the right signature.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;
use std::sync::Mutex;

/// A JIT-compiled function. The `ptr` points to the Cranelift-emitted native
/// function with `arity` `i64` parameters plus a hidden result pointer,
/// returning via that hidden pointer: the first i64 cell carries the result
/// value (sent as i64 bits for Float too), the second cell carries a tag:
/// 0 = Int, 1 = Float, 2 = bail to the interpreter.
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

/// Wipe the registry. Used by tests/playground to avoid leaking compiled
/// functions across runs.
pub fn clear() {
    JIT_FNS.lock().expect("jit_cache").clear();
}
