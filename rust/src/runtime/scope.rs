use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use crate::runtime::obj;
use crate::runtime::obj::Obj;
use crate::runtime::world;
pub type Scope = Rc<RefCell<ScopeData>>;
/// Insertion-ordered symbol names backed by shared `Rc<str>` values.
///
/// Preseeding allocates one `Rc<str>` per name and shares it with
/// `ScopeData::symbols` instead of copying the name into two owned `String`s.
/// `clone()` deliberately yields `Vec<String>` for compatibility with the VM
/// pattern matcher (`vm/mod.rs`), which consumes owned names; use
/// [`Names::shared`] to copy the shared representation itself.
pub struct Names(Vec<Rc<str>>);

impl Names {
    fn shared(&self) -> Self {
        Names(self.0.clone())
    }

    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> Vec<String> {
        self.0.iter().map(|name| name.to_string()).collect()
    }
}

impl std::ops::Deref for Names {
    type Target = Vec<Rc<str>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Names {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct ScopeData {
    pub is_function_scope: bool,
    pub parent: Option<Scope>,
    /// Symbol keys are shared `Rc<str>` values so preseeding and snapshots
    /// bump a refcount instead of reallocating each name.
    pub symbols: HashMap<Rc<str>, usize>,

    pub order: Names,
    pub slots: Vec<Obj>,

    pub slot_scope: Option<Scope>,
    /// True when this scope owns its own slot vectors (set by `preseed_slots`).
    /// Replaces a self-referential `slot_scope` that leaked one Rc per call.
    pub owns_slots: bool,
}
pub fn new(parent: Option<Scope>, is_function_scope: bool) -> Scope {
    Rc::new(RefCell::new(ScopeData {
        is_function_scope,
        parent,
        symbols: HashMap::with_capacity(8),
        order: Names(Vec::new()),
        slots: Vec::new(),
        slot_scope: None,
        owns_slots: false,
    }))
}
pub fn empty() -> Scope {
    new(None, false)
}
pub fn preseed_slots(scope: &Scope, names: &[String]) -> bool {
    let mut d = scope.borrow_mut();
    if !d.slots.is_empty() || !d.order.is_empty() {
        return false;
    }
    let count = names.len();
    d.slots.reserve(count);
    d.order.reserve(count);
    d.symbols.reserve(count);
    let sentinel = world::uninitialized_obj();
    for name in names {
        let idx = d.slots.len();
        d.slots.push(sentinel.clone());
        // One Rc allocation per name, shared between the insertion order and
        // the symbol table (previously two owned String allocations).
        let shared: Rc<str> = Rc::from(name.as_str());
        d.order.push(shared.clone());
        d.symbols.insert(shared, idx);
    }
    d.owns_slots = true;
    true
}

/// Scope that owns the slot vectors reachable from `scope`.
pub fn slot_owner(scope: &Scope) -> Scope {
    let d = scope.borrow();
    if d.owns_slots {
        return scope.clone();
    }
    d.slot_scope.clone().unwrap_or_else(|| scope.clone())
}

pub fn inherit_slot_scope(parent: &Scope, child: &Scope) {
    let inherited = parent.borrow().slot_scope.clone().or(Some(parent.clone()));
    child.borrow_mut().slot_scope = inherited;
}

pub fn get_slot(scope: &Scope, idx: usize) -> Option<Obj> {
    {
        let d = scope.borrow();
        let shares_owner = d.owns_slots
            || d
                .slot_scope
                .as_ref()
                .map(|owner| Rc::ptr_eq(owner, scope))
                .unwrap_or(false);
        if shares_owner {
            let v = d.slots.get(idx)?;
            return if world::is_uninitialized(v) {
                None
            } else {
                Some(v.clone())
            };
        }
    }
    let owner = scope.borrow().slot_scope.clone()?;
    let v = owner.borrow().slots.get(idx)?.clone();
    if world::is_uninitialized(&v) {
        None
    } else {
        Some(v)
    }
}

pub fn set_slot(scope: &Scope, idx: usize, value: Obj) -> bool {
    let d = scope.borrow();
    let is_owner = d.owns_slots
        || d
            .slot_scope
            .as_ref()
            .map(|owner| Rc::ptr_eq(owner, scope))
            .unwrap_or(false);
    drop(d);
    if is_owner {
        let mut d = scope.borrow_mut();
        return match d.slots.get_mut(idx) {
            Some(slot) => {
                *slot = value;
                true
            }
            None => false,
        };
    }
    let Some(owner) = scope.borrow().slot_scope.clone() else {
        return false;
    };
    let mut d = owner.borrow_mut();
    let Some(slot) = d.slots.get_mut(idx) else {
        return false;
    };
    *slot = value;
    true
}

pub fn is_function_scope(scope: &Scope) -> bool {
    scope.borrow().is_function_scope
}
pub fn parent(scope: &Scope) -> Option<Scope> {
    scope.borrow().parent.clone()
}
pub fn get(scope: &Scope, key: &str) -> Obj {
    match get_opt(scope, key) {
        Some(v) => v,
        None => obj::err(format!("variable '{key}' not found")),
    }
}
pub fn get_opt(scope: &Scope, key: &str) -> Option<Obj> {
    // Check the slot in place and clone only initialized values; an
    // uninitialized slot is treated as absent and the search continues upward.
    let parent = {
        let d = scope.borrow();
        if let Some(&idx) = d.symbols.get(key) {
            let value = &d.slots[idx];
            if !world::is_uninitialized(value) {
                return Some(value.clone());
            }
        }
        d.parent.clone()
    };
    parent.and_then(|p| get_opt(&p, key))
}
pub fn set(scope: &Scope, key: &str, value: Obj) {
    // Walk the parent chain once to find the scope that declares `key`, then
    // write directly there. Previously this called `contains_key` (a second
    // walk) followed by a recursive `set` (a third walk). If no scope declares
    // the key, declare it in the original scope as before.
    let mut current = scope.clone();
    loop {
        let (idx, parent) = {
            let d = current.borrow();
            (d.symbols.get(key).copied(), d.parent.clone())
        };
        if let Some(idx) = idx {
            current.borrow_mut().slots[idx] = value;
            return;
        }
        match parent {
            Some(p) => current = p,
            None => {
                declare_new(scope, key, value);
                return;
            }
        }
    }
}
pub fn set_local(scope: &Scope, key: &str, value: Obj) {
    // One RefCell borrow instead of a read borrow followed by a write borrow.
    // Local stores are among the most frequent operations in the interpreter.
    let mut d = scope.borrow_mut();
    if let Some(&idx) = d.symbols.get(key) {
        d.slots[idx] = value;
        return;
    }
    let idx = d.slots.len();
    let shared: Rc<str> = Rc::from(key);
    d.symbols.insert(shared.clone(), idx);
    d.order.push(shared);
    d.slots.push(value);
}
#[cfg(test)]
pub(crate) fn declare(scope: &Scope, key: &str, initial: Option<Obj>) {
    if scope.borrow().symbols.contains_key(key) {
        return;
    }
    declare_new(scope, key, initial.unwrap_or_else(obj::none));
}
fn declare_new(scope: &Scope, key: &str, value: Obj) {
    let mut d = scope.borrow_mut();
    let idx = d.slots.len();
    let shared: Rc<str> = Rc::from(key);
    d.symbols.insert(shared.clone(), idx);
    d.order.push(shared);
    d.slots.push(value);
}
pub fn local_symbols(scope: &Scope) -> Vec<(String, Obj)> {
    let d = scope.borrow();
    d.order
        .iter()
        .filter_map(|name| {
            let idx = *d.symbols.get(&**name)?;
            match d.slots.get(idx) {
                Some(value) if !world::is_uninitialized(value) => Some((name.to_string(), value.clone())),
                _ => None,
            }
        })
        .collect()
}
pub fn contains_key(scope: &Scope, key: &str) -> bool {
    let parent = {
        let d = scope.borrow();
        if d.symbols.contains_key(key) {
            return true;
        }
        d.parent.clone()
    };
    parent.is_some_and(|p| contains_key(&p, key))
}
pub fn contains_key_in_top(scope: &Scope, key: &str) -> bool {
    scope.borrow().symbols.contains_key(key)
}
#[cfg(test)]
pub(crate) fn remove(scope: &Scope, key: &str) -> bool {
    let mut d = scope.borrow_mut();
    let removed = d.symbols.remove(key).is_some();
    if removed {
        d.order.retain(|name| name.as_ref() != key);
    }
    removed
}
pub fn slot_count(scope: &Scope) -> usize {
    scope.borrow().slots.len()
}
pub fn snapshot(scope: &Scope) -> Scope {
    let (is_function_scope, parent, symbols, order, slots, slot_scope) = {
        let d = scope.borrow();
        (
            d.is_function_scope,
            d.parent.clone(),
            d.symbols.clone(),
            d.order.shared(),
            d.slots.clone(),
            d.slot_scope.clone(),
        )
    };
    Rc::new(RefCell::new(ScopeData {
        is_function_scope,
        parent: parent.map(|p| snapshot(&p)),
        symbols,
        order,
        slots,
        slot_scope: slot_scope.clone(),
        owns_slots: scope.borrow().owns_slots,
    }))
}
#[cfg(test)]
mod tests;
