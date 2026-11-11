//! collection objects: `List`, `Dict`, `Set`, `Tup`, `Spreads`, `Json`, `Stru`, `Enu`, `TObj`.
//!
//! every collection payload is a shared cell. key equality is `Eq()` (the reference
//! stores `Obj` keys in a `Dictionary`/`HashSet`, whose comparer is `Obj.Equals` -> `Eq`),
//! and bucketing uses `obj::hash_code`. per D-021 the reference's string hash is
//! process-randomised, so per D-025 Rust collections keep insertion order: int keys
//! already iterate in insertion order in the reference, and str-key order is not a
//! parity target.

use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::function;
use crate::runtime::iters;
use crate::runtime::obj;
use crate::runtime::obj::{BinOp, Obj, Payload, UnOp};
use crate::runtime::prims;
use crate::runtime::types::Types;
use crate::runtime::world;

/// the ordered key/value table of a `Dict`. each entry carries the key's `hash_code`,
/// so lookup is hash-then-`Eq`, exactly like the reference's `Dictionary<Obj, Obj>`.
pub struct KeyMap {
    pub entries: Vec<(u64, Obj, Obj)>,
}

impl KeyMap {
    pub fn new(entries: Vec<(Obj, Obj)>) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|(k, v)| (obj::hash_code(&k), k, v))
                .collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn find(&self, key: &Obj) -> Option<usize> {
        let h = obj::hash_code(key);
        self.entries
            .iter()
            .enumerate()
            .find(|(_, (hh, k, _))| *hh == h && obj::key_eq(k, key))
            .map(|(i, _)| i)
    }

    pub fn contains_key(&self, key: &Obj) -> bool {
        self.find(key).is_some()
    }

    pub fn get(&self, key: &Obj) -> Option<Obj> {
        self.find(key).map(|i| self.entries[i].2.clone())
    }

    /// `Value[key] = value`
    pub fn set(&mut self, key: &Obj, value: &Obj) {
        match self.find(key) {
            Some(i) => self.entries[i].2 = value.clone(),
            None => self
                .entries
                .push((obj::hash_code(key), key.clone(), value.clone())),
        }
    }

    /// `dict.add`: a duplicate key throws inside the native. the inner .NET message is
    /// reproduced verbatim and the `TargetInvocationException` wrapper is dropped (D-024).
    pub fn add(&mut self, key: &Obj, value: &Obj) -> Obj {
        if self.find(key).is_some() {
            return obj::panic(
                "System.ArgumentException",
                format!(
                    "An item with the same key has already been added. Key: {}",
                    clr_to_string(key)
                ),
            );
        }
        self.entries
            .push((obj::hash_code(key), key.clone(), value.clone()));
        obj::none()
    }

    pub fn remove(&mut self, key: &Obj) -> bool {
        match self.find(key) {
            Some(i) => {
                self.entries.remove(i);
                true
            }
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn keys(&self) -> Vec<Obj> {
        self.entries.iter().map(|(_, k, _)| k.clone()).collect()
    }

    pub fn values(&self) -> Vec<Obj> {
        self.entries.iter().map(|(_, _, v)| v.clone()).collect()
    }

    pub fn contains_value(&self, value: &Obj) -> bool {
        self.entries.iter().any(|(_, _, v)| obj::key_eq(v, value))
    }

    /// `Dict.Overlap(dict)`: every entry of the argument must be in self with an equal value.
    pub fn overlap(&self, other: &KeyMap) -> bool {
        for (_, k, v) in &other.entries {
            match self.get(k) {
                Some(mine) => {
                    if !obj::key_eq(&mine, v) {
                        return false;
                    }
                }
                None => return false,
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// constructors
// ---------------------------------------------------------------------------

pub fn list(values: Vec<Obj>) -> Obj {
    obj::obj_new("list", Payload::List(Rc::new(RefCell::new(values))))
}

pub fn dict(entries: Vec<(Obj, Obj)>) -> Obj {
    obj::obj_new("dict", Payload::Dict(Rc::new(RefCell::new(KeyMap::new(entries)))))
}

/// `new Set([.. values])`: a `HashSet` deduplicates on insertion.
pub fn set(items: Vec<Obj>) -> Obj {
    let mut kept: Vec<Obj> = Vec::new();
    for item in items {
        if !kept.iter().any(|k| obj::key_eq(k, &item)) {
            kept.push(item);
        }
    }
    obj::obj_new("set", Payload::Set(Rc::new(RefCell::new(kept))))
}

/// `Tup(values, names)`: non-empty names are mirrored into `Members`.
pub fn tup(values: Vec<Obj>, names: Vec<String>) -> Obj {
    let o = obj::obj_new("tuple", Payload::Tup(Rc::new(values), Rc::new(names)));
    let pairs: Vec<(String, Obj)> = {
        let d = o.borrow();
        match &d.payload {
            Payload::Tup(vals, ns) => ns
                .iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), vals[i].clone()))
                .collect(),
            _ => Vec::new(),
        }
    };
    for (name, value) in pairs {
        if !name.is_empty() {
            obj::member_set(&o, &name, &value);
        }
    }
    o
}

pub fn spread(values: Vec<Obj>) -> Obj {
    obj::obj_new("spread", Payload::Spread(Rc::new(values)))
}

pub fn json(inner: Obj) -> Obj {
    obj::obj_new("json", Payload::Json(inner))
}

pub fn stru(type_name: &str, names: Vec<String>) -> Obj {
    obj::obj_new(type_name, Payload::Stru(Rc::new(names)))
}

pub fn enu(type_name: &str, n: i64) -> Obj {
    obj::obj_new(type_name, Payload::Enu(n))
}

pub fn tobj(types: Types) -> Obj {
    obj::obj_new("type", Payload::TObj(types))
}

fn list_of(o: &Obj) -> Vec<Obj> {
    match obj::payload(o) {
        Payload::List(v) => v.borrow().clone(),
        _ => Vec::new(),
    }
}

fn set_of(o: &Obj) -> Vec<Obj> {
    match obj::payload(o) {
        Payload::Set(s) => s.borrow().clone(),
        _ => Vec::new(),
    }
}

fn tup_of(o: &Obj) -> (Vec<Obj>, Vec<String>) {
    match obj::payload(o) {
        Payload::Tup(v, n) => (v.to_vec(), n.to_vec()),
        _ => (Vec::new(), Vec::new()),
    }
}

fn out_of_range(index: i64, count: i64) -> bool {
    index < 0 || index >= count
}

/// `List.Overlap(self, list)`: every element of the argument appears in self.
fn overlap(needle: &[Obj], haystack: &[Obj]) -> bool {
    needle
        .iter()
        .all(|n| haystack.iter().any(|h| obj::key_eq(h, n)))
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

pub fn list_get_item(v: &Rc<RefCell<Vec<Obj>>>, key: &Obj) -> Obj {
    match obj::payload(key) {
        Payload::Int(i) => {
            let count = v.borrow().len() as i64;
            // the reference casts the long index through `(int)`, so a wide index wraps
            let idx = i as i32 as i64;
            if out_of_range(idx, count) {
                let wrapped = (i + count) as i32 as i64;
                if out_of_range(wrapped, count) {
                    return obj::err("list index out of range");
                }
                return v.borrow()[wrapped as usize].clone();
            }
            v.borrow()[idx as usize].clone()
        }
        _ => obj::err("invalid index type"),
    }
}

pub fn list_set_item(v: &Rc<RefCell<Vec<Obj>>>, key: &Obj, value: &Obj) -> Obj {
    match obj::payload(key) {
        Payload::Int(i) => {
            let count = v.borrow().len() as i64;
            let idx = i as i32 as i64;
            if !out_of_range(idx, count) {
                v.borrow_mut()[idx as usize] = value.clone();
                return value.clone();
            }
            let wrapped = (i + count) as i32 as i64;
            if !out_of_range(wrapped, count) {
                v.borrow_mut()[wrapped as usize] = value.clone();
                return value.clone();
            }
            obj::err("list index out of range")
        }
        _ => obj::err("invalid index type"),
    }
}

/// `List.Insert` (`List.cs:277`) — the shape `BindPattern`'s slice branch uses.
/// The tail shift runs from `Count-1` down to `idx`, the stored element is a copy
/// (`obj.Copy()`), so the caller's object is not aliased into the list.
/// An index outside `0..=Count` is `Err` here; the reference lets the .NET array indexer
/// throw `IndexOutOfRange` instead, which is documented as a divergence in phase 14.
pub fn list_insert(v: &Rc<RefCell<Vec<Obj>>>, value: &Obj, index: &Obj) -> Result<(), Obj> {
    let idx = match obj::payload(index) {
        Payload::Int(i) => i as i32 as i64,
        _ => return Err(obj::err("invalid arguments: index")),
    };

    let count = v.borrow().len() as i64;
    if count == 0 {
        v.borrow_mut().push(value.clone());
        return Ok(());
    }
    if idx < 0 || idx > count {
        return Err(obj::err("list index out of range"));
    }

    let mut data = v.borrow_mut();
    // `List` has spare capacity (`Resize`), so grow before shifting; the reference shifts
    // from `Count-1` down to `idx` and writes the copy at `idx`.
    data.push(obj::none());
    for i in (idx..count).rev() {
        let moved = data[i as usize].clone();
        data[i as usize + 1] = moved;
    }
    data[idx as usize] = obj::copy_obj(value);

    Ok(())
}

/// `List.RemoveAt` (`List.cs:358`) — out of range is `Bool.False`, otherwise the tail
/// shifts left and the count drops. The reference returns `self`, which `BindPattern`
/// ignores, so the result is dropped here.
pub fn list_remove_at(v: &Rc<RefCell<Vec<Obj>>>, index: &Obj) -> Result<(), Obj> {
    let idx = match obj::payload(index) {
        Payload::Int(i) => i as i32 as i64,
        _ => return Err(obj::err("invalid arguments: index")),
    };

    let mut data = v.borrow_mut();
    let count = data.len() as i64;
    if idx < 0 || idx >= count {
        return Ok(());
    }

    for i in idx..count - 1 {
        let moved = data[i as usize + 1].clone();
        data[i as usize] = moved;
    }
    data.pop();

    Ok(())
}

/// `List.Slice`: the `step == 1` fast path is an `Array.Copy`, so it never calls `Add`.
pub fn list_slice(
    v: &Rc<RefCell<Vec<Obj>>>,
    start: &Option<Obj>,
    end: &Option<Obj>,
    step: &Option<Obj>,
) -> Obj {
    let mine = v.borrow().clone();
    let len = mine.len() as i64;
    let (mut a, mut b, mut st) = (0usize, mine.len(), 1i64);

    if let Some(start) = start {
        let i = match obj::payload(start) {
            Payload::Int(i) => i,
            _ => return obj::err("slice start must be int"),
        };
        let mut i = i as i32 as i64;
        if i < 0 {
            i += len;
        }
        a = i.clamp(0, len) as usize;
    }
    if let Some(end) = end {
        let i = match obj::payload(end) {
            Payload::Int(i) => i,
            _ => return obj::err("slice end must be int"),
        };
        let mut i = i as i32 as i64;
        if i < 0 {
            i += len;
        }
        b = i.clamp(0, len) as usize;
    }
    if let Some(step) = step {
        let i = match obj::payload(step) {
            Payload::Int(i) => i,
            _ => return obj::err("slice step must be int"),
        };
        st = i as i32 as i64;
        if st == 0 {
            return obj::err("slice step cannot be zero");
        }
    }

    if st == 1 {
        let count = b as i64 - a as i64;
        if count <= 0 {
            return list(Vec::new());
        }
        return list(mine[a..a + count as usize].to_vec());
    }

    // `new List()` has capacity 0, so every `Add` in the step loop answers `Err` and
    // `Slice` drops it: any step != 1 gives an empty list. the `this[i]` evaluation is
    // kept, because a negative step with `s == len` throws
    // `IndexOutOfRangeException` out of the host.
    if st > 0 {
        let mut i = a as i64;
        while i < b as i64 {
            if i >= len {
                return index_out_of_range();
            }
            i += st;
        }
    } else {
        let mut i = a as i64;
        while i > b as i64 {
            // the reference clamps `s` to `len`, so `s == len` with a negative step
            // indexes past the end and throws inside the host
            if i >= len {
                return index_out_of_range();
            }
            i += st;
        }
    }
    list(Vec::new())
}

/// `List.In`: the argument must be a list or a tuple.
pub fn list_in(o: &Obj, other: &Obj) -> Obj {
    let ot = obj::type_name(other);
    let mine = list_of(o);
    match obj::payload(other) {
        Payload::List(v2) => obj::boolean(overlap(&v2.borrow().clone(), &mine)),
        Payload::Tup(vals, _) => obj::boolean(overlap(&vals, &mine)),
        _ => obj::err(format!(
            "cannot check if '{ot}' is in '{}'",
            obj::type_name(o)
        )),
    }
}

pub fn list_binop(o: &Obj, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    if op == BinOp::Eq {
        return element_eq(&list_of(o), other, |p| match p {
            Payload::List(v) => Some(v.borrow().clone()),
            _ => None,
        });
    }
    obj::base_binop(o, op, other, t, ot)
}

/// the shared `Eq` loop of `List` and `Tup`: compare lengths, then `NEq` each pair.
fn element_eq(
    mine: &[Obj],
    other: &Obj,
    same_kind: impl Fn(&Payload) -> Option<Vec<Obj>>,
) -> Obj {
    let theirs = match same_kind(&obj::payload(other)) {
        Some(v) => v,
        None => return obj::boolean(false),
    };
    if mine.len() != theirs.len() {
        return obj::boolean(false);
    }
    for i in 0..mine.len() {
        let neq = obj::binop(&mine[i], BinOp::NEq, &theirs[i]);
        match obj::payload(&neq) {
            Payload::Bool(v) => {
                if v {
                    return obj::boolean(false);
                }
            }
            Payload::Error { .. } => return neq,
            _ => return obj::err("equals operand is must be a boolean"),
        }
    }
    obj::boolean(true)
}

pub fn list_unop(v: &Rc<RefCell<Vec<Obj>>>, op: UnOp, t: &Rc<str>) -> Obj {
    let items = v.borrow().len();
    match op {
        UnOp::Len => obj::int(items as i64),
        UnOp::ToBool => obj::boolean(items != 0),
        UnOp::ToList => list(v.borrow().iter().map(|e| obj::copy_obj(e)).collect()),
        UnOp::ToTuple => tup(v.borrow().clone(), vec![String::new(); items]),
        UnOp::Spread => spread(v.borrow().clone()),
        // `List.Iter() => new(this)`: the enumerator walks the same cell, so items
        // appended during iteration are seen.
        UnOp::Iter => iters::seq_live(v),
        _ => obj::err(obj::UnOp::message(&op, t)),
    }
}

pub fn list_to_str(v: &Rc<RefCell<Vec<Obj>>>) -> String {
    let elements = v.borrow().clone();
    let mut out = String::from("[");
    for (i, e) in elements.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        match obj::payload(e) {
            Payload::Str(s) => {
                out.push('\'');
                out.push_str(&s);
                out.push('\'');
            }
            _ => out.push_str(&obj::display(e)),
        }
    }
    out.push(']');
    out
}

pub fn list_clone(v: &Rc<RefCell<Vec<Obj>>>) -> Obj {
    list(v.borrow().iter().map(|e| obj::clone_obj(e)).collect())
}

/// `List.Init`: 0 args keeps the object, 1 arg converts it, more become the args list.
pub fn list_init(o: &Obj, args: &[Obj]) -> Obj {
    match args.len() {
        0 => o.clone(),
        1 => {
            let converted = obj::to_list(&args[0]);
            if matches!(obj::payload(&converted), Payload::List(..)) {
                return converted;
            }
            list(args.to_vec())
        }
        _ => list(args.to_vec()),
    }
}

// ---------------------------------------------------------------------------
// Dict
// ---------------------------------------------------------------------------

pub fn dict_get_item(km: &Rc<RefCell<KeyMap>>, key: &Obj) -> Obj {
    match km.borrow().get(key) {
        Some(v) => v,
        None => obj::err(format!(
            "key '{}' not found in dictionary",
            obj::display(key)
        )),
    }
}

pub fn dict_set_item(km: &Rc<RefCell<KeyMap>>, key: &Obj, value: &Obj) -> Obj {
    km.borrow_mut().set(key, value);
    value.clone()
}

pub fn dict_in(o: &Obj, other: &Obj) -> Obj {
    match obj::payload(o) {
        Payload::Dict(km) => {
            let mine = km.borrow();
            match obj::payload(other) {
                Payload::Dict(km2) => obj::boolean(mine.overlap(&km2.borrow())),
                _ => obj::boolean(mine.contains_key(other)),
            }
        }
        _ => obj::boolean(false),
    }
}

pub fn dict_to_str(km: &Rc<RefCell<KeyMap>>) -> String {
    let snapshot: Vec<String> = km
        .borrow()
        .entries
        .iter()
        .map(|(_, k, v)| format!("{}:{}", obj::display(k), obj::display(v)))
        .collect();
    format!("{{{}}}", snapshot.join(", "))
}

fn pair(key: &Obj, value: &Obj) -> Obj {
    tup(
        vec![key.clone(), value.clone()],
        vec!["key".to_owned(), "value".to_owned()],
    )
}

pub fn dict_unop(km: &Rc<RefCell<KeyMap>>, op: UnOp, t: &Rc<str>) -> Obj {
    let snapshot: Vec<(Obj, Obj)> = km
        .borrow()
        .entries
        .iter()
        .map(|(_, k, v)| (k.clone(), v.clone()))
        .collect();
    match op {
        UnOp::Len => obj::int(snapshot.len() as i64),
        UnOp::ToList => list(snapshot.iter().map(|(k, v)| pair(k, v)).collect()),
        UnOp::ToTuple => tup(snapshot.iter().map(|(k, v)| pair(k, v)).collect(), Vec::new()),
        UnOp::Spread => spread(snapshot.iter().map(|(k, v)| pair(k, v)).collect()),
        // `Dict.Iter() => new([.. key/value pairs])`, materialized at construction
        UnOp::Iter => iters::seq(snapshot.iter().map(|(k, v)| pair(k, v)).collect()),
        _ => obj::err(UnOp::message(&op, t)),
    }
}

pub fn dict_clone(km: &Rc<RefCell<KeyMap>>) -> Obj {
    obj::obj_new(
        "dict",
        Payload::Dict(Rc::new(RefCell::new(KeyMap {
            entries: km.borrow().entries.clone(),
        }))),
    )
}

pub fn dict_init(o: &Obj, args: &[Obj]) -> Obj {
    match args.len() {
        0 => dict(Vec::new()),
        1 => {
            // `Tup` entries keep the `Name` order; `Stru` has no `ToTuple`, so the
            // reference's `Init(new([st.ToTuple()]))` path ends in the same error.
            match obj::payload(&args[0]) {
                Payload::Tup(vals, names) => {
                    let mut entries: Vec<(Obj, Obj)> = Vec::new();
                    for (i, name) in names.iter().enumerate() {
                        if name.is_empty() {
                            continue;
                        }
                        entries.push((obj::str(name), vals[i].clone()));
                    }
                    dict(entries)
                }
                Payload::Stru(_) => dict_init(o, &[obj::to_tuple(&args[0])]),
                _ => obj::err("invaild 'dict' initialize"),
            }
        }
        _ => obj::err("invaild 'dict' initialize"),
    }
}

// ---------------------------------------------------------------------------
// Set
// ---------------------------------------------------------------------------

/// `HashSet.TryGetValue` returns the stored element, not the queried key.
pub fn set_get_item(s: &Rc<RefCell<Vec<Obj>>>, key: &Obj) -> Obj {
    match s.borrow().iter().find(|item| obj::key_eq(item, key)).cloned() {
        Some(item) => item,
        None => obj::err(format!("key {} not found in set", obj::display(key))),
    }
}

pub fn set_binop(o: &Obj, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    let mine = set_of(o);
    match op {
        BinOp::Add => match obj::payload(other) {
            Payload::Set(s2) => set(union(&mine, &s2.borrow().clone())),
            _ => obj::err(format!(
                "unsupported operand type(s) for +: 'set' and '{ot}'"
            )),
        },
        BinOp::Sub => match obj::payload(other) {
            Payload::Set(s2) => set(except(&mine, &s2.borrow().clone())),
            _ => obj::err(format!(
                "unsupported operand type(s) for -: 'set' and '{ot}'"
            )),
        },
        BinOp::BXor => match obj::payload(other) {
            Payload::Set(s2) => set(intersect(&mine, &s2.borrow().clone())),
            _ => obj::err(format!(
                "unsupported operand type(s) for ^: 'set' and '{ot}'"
            )),
        },
        _ => obj::base_binop(o, op, other, t, ot),
    }
}

fn union(a: &[Obj], b: &[Obj]) -> Vec<Obj> {
    let mut out = a.to_vec();
    for item in b {
        if !out.iter().any(|k| obj::key_eq(k, item)) {
            out.push(item.clone());
        }
    }
    out
}

fn except(a: &[Obj], b: &[Obj]) -> Vec<Obj> {
    a.iter()
        .filter(|item| !b.iter().any(|k| obj::key_eq(k, item)))
        .cloned()
        .collect()
}

fn intersect(a: &[Obj], b: &[Obj]) -> Vec<Obj> {
    let mut out: Vec<Obj> = Vec::new();
    for item in a {
        if b.iter().any(|k| obj::key_eq(k, item)) && !out.iter().any(|k| obj::key_eq(k, item)) {
            out.push(item.clone());
        }
    }
    out
}

pub fn set_to_str(s: &Rc<RefCell<Vec<Obj>>>) -> String {
    let parts: Vec<String> = s.borrow().iter().map(|i| obj::display(i)).collect();
    format!("{{{}}}", parts.join(", "))
}

pub fn set_unop(s: &Rc<RefCell<Vec<Obj>>>, op: UnOp, t: &Rc<str>) -> Obj {
    // `Set` overrides `Len`, `GetItem`, `Copy`, `Clone`, `ToStr` and `Spread` only
    match op {
        UnOp::Len => obj::int(s.borrow().len() as i64),
        UnOp::Spread => spread(s.borrow().clone()),
        _ => obj::err(UnOp::message(&op, t)),
    }
}

pub fn set_init(o: &Obj, args: &[Obj]) -> Obj {
    // `new Set([.. args.ToList()])`
    let _ = o;
    set(args.to_vec())
}

pub fn set_clone(s: &Rc<RefCell<Vec<Obj>>>) -> Obj {
    set(s.borrow().clone())
}

// ---------------------------------------------------------------------------
// Tup
// ---------------------------------------------------------------------------

pub fn tup_get_item(vals: &Rc<Vec<Obj>>, _names: &Rc<Vec<String>>, o: &Obj, key: &Obj) -> Obj {
    match obj::payload(key) {
        Payload::Int(i) => {
            let count = vals.len() as i64;
            let idx = i as i32 as i64;
            if out_of_range(idx, count) {
                let wrapped = (i + count) as i32 as i64;
                if out_of_range(wrapped, count) {
                    return obj::err("tuple index out of range");
                }
                return vals[wrapped as usize].clone();
            }
            vals[idx as usize].clone()
        }
        Payload::Str(s) => match obj::member(o, &s) {
            Some(v) => v,
            None => obj::err(format!("tuple has no attribute '{s}'")),
        },
        _ => obj::err("invalid index type"),
    }
}

/// `Tup.Slice` keeps the parallel `Name` entries.
pub fn tup_slice(
    vals: &Rc<Vec<Obj>>,
    names: &Rc<Vec<String>>,
    start: &Option<Obj>,
    end: &Option<Obj>,
    step: &Option<Obj>,
) -> Obj {
    let len = vals.len() as i64;
    let (mut a, mut b, mut st) = (0usize, vals.len(), 1i64);

    if let Some(start) = start {
        let i = match obj::payload(start) {
            Payload::Int(i) => i,
            _ => return obj::err("slice start must be int"),
        };
        let mut i = i as i32 as i64;
        if i < 0 {
            i += len;
        }
        a = i.clamp(0, len) as usize;
    }
    if let Some(end) = end {
        let i = match obj::payload(end) {
            Payload::Int(i) => i,
            _ => return obj::err("slice end must be int"),
        };
        let mut i = i as i32 as i64;
        if i < 0 {
            i += len;
        }
        b = i.clamp(0, len) as usize;
    }
    if let Some(step) = step {
        let i = match obj::payload(step) {
            Payload::Int(i) => i,
            _ => return obj::err("slice step must be int"),
        };
        st = i as i32 as i64;
        if st == 0 {
            return obj::err("slice step cannot be zero");
        }
    }

    let mut new_values: Vec<Obj> = Vec::new();
    let mut new_names: Vec<String> = Vec::new();
    if st > 0 {
        let mut i = a as i64;
        while i < b as i64 {
            if i >= len {
                return index_out_of_range();
            }
            new_values.push(vals[i as usize].clone());
            new_names.push(names[i as usize].clone());
            i += st;
        }
    } else {
        let mut i = a as i64;
        while i > b as i64 {
            if i >= len {
                return index_out_of_range();
            }
            new_values.push(vals[i as usize].clone());
            new_names.push(names[i as usize].clone());
            i += st;
        }
    }
    tup(new_values, new_names)
}

/// `Tup.Slice` / `List.Slice` index the backing array directly, so a clamped
/// `start == len` with a negative step throws inside the host (D-024)
fn index_out_of_range() -> Obj {
    obj::panic(
        "System.IndexOutOfRangeException",
        "Index was outside the bounds of the array.",
    )
}

/// `Tup.In`: `Eq` each element; a non-boolean result is the error.
pub fn tup_in(vals: &Rc<Vec<Obj>>, other: &Obj) -> Obj {
    for value in vals.iter() {
        let eq = obj::binop(value, BinOp::Eq, other);
        match obj::payload(&eq) {
            Payload::Bool(v) => {
                if v {
                    return obj::boolean(true);
                }
            }
            Payload::Error { .. } => return eq,
            _ => return obj::err("equals operand is must be a boolean"),
        }
    }
    obj::boolean(false)
}

pub fn tup_binop(o: &Obj, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    if op == BinOp::Eq {
        let (mine, _) = tup_of(o);
        return element_eq(&mine, other, |p| match p {
            Payload::Tup(v, _) => Some(v.to_vec()),
            _ => None,
        });
    }
    obj::base_binop(o, op, other, t, ot)
}

pub fn tup_to_str(vals: &Rc<Vec<Obj>>) -> String {
    let parts: Vec<String> = vals.iter().map(|v| obj::display(v)).collect();
    format!("({})", parts.join(", "))
}

pub fn tup_unop(vals: &Rc<Vec<Obj>>, names: &Rc<Vec<String>>, op: UnOp, t: &Rc<str>) -> Obj {
    match op {
        UnOp::Len => obj::int(vals.len() as i64),
        UnOp::ToBool => obj::boolean(!vals.is_empty()),
        UnOp::ToList => list(vals.to_vec()),
        UnOp::ToTuple => tup(vals.to_vec(), names.to_vec()),
        UnOp::Spread => spread(vals.to_vec()),
        // `Tup.Iter() => new(this)`: walks the tuple's values
        UnOp::Iter => iters::seq(vals.to_vec()),
        _ => obj::err(UnOp::message(&op, t)),
    }
}

/// `Tup.Init`: 0 args keeps the object, 1 arg converts it, more become the args tuple.
pub fn tup_init(o: &Obj, args: &[Obj]) -> Obj {
    match args.len() {
        0 => o.clone(),
        1 => {
            let converted = obj::to_tuple(&args[0]);
            if matches!(obj::payload(&converted), Payload::Tup(..)) {
                return converted;
            }
            o.clone()
        }
        // `args` is the argument tuple, whose names are all empty
        _ => tup(args.to_vec(), vec![String::new(); args.len()]),
    }
}

pub fn tup_copy(vals: &Rc<Vec<Obj>>, names: &Rc<Vec<String>>) -> Obj {
    tup(vals.iter().map(|e| obj::copy_obj(e)).collect(), names.to_vec())
}

pub fn tup_clone(vals: &Rc<Vec<Obj>>, names: &Rc<Vec<String>>) -> Obj {
    tup(
        vals.iter().map(|e| obj::clone_obj(e)).collect(),
        names.to_vec(),
    )
}

pub fn is_single(vals: &Rc<Vec<Obj>>, names: &Rc<Vec<String>>) -> bool {
    vals.len() == 1 && (names.is_empty() || names[0].is_empty())
}

// ---------------------------------------------------------------------------
// Spread
// ---------------------------------------------------------------------------

pub fn spread_unop(o: &Obj, op: UnOp, t: &Rc<str>) -> Obj {
    // `Spreads.Spread() => this`
    if op == UnOp::Spread {
        return o.clone();
    }
    obj::err(obj::UnOp::message(&op, t))
}

// ---------------------------------------------------------------------------
// Json
// ---------------------------------------------------------------------------

pub fn json_unop(inner: &Obj, op: UnOp, t: &Rc<str>) -> Obj {
    match op {
        UnOp::Len => obj::int(json_len(inner)),
        _ => obj::err(UnOp::message(&op, t)),
    }
}

pub fn json_len(inner: &Obj) -> i64 {
    match obj::payload(inner) {
        Payload::Dict(km) => km.borrow().len() as i64,
        Payload::List(v) => v.borrow().len() as i64,
        _ => 1,
    }
}

pub fn json_to_str(inner: &Obj) -> String {
    stringify(inner, 0)
}

pub fn json_clone(inner: &Obj) -> Obj {
    json(obj::clone_obj(inner))
}

pub fn json_init(o: &Obj, args: &[Obj]) -> Obj {
    let _ = o;
    match args.len() {
        0 => json(obj::none()),
        1 => match obj::payload(&args[0]) {
            Payload::Str(s) => json_parse(&s),
            Payload::Dict(..) | Payload::List(..) => json(args[0].clone()),
            _ => obj::err(format!(
                "invalid json: expected string, dict, or list, got '{}'",
                obj::type_name(&args[0])
            )),
        },
        n => obj::err(format!("invalid json: expected 1 argument, got {n}")),
    }
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            _ => out.push(c),
        }
    }
    out
}

