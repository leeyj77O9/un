
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
