use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::Node;
use crate::bytecode::Program;
use crate::runtime::context::Context;
use crate::runtime::function::Flow;
use crate::runtime::obj::Obj;
use crate::runtime::scope::{self, Scope};

/// the hook the VM registers so a `function::Body::Nodes` call can run its AST body.
pub type Executor = Rc<dyn Fn(&Context, &[Node]) -> Flow>;

/// the hook the VM registers so a compiled proto body can be executed. the program
/// travels with the call, because a file module has its own program.
pub type ProtoExecutor = Rc<dyn Fn(&Rc<Program>, &Context, usize) -> Flow>;

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
    proto_executor: Option<ProtoExecutor>,
    current_program: Option<Rc<Program>>,
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
    /// `Global.PATH`, the module search root. file-module imports (phase 11) read it.
    path: String,
    /// `Global.scope` — the module/global scope the root `Context` runs in.
    global_scope: Scope,
    /// `Global.natives`: native module name -> member map, in registration order
    natives: Vec<(String, Vec<(String, Obj)>)>,
    /// sandbox stdout of the reference: each `io.write` flushes after writing, so the
    /// buffer is flushed to the real stdout at the same points.
    stdout: Rc<RefCell<String>>,
    /// everything handed to the real stdout since the last reset. `stdout` is drained by
    /// each flush, so a test reads the whole output of a run from here.
    log: Rc<RefCell<String>>,
    /// `Global.SetAllowedModules` — `None` allows everything
    allowed: Option<HashSet<String>>,
    /// `Global._moduleCache` — the loaded module maps, keyed by the full path.
    module_cache: HashMap<String, Vec<(String, Obj)>>,
    /// `Global._loading` — the paths that are being loaded right now. a path that is
    /// already in the set is a circular import.
    loading: HashSet<String>,
    /// `Global.MAXRECURSIONDEPTH` — the recursion limit of the function calls.
    max_recursion_depth: usize,
}

impl World {
    fn new() -> Self {
        let mut type_cache = HashMap::new();
        for name in REGISTERED_TYPES {
            type_cache.insert((*name).to_owned(), Rc::<str>::from(*name));
        }

        Self {
            executor: None,
            proto_executor: None,
            current_program: None,
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
            path: String::new(),
            global_scope: scope::new(None, false),
            natives: Vec::new(),
            stdout: Rc::new(RefCell::new(String::new())),
            log: Rc::new(RefCell::new(String::new())),
            allowed: None,
            module_cache: HashMap::new(),
            loading: HashSet::new(),
            max_recursion_depth: crate::runtime::function::MAX_RECURSION_DEPTH,
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

pub fn set_proto_executor(executor: Option<ProtoExecutor>) {
    WORLD.with(|w| w.borrow_mut().proto_executor = executor);
}

/// `Global._moduleCache` — the loaded module maps.
pub fn module_cache_get(key: &str) -> Option<Vec<(String, Obj)>> {
    WORLD.with(|w| w.borrow().module_cache.get(key).cloned())
}

pub fn module_cache_set(key: &str, map: Vec<(String, Obj)>) {
    WORLD.with(|w| w.borrow_mut().module_cache.insert(key.to_owned(), map));
}

/// `Global._loading.Add` — `false` when the path is already being loaded.
pub fn loading_add(key: &str) -> bool {
    WORLD.with(|w| w.borrow_mut().loading.insert(key.to_owned()))
}

pub fn loading_remove(key: &str) {
    WORLD.with(|w| w.borrow_mut().loading.remove(key));
}

/// `Global.MAXRECURSIONDEPTH`
pub fn max_recursion_depth() -> usize {
    WORLD.with(|w| w.borrow().max_recursion_depth)
}

pub fn set_max_recursion_depth(depth: usize) {
    WORLD.with(|w| w.borrow_mut().max_recursion_depth = depth);
}

pub fn set_current_program(program: Option<Rc<Program>>) {
    WORLD.with(|w| w.borrow_mut().current_program = program);
}

pub fn current_program() -> Option<Rc<Program>> {
    WORLD.with(|w| w.borrow().current_program.clone())
}

/// `execute_proto` runs a compiled function body. the program travels with the
/// function, because a file module has its own program.
pub fn execute_proto(program: &Rc<Program>, ctx: &Context, proto_index: usize) -> Flow {
    let executor = WORLD.with(|w| w.borrow().proto_executor.clone());
    match executor {
        Some(f) => f(program, ctx, proto_index),
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

/// `Global.TryGetOriginalClass` — the member table of a type's original class.
pub fn original_class_attrs(type_name: &str) -> Option<Vec<(String, Obj)>> {
    WORLD.with(|w| w.borrow().original_classes.get(type_name).cloned())
}

/// `Global.TrySetOriginalClass` — the extension method is only registered when the
/// type already has an original class, exactly as the reference does.
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

// ---------------------------------------------------------------------------
// global scope, native module registry, sandbox stdout
// ---------------------------------------------------------------------------

/// `Global.GetGlobalScope()` — the root `Context` scope is this scope, so a top-level
/// assignment and a global function live in the same slot table.
pub fn global_scope() -> Scope {
    WORLD.with(|w| w.borrow().global_scope.clone())
}

/// `Global.scope.Set(name, value)`
pub fn set_global(name: &str, value: Obj) {
    scope::set(&global_scope(), name, value);
}

/// `Global.TryGetGlobalVariable`
pub fn try_get_global(name: &str) -> Option<Obj> {
    scope::get_opt(&global_scope(), name)
}

pub fn contains_global(name: &str) -> bool {
    scope::contains_key(&global_scope(), name)
}

pub fn set_path(path: &str) {
    WORLD.with(|w| w.borrow_mut().path = path.to_owned());
}

pub fn path() -> String {
    WORLD.with(|w| w.borrow().path.clone())
}

/// `Global.natives` is keyed by `NativeModuleAttribute.Name`, and `BuildNativeMap` is the
/// member map. the map order is the reflection order of the reference, so entries stay in
/// registration order.
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

/// `Global.SetAllowedModules`
pub fn set_allowed(names: Vec<String>) {
    WORLD.with(|w| w.borrow_mut().allowed = Some(names.into_iter().collect()));
}

pub fn is_allowed(name: &str) -> bool {
    WORLD.with(|w| match &w.borrow().allowed {
        None => true,
        Some(set) => set.contains(name),
    })
}

pub fn stdout() -> Rc<RefCell<String>> {
    WORLD.with(|w| w.borrow().stdout.clone())
}

pub fn stdout_text() -> String {
    WORLD.with(|w| w.borrow().stdout.clone()).borrow().clone()
}

pub fn stdout_clear() {
    stdout().borrow_mut().clear();
}

/// the reference stream flushes after every write, so the buffer is handed to the real
/// stdout at the same points and the byte order of a run is preserved
pub fn flush_stdout() {
    use std::io::Write;

    let text = stdout_text();
    stdout_clear();

    if text.is_empty() {
        return;
    }

    WORLD.with(|w| w.borrow().log.borrow_mut().push_str(&text));

    let out = std::io::stdout();
    let mut out = out.lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

/// the whole stdout text of the current world, `stdout` drained by each flush
pub fn log_text() -> String {
    WORLD.with(|w| w.borrow().log.clone()).borrow().clone()
}

pub fn log_clear() {
    WORLD.with(|w| w.borrow().log.borrow_mut().clear());
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