/// `Json.Stringfy`: dicts are indented 3 spaces per depth, lists stay compact.
pub fn stringify(o: &Obj, depth: usize) -> String {
    if let Payload::Dict(km) = obj::payload(o) {
        let snapshot: Vec<(Obj, Obj)> = km
            .borrow()
            .entries
            .iter()
            .map(|(_, k, v)| (k.clone(), v.clone()))
            .collect();
        let mut buf = String::from("{\n");
        for (k, v) in snapshot {
            buf.push_str(&" ".repeat(3 * depth + 1));
            buf.push('"');
            buf.push_str(&escape(&obj::display(&k)));
            buf.push('"');
            buf.push_str(": ");
            buf.push_str(&stringify(&v, depth + 1));
            buf.push_str(",\n");
        }
        while buf.ends_with(',') || buf.ends_with('\n') {
            buf.pop();
        }
        buf.push('\n');
        buf.push_str(&" ".repeat(3 * depth));
        buf.push('}');
        return buf;
    }

    match obj::payload(o) {
        Payload::Plain if obj::is_none(o) => "null".to_owned(),
        Payload::Json(inner) => stringify(&inner, depth),
        Payload::Int(v) => v.to_string(),
        Payload::Float(v) => prims::fmt_double(v),
        Payload::Bool(v) => if v { "true".to_owned() } else { "false".to_owned() },
        Payload::List(v) => {
            let parts: Vec<String> = v
                .borrow()
                .iter()
                .map(|i| stringify(i, depth + 1))
                .collect();
            format!("[{}]", parts.join(", "))
        }
        _ => format!("\"{}\"", escape(&obj::display(o))),
    }
}

