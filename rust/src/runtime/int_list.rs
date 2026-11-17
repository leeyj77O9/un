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
//! Nested (homogeneous `List[List[Int|Bool]]`) lists enter as `kind == 4`
//! (`KIND_OUTER`): `items` holds child IntVec handles instead of values.
//! Reading `parent[i][j]` lowers to `un_list_int_get(parent, i)` (returns
//! the child handle) followed by `un_list_int_get(child, j)` ??the same
//! intrinsic, polymorphic on the slot kind. Pushing a child handle into an
//! empty flat slot promotes the parent to `KIND_OUTER` and consumes the
//! child (its slot is tombstoned). Snapshotting rebuilds the whole nested
//! tree and `un_list_int_free` drops it recursively.
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
const KIND_OUTER: u8 = 4;

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
/// 0 means "invalid" (dead list). `KIND_OUTER` lists are not iterable here;
/// the JIT bails and the interpreter rebuilds the nested loop instead.
#[no_mangle]
pub extern "C" fn un_list_int_iter_new(v: i64) -> i64 {
    match arena().get(v) {
        Some(list) if list.kind == KIND_OUTER => return 0,
        Some(_) => {}
        None => return 0,
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
    // Number of outer handles that own this slot (one per `out->add(row)`
    // or `materialize_nested` reference). Bulk frees from the JIT bail
    // path must not kill a row its parent still owns; dropped children are
    // only released once the count reaches zero (or when the parent drops).
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
            // Owned by at least one `KIND_OUTER` list; release the parent's
            // reference and keep the slot live for the other owners.
            self.slots[idx].as_mut().map(|l| l.refs -= 1);
            return true;
        }
        self.free_force(id)
    }

    /// Drop the slot unconditionally, recurring into outer children. Used
    /// when the caller (bulk teardown) knows no one else references it.
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
                        // Parent owns the children; force-drop them so a
                        // ref-count anchored by the parent doesn't leak.
                        let _ = self.free_force(child);
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

/// Allocate a fresh empty IntVec; returns its canonical handle (??<<62;
/// 0 means "invalid"). Called via inline JIT code.
#[no_mangle]
pub extern "C" fn un_list_int_empty() -> i64 {
    arena().alloc()
}

/// Is `v` a live IntVec handle? 1 for a flat Int/Bool list (including a
/// row of a nested list), 2 for a `KIND_OUTER` list of lists, 0 when the
/// value can never be a handle. The JIT checks this before any
/// specialisation so an arbitrary Int never aliases a list.
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

/// Kind transition for an element of kind `inc` (1 = Int, 2 = Bool).
fn kind_join(cur: u8, inc: u8) -> u8 {
    match (cur, inc) {
        (0, i) => i,
        (i, j) if i == j => i,
        _ => 3,
    }
}

/// Is `value` a live handle in this arena? Used to decide outer-row push.
fn live_handle_idx(slots: &IntListArena, v: i64) -> Option<usize> {
    decode(v).and_then(|i| i.checked_sub(1)).filter(|&i| {
        i < slots.slots.len() && slots.slots[i].is_some()
    })
}

/// Push `value` onto the back of the IntVec, returning the value itself.
/// A `KIND_OUTER` receiver pushes a row handle: if the receiver is a virgin
/// (empty, kind == 0) flat slot, it is promoted to `KIND_OUTER` on the
/// first row push. A row push into an already scalar-populated flat slot,
/// or a scalar push into an outer list, poisons the receiver so the whole
/// call falls back to the interpreter. `is_bool` is nonzero when the
/// payload is a Bool (0/1), not an Int; ignored for outer pushes.
#[no_mangle]
pub extern "C" fn un_list_int_push(v: i64, value: i64, is_bool: i64) -> i64 {
    let mut slots = arena();
    let Some(v_idx) = live_handle_idx(&slots, v) else {
        return 0;
    };
    // Is `value` a live handle (a row)? Only if it doesn't alias `v`.
    let value_is_row = live_handle_idx(&slots, value).map(|ci| ci != v_idx).unwrap_or(false);
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
                // Scalar-populated flat list: a row can't go in. Poison.
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
                // A scalar can't go into an outer list. Poison.
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

/// Copy `items[start..end]` into a fresh list; returns its handle (0 on
/// failure). Window clamped to the list bounds. A `KIND_OUTER` receiver
/// returns a fresh outer list pointing at the same rows (subclass may alias
/// rows), with refs bumped accordingly.
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

/// Snapshot an all-Int (or all-Bool) IntVec as owned Un values. Returns
/// `None` for a poisoned (mixed) list or an invalid handle; the caller then
/// falls back to the interpreter.
/// Materialise an `Obj::List` whose every element is a homogeneous Int/Bool
/// list of any length into a fresh `KIND_OUTER` handle. Lists of scalars
/// (the `materialize` shape) decline with `None`; anything else declines
/// too, so a misshapen outer argument falls back to the interpreter. Public
/// to the FFI layer (`jit_fast`); used when the interpreter passes a nested
/// homogeneous list into a JIT-eligible function.
pub(crate) fn materialize_nested(list: &Obj) -> Option<i64> {
    let crate::runtime::obj::Payload::List(cell) = obj::payload(list) else {
        return None;
    };
    let items = cell.borrow();
    if items.is_empty() {
        // An empty outer list would silently pass all the per-row checks
        // below; decline and let the scalar path handle it.
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
        refs: 0,
    }));
    Some(id_of(idx))
}

/// Read `items[idx]` (0-indexed; out-of-range returns i64::MIN sentinel).
/// A `KIND_OUTER` slot indexes into its row handles instead of values.
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

/// Write `items[idx] = value` (0-indexed; out-of-range is a no-op returning
/// the "unwritten" sentinel). A `KIND_OUTER` receiver swaps row handles,
/// releasing the old row reference and taking one on the new one.
#[no_mangle]
pub extern "C" fn un_list_int_set(v: i64, idx: i64, value: i64, is_bool: i64) -> i64 {
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
        let value_idx = match live_handle_idx(&slots, value) {
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
                slots.slots[old_idx].as_mut().map(|c| c.refs -= 1);
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
