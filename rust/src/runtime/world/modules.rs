//! Module loading and global registration: the import cache, the
//! circular-import guard, the working path, the global scope, and the
//! native module registry.

use super::WORLD;
use crate::runtime::obj::Obj;
use crate::runtime::scope::{self, Scope};

pub fn module_cache_get(key: &str) -> Option<Vec<(String, Obj)>> {
    WORLD.with(|w| w.borrow().module_cache.get(key).cloned())
}
pub fn module_cache_set(key: &str, map: Vec<(String, Obj)>) {
    WORLD.with(|w| w.borrow_mut().module_cache.insert(key.to_owned(), map));
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