// ---------------------------------------------------------------------------
// JSON parsing (`json(str)`)
// ---------------------------------------------------------------------------

/// `Json.ToJson` runs `JsonDocument.Parse`, so a malformed document throws inside the
/// host. the inner `System.Text.Json.JsonException` name is kept; the reference message
/// text (which encodes byte offsets) is not reproduced, recorded as a divergence.
/// `JsonDocument.Parse` + `Convert`: the raw document value, without the `Json`
/// wrapper. `Json.Init` adds the wrapper, so its display is `Stringfy`.
pub fn parse_json_value(text: &str) -> Obj {
    let mut p = JsonParser { s: text, i: 0 };
    p.skip_ws();
    let value = match p.value() {
        Ok(v) => v,
        Err(e) => return e,
    };
    p.skip_ws();
    if p.i < text.len() {
        return json_exception("extra input after the JSON value");
    }
    value
}

/// `Json.Init` on a string is `new Json(ToJson(text))`, so the parsed document is
/// always wrapped and its display is `Stringfy`, not the inner object's `ToStr`.
/// a parse failure is a host exception, and the wrapper is not added to it.
pub fn json_parse(text: &str) -> Obj {
    let value = parse_json_value(text);
    if obj::is_panic(&value) {
        return value;
    }
    json(value)
}

