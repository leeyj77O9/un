use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use crate::runtime::obj;
use crate::runtime::obj::Obj;
pub type Scope = Rc<RefCell<ScopeData>>;
pub struct ScopeData {
    pub is_function_scope: bool,
    pub parent: Option<Scope>,
    pub symbols: HashMap<String, usize>,

    pub order: Vec<String>,
    pub slots: Vec<Obj>,
}
pub fn new(parent: Option<Scope>, is_function_scope: bool) -> Scope {
    Rc::new(RefCell::new(ScopeData {
        is_function_scope,
        parent,
        symbols: HashMap::new(),
        order: Vec::new(),
        slots: Vec::new(),
    }))
}
pub fn empty() -> Scope {
    new(None, false)
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
            return Some(d.slots[idx].clone());
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
}
pub fn local_symbols(scope: &Scope) -> Vec<(String, Obj)> {
    let d = scope.borrow();
    d.order
        .iter()
        .filter_map(|name| {
            let idx = *d.symbols.get(name)?;
            d.slots.get(idx).map(|value| (name.clone(), value.clone()))
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
    let (is_function_scope, parent, symbols, order, slots) = {
        let d = scope.borrow();
        (
            d.is_function_scope,
            d.parent.clone(),
            d.symbols.clone(),
            d.order.clone(),
            d.slots.clone(),
        )
    };
    Rc::new(RefCell::new(ScopeData {
        is_function_scope,
        parent: parent.map(|p| snapshot(&p)),
        symbols,
        order,
        slots,
    }))
}
#[cfg(test)]
mod tests;
