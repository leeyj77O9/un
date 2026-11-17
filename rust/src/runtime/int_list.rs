//! Shared native support for Int-only list intrinsics used by the JIT.
//! The JIT-emitted code passes a raw i64 handle between calls; the arena
//! lives independently of the Un object model so there's no need to
//! allocate Obj wrappers for transient lists.
//!
//! Handle namespace: valid handles are `idx + ID_BASE` where `idx` is a
//! monotonically increasing arena slot (freed slots are never reused, so
//! ids are unique for the process). Values below `ID_BASE` can never be
//! live handles, so a bare Int is never misread as a list reference.
//! `0` (as an idx or as an id) always means "invalid".
//!
//! Uniform-kind tracking: 0 = empty (nothing pushed yet), 1 = all Int,
//! 2 = all Bool, 3 = poisoned (mixed). Materialisation at the FFI boundary
//! inspects the kind; a poisoned list bails to the interpreter.
//!
//! This module is **only** used by JIT-emitted code; the interpreter does
//! not use it.

use std::sync::{Mutex, MutexGuard};

use crate::runtime::obj::{self, Obj};

/// The arena is shared with JIT-native code, which must never unwind across
/// the FFI boundary. A poisoned lock therefore cannot happen in practice,
/// but if it ever does, taking the guard via `into_inner` keeps the JIT
/// callable instead of poisoning every subsequent list operation for the
/// process.
fn arena() -> MutexGuard<'static, IntListArena> {
    INT_LIST_ARENA
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

const ID_BASE: i64 = 1i64 << 62;

fn id_of(idx: usize) -> i64 {
    idx as i64 + ID_BASE
}

fn decode(v: i64) -> Option<usize> {
    (v >= ID_BASE).then(|| (v - ID_BASE) as usize)
}

/// Iterator arena: independent of the list arena so iterator ids never
/// alias list ids. Handles are `idx + ID_BASE` like lists. The iterator
/// borrows the list handle and an element index; no Un object is involved.
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

/// Allocate a new forward iterator over the IntVec `v`; returns its handle.
/// 0 means "invalid" (dead list).
#[no_mangle]
pub extern "C" fn un_list_int_iter_new(v: i64) -> i64 {
    if arena().get(v).is_none() {
        return 0;
    }
    let mut a = iter_arena();
    a.slots.push(Some(IntListIter {
        handle: v,
        idx: 0,
    }));
    iter_id_of(a.slots.len())
}

/// 1 while the iterator has more elements, 0 when exhausted (or dead).
#[no_mangle]
pub extern "C" fn un_list_int_iter_has_next(iter: i64) -> i64 {
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

/// Read and advance one element; returns i64::MAX when exhausted (callers
/// never read past their own `has_next` guard, so this is only a safeguard).
#[no_mangle]
pub extern "C" fn un_list_int_iter_next(iter: i64) -> i64 {
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

/// Free the IntVec iterator behind `iter`.
#[no_mangle]
pub extern "C" fn un_list_int_iter_free(iter: i64) {
    let Some(idx) = iter_decode(iter).and_then(|i| i.checked_sub(1)) else {
        return;
    };
    let mut a = iter_arena();
    a.slots.get_mut(idx).map(|s| s.take());
}

/// Drop every IntVec iterator in the arena. Intended for whole-process
/// teardown; the lists themselves are flushed by `un_list_int_free`'s
/// caller.
pub(crate) fn iter_clear() -> usize {
    let mut a = iter_arena();
    let n = a.slots.iter().filter(|s| s.is_some()).count();
    a.slots.clear();
    n
}

struct IntList {
    items: Vec<i64>,
    kind: u8,
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
        if idx < self.slots.len() {
            self.slots[idx].take().is_some()
        } else {
            false
        }
    }

    fn clear(&mut self) {
        self.slots.clear();
    }
}

static INT_LIST_ARENA: Mutex<IntListArena> = Mutex::new(IntListArena::new());

/// Allocate a fresh empty IntVec; returns its canonical handle (≥1<<62;
/// 0 means "invalid"). Called via inline JIT code.
#[no_mangle]
pub extern "C" fn un_list_int_empty() -> i64 {
    arena().alloc()
}

/// Is `v` a live IntVec handle? 1 when yes, 0 when no. The JIT checks this
/// before any specialisation so an arbitrary Int never aliases a list.
#[no_mangle]
pub extern "C" fn un_list_int_is_handle(v: i64) -> i64 {
    let slots = arena();
    match decode(v).and_then(|i| i.checked_sub(1)) {
        Some(i) => i64::from(i < slots.slots.len() && slots.slots[i].is_some()),
        None => 0,
    }
}

/// Kind transition for an element of kind `inc` (1 = Int, 2 = Bool).
fn kind_join(cur: u8, inc: u8) -> u8 {
    match (cur, inc) {
        (0, i) => i,
        (i, j) if i == j => i,
        _ => 3,
    }
}

/// Push `value` onto the back of the IntVec, returning the value itself.
/// `is_bool` is nonzero when the payload is a Bool (0/1), not an Int.
#[no_mangle]
pub extern "C" fn un_list_int_push(v: i64, value: i64, is_bool: i64) -> i64 {
    let mut slots = arena();
    match slots.get_mut(v) {
        Some(list) => {
            list.items.push(value);
            list.kind = kind_join(list.kind, if is_bool != 0 { 2 } else { 1 });
            value
        }
        None => 0,
    }
}

/// Copy `items[start..end]` into a fresh list; returns its handle (0 on
/// failure). Window clamped to the list bounds.
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
    let idx = slots.slots.len() + 1;
    slots.slots.push(Some(IntList { items, kind }));
    id_of(idx)
}

