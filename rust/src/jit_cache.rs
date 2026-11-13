//! Global registry of JIT-compiled Un functions.
//!
//! Populated at program load time by the runner: any Un function that can
//! be translated to an unboxed `i64 -> i64` form (single Int parameter, Int
//! locals, `iter.range` loops, integer arithmetic) is pre-compiled to native
//! code. The register cache is consulted inside `vm::call_proto`, bypassing
//! the interpreter for hot loops.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;

type JitFnPtr = unsafe extern "C" fn(i64) -> i64;

static JIT_FNS: LazyLock<Mutex<HashMap<String, JitFnPtr>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

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

/// Register a JIT-compiled Un function under its Un name.
pub fn register_jit_fn(name: &str, ptr: JitFnPtr) {
    if !jit_enabled() {
        return;
    }
    JIT_FNS.lock().expect("jit_cache").insert(name.to_owned(), ptr);
}

/// Look up a previously registered JIT-compiled Un function.
pub fn get_jit_fn(name: &str) -> Option<JitFnPtr> {
    if !jit_enabled() {
        return None;
    }
    JIT_FNS.lock().ok()?.get(name).copied()
}

/// Wipe the registry. Used by tests/playground to avoid leaking compiled
/// functions across runs.
pub fn clear() {
    JIT_FNS.lock().expect("jit_cache").clear();
}