fn json_exception(message: &str) -> Obj {
    obj::panic(
        "System.Text.Json.JsonException",
        format!("The input does not contain any JSON tokens. {message}"),
    )
}

struct JsonParser<'a> {
    s: &'a str,
    i: usize,
}

impl JsonParser<'_> {
    fn byte(&self) -> Option<u8> {
        self.s.as_bytes().get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        loop {
            match self.byte() {
                Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => self.i += 1,
                Some(b'/') => {
                    if !self.consume_comment() {
                        return;
                    }
                }
                _ => return,
            }
        }
    }

    /// `JsonDocument.Parse` skips comments; `//` and `/* */` are handled here.
    fn consume_comment(&mut self) -> bool {
        let bytes = self.s.as_bytes();
        if bytes.get(self.i + 1) == Some(&b'/') {
            self.i += 2;
            while self.i < bytes.len() && bytes[self.i] != b'\n' {
                self.i += 1;
            }
            return true;
        }
        if bytes.get(self.i + 1) == Some(&b'*') {
            self.i += 2;
            while self.i + 1 < bytes.len() {
                if bytes[self.i] == b'*' && bytes[self.i + 1] == b'/' {
                    self.i += 2;
                    return true;
                }
                self.i += 1;
            }
            self.i = bytes.len();
            return true;
        }
        false
    }

    fn value(&mut self) -> Result<Obj, Obj> {
        match self.byte() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(|s| obj::str(&s)),
            Some(b't') => self.word("true", obj::boolean(true)),
            Some(b'f') => self.word("false", obj::boolean(false)),
            Some(b'n') => self.word("null", obj::none()),
            Some(b) if b == b'-' || b.is_ascii_digit() => self.number(),
            _ => Err(json_exception("expected a JSON value")),
        }
    }

    fn word(&mut self, word: &str, value: Obj) -> Result<Obj, Obj> {
        if self.s[self.i..].starts_with(word) {
            self.i += word.len();
            return Ok(value);
        }
        Err(json_exception("expected a JSON value"))
    }

    fn object(&mut self) -> Result<Obj, Obj> {
        self.i += 1;
        let mut entries: Vec<(Obj, Obj)> = Vec::new();
        self.skip_ws();
        if self.byte() == Some(b'}') {
            self.i += 1;
            return Ok(dict(entries));
        }
        loop {
            self.skip_ws();
            if self.byte() != Some(b'"') {
                return Err(json_exception("expected a property name"));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.byte() != Some(b':') {
                return Err(json_exception("expected ':'"));
            }
            self.i += 1;
            self.skip_ws();
            let value = self.value()?;
            entries.push((obj::str(&key), value));
            self.skip_ws();
            match self.byte() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(dict(entries));
                }
                _ => return Err(json_exception("expected ',' or '}'")),
            }
        }
    }

    fn array(&mut self) -> Result<Obj, Obj> {
        self.i += 1;
        let mut items: Vec<Obj> = Vec::new();
        self.skip_ws();
        if self.byte() == Some(b']') {
            self.i += 1;
            return Ok(list(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value()?);
            self.skip_ws();
            match self.byte() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(list(items));
                }
                _ => return Err(json_exception("expected ',' or ']'")),
            }
        }
    }

    fn string(&mut self) -> Result<String, Obj> {
        self.i += 1;
        let mut out = String::new();
        loop {
            match self.byte() {
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                None => return Err(json_exception("unterminated JSON string")),
                Some(b'\\') => {
                    self.i += 1;
                    match self.byte() {
                        Some(b'"') => out.push('"'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'/') => out.push('/'),
                        Some(b'b') => out.push('\u{8}'),
                        Some(b'f') => out.push('\u{c}'),
                        Some(b'n') => out.push('\n'),
                        Some(b'r') => out.push('\r'),
                        Some(b't') => out.push('\t'),
                        Some(b'u') => {
                            let hex = self.s.get(self.i + 1..self.i + 5);
                            let code = hex
                                .and_then(|h| u32::from_str_radix(h, 16).ok())
                                .ok_or_else(|| json_exception("invalid unicode escape"))?;
                            // `self.i` is on the `u`, so advance to the last hex digit and
                            // let the shared `self.i += 1` below land on the next character
                            self.i += 4;
                            out.push(
                                char::from_u32(code)
                                    .unwrap_or('\u{0}'),
                            );
                        }
                        _ => return Err(json_exception("invalid escape character")),
                    }
                    self.i += 1;
                }
                Some(b) if b < 0x20 => {
                    return Err(json_exception("invalid control character in a JSON string"))
                }
                Some(_) => {
                    let len = self.s[self.i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                    out.extend(self.s[self.i..self.i + len].chars());
                    self.i += len;
                }
            }
        }
    }

    fn number(&mut self) -> Result<Obj, Obj> {
        let bytes = self.s.as_bytes();
        let start = self.i;
        while self.i < bytes.len() {
            let b = bytes[self.i];
            if b.is_ascii_digit()
                || b == b'-'
                || b == b'+'
                || b == b'.'
                || b == b'e'
                || b == b'E'
            {
                self.i += 1;
            } else {
                break;
            }
        }
        let token = &self.s[start..self.i];
        // `TryGetInt64` first: a token without a '.' is an int when it fits.
        if !token.contains('.') {
            if let Ok(v) = token.parse::<i64>() {
                return Ok(obj::int(v));
            }
        }
        match token.parse::<f64>() {
            Ok(v) => {
                // `TryGetInt64` accepts exponent notation, so `1e2` is int 100.
                if !token.contains('.') && v.fract() == 0.0 && v.abs() < 9.2233720368547758e18 {
                    return Ok(obj::int(v as i64));
                }
                Ok(obj::float(v))
            }
            Err(_) => Err(json_exception("invalid number")),
        }
    }
}