/// Snapshot an all-Int (or all-Bool) IntVec as owned Un values. Returns
/// `None` for a poisoned (mixed) list or an invalid handle; the caller then
/// falls back to the interpreter.
pub(crate) fn snapshot(v: i64) -> Option<Vec<Obj>> {
    let slots = arena();
    let list = slots.get(v)?;
    let bools = match list.kind {
        0 | 1 => false,
        2 => true,
        _ => return None, // poisoned
    };
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

/// Materialise an `Obj::List` of all-Int/all-Bool values into a fresh
/// IntVec handle; mixed elements decline with `None`. Public to the FFI
/// layer (`jit_fast`); used when the interpreter passes a list into a
/// JIT-eligible function.
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
    }));
    Some(id_of(idx))
}

/// Read `items[idx]` (0-indexed; out-of-range returns i64::MIN sentinel).
#[no_mangle]
pub extern "C" fn un_list_int_get(v: i64, idx: i64) -> i64 {
    let slots = arena();
    match slots.get(v) {
        Some(list) if idx >= 0 => list.items.get(idx as usize).copied().unwrap_or(i64::MIN),
        _ => i64::MIN,
    }
}

/// Write `items[idx] = value` (0-indexed; out-of-range is a no-op returning
/// the "unwritten" sentinel).
#[no_mangle]
pub extern "C" fn un_list_int_set(v: i64, idx: i64, value: i64, is_bool: i64) -> i64 {
    let mut slots = arena();
    match slots.get_mut(v) {
        Some(list) => {
            if idx >= 0 && (idx as usize) < list.items.len() {
                list.items[idx as usize] = value;
                list.kind = kind_join(list.kind, if is_bool != 0 { 2 } else { 1 });
                value
            } else {
                1 << 62 // "unwritten" sentinel
            }
        }
        None => 1 << 62,
    }
}

/// Return the length of the IntVec; `-1` when `v` is not a live handle (the
/// caller emits a bail instead of proceeding blindly).
#[no_mangle]
pub extern "C" fn un_list_int_len(v: i64) -> i64 {
    let slots = arena();
    match slots.get(v) {
        Some(list) => list.items.len() as i64,
        None => -1,
    }
}

/// Free the IntVec behind `v` (fell out of scope or naturally dead at an
/// emit point). Tombstone ids stay valid-but-dead; later reads behave like
/// empty lists and later `snapshot` fails so the interpreter rebuilds.
#[no_mangle]
pub extern "C" fn un_list_int_free(v: i64) {
    let _ = arena().free(v);
}

/// Drop every IntVec in the arena. Intended for whole-process teardown next
/// to `jit_cache::clear()`; afterwards any in-flight JIT code is out of
/// contract. Returns the number of tables dropped.
pub(crate) fn clear() -> usize {
    let _ = iter_clear();
    let mut arena = arena();
    let n = arena.slots.iter().filter(|s| s.is_some()).count();
    arena.clear();
    n
}
