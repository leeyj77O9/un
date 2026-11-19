
use std::sync::{Mutex, MutexGuard};

use crate::runtime::obj::{self, Obj};

fn arena() -> MutexGuard<'static, IntListArena> {
    INT_LIST_ARENA
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

const ID_BASE: i64 = 1i64 << 62;
const KIND_OUTER: u8 = 4;

pub(crate) const fn id_base() -> i64 {
    ID_BASE
}

fn id_of(idx: usize) -> i64 {
    idx as i64 + ID_BASE
}

fn decode(v: i64) -> Option<usize> {
    (v >= ID_BASE).then(|| (v - ID_BASE) as usize)
}

struct IntListIter {
    handle: i64,
    idx: usize,
}

struct IntListIterArena {
    slots: Vec<Option<IntListIter>>,
}

impl IntListIterArena {
    const fn new() -> Self {
        Self { slots: Vec::new() }
    }
}

static INT_LIST_ITER_ARENA: Mutex<IntListIterArena> = Mutex::new(IntListIterArena::new());

fn iter_arena() -> MutexGuard<'static, IntListIterArena> {
    INT_LIST_ITER_ARENA
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn iter_id_of(idx: usize) -> i64 {
    idx as i64 + ID_BASE
}

fn iter_decode(v: i64) -> Option<usize> {
    (v >= ID_BASE).then(|| (v - ID_BASE) as usize)
}

struct IntOuterIter {
    handle: i64,
    idx: usize,
}

struct IntOuterIterArena {
    slots: Vec<Option<IntOuterIter>>,
}

static INT_OUTER_ITER_ARENA: Mutex<IntOuterIterArena> = Mutex::new(IntOuterIterArena {
    slots: Vec::new(),
});

fn outer_iter_arena() -> MutexGuard<'static, IntOuterIterArena> {
    let g = INT_OUTER_ITER_ARENA.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    g
}

fn outer_id_of(idx: usize) -> i64 {
    idx as i64 + ID_BASE + (1i64 << 61)}

fn outer_decode(v: i64) -> Option<usize> {
    let base = ID_BASE + (1i64 << 61);
    (v >= base).then(|| (v - base) as usize)?.checked_sub(1)
}

fn outer_iter_alloc(handle: i64, idx: usize) -> i64 {
    let mut a = outer_iter_arena();
    a.slots.push(Some(IntOuterIter { handle, idx }));
    outer_id_of(a.slots.len())
}

fn outer_iter_has_next(iter: i64) -> i64 {
    let idx = match outer_decode(iter) {
        Some(i) => i,
        None => return 0,
    };
    let cur = {
        let a = outer_iter_arena();
        a.slots.get(idx).and_then(|s| s.as_ref().map(|i| (i.handle, i.idx)))
    };
    match cur {
        Some((handle, idx)) => {
            let len = arena().get(handle).map(|l| l.items.len()).unwrap_or(0);
            i64::from(idx < len)
        }
        None => 0,
    }
}

