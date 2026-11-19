
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

static JIT_FNS: LazyLock<Mutex<HashMap<u64, HashMap<u32, JitEntry>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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
    JIT_FNS
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
    JIT_FNS.lock().ok()?.get(&program_id)?.get(&proto_index).cloned()
}

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
    for (_id, modules) in modules.into_iter() {
        for module in modules {
            unsafe { module.free_memory() };
        }
    }
}
