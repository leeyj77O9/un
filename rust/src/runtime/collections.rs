use std::cell::RefCell;
use std::rc::Rc;
use crate::runtime::function;
use crate::runtime::iters;
use crate::runtime::obj;
use crate::runtime::obj::{BinOp, Obj, Payload, UnOp};
use crate::runtime::prims;
use crate::runtime::types::Types;
use crate::runtime::world;
pub struct KeyMap {
    entries: Vec<Entry>,

    buckets: Vec<usize>,

    next: Vec<usize>,
    free_head: usize,
    live: usize,
}
struct Entry {
    hash: u64,
    key: Obj,
    value: Obj,
    used: bool,
}
const NONE: usize = usize::MAX;
const INITIAL_BUCKETS: usize = 8;
impl KeyMap {
    pub fn new(entries: Vec<(Obj, Obj)>) -> Self {
        let mut map = Self {
            entries: Vec::new(),
            buckets: vec![NONE; INITIAL_BUCKETS],
            next: Vec::new(),
            free_head: NONE,
            live: 0,
        };
        for (k, v) in entries {
            map.set(&k, &v);
        }
        map
    }
    fn bucket(&self, hash: u64) -> usize {
        (hash as usize) & (self.buckets.len() - 1)
    }
    pub fn len(&self) -> usize {
        self.live
    }
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }
    fn find(&self, key: &Obj) -> Option<usize> {
        if self.buckets.is_empty() {
            return None;
        }
        let h = obj::hash_code(key);
        let mut idx = self.buckets[self.bucket(h)];
        let mut found: Option<usize> = None;

        while idx != NONE {
            let e = &self.entries[idx];
            if e.used && e.hash == h && obj::key_eq(&e.key, key) {
                found = Some(match found {
                    Some(prev) => prev.min(idx),
                    None => idx,
                });
            }
            idx = self.next[idx];
        }
        found
    }
    pub fn contains_key(&self, key: &Obj) -> bool {
        self.find(key).is_some()
    }
    pub fn get(&self, key: &Obj) -> Option<Obj> {
        self.find(key).map(|i| self.entries[i].value.clone())
    }

    pub fn get_key(&self, key: &Obj) -> Option<Obj> {
        self.find(key).map(|i| self.entries[i].key.clone())
    }

    pub fn iter_pairs(&self) -> impl Iterator<Item = (&Obj, &Obj)> {
        self.entries
            .iter()
            .filter(|e| e.used)
            .map(|e| (&e.key, &e.value))
    }
    fn link_into_bucket(&mut self, idx: usize) {
        let b = self.bucket(self.entries[idx].hash);
        self.next[idx] = self.buckets[b];
        self.buckets[b] = idx;
    }
    fn unlink_from_bucket(&mut self, idx: usize) {
        let b = self.bucket(self.entries[idx].hash);
        let mut cur = self.buckets[b];
        if cur == idx {
            self.buckets[b] = self.next[idx];
            return;
        }
        while cur != NONE {
            let next = self.next[cur];
            if next == idx {
                self.next[cur] = self.next[idx];
                return;
            }
            cur = next;
        }
    }
    fn insert_new_entry(&mut self, hash: u64, key: Obj, value: Obj) -> usize {
        if self.live >= self.buckets.len() {
            self.grow();
        }
        let idx;
        if self.free_head != NONE {
            idx = self.free_head;
            self.free_head = self.next[idx];
            self.entries[idx] = Entry {
                hash,
                key,
                value,
                used: true,
            };
            self.next[idx] = NONE;
        } else {
            idx = self.entries.len();
            self.entries.push(Entry {
                hash,
                key,
                value,
                used: true,
            });
            self.next.push(NONE);
        }
        self.link_into_bucket(idx);
        self.live += 1;
        idx
    }
    fn grow(&mut self) {

        let mut live_entries: Vec<Entry> = Vec::with_capacity(self.live);
        for e in std::mem::take(&mut self.entries) {
            if e.used {
                live_entries.push(e);
            }
        }
        let new_buckets = vec![NONE; self.buckets.len() * 2];
        self.buckets = new_buckets;
        self.next = vec![NONE; live_entries.len()];
        self.entries = live_entries;
        self.free_head = NONE;
        self.live = self.entries.len();
        for i in 0..self.entries.len() {
            self.link_into_bucket(i);
        }
    }

    pub fn set(&mut self, key: &Obj, value: &Obj) {
        match self.find(key) {
            Some(i) => self.entries[i].value = value.clone(),
            None => {
                self.insert_new_entry(obj::hash_code(key), key.clone(), value.clone());
            }
        }
    }

    pub fn try_insert(&mut self, key: &Obj, value: Obj) -> bool {
        if self.find(key).is_some() {
            return false;
        }
        self.insert_new_entry(obj::hash_code(key), key.clone(), value);
        true
    }

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
        self.insert_new_entry(obj::hash_code(key), key.clone(), value.clone());
        obj::none()
    }
    pub fn remove(&mut self, key: &Obj) -> bool {
        match self.find(key) {
            Some(i) => {
                self.unlink_from_bucket(i);
                self.entries[i].used = false;
                self.next[i] = self.free_head;
                self.free_head = i;
                self.live -= 1;
                true
            }
            None => false,
        }
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.next.clear();
        self.buckets.fill(NONE);
        self.free_head = NONE;
        self.live = 0;
    }
    pub fn keys(&self) -> Vec<Obj> {
        self.iter_pairs().map(|(k, _)| k.clone()).collect()
    }
    pub fn values(&self) -> Vec<Obj> {
        self.iter_pairs().map(|(_, v)| v.clone()).collect()
    }
    pub fn contains_value(&self, value: &Obj) -> bool {
        self.iter_pairs().any(|(_, v)| obj::key_eq(v, value))
    }

    pub fn overlap(&self, other: &KeyMap) -> bool {
        for (k, v) in other.iter_pairs() {
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
pub fn list(values: Vec<Obj>) -> Obj {
    obj::obj_new("list", Payload::List(Rc::new(RefCell::new(values))))
}
pub fn dict(entries: Vec<(Obj, Obj)>) -> Obj {
    obj::obj_new("dict", Payload::Dict(Rc::new(RefCell::new(KeyMap::new(entries)))))
}
pub fn set(items: Vec<Obj>) -> Obj {
    let mut map = KeyMap::new(Vec::new());
    for item in items {
        map.try_insert(&item, obj::none());
    }
    obj::obj_new("set", Payload::Set(Rc::new(RefCell::new(map))))
}
pub fn tup(values: Vec<Obj>, names: Vec<String>) -> Obj {
    let mut names = names;
    while names.len() < values.len() {
        names.push(String::new());
    }
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
fn tup_of(o: &Obj) -> (Vec<Obj>, Vec<String>) {
    match obj::payload(o) {
        Payload::Tup(v, n) => (v.to_vec(), n.to_vec()),
        _ => (Vec::new(), Vec::new()),
    }
}
fn out_of_range(index: i64, count: i64) -> bool {
    index < 0 || index >= count
}
fn overlap(needle: &[Obj], haystack: &[Obj]) -> bool {
    needle
        .iter()
        .all(|n| haystack.iter().any(|h| obj::key_eq(h, n)))
}
pub fn list_get_item(v: &Rc<RefCell<Vec<Obj>>>, key: &Obj) -> Obj {
    match obj::payload(key) {
        Payload::Int(i) => {
            let count = v.borrow().len() as i64;

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

    data.push(obj::none());
    for i in (idx..count).rev() {
        let moved = data[i as usize].clone();
        data[i as usize + 1] = moved;
    }
    data[idx as usize] = obj::copy_obj(value);
    Ok(())
}
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

            if i >= len {
                return index_out_of_range();
            }
            i += st;
        }
    }
    list(Vec::new())
}
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
        .iter_pairs()
        .map(|(k, v)| format!("{}:{}", obj::display(k), obj::display(v)))
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
        .iter_pairs()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    match op {
        UnOp::Len => obj::int(snapshot.len() as i64),
        UnOp::ToList => list(snapshot.iter().map(|(k, v)| pair(k, v)).collect()),
        UnOp::ToTuple => tup(snapshot.iter().map(|(k, v)| pair(k, v)).collect(), Vec::new()),
        UnOp::Spread => spread(snapshot.iter().map(|(k, v)| pair(k, v)).collect()),

        UnOp::Iter => iters::seq(snapshot.iter().map(|(k, v)| pair(k, v)).collect()),
        _ => obj::err(UnOp::message(&op, t)),
    }
}
pub fn dict_clone(km: &Rc<RefCell<KeyMap>>) -> Obj {
    obj::obj_new(
        "dict",
        Payload::Dict(Rc::new(RefCell::new(KeyMap::new(
            km.borrow().iter_pairs().map(|(k, v)| (k.clone(), v.clone())).collect(),
        )))),
    )
}
pub fn dict_init(o: &Obj, args: &[Obj]) -> Obj {
    match args.len() {
        0 => dict(Vec::new()),
        1 => {

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
pub fn set_get_item(s: &Rc<RefCell<KeyMap>>, key: &Obj) -> Obj {
    match s.borrow().get_key(key) {
        Some(item) => item,
        None => obj::err(format!("key {} not found in set", obj::display(key))),
    }
}
pub fn set_binop(o: &Obj, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    let mine = match obj::payload(o) {
        Payload::Set(s) => s.clone(),
        _ => unreachable!("set_binop receiver is a set"),
    };
    match op {
        BinOp::Add => match obj::payload(other) {
            Payload::Set(s2) => set(union(&mine.borrow(), &s2.borrow())),
            _ => obj::err(format!(
                "unsupported operand type(s) for +: 'set' and '{ot}'"
            )),
        },
        BinOp::Sub => match obj::payload(other) {
            Payload::Set(s2) => set(except(&mine.borrow(), &s2.borrow())),
            _ => obj::err(format!(
                "unsupported operand type(s) for -: 'set' and '{ot}'"
            )),
        },
        BinOp::BXor => match obj::payload(other) {
            Payload::Set(s2) => set(intersect(&mine.borrow(), &s2.borrow())),
            _ => obj::err(format!(
                "unsupported operand type(s) for ^: 'set' and '{ot}'"
            )),
        },
        _ => obj::base_binop(o, op, other, t, ot),
    }
}
fn union(a: &KeyMap, b: &KeyMap) -> Vec<Obj> {
    let mut map = KeyMap::new(Vec::new());
    for k in a.keys() {
        map.try_insert(&k, obj::none());
    }
    for k in b.keys() {
        map.try_insert(&k, obj::none());
    }
    map.keys()
}
fn except(a: &KeyMap, b: &KeyMap) -> Vec<Obj> {
    a.keys().into_iter().filter(|k| !b.contains_key(k)).collect()
}
fn intersect(a: &KeyMap, b: &KeyMap) -> Vec<Obj> {

    a.keys().into_iter().filter(|k| b.contains_key(k)).collect()
}
pub fn set_to_str(s: &Rc<RefCell<KeyMap>>) -> String {
    let parts: Vec<String> = s.borrow().keys().iter().map(|i| obj::display(i)).collect();
    format!("{{{}}}", parts.join(", "))
}
pub fn set_unop(s: &Rc<RefCell<KeyMap>>, op: UnOp, t: &Rc<str>) -> Obj {

    match op {
        UnOp::Len => obj::int(s.borrow().len() as i64),
        UnOp::Spread => spread(s.borrow().keys()),
        _ => obj::err(UnOp::message(&op, t)),
    }
}
pub fn set_init(o: &Obj, args: &[Obj]) -> Obj {

    let _ = o;
    set(args.to_vec())
}
pub fn set_clone(s: &Rc<RefCell<KeyMap>>) -> Obj {
    set(s.borrow().keys())
}
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
fn index_out_of_range() -> Obj {
    obj::panic(
        "System.IndexOutOfRangeException",
        "Index was outside the bounds of the array.",
    )
}
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

        UnOp::Iter => iters::seq(vals.to_vec()),
        _ => obj::err(UnOp::message(&op, t)),
    }
}
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
pub fn spread_unop(o: &Obj, op: UnOp, t: &Rc<str>) -> Obj {

    if op == UnOp::Spread {
        return o.clone();
    }
    obj::err(obj::UnOp::message(&op, t))
}
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
pub fn stringify(o: &Obj, depth: usize) -> String {
    if let Payload::Dict(km) = obj::payload(o) {
        let snapshot: Vec<(Obj, Obj)> = km
            .borrow()
            .iter_pairs()
            .map(|(k, v)| (k.clone(), v.clone()))
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

        if !token.contains('.') {
            if let Ok(v) = token.parse::<i64>() {
                return Ok(obj::int(v));
            }
        }
        match token.parse::<f64>() {
            Ok(v) => {

                if !token.contains('.') && v.fract() == 0.0 && v.abs() < 9.2233720368547758e18 {
                    return Ok(obj::int(v as i64));
                }
                Ok(obj::float(v))
            }
            Err(_) => Err(json_exception("invalid number")),
        }
    }
}
pub fn stru_names(o: &Obj) -> Vec<String> {
    match obj::payload(o) {
        Payload::Stru(names) => names.to_vec(),
        _ => Vec::new(),
    }
}
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
pub fn stru_init(o: &Obj, args: &[Obj]) -> Obj {
    let names = all_names(o);
    if args.len() != names.len() {
        return obj::err("invalid initialize");
    }
    for (i, name) in names.iter().enumerate() {
        if let Some(m) = obj::member(o, name) {
            if obj::is_property(&m) {

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

        d.members = members;
    }
    created
}
pub fn stru_clone(o: &Obj) -> Obj {
    stru_copy(o)
}
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

            Payload::Int(i) => enu(&t, i as i32 as i64),
            _ => obj::err(format!("'{t}' takes at most 1 argument, 1 given")),
        },
        count => obj::err(format!("'{t}' takes at most 1 argument, {count} given")),
    }
}
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
