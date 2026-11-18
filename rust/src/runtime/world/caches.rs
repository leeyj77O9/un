//! Interned type names and the scalar object caches (`int`, `str`, `bool`,
//! `none`, `null`, `__uninitialized__`).
//!
//! Scalars are interned so that identity-based comparisons stay fast and the
//! `Obj` cells are shared for process lifetime.

use super::WORLD;
use crate::runtime::obj::{Obj, int_obj, plain, str_obj};

pub const MAX_POOL_SIZE: usize = 10000;
pub const MAX_POOLED_LEN: usize = 1000;

pub(crate) const REGISTERED_TYPES: &[&str] = &[
    "obj", "error", "null", "none", "any", "int", "float", "bool", "str", "date", "list", "tuple",
    "set", "dict", "func", "iter", "spread", "future", "type", "module", "T", "U", "...", "skip",
    "break", "__uninitialized__",
];

pub fn intern_type(name: &str) -> std::rc::Rc<str> {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        if let Some(t) = w.type_cache.get(name) {
            return t.clone();
        }
        let t: std::rc::Rc<str> = std::rc::Rc::from(name);
        w.type_cache.insert(name.to_owned(), t.clone());
        t
    })
}
pub fn obj_type() -> std::rc::Rc<str> {
    WORLD.with(|w| w.borrow().type_cache["obj"].clone())
}
pub fn str_type() -> std::rc::Rc<str> {
    WORLD.with(|w| w.borrow().type_cache["str"].clone())
}
pub fn int_type() -> std::rc::Rc<str> {
    WORLD.with(|w| w.borrow().type_cache["int"].clone())
}
pub fn bool_type() -> std::rc::Rc<str> {
    WORLD.with(|w| w.borrow().type_cache["bool"].clone())
}
pub fn none_type() -> std::rc::Rc<str> {
    WORLD.with(|w| w.borrow().type_cache["none"].clone())
}

pub fn none_obj() -> Obj {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        if let Some(n) = &w.none {
            return n.clone();
        }
        let o = plain(w.type_cache["none"].clone());
        w.none = Some(o.clone());
        o
    })
}
pub fn null_obj() -> Obj {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        if let Some(n) = &w.null {
            return n.clone();
        }
        let o = plain(w.type_cache["null"].clone());
        w.null = Some(o.clone());
        o
    })
}
pub fn uninitialized_obj() -> Obj {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        if let Some(u) = &w.uninitialized {
            return u.clone();
        }
        let o = plain(w.type_cache["__uninitialized__"].clone());
        w.uninitialized = Some(o.clone());
        o
    })
}
pub fn is_uninitialized(o: &Obj) -> bool {
    std::rc::Rc::ptr_eq(o, &uninitialized_obj())
}

pub fn bool_obj(value: bool) -> Obj {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        let names = (w.type_cache["bool"].clone(), w.type_cache["obj"].clone());
        let slot = if value { &mut w.true_ } else { &mut w.false_ };
        if let Some(b) = slot {
            return b.clone();
        }
        let o = crate::runtime::obj::bool_obj_new(value, names.0);
        *slot = Some(o.clone());
        o
    })
}

pub fn int_cached(value: i64) -> Obj {
    const LOW: i64 = -1024;
    const HIGH: i64 = 100000;
    if value < LOW || value > HIGH {
        return int_obj(value, int_type());
    }
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        if w.int_cache.is_empty() {
            let int_name = w.type_cache["int"].clone();
            for i in LOW..=HIGH {
                w.int_cache
                    .push(int_obj(i, int_name.clone()));
            }
        }
        w.int_cache[(value - LOW) as usize].clone()
    })
}

pub fn str_pooled(value: &str) -> Obj {
    if value.encode_utf16().count() > MAX_POOLED_LEN {
        return str_obj(std::rc::Rc::from(value), str_type());
    }
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        let key: std::rc::Rc<str> = std::rc::Rc::from(value);
        if let Some(o) = w.str_pool.get(&key) {
            return o.clone();
        }
        if w.str_pool_size >= MAX_POOL_SIZE {
            w.str_pool.clear();
            w.str_pool_size = 0;
        }
        let o = str_obj(
            key.clone(),
            w.type_cache["str"].clone(),
        );
        w.str_pool.insert(key, o.clone());
        w.str_pool_size += 1;
        o
    })
}