// ---------------------------------------------------------------------------
// Stru
// ---------------------------------------------------------------------------

pub fn stru_names(o: &Obj) -> Vec<String> {
    match obj::payload(o) {
        Payload::Stru(names) => names.to_vec(),
        _ => Vec::new(),
    }
}

/// `Stru.GetAllNames`: base-stru names first (recursive), then the instance's own names.
pub fn all_names(o: &Obj) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut seen: Vec<Obj> = Vec::new();
    collect(o, &mut result, &mut seen);
    result.extend(stru_names(o));
    result
}

fn collect(cur: &Obj, result: &mut Vec<String>, seen: &mut Vec<Obj>) {
    for base in obj::base_chain(cur) {
        if !matches!(obj::payload(&base), Payload::Stru(_)) {
            continue;
        }
        if seen.iter().any(|s| Rc::ptr_eq(s, &base)) {
            continue;
        }
        seen.push(base.clone());
        collect(&base, result, seen);
        result.extend(stru_names(&base));
    }
}

pub fn stru_len(o: &Obj) -> i64 {
    all_names(o).len() as i64
}

pub fn stru_unop(o: &Obj, op: UnOp, t: &Rc<str>) -> Obj {
    match op {
        UnOp::Len => obj::int(stru_len(o)),
        UnOp::Spread => {
            let values: Vec<Obj> = all_names(o)
                .iter()
                .map(|name| obj::member(o, name).unwrap_or_else(|| obj::key_not_found(name)))
                .collect();
            obj::spread(&list(values))
        }
        _ => obj::err(UnOp::message(&op, t)),
    }
}

