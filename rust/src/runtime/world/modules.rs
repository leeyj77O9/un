
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
/// `class_names` holds the class names that first appeared in the global class
/// registries while this module was loaded (a before/after diff), so an unload
/// can remove exactly those registry entries without touching native classes.
#[derive(Clone)]
pub struct CachedModule {
    pub exports: Vec<(String, Obj)>,
    pub program_id: u64,
    pub extra_program_ids: Vec<u64>,
    pub class_names: Vec<String>,
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
/// its program(s) via `jit_cache::clear_program`, the translated plans via
/// `vm::clear_plan_cache`, and the class/layout registry entries this module
/// introduced via `unregister_class`.
///
/// Returns `true` when the key was present. Only the cache entry is dropped:
/// caller scopes keep their alias `Obj`s (there is no hot-path alias registry),
/// and those `Rc`s unwind naturally. Per D-093 §2, object memory held by
/// closure/scope cycles remains a bounded leak; only the registry entries are
/// removed, never the objects themselves. Native classes and classes present
/// before the load are not tracked and are never removed. **After this call the
/// unloaded program's JIT entries must not be re-entered** (D-093 §3 / D-094 §3).
/// A fresh import of the same path compiles a new program with a new id and is
/// safe.
pub fn module_cache_remove(key: &str) -> bool {
    let entry = WORLD.with(|w| w.borrow_mut().module_cache.remove(key));
    if let Some(entry) = entry {
        release_jit(&entry);
        true
    } else {
        false
    }
}
/// Remove every cached module and release all of their JIT/TIR code, plans
/// and module-owned class/layout registrations.
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
        crate::vm::clear_plan_cache(program_id);
    }
    for name in &entry.class_names {
        super::unregister_class(name);
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
    use crate::runtime::obj;
    use crate::runtime::world::{self, get_class};

    fn cached(exports: Vec<(String, Obj)>, program_id: u64) -> CachedModule {
        CachedModule {
            exports,
            program_id,
            extra_program_ids: Vec::new(),
            class_names: Vec::new(),
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

    #[test]
    fn unload_clears_tracked_class_registries_and_keeps_native_classes() {
        crate::native::init(".");
        let id = u64::MAX - 9300;
        let class_name = "UnloadManualFixture";
        world::set_class(class_name, obj::obj_new(class_name, obj::Payload::Plain));
        world::register_original_class(class_name, vec![("f".into(), obj::int(1))]);
        world::register_original_base(class_name, "int");
        world::set_class_layout(class_name, vec!["f".into()]);
        let _ = world::class_tobj(class_name);
        assert!(get_class(class_name).is_some());
        assert!(world::registered_class_names().iter().any(|n| n == class_name));
        assert!(get_class("int").is_some());

        let mut entry = cached(Vec::new(), id);
        entry.class_names = vec![class_name.to_owned()];
        module_cache_set("unload.classes", entry);
        assert!(module_cache_remove("unload.classes"));

        assert!(get_class(class_name).is_none());
        assert!(world::original_class_attrs(class_name).is_none());
        assert!(world::class_layout(class_name).is_none());
        assert!(!world::registered_class_names().iter().any(|n| n == class_name));
        // Native/builtin classes were in the "before" snapshot: untouched.
        assert!(get_class("int").is_some());
        assert!(world::registered_class_names().iter().any(|n| n == "int"));
    }

    #[test]
    fn unload_removes_module_class_and_reimport_reregisters_it() {
        crate::native::init(".");
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let class_name = format!("UnloadFixture{seq}");
        let path = std::env::temp_dir().join(format!(
            "un_unload_class_{}_{seq}.un",
            std::process::id()
        ));
        let text = format!(
            "class {class_name}\n    n: int = 1\n\n    fn get(self)\n        -> self.n\n\nvalue = {class_name}()\n"
        );
        std::fs::write(&path, text).expect("write temp class module");
        let path_str = path.to_string_lossy().into_owned();
        let key = std::fs::canonicalize(&path)
            .expect("canonicalize temp class module")
            .display()
            .to_string();

        let exports = match crate::module::load(&path_str, None) {
            Ok(exports) => exports,
            Err(_) => panic!("class module import failed"),
        };
        assert!(exports.iter().any(|(name, _)| name == "value"));
        assert!(
            get_class(&class_name).is_some(),
            "class not registered on import"
        );
        let first = module_cache_get(&key).expect("cached class module");
        assert!(first.class_names.iter().any(|n| *n == class_name));
        let first_id = first.program_id;
        assert_ne!(first_id, 0);

        assert!(module_cache_remove(&key));
        assert!(get_class(&class_name).is_none(), "class survived unload");
        assert!(
            !world::registered_class_names()
                .iter()
                .any(|n| *n == class_name)
        );
        assert!(get_class("int").is_some(), "native class removed by unload");

        let again = match crate::module::load(&path_str, None) {
            Ok(exports) => exports,
            Err(_) => panic!("class module reimport failed"),
        };
        assert!(again.iter().any(|(name, _)| name == "value"));
        assert!(get_class(&class_name).is_some(), "class not reregistered");
        let second = module_cache_get(&key).expect("cached after reimport");
        assert_ne!(
            first_id, second.program_id,
            "reimport reused the old program id"
        );
        assert!(second.class_names.iter().any(|n| *n == class_name));

        assert!(module_cache_remove(&key));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unload_clears_the_programs_plan_cache_entry() {
        crate::native::init(".");
        let (program, source) = crate::compiler::tests::compile_text("x = 40 + 2\n");
        let program_id = program.id;
        let mut vm = crate::vm::VM::new(program, source).expect("vm");
        assert!(matches!(vm.run(), crate::vm::Outcome::Value(_)));
        assert!(crate::vm::plan_cache_contains(program_id), "plan not seeded");

        module_cache_set("unload.plan", cached(Vec::new(), program_id));
        assert!(module_cache_remove("unload.plan"));
        assert!(
            !crate::vm::plan_cache_contains(program_id),
            "plan survived unload"
        );
        assert_eq!(crate::vm::clear_plan_cache(program_id), 0);
    }
}
