use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::Node;
use crate::runtime::context::Context;
use crate::runtime::function::Flow;
use crate::runtime::obj::Obj;

/// the hook the VM registers so a `function::Body::Nodes` call can run its AST body.
pub type Executor = Rc<dyn Fn(&Context, &[Node]) -> Flow>;

/// `Str` interning limits of the reference implementation (`Str.MAX_POOL_SIZE`,
/// `Str.MAX_POOLED_STRING_LEGNTH` — the typo is the reference's).
pub const MAX_POOL_SIZE: usize = 10000;
pub const MAX_POOLED_LEN: usize = 1000;

/// every `UnType` the reference registers as a static field (`UnType.cs`), so type
/// names are interned once and type equality is name equality.
const REGISTERED_TYPES: &[&str] = &[
    "obj", "error", "null", "none", "any", "int", "float", "bool", "str", "date", "list", "tuple", "set",
    "dict", "func", "iter", "spread", "future", "type", "module", "T", "U", "...", "skip", "break",
];

/// mirrors the process-wide static state of `Global`: interned type names, the
/// `Str` pool, the small `Int` cache and the original class registries.
pub struct World {
    /// the body executor the `LFn` payload calls. the VM registers its own evaluator
    /// in phase 8, so `function.rs` can be tested with a fake executor before the VM
    /// exists, and `reset()` clears the hook with the rest of the global state.
    executor: Option<Executor>,
    type_cache: HashMap<String, Rc<str>>,
    str_pool: HashMap<Rc<str>, Obj>,
    str_pool_size: usize,
    int_cache: Vec<Obj>,
    none: Option<Obj>,
    null: Option<Obj>,
    true_: Option<Obj>,
    false_: Option<Obj>,
    original_classes: HashMap<String, Vec<(String, Obj)>>,
    original_base_types: HashMap<String, String>,
    classes: Vec<(String, Obj)>,
}

impl World {
    fn new() -> Self {
        let mut type_cache = HashMap::new();
        for name in REGISTERED_TYPES {
            type_cache.insert((*name).to_owned(), Rc::<str>::from(*name));
        }

        Self {
            executor: None,
            type_cache,
            str_pool: HashMap::new(),
            str_pool_size: 0,
            int_cache: Vec::new(),
            none: None,
            null: None,
            true_: None,
            false_: None,
            original_classes: HashMap::new(),
            original_base_types: HashMap::new(),
            classes: Vec::new(),
        }
    }
}

thread_local! {
    static WORLD: RefCell<World> = RefCell::new(World::new());
}

pub fn reset() {
    WORLD.with(|w| *w.borrow_mut() = World::new());
}

/// the VM registers its evaluator here. an unregistered hook yields `Flow::Normal`,
/// which is what the pre-VM tests use to check bind / depth / defer behaviour.
pub fn set_executor(executor: Option<Executor>) {
    WORLD.with(|w| w.borrow_mut().executor = executor);
}

pub fn execute(ctx: &Context, body: &[Node]) -> Flow {
    let executor = WORLD.with(|w| w.borrow().executor.clone());
    match executor {
        Some(f) => f(ctx, body),
        None => Flow::Normal,
    }
}

/// `UnType.Create(name)` — the cache is keyed by name, so two lookups of the same
/// name yield the same interned string.
pub fn intern_type(name: &str) -> Rc<str> {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        if let Some(t) = w.type_cache.get(name) {
            return t.clone();
        }
        let t: Rc<str> = Rc::from(name);
        w.type_cache.insert(name.to_owned(), t.clone());
        t
    })
}

pub fn obj_type() -> Rc<str> {
    WORLD.with(|w| w.borrow().type_cache["obj"].clone())
}

pub fn str_type() -> Rc<str> {
    WORLD.with(|w| w.borrow().type_cache["str"].clone())
}

pub fn int_type() -> Rc<str> {
    WORLD.with(|w| w.borrow().type_cache["int"].clone())
}

pub fn bool_type() -> Rc<str> {
    WORLD.with(|w| w.borrow().type_cache["bool"].clone())
}

pub fn none_type() -> Rc<str> {
    WORLD.with(|w| w.borrow().type_cache["none"].clone())
}

/// `Obj.None` — a shared singleton, so `is` identity on `none` is observable.
pub fn none_obj() -> Obj {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        if let Some(n) = &w.none {
            return n.clone();
        }
        let o = crate::runtime::obj::plain(w.type_cache["none"].clone(), w.type_cache["obj"].clone());
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
        let o = crate::runtime::obj::plain(w.type_cache["null"].clone(), w.type_cache["obj"].clone());
        w.null = Some(o.clone());
        o
    })
}

pub fn bool_obj(value: bool) -> Obj {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        let names = (w.type_cache["bool"].clone(), w.type_cache["obj"].clone());
        let slot = if value { &mut w.true_ } else { &mut w.false_ };
        if let Some(b) = slot {
            return b.clone();
        }
        let o = crate::runtime::obj::bool_obj_new(value, names.0, names.1);
        *slot = Some(o.clone());
        o
    })
}

/// `Int.From(value)` — small values (-5..256) are cached, so identity is observable.
pub fn int_cached(value: i64) -> Obj {
    const LOW: i64 = -5;
    const HIGH: i64 = 256;

    if value < LOW || value > HIGH {
        return crate::runtime::obj::int_obj(value, int_type(), obj_type());
    }

    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        if w.int_cache.is_empty() {
            let int_name = w.type_cache["int"].clone();
            let obj_name = w.type_cache["obj"].clone();
            for i in LOW..=HIGH {
                w.int_cache.push(crate::runtime::obj::int_obj(i, int_name.clone(), obj_name.clone()));
            }
        }
        w.int_cache[(value - LOW) as usize].clone()
    })
}

/// `Str.From(value)` — pooled, cleared wholesale when the pool is full.
pub fn str_pooled(value: &str) -> Obj {
    if value.encode_utf16().count() > MAX_POOLED_LEN {
        return crate::runtime::obj::str_obj(Rc::from(value), str_type(), obj_type());
    }

    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        let key: Rc<str> = Rc::from(value);
        if let Some(o) = w.str_pool.get(&key) {
            return o.clone();
        }
        if w.str_pool_size >= MAX_POOL_SIZE {
            w.str_pool.clear();
            w.str_pool_size = 0;
        }
        let o = crate::runtime::obj::str_obj(key.clone(), w.type_cache["str"].clone(), w.type_cache["obj"].clone());
        w.str_pool.insert(key, o.clone());
        w.str_pool_size += 1;
        o
    })
}

/// `Global.TryGetOriginalValue` — walks `originalClasses` up `originalBaseTypes`.
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

pub fn register_original_base(type_name: &str, base: &str) {
    WORLD.with(|w| {
        w.borrow_mut()
            .original_base_types
            .insert(type_name.to_owned(), base.to_owned());
    })
}

pub fn set_class(name: &str, obj: Obj) {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        for slot in &mut w.classes {
            if slot.0 == name {
                slot.1 = obj;
                return;
            }
        }
        w.classes.push((name.to_owned(), obj));
    })
}

pub fn get_class(name: &str) -> Option<Obj> {
    WORLD.with(|w| w.borrow().classes.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))
}