/// `Stru.Init`: property members are written through the property, others directly.
pub fn stru_init(o: &Obj, args: &[Obj]) -> Obj {
    let names = all_names(o);
    if args.len() != names.len() {
        return obj::err("invalid initialize");
    }
    for (i, name) in names.iter().enumerate() {
        if let Some(m) = obj::member(o, name) {
            if obj::is_property(&m) {
                // `property.Write(this, args[i])` — the header initializer stores the
                // backing value directly, it does not go through the setter
                function::prop_write(&m, o, &args[i]);
                continue;
            }
        }
        obj::member_set(o, name, &args[i]);
    }
    o.clone()
}

pub fn stru_to_str(o: &Obj) -> Obj {
    let mut parts: Vec<String> = Vec::new();
    for name in stru_names(o) {
        match obj::member(o, &name) {
            Some(v) => parts.push(format!("{}: {}", name, obj::display(&v))),
            // `Members[name]` on a missing key throws inside the host (D-024)
            None => return obj::key_not_found(&name),
        }
    }
    obj::str(&format!(
        "{}({})",
        obj::type_name(o),
        parts.join(", ")
    ))
}

pub fn stru_binop(o: &Obj, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    match op {
        BinOp::Add => match obj::payload(other) {
            Payload::Str(_) => obj::binop(&obj::to_str(o), BinOp::Add, other),
            _ => obj::base_binop(o, op, other, t, ot),
        },
        BinOp::Eq => stru_eq(o, other),
        _ => obj::base_binop(o, op, other, t, ot),
    }
}

