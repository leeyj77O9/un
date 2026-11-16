//! Per-process global state for the runtime.
//!
//! `World` is the single thread-local cell every piece of runtime state
//! lives in: interned type names, the scalar caches (`int`, `str`, `bool`,
//! `none`), the class registries (classes, the metaclass `TObj`, original
//! class attribute tables, per-class field layouts), the module cache,
//! stdout/log/stdin, recursion depth, and the executor hooks used by the
//! CLI and tests to run compiled protos.
//!
//! The module is split by responsibility; the submodules re-export their
//! items so callers use `world::int_cached` and friends exactly as before.
//! `World` itself stays private to this module: every field is owned by one
//! submodule, which is the order in which the state is described below.

mod caches;
mod classes;
mod depth;
mod executors;
mod io;
mod modules;

pub use self::caches::*;
pub use self::classes::*;
pub use self::depth::*;
pub use self::executors::*;
pub use self::io::*;
pub use self::modules::*;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

pub struct World {
    executor: Option<Executor>,
    proto_executor: Option<ProtoExecutor>,

    type_cache: HashMap<String, std::rc::Rc<str>>,

    str_pool: HashMap<std::rc::Rc<str>, crate::runtime::obj::Obj>,
    str_pool_size: usize,
    int_cache: Vec<crate::runtime::obj::Obj>,

    none: Option<crate::runtime::obj::Obj>,
    null: Option<crate::runtime::obj::Obj>,
    uninitialized: Option<crate::runtime::obj::Obj>,
    true_: Option<crate::runtime::obj::Obj>,
    false_: Option<crate::runtime::obj::Obj>,

    original_classes: HashMap<String, Vec<(String, crate::runtime::obj::Obj)>>,
    original_base_types: HashMap<String, String>,
    /// Template member name order at class registration time.
    /// Not authoritative for runtime field ordering (instances grow in
    /// `__init__`), but a stable anchor for layout-related work.
    class_layouts: HashMap<String, Vec<String>>,
    classes: HashMap<String, crate::runtime::obj::Obj>,
    classes_tobj: HashMap<String, crate::runtime::obj::Obj>,

    path: String,

    global_scope: crate::runtime::scope::Scope,

    natives: Vec<(String, Vec<(String, crate::runtime::obj::Obj)>)>,

    stdout: std::rc::Rc<std::cell::RefCell<String>>,
    log: std::rc::Rc<std::cell::RefCell<String>>,

    allowed: Option<HashSet<String>>,
    allow_exit: bool,

    module_cache: HashMap<String, Vec<(String, crate::runtime::obj::Obj)>>,
    loading: HashSet<String>,

    max_recursion_depth: usize,

    stdin: Option<VecDeque<String>>,
    allow_eof: bool,
}

thread_local! {
    // Child modules access `super::WORLD`; private to the `world` module.
    static WORLD: RefCell<World> = RefCell::new(World::new());
}

impl World {
    fn new() -> Self {
        let mut type_cache = HashMap::new();
        for name in caches::REGISTERED_TYPES {
            type_cache.insert((*name).to_owned(), std::rc::Rc::<str>::from(*name));
        }
        Self {
            executor: None,
            proto_executor: None,
            type_cache,
            str_pool: HashMap::new(),
            str_pool_size: 0,
            int_cache: Vec::new(),
            none: None,
            null: None,
            uninitialized: None,
            true_: None,
            false_: None,
            original_classes: HashMap::new(),
            original_base_types: HashMap::new(),
            class_layouts: HashMap::new(),
            classes: HashMap::new(),
            classes_tobj: HashMap::new(),
            path: String::new(),
            global_scope: crate::runtime::scope::new(None, false),
            natives: Vec::new(),
            stdout: std::rc::Rc::new(std::cell::RefCell::new(String::new())),
            log: std::rc::Rc::new(std::cell::RefCell::new(String::new())),
            allowed: None,
            allow_exit: true,
            module_cache: HashMap::new(),
            loading: HashSet::new(),
            max_recursion_depth: crate::runtime::function::MAX_RECURSION_DEPTH,
            stdin: None,
            allow_eof: false,
        }
    }
}

pub fn reset() {
    WORLD.with(|w| *w.borrow_mut() = World::new());
}
