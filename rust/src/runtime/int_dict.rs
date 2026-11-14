//! Shared native support for Int-only dictionary intrinsics used by the
//! JIT. The JIT-emitted code passes a raw i64 "dict id" between calls; the
//! arena lives independently of the Un object model so there's no need to
//! allocate Obj wrappers for transient tables.
//!
//! Arena slots index into an `i64` id where `0` means "uninitialised"; the
//! real index is an offset by 1. Each `IntDict` allocates four tables
//! (map, entries, etc.) — each id is unique.
//!
//! This module is **only** used by JIT-emitted code; the interpreter does
//! not use it.

use std::collections::HashMap;
use std::sync::Mutex;

struct IntDict {
    map: HashMap<i64, i64>,
}

impl IntDict {
    fn new() -> Self {
        Self { map: HashMap::new() }
    }
}

struct IntDictArena {
    slots: Vec<Option<IntDict>>,
}

impl IntDictArena {
    const fn new() -> Self {
        Self { slots: Vec::new() }
    }

    fn alloc(&mut self) -> i64 {
        let idx = self.slots.len() as i64 + 1;
        self.slots.push(Some(IntDict::new()));
        idx
    }

    fn get_mut(&mut self, id: i64) -> Option<&mut IntDict> {
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

    fn get(&self, id: i64) -> Option<&IntDict> {
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
}

static INT_DICT_ARENA: Mutex<IntDictArena> = Mutex::new(IntDictArena::new());

/// Allocate a fresh empty Int→Int dictionary; returns its canonical i64 id
/// (always ≥ 1; 0 means "invalid"). Called via inline JIT code.
#[no_mangle]
pub extern "C" fn un_dict_int_empty() -> i64 {
    INT_DICT_ARENA.lock().unwrap().alloc()
}

/// Insert `value` into the `key` slot of the specified dict, returning the
/// inserted value. No-ops (returning `0`) if id/key is inconsistent. JIT
/// callsite arguments must be raw i64.
#[no_mangle]
pub extern "C" fn un_dict_int_set(d: i64, key: i64, value: i64) -> i64 {
    let mut slots = INT_DICT_ARENA.lock().unwrap();
    if let Some(dict) = slots.get_mut(d) {
        dict.map.insert(key, value);
        value
    } else {
        0
    }
}

/// Look up `key` in `d`; if found return the value (sentinel `-1` for
/// "missing"). JIT ABI must accept the Int-typed key without sentinel
/// confusion because callers opt in to sentinel reporting.
#[no_mangle]
pub extern "C" fn un_dict_int_get(d: i64, key: i64) -> i64 {
    let slots = INT_DICT_ARENA.lock().unwrap();
    if let Some(dict) = slots.get(d) {
        match dict.map.get(&key) {
            Some(v) => *v,
            None => -1,
        }
    } else {
        -1
    }
}