fn outer_iter_next(iter: i64) -> i64 {
    let slot_idx = match outer_decode(iter) {
        Some(i) => i,
        None => return 0,
    };
    let cur = {
        let a = outer_iter_arena();
        a.slots.get(slot_idx).and_then(|s| s.as_ref().map(|it| (it.handle, it.idx)))
    };
    let Some((handle, it_idx)) = cur else {
        return 0;
    };
    let res = {
        let lists = arena();
        lists.get(handle).and_then(|l| l.items.get(it_idx).copied())
    };
    {
        let mut a = outer_iter_arena();
        if let Some(slot) = a.slots.get_mut(slot_idx).and_then(|s| s.as_mut()) {
            slot.idx = it_idx + 1;
        }
    }
    res.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn un_list_int_iter_new(v: i64) -> i64 {
    match arena().get(v) {
        Some(list) => {
            if list.kind == KIND_OUTER {
                return outer_iter_alloc(v, 0);
            }
        }
        None => return 0,
    }
    let mut a = iter_arena();
    a.slots.push(Some(IntListIter {
        handle: v,
        idx: 0,
    }));
    iter_id_of(a.slots.len())
}

#[no_mangle]
pub extern "C" fn un_list_int_iter_has_next(iter: i64) -> i64 {
    if outer_decode(iter).is_some() {
        return outer_iter_has_next(iter);
    }
    let idx = match iter_decode(iter).and_then(|i| i.checked_sub(1)) {
        Some(i) => i,
        None => return 0,
    };
    let cur = {
        let a = iter_arena();
        a.slots.get(idx).and_then(|s| s.as_ref().map(|i| (i.handle, i.idx)))
    };
    match cur {
        Some((handle, idx)) => {
            let len = arena().get(handle).map(|l| l.items.len()).unwrap_or(0);
            i64::from(idx < len)
        }
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn un_list_int_iter_next(iter: i64) -> i64 {
    if outer_decode(iter).is_some() {
        let h = outer_iter_next(iter);
        return if h >= ID_BASE { h } else { i64::MAX };
    }
    let idx = match iter_decode(iter).and_then(|i| i.checked_sub(1)) {
        Some(i) => i,
        None => return i64::MAX,
    };
    let mut a = iter_arena();
    let Some(slot) = a.slots.get_mut(idx) else {
        return i64::MAX;
    };
    let Some(it) = slot.as_mut() else {
        return i64::MAX;
    };
    let res = {
        let lists = arena();
        lists.get(it.handle).and_then(|l| l.items.get(it.idx).copied())
    };
    match res {
        Some(v) => {
            it.idx += 1;
            v
        }
        None => i64::MAX,
    }
}

#[no_mangle]
pub extern "C" fn un_list_int_iter_free(iter: i64) {
    if let Some(o) = outer_decode(iter) {
        let mut a = outer_iter_arena();
        a.slots.get_mut(o).map(|s| s.take());
        return;
    }
    let Some(idx) = iter_decode(iter).and_then(|i| i.checked_sub(1)) else {
        return;
    };
    let mut a = iter_arena();
    a.slots.get_mut(idx).map(|s| s.take());
}

pub(crate) fn iter_clear() -> usize {
    let mut a = iter_arena();
    let n = a.slots.iter().filter(|s| s.is_some()).count();
    a.slots.clear();
    n
}

struct IntList {
    items: Vec<i64>,
    kind: u8,
    refs: u32,
}

struct IntListArena {
    slots: Vec<Option<IntList>>,
}

impl IntListArena {
    const fn new() -> Self {
        Self { slots: Vec::new() }
    }

    fn alloc(&mut self) -> i64 {
        let idx = self.slots.len() + 1;
        self.slots.push(Some(IntList {
            items: Vec::new(),
            kind: 0,
            refs: 0,
        }));
        id_of(idx)
    }

    fn get(&self, id: i64) -> Option<&IntList> {
        let idx = decode(id)?.checked_sub(1)?;
        self.slots.get(idx)?.as_ref()
    }

    fn get_mut(&mut self, id: i64) -> Option<&mut IntList> {
        let idx = decode(id)?;
        let idx = idx.checked_sub(1)?;
        self.slots.get_mut(idx)?.as_mut()
    }

    fn free(&mut self, id: i64) -> bool {
        let Some(idx) = decode(id) else {
            return false;
        };
        let Some(idx) = idx.checked_sub(1) else {
            return false;
        };
        let Some(list) = self.slots.get(idx).and_then(|s| s.as_ref()) else {
            return false;
        };
        if list.refs > 0 {
            self.slots[idx].as_mut().map(|l| l.refs -= 1);
            return true;
        }
        self.free_force(id)
    }

    fn free_force(&mut self, id: i64) -> bool {
        let Some(idx) = decode(id) else {
            return false;
        };
        let Some(idx) = idx.checked_sub(1) else {
            return false;
        };
        match self.slots[idx].take() {
            Some(l) => {
                if l.kind == KIND_OUTER {
                    for child in l.items {
                        let _ = self.free(child);
                    }
                }
                true
            }
            None => false,
        }
    }

    fn clear(&mut self) {
        self.slots.clear();
    }
}

static INT_LIST_ARENA: Mutex<IntListArena> = Mutex::new(IntListArena::new());

#[no_mangle]
pub extern "C" fn un_list_int_empty() -> i64 {
    arena().alloc()
}

#[no_mangle]
pub extern "C" fn un_list_int_is_handle(v: i64) -> i64 {
    let slots = arena();
    match decode(v).and_then(|i| i.checked_sub(1)) {
        Some(i) => match slots.slots.get(i).and_then(|s| s.as_ref()) {
            Some(list) if list.kind == KIND_OUTER => 2,
            Some(_) => 1,
            None => 0,
        },
        None => 0,
    }
}

fn kind_join(cur: u8, inc: u8) -> u8 {
    match (cur, inc) {
        (0, i) => i,
        (i, j) if i == j => i,
        _ => 3,
    }
}

fn live_handle_idx(slots: &IntListArena, v: i64) -> Option<usize> {
    decode(v).and_then(|i| i.checked_sub(1)).filter(|&i| {
        i < slots.slots.len() && slots.slots[i].is_some()
    })
}

fn push_impl(v: i64, value: i64, is_bool: i64, value_may_be_row: bool) -> i64 {
    let mut slots = arena();
    let Some(v_idx) = live_handle_idx(&slots, v) else {
        return 0;
    };
    let value_is_row = value_may_be_row
        && live_handle_idx(&slots, value).map(|ci| ci != v_idx).unwrap_or(false);
    let v_kind = slots.slots[v_idx].as_ref().map(|l| l.kind).unwrap_or(3);
    if value_is_row {
        match v_kind {
            0 => {
                let list = slots.slots[v_idx].as_mut().unwrap();
                list.kind = KIND_OUTER;
                list.items.push(value);
            }
            KIND_OUTER => {
                slots.slots[v_idx].as_mut().unwrap().items.push(value);
            }
            _ => {
                slots.slots[v_idx].as_mut().unwrap().kind = 3;
                return 0;
            }
        }
        let value_idx = decode(value).and_then(|i| i.checked_sub(1)).unwrap();
        slots.slots[value_idx].as_mut().map(|c| c.refs += 1);
        value
    } else {
        match v_kind {
            KIND_OUTER => {
                slots.slots[v_idx].as_mut().unwrap().kind = 3;
                0
            }
            _ => {
                let list = slots.slots[v_idx].as_mut().unwrap();
                list.items.push(value);
                list.kind = kind_join(list.kind, if is_bool != 0 { 2 } else { 1 });
                value
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn un_list_int_push(v: i64, value: i64, is_bool: i64) -> i64 {
    push_impl(v, value, is_bool, true)
}

#[no_mangle]
pub extern "C" fn un_list_int_push_scalar(v: i64, value: i64, is_bool: i64) -> i64 {
    push_impl(v, value, is_bool, false)
}

#[no_mangle]
pub extern "C" fn un_list_int_slice(v: i64, start: i64, end: i64) -> i64 {
    let mut slots = arena();
    let Some(list) = slots.get(v) else {
        return 0;
    };
    let len = list.items.len() as i64;
    let s = start.clamp(0, len) as usize;
    let e = end.clamp(0, len) as usize;
    let (items, kind) = if s >= e {
        (Vec::new(), list.kind)
    } else {
        (list.items[s..e].to_vec(), list.kind)
    };
    if kind == KIND_OUTER {
        for h in &items {
            if let Some(slot) = slots.get_mut(*h) {
                slot.refs += 1;
            }
        }
    }
    let idx = slots.slots.len() + 1;
    slots.slots.push(Some(IntList { items, kind, refs: 0 }));
    id_of(idx)
}

pub(crate) fn materialize_nested(list: &Obj) -> Option<i64> {
    let crate::runtime::obj::Payload::List(cell) = obj::payload(list) else {
        return None;
    };
    let items = cell.borrow();
    if items.is_empty() {
        return None;
    }
    let mut handles = Vec::with_capacity(items.len());
    for it in items.iter() {
        match obj::payload(it) {
            crate::runtime::obj::Payload::List(_) => {}
            _ => return None,
        }
        handles.push(materialize(it)?);
    }
    let mut slots = arena();
    for h in &handles {
        if let Some(slot) = slots.get_mut(*h) {
            slot.refs += 1;
        }
    }
    let idx = slots.slots.len() + 1;
    slots.slots.push(Some(IntList {
        items: handles,
        kind: KIND_OUTER,
        refs: 0,
    }));
    Some(id_of(idx))
}

pub(crate) fn snapshot(v: i64) -> Option<Vec<Obj>> {
    let slots = arena();
    let list = slots.get(v)?;
    if list.kind == KIND_OUTER {
        let handles = list.items.clone();
        drop(slots);
        let mut out = Vec::with_capacity(handles.len());
        for h in handles {
            let child = snapshot(h)?;
            out.push(crate::runtime::collections::list(child));
        }
        return Some(out);
    }
    let bools = match list.kind {
        0 | 1 => false,
        2 => true,
        _ => return None,    };
    Some(
        list.items
            .iter()
            .map(|&i| {
                if bools {
                    obj::boolean(i != 0)
                } else {
                    crate::runtime::world::int_cached(i)
                }
            })
            .collect(),
    )
}

fn snapshot_list(v: i64) -> Option<Obj> {
    snapshot(v).map(crate::runtime::collections::list)
}

fn write_back(orig: &Obj, handle: i64) -> bool {
    enum Level {
        Flat(bool, Vec<i64>),
        Outer(Vec<i64>),
    }
    let level = {
        let slots = arena();
        match slots.get(handle) {
            Some(l) => match l.kind {
                KIND_OUTER => Level::Outer(l.items.clone()),
                0 => Level::Flat(false, Vec::new()),
                1 => Level::Flat(false, l.items.clone()),
                2 => Level::Flat(true, l.items.clone()),
                _ => return false,            },
            None => return false,
        }
    };
    let crate::runtime::obj::Payload::List(cell) = obj::payload(orig) else {
        return false;
    };
    let mut items = cell.borrow_mut();
    match level {
        Level::Flat(bools, elems) => {
            let new_items: Vec<Obj> = elems
                .into_iter()
                .map(|i| {
                    if bools {
                        obj::boolean(i != 0)
                    } else {
                        crate::runtime::world::int_cached(i)
                    }
                })
                .collect();
            *items = new_items;
            true
        }
        Level::Outer(rows) => {
            let common = items.len().min(rows.len());
            for i in 0..common {
                let row = rows[i];
                if matches!(obj::payload(&items[i]), crate::runtime::obj::Payload::List(_)) {
                    let child = items[i].clone();
                    if write_back(&child, row) {
                        continue;
                    }
                }
                match snapshot_list(row) {
                    Some(list) => items[i] = list,
                    None => return false,                }
            }
            for &row in rows.iter().skip(common) {
                match snapshot_list(row) {
                    Some(list) => items.push(list),
                    None => return false,                }
            }
            items.truncate(rows.len());
            true
        }
    }
}

pub(crate) fn sync_into(orig: &Obj, handle: i64) -> bool {
    if snapshot(handle).is_none() {
        return false;
    }
    write_back(orig, handle)
}

pub(crate) fn materialize(list: &Obj) -> Option<i64> {
    let crate::runtime::obj::Payload::List(cell) = obj::payload(list) else {
        return None;
    };
    let items = cell.borrow();
    let mut elems = Vec::with_capacity(items.len());
    let mut kind = 0u8;
    for it in items.iter() {
        match obj::payload(it) {
            crate::runtime::obj::Payload::Int(n) => {
                elems.push(n);
                kind = kind_join(kind, 1);
            }
            crate::runtime::obj::Payload::Bool(b) => {
                elems.push(i64::from(b));
                kind = kind_join(kind, 2);
            }
            _ => return None,
        }
    }
    let mut slots = arena();
    let idx = slots.slots.len() + 1;
    slots.slots.push(Some(IntList {
        items: elems,
        kind,
        refs: 0,
    }));
    Some(id_of(idx))
}

#[no_mangle]
pub extern "C" fn un_list_int_get(v: i64, idx: i64) -> i64 {
    let slots = arena();
    match slots.get(v) {
        Some(list) if list.kind == KIND_OUTER && idx >= 0 => {
            list.items.get(idx as usize).copied().unwrap_or(0)
        }
        Some(list) if idx >= 0 => list.items.get(idx as usize).copied().unwrap_or(i64::MIN),
        _ => i64::MIN,
    }
}

fn set_impl(v: i64, idx: i64, value: i64, is_bool: i64, value_may_be_row: bool) -> i64 {
    let mut slots = arena();
    if idx < 0 {
        return 1 << 62;
    }
    let Some(v_idx) = decode(v).and_then(|i| i.checked_sub(1)) else {
        return 1 << 62;
    };
    let Some(v_kind) = slots.slots.get(v_idx).and_then(|s| s.as_ref()).map(|l| l.kind) else {
        return 1 << 62;
    };
    if v_kind == KIND_OUTER {
        let value_idx = match value_may_be_row.then(|| live_handle_idx(&slots, value)).flatten() {
            Some(i) if i != v_idx => i,
            _ => {
                slots.slots[v_idx].as_mut().unwrap().kind = 3;
                return 1 << 62;
            }
        };
        let plen = slots.slots[v_idx].as_ref().map(|l| l.items.len()).unwrap_or(0);
        if (idx as usize) >= plen {
            return 1 << 62;
        }
        let old = {
            let parent = slots.slots[v_idx].as_mut().unwrap();
            std::mem::replace(&mut parent.items[idx as usize], value)
        };
        slots.slots[value_idx].as_mut().map(|c| c.refs += 1);
        if let Some(old_idx) = decode(old).and_then(|i| i.checked_sub(1)) {
            if old_idx != v_idx && old_idx < slots.slots.len() {
                slots.slots[old_idx].as_mut().map(|c| c.refs = c.refs.saturating_sub(1));
            }
        }
        return value;
    }
    match slots.get_mut(v) {
        Some(list) => {
            if idx >= 0 && (idx as usize) < list.items.len() {
                list.items[idx as usize] = value;
                list.kind = kind_join(list.kind, if is_bool != 0 { 2 } else { 1 });
                value
            } else {
                1 << 62            }
        }
        None => 1 << 62,
    }
}

#[no_mangle]
pub extern "C" fn un_list_int_set(v: i64, idx: i64, value: i64, is_bool: i64) -> i64 {
    set_impl(v, idx, value, is_bool, true)
}

#[no_mangle]
pub extern "C" fn un_list_int_set_scalar(v: i64, idx: i64, value: i64, is_bool: i64) -> i64 {
    set_impl(v, idx, value, is_bool, false)
}

#[no_mangle]
pub extern "C" fn un_list_int_len(v: i64) -> i64 {
    let slots = arena();
    match slots.get(v) {
        Some(list) => list.items.len() as i64,
        None => -1,
    }
}

#[no_mangle]
pub extern "C" fn un_list_int_free(v: i64) {
    let _ = arena().free(v);
}

pub(crate) fn clear() -> usize {
    let _ = iter_clear();
    let mut arena = arena();
    let n = arena.slots.iter().filter(|s| s.is_some()).count();
    arena.clear();
    n
}
