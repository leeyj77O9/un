
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

fn arena() -> MutexGuard<'static, IntDictArena> {
    INT_DICT_ARENA
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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

static INT_DICT_ARENA: Mutex<IntDictArena> = Mutex::new(IntDictArena::new());

#[no_mangle]
pub extern "C" fn un_dict_int_empty() -> i64 {
    arena().alloc()
}

#[no_mangle]
pub extern "C" fn un_dict_int_set(d: i64, key: i64, value: i64) -> i64 {
    let mut slots = arena();
    if let Some(dict) = slots.get_mut(d) {
        dict.map.insert(key, value);
        value
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn un_dict_int_get(d: i64, key: i64) -> i64 {
    let slots = arena();
    if let Some(dict) = slots.get(d) {
        match dict.map.get(&key) {
            Some(v) => *v,
            None => -1,
        }
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn un_dict_int_has(d: i64, key: i64) -> i64 {
    let slots = arena();
    match slots.get(d) {
        Some(dict) => i64::from(dict.map.contains_key(&key)),
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn un_dict_int_free(id: i64) {
    let _ = arena().free(id);
}

pub(crate) fn clear() -> usize {
    let mut arena = arena();
    let n = arena.slots.iter().filter(|slot| slot.is_some()).count();
    arena.clear();
    n
}
