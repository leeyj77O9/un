//! `Scope` — the parent-chained slot table of `Un/Runner/Scope.cs`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::runtime::obj;
use crate::runtime::obj::Obj;

/// the reference keeps `Dictionary<string, int>` (name → slot index) beside a
/// `List<Obj>` of slots, so assigning a captured variable writes into the slot that
/// declares it. that is what makes `sum += v` inside a `for` body accumulate.
pub type Scope = Rc<RefCell<ScopeData>>;

pub struct ScopeData {
    pub is_function_scope: bool,
    pub parent: Option<Scope>,
    pub symbols: HashMap<String, usize>,
    pub slots: Vec<Obj>,
}

pub fn new(parent: Option<Scope>, is_function_scope: bool) -> Scope {
    Rc::new(RefCell::new(ScopeData {
        is_function_scope,
        parent,
        symbols: HashMap::new(),
        slots: Vec::new(),
    }))
}

/// `Scope.Empty`
pub fn empty() -> Scope {
    new(None, false)
}

pub fn is_function_scope(scope: &Scope) -> bool {
    scope.borrow().is_function_scope
}

pub fn parent(scope: &Scope) -> Option<Scope> {
    scope.borrow().parent.clone()
}

/// `Scope.Get(key)` — missing keys are `Err`, matching the reference message.
pub fn get(scope: &Scope, key: &str) -> Obj {
    match get_opt(scope, key) {
        Some(v) => v,
        None => obj::err(format!("variable '{key}' not found")),
    }
}

/// `Scope.Get(key, out value)`
pub fn get_opt(scope: &Scope, key: &str) -> Option<Obj> {
    let parent = {
        let d = scope.borrow();
        if let Some(&idx) = d.symbols.get(key) {
            return Some(d.slots[idx].clone());
        }
        d.parent.clone()
    };
    parent.and_then(|p| get_opt(&p, key))
}

/// `Scope.Set(key, value)` — local slot, else the scope chain that declares the key,
/// else declare it here.
pub fn set(scope: &Scope, key: &str, value: Obj) {
    let (slot, parent) = {
        let d = scope.borrow();
        match d.symbols.get(key) {
            Some(&idx) => (Some(idx), None),
            None => (None, d.parent.clone()),
        }
    };

    if let Some(idx) = slot {
        scope.borrow_mut().slots[idx] = value;
        return;
    }

    if let Some(p) = parent {
        if contains_key(&p, key) {
            set(&p, key, value);
            return;
        }
    }

    declare_new(scope, key, value);
}

/// `Scope.SetLocal` / `Scope.SetLocalOrDeclare` — both write only the local slot table,
/// so a function body assignment never touches an outer binding.
pub fn set_local(scope: &Scope, key: &str, value: Obj) {
    let slot = scope.borrow().symbols.get(key).copied();
    match slot {
        Some(idx) => scope.borrow_mut().slots[idx] = value,
        None => declare_new(scope, key, value),
    }
}

/// `Scope.Declare(key, initial)` — never overwrites an existing slot.
pub fn declare(scope: &Scope, key: &str, initial: Option<Obj>) {
    if scope.borrow().symbols.contains_key(key) {
        return;
    }
    declare_new(scope, key, initial.unwrap_or_else(obj::none));
}

fn declare_new(scope: &Scope, key: &str, value: Obj) {
    let mut d = scope.borrow_mut();
    let idx = d.slots.len();
    d.symbols.insert(key.to_owned(), idx);
    d.slots.push(value);
}

/// `Scope.ContainsKey` — walks the whole parent chain.
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

/// `Scope.ContainsKeyInTop`
pub fn contains_key_in_top(scope: &Scope, key: &str) -> bool {
    scope.borrow().symbols.contains_key(key)
}

/// `Scope.Remove(key)` — returns whether a slot name was removed.
pub fn remove(scope: &Scope, key: &str) -> bool {
    scope.borrow_mut().symbols.remove(key).is_some()
}

pub fn slot_count(scope: &Scope) -> usize {
    scope.borrow().slots.len()
}

#[cfg(test)]
mod tests;
