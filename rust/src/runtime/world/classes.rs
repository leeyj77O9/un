
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

pub fn class_has_property(type_name: &str) -> bool {
    WORLD.with(|w| {
        let w = w.borrow();
        let mut current = type_name.to_owned();
        loop {
            if let Some(attrs) = w.original_classes.get(&current) {
                for (_, value) in attrs {
                    if crate::runtime::function::is_property(value) {
                        return true;
                    }
                }
            }
            match w.original_base_types.get(&current) {
                Some(base) => {
                    let base = base.clone();
                    current = base;
                }
                None => return false,
            }
        }
    })
}
pub fn try_set_original_class(type_name: &str, name: &str, value: Obj) -> bool {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        let attrs = w
            .original_classes
            .entry(type_name.to_owned())
            .or_default();
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
        if !w.class_layouts.contains_key(name) {
            let layout: Vec<String> = obj.borrow().members.iter().map(|(n, _)| n.clone()).collect();
            w.class_layouts.insert(name.to_owned(), layout);
        }
        w.classes.insert(name.to_owned(), obj);
    })
}
pub fn get_class(name: &str) -> Option<Obj> {
    WORLD.with(|w| w.borrow().classes.get(name).cloned())
}
#[allow(dead_code)]
pub fn class_layout(name: &str) -> Option<Vec<String>> {
    WORLD.with(|w| w.borrow().class_layouts.get(name).cloned())
}
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

/// Names present in at least one of the five class/layout registries.
///
/// The module loader takes a snapshot of this set immediately before a fresh
/// import and records only the names that appear afterwards. Native classes
/// (registered by `native::init` / `runtime::modules::init_classes`) and any
/// class already present at program start are in the "before" snapshot, so an
/// opt-in module unload can never remove them.
pub fn registered_class_names() -> Vec<String> {
    WORLD.with(|w| {
        let w = w.borrow();
        let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        names.extend(w.classes.keys().cloned());
        names.extend(w.classes_tobj.keys().cloned());
        names.extend(w.original_classes.keys().cloned());
        names.extend(w.original_base_types.keys().cloned());
        names.extend(w.class_layouts.keys().cloned());
        names.into_iter().collect()
    })
}

/// Drop one class name from every class/layout registry.
///
/// Returns how many of the five registries held the name (0..=5). This is
/// registry cleanup only: `Obj`s reachable from caller scopes or closure
/// cycles are left to natural `Rc` drop (D-093 §2 bounded leak), and
/// `world::reset()` stays the full state teardown. Callers must only pass
/// names they observed as *new* during their own module load — passing a
/// builtin/native name would break later lookups of that type.
pub fn unregister_class(name: &str) -> usize {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        let mut removed = 0;
        if w.classes.remove(name).is_some() {
            removed += 1;
        }
        if w.classes_tobj.remove(name).is_some() {
            removed += 1;
        }
        if w.original_classes.remove(name).is_some() {
            removed += 1;
        }
        if w.original_base_types.remove(name).is_some() {
            removed += 1;
        }
        if w.class_layouts.remove(name).is_some() {
            removed += 1;
        }
        removed
    })
}
