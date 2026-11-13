use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use crate::runtime::obj;
use crate::runtime::obj::Obj;
use crate::runtime::world;
pub type Scope = Rc<RefCell<ScopeData>>;
pub struct ScopeData {
    pub is_function_scope: bool,
    pub parent: Option<Scope>,
    pub symbols: HashMap<String, usize>,

    pub order: Vec<String>,
    pub slots: Vec<Obj>,
    pub slot_names: Vec<String>,

    /// the scope that owns the function's slot table, if any; children inherit it
    pub slot_scope: Option<Scope>,
}
pub fn new(parent: Option<Scope>, is_function_scope: bool) -> Scope {
    Rc::new(RefCell::new(ScopeData {
        is_function_scope,
        parent,
        symbols: HashMap::new(),
        order: Vec::new(),
        slots: Vec::new(),
        slot_names: Vec::new(),
        slot_scope: None,
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
    d.slot_names.reserve(count);
    d.order.reserve(count);
    d.symbols.reserve(count);
    let sentinel = world::uninitialized_obj();
    for name in names {
        let idx = d.slots.len();
        d.slots.push(sentinel.clone());
        d.slot_names.push(name.clone());
        d.order.push(name.clone());
        d.symbols.insert(name.clone(), idx);
    }
    drop(d);
    scope.borrow_mut().slot_scope = Some(scope.clone());
    true
}

pub fn inherit_slot_scope(parent: &Scope, child: &Scope) {
    let inherited = parent.borrow().slot_scope.clone().or(Some(parent.clone()));
    child.borrow_mut().slot_scope = inherited;
}

pub fn get_slot(scope: &Scope, idx: usize) -> Option<Obj> {
    {
        let d = scope.borrow();
        let shares_owner = d
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

pub fn get_slot_name(scope: &Scope, idx: usize) -> Option<String> {
    let owner = scope.borrow().slot_scope.clone()?;
    let d = owner.borrow();
    d.slot_names.get(idx).cloned()
}

pub fn set_slot(scope: &Scope, idx: usize, value: Obj) -> bool {
    let is_owner = scope
        .borrow()
        .slot_scope
        .as_ref()
        .map(|owner| Rc::ptr_eq(owner, scope))
        .unwrap_or(false);
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
    let parent = {
        let d = scope.borrow();
        if let Some(&idx) = d.symbols.get(key) {
            let v = d.slots[idx].clone();
            if world::is_uninitialized(&v) {
                let parent = d.parent.clone();
                return parent.and_then(|p| get_opt(&p, key));
            }
            return Some(v);
        }
        d.parent.clone()
    };
    parent.and_then(|p| get_opt(&p, key))
}
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
pub fn set_local(scope: &Scope, key: &str, value: Obj) {
    let slot = scope.borrow().symbols.get(key).copied();
    match slot {
        Some(idx) => scope.borrow_mut().slots[idx] = value,
        None => declare_new(scope, key, value),
    }
}
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
    d.order.push(key.to_owned());
    d.slots.push(value);
    d.slot_names.push(key.to_owned());
}
pub fn local_symbols(scope: &Scope) -> Vec<(String, Obj)> {
    let d = scope.borrow();
    d.order
        .iter()
        .filter_map(|name| {
            let idx = *d.symbols.get(name)?;
            match d.slots.get(idx) {
                Some(value) if !world::is_uninitialized(value) => Some((name.clone(), value.clone())),
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
pub fn remove(scope: &Scope, key: &str) -> bool {
    let mut d = scope.borrow_mut();
    let removed = d.symbols.remove(key).is_some();
    if removed {
        d.order.retain(|name| name != key);
    }
    removed
}
pub fn slot_count(scope: &Scope) -> usize {
    scope.borrow().slots.len()
}
pub fn snapshot(scope: &Scope) -> Scope {
    let (is_function_scope, parent, symbols, order, slots, slot_names, slot_scope) = {
        let d = scope.borrow();
        (
            d.is_function_scope,
            d.parent.clone(),
            d.symbols.clone(),
            d.order.clone(),
            d.slots.clone(),
            d.slot_names.clone(),
            d.slot_scope.clone(),
        )
    };
    Rc::new(RefCell::new(ScopeData {
        is_function_scope,
        parent: parent.map(|p| snapshot(&p)),
        symbols,
        order,
        slots,
        slot_names: slot_names.clone(),
        slot_scope: slot_scope.clone(),
    }))
}
#[cfg(test)]
mod tests;
