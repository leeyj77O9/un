
use super::WORLD;
use crate::runtime::obj::Obj;
use crate::runtime::scope::{self, Scope};

/// One entry in the module cache.
///
/// `exports` is the symbol table produced by the module top level and is
/// cloned on every `use` (the existing hot path). `program_id` is the
/// `bytecode::Program::id` of the program that produced those exports, so an
/// opt-in unload can release the JIT/TIR entries registered under that id
/// (D-094 §3). Directory modules execute more than one file; their additional
/// program ids live in `extra_program_ids` so every compiled file is covered.
#[derive(Clone)]
pub struct CachedModule {
    pub exports: Vec<(String, Obj)>,
    pub program_id: u64,
    pub extra_program_ids: Vec<u64>,
}

impl CachedModule {
    /// Every bytecode program id owned by this cached module.
    pub fn program_ids(&self) -> impl Iterator<Item = u64> + '_ {
        std::iter::once(self.program_id).chain(self.extra_program_ids.iter().copied())
    }
}

pub fn module_cache_get(key: &str) -> Option<CachedModule> {
    WORLD.with(|w| w.borrow().module_cache.get(key).cloned())
}
pub fn module_cache_set(key: &str, entry: CachedModule) {
    WORLD.with(|w| w.borrow_mut().module_cache.insert(key.to_owned(), entry));
}
/// Remove one module from the cache and release the JIT/TIR code compiled for
/// its program(s) via `jit_cache::clear_program`.
///
/// Returns `true` when the key was present. Only the cache entry is dropped:
/// caller scopes keep their alias `Obj`s (there is no hot-path alias registry),
/// and those `Rc`s unwind naturally. Per D-093 §2, object memory held by
/// closure/scope cycles remains a bounded leak; class/layout registry entries
/// are process-lifetime in this scope. **After this call the unloaded program's
/// JIT entries must not be re-entered** (D-093 §3 / D-094 §3). A fresh import of
/// the same path compiles a new program with a new id and is safe.
pub fn module_cache_remove(key: &str) -> bool {
    let entry = WORLD.with(|w| w.borrow_mut().module_cache.remove(key));
    if let Some(entry) = entry {
        release_jit(&entry);
        true
    } else {
        false
    }
}
/// Remove every cached module and release all of their JIT/TIR code.
///
/// Returns the number of removed cache entries. The same contract as
/// [`module_cache_remove`] applies: no caller may re-enter an unloaded
/// program's JIT entries, and alias `Obj`s are left to natural `Rc` drop.
pub fn module_cache_clear() -> usize {
    let entries: Vec<CachedModule> = WORLD.with(|w| {
        w.borrow_mut()
            .module_cache
            .drain()
            .map(|(_, entry)| entry)
            .collect()
    });
    let count = entries.len();
    for entry in &entries {
        release_jit(entry);
    }
    count
}

fn release_jit(entry: &CachedModule) {
    for program_id in entry.program_ids() {
        let _freed = crate::jit_cache::clear_program(program_id);
    }
}

pub fn loading_add(key: &str) -> bool {
    WORLD.with(|w| w.borrow_mut().loading.insert(key.to_owned()))
}
pub fn loading_remove(key: &str) {
    WORLD.with(|w| w.borrow_mut().loading.remove(key));
}
pub fn set_path(path: &str) {
    WORLD.with(|w| w.borrow_mut().path = path.to_owned());
}
pub fn path() -> String {
    WORLD.with(|w| w.borrow().path.clone())
}

pub fn global_scope() -> Scope {
    WORLD.with(|w| w.borrow().global_scope.clone())
}
pub fn set_global(name: &str, value: Obj) {
    scope::set(&global_scope(), name, value);
}
pub fn try_get_global(name: &str) -> Option<Obj> {
    scope::get_opt(&global_scope(), name)
}
pub fn contains_global(name: &str) -> bool {
    scope::contains_key(&global_scope(), name)
}