pub fn stru_eq(o: &Obj, other: &Obj) -> Obj {
    if obj::type_name(o) != obj::type_name(other) {
        return obj::boolean(false);
    }
    for name in all_names(o) {
        let mine = match obj::member(o, &name) {
            Some(v) => v,
            None => return obj::key_not_found(&name),
        };
        let theirs = match obj::member(other, &name) {
            Some(v) => v,
            None => return obj::key_not_found(&name),
        };
        let neq = obj::binop(&mine, BinOp::NEq, &theirs);
        match obj::payload(&neq) {
            Payload::Bool(v) => {
                if v {
                    return obj::boolean(false);
                }
            }
            Payload::Error { .. } => return neq,
            _ => return obj::err("equals operand is must be a boolean"),
        }
    }
    obj::boolean(true)
}

/// `Stru.Copy` / `Stru.Clone` build a new `Stru` with the same names and an empty
/// `Members`; `Annotations`, `Super`, `Parents` carry over and `Self` is dropped.
pub fn stru_copy(o: &Obj) -> Obj {
    let (type_name, names, annotations, parents, super_, members) = {
        let d = o.borrow();
        (
            d.type_name.clone(),
            stru_names(o),
            d.annotations.clone(),
            d.parents.clone(),
            d.super_.clone(),
            d.members.clone(),
        )
    };
    let created = obj::obj_new(&type_name, Payload::Stru(Rc::new(names)));
    {
        let mut d = created.borrow_mut();
        d.annotations = annotations;
        d.parents = parents;
        d.super_ = super_;
        // `Stru.Clone` copies the member table
        d.members = members;
    }
    created
}

