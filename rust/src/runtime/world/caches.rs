
use super::WORLD;
use std::cell::RefCell;
use crate::runtime::obj::{Obj, int_obj, plain, str_obj};

pub const MAX_POOL_SIZE: usize = 10000;
pub const MAX_POOLED_LEN: usize = 1000;

pub(crate) const REGISTERED_TYPES: &[&str] = &[
    "obj", "error", "null", "none", "any", "int", "float", "bool", "str", "date", "list", "tuple",
    "set", "dict", "func", "iter", "spread", "future", "type", "module", "T", "U", "...", "skip",
    "break", "__uninitialized__",
];

pub(crate) fn with_intern_type<R>(
    name: &str,
    f: impl FnOnce(Option<&std::rc::Rc<str>>) -> R,
) -> R {
    WORLD.with(|w| {
        let w = w.borrow();
        match w.type_cache.get(name) {
            Some(t) => f(Some(t)),
            None => f(None),
        }
    })
}


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

// ---------------------------------------------------------------------------
// Dedicated thread-local caches for the hot singleton / small-int objects.
//
// These used to be fields of the giant `WORLD` RefCell, so every int/bool/
// sentinel lookup paid a `WORLD.with(|w| w.borrow_mut())` plus a Vec/Option
// lookup and an `Rc` clone. `int_ops` touches them millions of times, so the
// accumulated WORLD traffic was measurable. Keeping them in their own
// thread_local removes that round trip while preserving identity semantics:
// one canonical `Rc` per value, shared for the lifetime of the cache.
// ---------------------------------------------------------------------------

const INT_CACHE_LOW: i64 = -1024;
const INT_CACHE_HIGH: i64 = 100000;

struct ObjCaches {
    int_cache: Vec<Obj>,
    uninitialized: Option<Obj>,
    true_: Option<Obj>,
    false_: Option<Obj>,
}

impl ObjCaches {
    fn new() -> Self {
        Self {
            int_cache: Vec::new(),
            uninitialized: None,
            true_: None,
            false_: None,
        }
    }
}

thread_local! {
    static OBJ_CACHES: RefCell<ObjCaches> = RefCell::new(ObjCaches::new());
}

/// Drop and rebuild the dedicated caches. Mirrors the old behaviour where
/// `world::reset()` replaced the whole `World` (and therefore dropped these
/// fields), so stale pre-reset sentinels/ints never compare equal to fresh
/// ones and the small-int pool is rebuilt against the new interned type name.
pub(crate) fn reset_obj_caches() {
    OBJ_CACHES.with(|c| *c.borrow_mut() = ObjCaches::new());
}

pub fn uninitialized_obj() -> Obj {
    OBJ_CACHES.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(u) = &c.uninitialized {
            return u.clone();
        }
        // Lazy: only the first miss needs the WORLD interned type name.
        let type_name = WORLD.with(|w| w.borrow().type_cache["__uninitialized__"].clone());
        let o = plain(type_name);
        c.uninitialized = Some(o.clone());
        o
    })
}
pub fn is_uninitialized(o: &Obj) -> bool {
    // Immutable borrow of the dedicated cache + ptr_eq, equivalent to the old
    // immutable WORLD borrow. After reset the cache is empty, so a stale
    // pre-reset sentinel compares false just as before.
    OBJ_CACHES.with(|c| {
        c.borrow()
            .uninitialized
            .as_ref()
            .is_some_and(|u| std::rc::Rc::ptr_eq(o, u))
    })
}

pub fn bool_obj(value: bool) -> Obj {
    OBJ_CACHES.with(|c| {
        let mut c = c.borrow_mut();
        let slot = if value { &mut c.true_ } else { &mut c.false_ };
        if let Some(b) = slot {
            return b.clone();
        }
        let type_name = WORLD.with(|w| w.borrow().type_cache["bool"].clone());
        let o = crate::runtime::obj::bool_obj_new(value, type_name);
        *slot = Some(o.clone());
        o
    })
}

pub fn int_cached(value: i64) -> Obj {
    if value < INT_CACHE_LOW || value > INT_CACHE_HIGH {
        return int_obj(value, int_type());
    }
    OBJ_CACHES.with(|c| {
        let mut c = c.borrow_mut();
        if c.int_cache.is_empty() {
            let int_name = WORLD.with(|w| w.borrow().type_cache["int"].clone());
            for i in INT_CACHE_LOW..=INT_CACHE_HIGH {
                c.int_cache.push(int_obj(i, int_name.clone()));
            }
        }
        c.int_cache[(value - INT_CACHE_LOW) as usize].clone()
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