pub fn register_native(name: &str, members: Vec<(String, Obj)>) {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        for slot in &mut w.natives {
            if slot.0 == name {
                slot.1 = members;
                return;
            }
        }
        w.natives.push((name.to_owned(), members));
    })
}
pub fn is_native(name: &str) -> bool {
    WORLD.with(|w| w.borrow().natives.iter().any(|(k, _)| k == name))
}
pub fn native_members(name: &str) -> Option<Vec<(String, Obj)>> {
    WORLD.with(|w| w.borrow().natives.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))
}
/// Names of all registered native modules, in registration order. Lets the
/// LSP discover modules instead of keeping a hand-maintained list in sync.
pub fn native_module_names() -> Vec<String> {
    WORLD.with(|w| w.borrow().natives.iter().map(|(name, _)| name.clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jit_cache::{get_tir_entry, register_tir_entry, JitEntry};

    fn cached(exports: Vec<(String, Obj)>, program_id: u64) -> CachedModule {
        CachedModule {
            exports,
            program_id,
            extra_program_ids: Vec::new(),
        }
    }

    fn tir_stub() -> JitEntry {
        JitEntry {
            ptr: std::ptr::null(),
            arity: 0,
            arg_kinds: Vec::new(),
        }
    }

    #[test]
    fn remove_drops_entry_and_clears_its_jit() {
        crate::runtime::world::reset();
        // Ids far above the monotonic allocator range cannot collide with a
        // program compiled by another test in this process.
        let id = u64::MAX - 9100;
        register_tir_entry(id, 0, tir_stub());
        module_cache_set("unload.remove", cached(vec![("x".into(), crate::runtime::obj::int(1))], id));
        assert!(get_tir_entry(id, 0).is_some());

        assert!(module_cache_remove("unload.remove"));
        assert!(module_cache_get("unload.remove").is_none());
        assert!(get_tir_entry(id, 0).is_none());
        assert!(!module_cache_remove("unload.remove"));
    }

    #[test]
    fn clear_drops_all_entries_and_their_jit() {
        crate::runtime::world::reset();
        let ids = [u64::MAX - 9200, u64::MAX - 9201];
        for (i, id) in ids.iter().enumerate() {
            register_tir_entry(*id, 0, tir_stub());
            module_cache_set(&format!("unload.clear.{i}"), cached(Vec::new(), *id));
        }
        assert_eq!(module_cache_clear(), ids.len());
        assert!(module_cache_get("unload.clear.0").is_none());
        assert!(module_cache_get("unload.clear.1").is_none());
        for id in ids {
            assert!(get_tir_entry(id, 0).is_none());
        }
    }

    #[test]
    fn unload_then_reimport_compiles_a_fresh_program() {
        crate::runtime::world::reset();
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "un_unload_reimport_{}_{seq}.un",
            std::process::id()
        ));
        std::fs::write(&path, "x = 1\n").expect("write temp module");
        let path_str = path.to_string_lossy().into_owned();
        let key = std::fs::canonicalize(&path)
            .expect("canonicalize temp module")
            .display()
            .to_string();

        // `ModuleError` intentionally has no Debug impl (it carries an Obj),
        // so avoid `Result::expect` here.
        let first = match crate::module::load(&path_str, None) {
            Ok(exports) => exports,
            Err(_) => panic!("first import failed"),
        };
        assert!(first.iter().any(|(name, _)| name == "x"));
        let first_id = module_cache_get(&key).expect("cached after first import").program_id;
        assert_ne!(first_id, 0);

        assert!(module_cache_remove(&key));
        assert!(module_cache_get(&key).is_none());

        let second = match crate::module::load(&path_str, None) {
            Ok(exports) => exports,
            Err(_) => panic!("reimport failed"),
        };
        assert!(second.iter().any(|(name, _)| name == "x"));
        let second_id = module_cache_get(&key).expect("cached after reimport").program_id;
        assert_ne!(first_id, second_id);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn directory_module_tracks_every_program_id() {
        crate::runtime::world::reset();
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "un_unload_dir_{}_{seq}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp module dir");
        std::fs::write(dir.join("a.un"), "a = 1\n").expect("write a.un");
        std::fs::write(dir.join("b.un"), "b = 2\n").expect("write b.un");
        let dir_str = dir.to_string_lossy().into_owned();
        let key = std::fs::canonicalize(&dir)
            .expect("canonicalize temp module dir")
            .display()
            .to_string();

        let exports = match crate::module::load(&dir_str, None) {
            Ok(exports) => exports,
            Err(_) => panic!("directory import failed"),
        };
        assert!(exports.iter().any(|(name, _)| name == "a"));
        assert!(exports.iter().any(|(name, _)| name == "b"));
        let entry = module_cache_get(&key).expect("cached directory module");
        let ids: Vec<u64> = entry.program_ids().collect();
        assert_eq!(ids.len(), 2);
        assert_eq!(entry.extra_program_ids.len(), 1);
        // Both compiled files' JIT entries must be released on unload.
        for id in &ids {
            register_tir_entry(*id, 0, tir_stub());
        }
        assert!(module_cache_remove(&key));
        for id in ids {
            assert!(get_tir_entry(id, 0).is_none());
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