pub fn stru_clone(o: &Obj) -> Obj {
    stru_copy(o)
}

// ---------------------------------------------------------------------------
// Enu
// ---------------------------------------------------------------------------

pub fn enu_to_str(o: &Obj, n: i64) -> Obj {
    obj::str(&format!("{}:{}", obj::type_name(o), n))
}

pub fn enu_binop(o: &Obj, n: i64, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    match op {
        BinOp::Eq => match obj::payload(other) {
            Payload::Int(i) => obj::boolean(n == i),
            Payload::Enu(e) => obj::boolean(obj::type_name(o) == obj::type_name(other) && n == e),
            _ => obj::base_binop(o, op, other, t, ot),
        },
        BinOp::Lt => match obj::payload(other) {
            Payload::Int(i) => obj::boolean(n < i),
            Payload::Enu(e) => obj::boolean(obj::type_name(o) == obj::type_name(other) && n < e),
            _ => obj::base_binop(o, op, other, t, ot),
        },
        _ => obj::base_binop(o, op, other, t, ot),
    }
}

pub fn enu_unop(n: i64, op: UnOp, t: &Rc<str>) -> Obj {
    match op {
        UnOp::ToInt => obj::int(n),
        _ => obj::err(UnOp::message(&op, t)),
    }
}

pub fn enu_clone(o: &Obj, n: i64) -> Obj {
    enu(&obj::type_name(o), n)
}

pub fn enu_init(o: &Obj, _n: i64, args: &[Obj]) -> Obj {
    let t = obj::type_name(o);
    match args.len() {
        0 => enu(&t, 0),
        1 => match obj::payload(&args[0]) {
            // `Enu.Init` casts the long index through `(int)`
            Payload::Int(i) => enu(&t, i as i32 as i64),
            _ => obj::err(format!("'{t}' takes at most 1 argument, 1 given")),
        },
        count => obj::err(format!("'{t}' takes at most 1 argument, {count} given")),
    }
}

// ---------------------------------------------------------------------------
// TObj
// ---------------------------------------------------------------------------

pub fn tobj_binop(t: &Types, op: BinOp, other: &Obj, tname: &Rc<str>, ot: &Rc<str>) -> Obj {
    match op {
        BinOp::BOr => match obj::payload(other) {
            Payload::TObj(t2) => tobj(Types::create(&[t.clone(), t2])),
            _ => obj::base_binop_of(op, tname, ot),
        },
        BinOp::Eq => obj::boolean(matches!(obj::payload(other), Payload::TObj(t2) if t == &t2)),
        _ => obj::base_binop_of(op, tname, ot),
    }
}

/// `TObj.Is`: `Value == o.Value` for a type, `Matches(obj)` otherwise.
pub fn tobj_is(t: &Types, other: &Obj) -> Obj {
    match obj::payload(other) {
        Payload::TObj(t2) => obj::boolean(t == &t2),
        _ => obj::boolean(obj::types_of(other).contains(t) || *obj::type_name(other) == t.display()),
    }
}

pub fn tobj_in(t: &Types, other: &Obj) -> Obj {
    let target = match obj::payload(other) {
        Payload::TObj(t2) => t2,
        _ => Types::Single(obj::type_name(other)),
    };
    obj::boolean(t == &target || t.contains(&target))
}

pub fn tobj_to_str(t: &Types) -> Obj {
    obj::str(&format!("<type: {}>", t.display()))
}

/// `TObj.Call` = `Global.GetClass(this).Clone().Init(args)`
pub fn tobj_call(t: &Types, args: &[Obj]) -> Obj {
    let name = match t.name() {
        Some(n) => n,
        None => {
            return obj::err(format!("type '{}' is not a class", t.display()));
        }
    };
    match world::get_class(&name) {
        Some(class) => obj::init(&obj::clone_obj(&class), args),
        None => obj::err(format!("class '{name}' not found")),
    }
}

// ---------------------------------------------------------------------------
// `Obj.ToString()`: the CLR form used inside host exception messages (D-024)
// ---------------------------------------------------------------------------

pub fn clr_to_string(o: &Obj) -> String {
    match obj::payload(o) {
        Payload::Str(s) => s.to_string(),
        Payload::Int(v) => v.to_string(),
        Payload::Float(v) => prims::fmt_double(v),
        Payload::Bool(v) => if v { "True".to_owned() } else { "False".to_owned() },
        Payload::Plain if obj::is_none(o) => "None".to_owned(),
        _ => clr_type_name(&obj::type_name(o)),
    }
}

fn clr_type_name(t: &str) -> String {
    match t {
        "obj" | "none" | "null" => "Un.Object.Obj".to_owned(),
        "error" => "Un.Object.Err".to_owned(),
        "int" => "Un.Object.Primitive.Int".to_owned(),
        "float" => "Un.Object.Primitive.Float".to_owned(),
        "bool" => "Un.Object.Primitive.Bool".to_owned(),
        "str" => "Un.Object.Primitive.Str".to_owned(),
        "date" => "Un.Object.Primitive.Date".to_owned(),
        "list" => "Un.Object.Collections.List".to_owned(),
        "tuple" => "Un.Object.Collections.Tup".to_owned(),
        "set" => "Un.Object.Collections.Set".to_owned(),
        "dict" => "Un.Object.Collections.Dict".to_owned(),
        "json" => "Un.Object.Collections.Json".to_owned(),
        "spread" => "Un.Object.Collections.Spreads".to_owned(),
        "func" => "Un.Object.Function.Fn".to_owned(),
        "iter" => "Un.Object.Iter.Iters".to_owned(),
        "type" => "Un.Object.TObj".to_owned(),
        "future" => "Un.Object.Function.Future".to_owned(),
        "stru" => "Un.Stru".to_owned(),
        "module" => "Un.Object.Module".to_owned(),
        _ => format!("Un.{t}"),
    }
}

#[cfg(test)]
mod tests;
