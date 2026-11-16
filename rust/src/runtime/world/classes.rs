//! Class registries: instance templates, the `TObj` metaclasses, the
//! original-class attribute tables, and per-class field layouts.
//!
//! `original_classes` stores the class object's member list as registered
//! (before any instance writes grow it); `classes_tobj` caches the canonical
//! `TObj(Types::Single(name))`; `class_layouts` records member order at
//! registration time (and gets seeded with the `__init__`-assigned fields by
//! the JIT planner).

use super::WORLD;
use crate::runtime::obj::Obj;

pub fn try_get_original_value(type_name: &str, name: &str) -> Option<Obj> {
    WORLD.with(|w| {
        let w = w.borrow();
        let mut current = type_name.to_owned();
        loop {
            if let Some(attrs) = w.original_classes.get(&current) {
                if let Some((_, value)) = attrs.iter().find(|(k, _)| k == name) {
                    return Some(value.clone());
                }
            }
            match w.original_base_types.get(&current) {
                Some(base) => {
                    let base = base.clone();
                    current = base;
                }
                None => return None,
            }
        }
    })
}
pub fn register_original_class(type_name: &str, attrs: Vec<(String, Obj)>) {
    WORLD.with(|w| {
        w.borrow_mut()
            .original_classes
            .insert(type_name.to_owned(), attrs);
    })
}
pub fn original_class_attrs(type_name: &str) -> Option<Vec<(String, Obj)>> {
    WORLD.with(|w| w.borrow().original_classes.get(type_name).cloned())
}
pub fn try_set_original_class(type_name: &str, name: &str, value: Obj) -> bool {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        let Some(attrs) = w.original_classes.get_mut(type_name) else {
            return false;
        };
        if let Some(entry) = attrs.iter_mut().find(|(k, _)| k == name) {
            entry.1 = value;
        } else {
            attrs.push((name.to_owned(), value));
        }
        true
    })
}
pub fn register_original_base(type_name: &str, base: &str) {
    WORLD.with(|w| {
        w.borrow_mut()
            .original_base_types
            .insert(type_name.to_owned(), base.to_owned());
    })
}

pub fn set_class_layout(name: &str, fields: Vec<String>) {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        if let Some(entry) = w.class_layouts.get_mut(name) {
            // De-duplicate field names and preserve their insertion order —
            // runtime reseed cycles become no-ops.
            for f in fields {
                if !entry.contains(&f) {
                    entry.push(f);
                }
            }
        } else {
            w.class_layouts.insert(name.to_owned(), fields);
        }
    })
}

pub fn set_class(name: &str, obj: Obj) {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        // Snapshot member order at registration so layout computations can
        // rely on it later (the template accumulates `__init__`/methods in
        // the order the class body was compiled — instances inherit this order).
        let layout: Vec<String> = obj.borrow().members.iter().map(|(n, _)| n.clone()).collect();
        w.class_layouts.insert(name.to_owned(), layout);
        w.classes.insert(name.to_owned(), obj);
    })
}
pub fn get_class(name: &str) -> Option<Obj> {
    WORLD.with(|w| w.borrow().classes.get(name).cloned())
}
/// Template member name order at class registration time (pre-init).
/// Intended for layout-related work; not a complete field layout.
#[allow(dead_code)]
pub fn class_layout(name: &str) -> Option<Vec<String>> {
    WORLD.with(|w| w.borrow().class_layouts.get(name).cloned())
}
/// Canonical `TObj(Types::Single(name))` for a registered class — allocated
/// once per class per world reset instead of once per lookup.
pub fn class_tobj(name: &str) -> Obj {
    if let Some(o) = WORLD.with(|w| w.borrow().classes_tobj.get(name).cloned()) {
        return o;
    }
    let t = crate::runtime::collections::tobj(crate::runtime::types::Types::single(name));
    WORLD.with(|w| {
        w.borrow_mut()
            .classes_tobj
            .entry(name.to_owned())
            .or_insert_with(|| t.clone())
            .clone()
    })
}
