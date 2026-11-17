//! Shared native support for Int-only list intrinsics used by the JIT.
//! The JIT-emitted code passes a raw i64 "list id" between calls; the arena
//! lives independently of the Un object model so there's no need to
//! allocate Obj wrappers for transient tables.
//!
//! This module is **only** used by JIT-emitted code; the interpreter does
//! not use it.

use std::sync::{Mutex, MutexGuard};

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

struct IntList {
    items: Vec<i64>,
}

struct IntListArena {
    slots: Vec<Option<IntList>>,
}

impl IntListArena {
    const fn new() -> Self {
        Self { slots: Vec::new() }
    }

    fn alloc(&mut self) -> i64 {
        let idx = self.slots.len() as i64 + 1;
        self.slots.push(Some(IntList { items: Vec::new() }));
        idx
    }

    fn get_mut(&mut self, id: i64) -> Option<&mut IntList> {
        let idx = id as usize;
        if idx == 0 {
            return None;
        }
        let idx = idx - 1;
        if idx < self.slots.len() {
            self.slots[idx].as_mut()
        } else {
            None
        }
    }

    fn get(&self, id: i64) -> Option<&IntList> {
        let idx = id as usize;
        if idx == 0 {
            return None;
        }
        let idx = idx - 1;
        if idx < self.slots.len() {
            self.slots[idx].as_ref()
        } else {
            None
        }
    }

    fn free(&mut self, id: i64) -> bool {
        let idx = id as usize;
        if idx == 0 {
            return false;
        }
        let idx = idx - 1;
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

/// Allocate a fresh empty IntVec; returns its canonical i64 id (always ≥1;
/// 0 means "invalid"). Called via inline JIT code.
#[no_mangle]
pub extern "C" fn un_list_int_empty() -> i64 {
    arena().alloc()
}

/// Push `value` onto the back of the IntVec, returning the inserted value.
/// JIT callsite arguments must be raw i64.
#[no_mangle]
pub extern "C" fn un_list_int_push(v: i64, value: i64) -> i64 {
    let mut slots = arena();
    if let Some(list) = slots.get_mut(v) {
        list.items.push(value);
        value
    } else {
        0
    }
}

/// Read `items[idx]` (0-indexed; out-of-range returns i64::MIN sentinel).
/// JIT callsite arguments must be raw i64.
#[no_mangle]
pub extern "C" fn un_list_int_get(v: i64, idx: i64) -> i64 {
    let slots = arena();
    if let Some(list) = slots.get(v) {
        if idx >= 0 && (idx as usize) < list.items.len() {
            list.items[idx as usize]
        } else {
            i64::MIN
        }
    } else {
        i64::MIN
    }
}

/// Write `items[idx] = value` (0-indexed; out-of-range is a no-op).
#[no_mangle]
pub extern "C" fn un_list_int_set(v: i64, idx: i64, value: i64) -> i64 {
    let mut slots = arena();
    if let Some(list) = slots.get_mut(v) {
        if idx >= 0 && (idx as usize) < list.items.len() {
            list.items[idx as usize] = value;
            value
        } else {
            1 << 62 // "unwritten" sentinel
        }
    } else {
        1 << 62
    }
}

/// Return the length of the IntVec (0 for invalid ids).
#[no_mangle]
pub extern "C" fn un_list_int_len(v: i64) -> i64 {
    let slots = arena();
    slots
        .get(v)
        .map(|l| l.items.len() as i64)
        .unwrap_or(0)
}

/// Free the IntVec behind `id`, dropping its table. Ids freed this way become
/// tombstones; subsequent reads behave like empty lists.
#[no_mangle]
pub extern "C" fn un_list_int_free(id: i64) {
    let _ = arena().free(id);
}

/// Drop every IntList in the arena. Intended for whole-process teardown next
/// to `jit_cache::clear()`; afterwards any in-flight JIT code is out of
/// contract. Returns the number of tables dropped.
pub(crate) fn clear() -> usize {
    let mut arena = arena();
    let n = arena.slots.iter().filter(|s| s.is_some()).count();
    arena.clear();
    n
}
