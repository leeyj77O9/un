//! JIT-only support for the class-constructor intrinsic.
//!
//! The Un JIT can specialise a constructor call `Point(i, i + 1)` in a
//! hot loop when three things hold: the callee is a class the current
//! program declares (compiled-time knowledge), the class has a
//! constructor whose signature is Int-only, and the instance does not
//! escape the native frame. This module backs that fast path.
//!
//! Protocol:
//!
//! - `un_class_make::<N>` is one `#[no_mangle] extern "C"` entry per Int
//!   arity (0..=8). The JIT emits the call with the raw `i64` args it
//!   already holds; every argument must be a plain Int (or Bool 0/1).
//! - The instance is allocated with the same code path the interpreter
//!   uses (`clone_obj` + `obj::init`, so properties, `__setattr__`,
//!   `super` and base-class `__init__` all behave identically), then
//!   parked in a thread-local arena owned by the enclosing `jit_call`.
//!   Native code receives the raw `Rc<RefCell<ObjData>>` pointer as an
//!   i64; the arena outlives the native frame, so the pointer is always
//!   valid for the duration of the call and is released wholesale when
//!   `jit_call` returns.
//! - A failure (unknown class, wrong arity, unwind) returns `0`; the JIT
//!   emits a tag=2 bail and the enclosing function re-runs entirely in the
//!   interpreter, byte-identically to UV_JIT=0.
//!
//! Nothing here is visible to the interpreter: class construction semantics
//! are *defined* by `tobj_call`/`obj::init`; this module merely routes a
//! specialised call to the exact same code.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};

use crate::runtime::obj::{self, Obj};
use crate::runtime::world;

/// One constructor descriptor per (program, class) pair known at JIT plan
/// time. The id is a dense 1-based index into [`INT_CLASSES`]; 0 means
/// "unknown".
#[derive(Clone)]
pub struct IntClassDesc {
    /// Un-level class name (`"Point"`).
    pub name: String,
}

fn desc_arena() -> MutexGuard<'static, Vec<IntClassDesc>> {
    INT_CLASSES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

static INT_CLASSES: Mutex<Vec<IntClassDesc>> = Mutex::new(Vec::new());

/// Return the canonical id for `name`, registering it if absent. Always
/// `>= 1`.
pub fn register(name: &str) -> i64 {
    let mut arena = desc_arena();
    if let Some(pos) = arena.iter().position(|d| d.name == name) {
        return pos as i64 + 1;
    }
    arena.push(IntClassDesc {
        name: name.to_owned(),
    });
    arena.len() as i64
}

/// Drop every registered desc. Intended for whole-process teardown next
/// to `jit_cache::clear()`; afterwards any in-flight JIT code is out of
/// contract. Returns the number of descs dropped.
pub(crate) fn clear() -> usize {
    let mut arena = desc_arena();
    let n = arena.len();
    arena.clear();
    n
}

thread_local! {
    /// Instances constructed by the JIT intrinsic inside the currently
    /// executing `jit_call`. Truncated by `temp_truncate` when that call
    /// returns, so native temporaries are freed deterministically and no
    /// raw pointer outlives its `Rc`.
    static TEMP_INSTANCES: RefCell<Vec<Obj>> = const { RefCell::new(Vec::new()) };
}

/// Marker used by `jit_call` to bound the temporary arena.
pub(crate) fn temp_mark() -> usize {
    TEMP_INSTANCES.with(|v| v.borrow().len())
}

/// Drop every temporary allocated after `mark` was taken. Pointers into
/// those instances are no longer valid afterwards.
pub(crate) fn temp_truncate(mark: usize) {
    TEMP_INSTANCES.with(|v| v.borrow_mut().truncate(mark));
}

/// Shared implementation for the per-arity entries. Returns the raw
/// `Rc<RefCell<ObjData>>` pointer, or 0 on any failure.
fn class_make_impl(desc_id: i64, args: &[i64]) -> i64 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Option<i64> {
        let arena = desc_arena();
        let desc = if desc_id >= 1 && (desc_id as usize) <= arena.len() {
            arena[desc_id as usize - 1].clone()
        } else {
            return None;
        };
        drop(arena);
        let class = world::get_class(&desc.name)?;
        let arg_objs: Vec<Obj> = args.iter().map(|&v| world::int_cached(v)).collect();
        let instance = obj::init(&obj::clone_obj(&class), &arg_objs);
        let ptr = Rc::as_ptr(&instance) as i64;
        TEMP_INSTANCES.with(|v| v.borrow_mut().push(instance));
        Some(ptr)
    }));
    result.ok().flatten().unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn un_class_make_0(desc_id: i64) -> i64 {
    class_make_impl(desc_id, &[])
}
#[no_mangle]
pub extern "C" fn un_class_make_1(desc_id: i64, a0: i64) -> i64 {
    class_make_impl(desc_id, &[a0])
}
#[no_mangle]
pub extern "C" fn un_class_make_2(desc_id: i64, a0: i64, a1: i64) -> i64 {
    class_make_impl(desc_id, &[a0, a1])
}
#[no_mangle]
pub extern "C" fn un_class_make_3(desc_id: i64, a0: i64, a1: i64, a2: i64) -> i64 {
    class_make_impl(desc_id, &[a0, a1, a2])
}
#[no_mangle]
pub extern "C" fn un_class_make_4(desc_id: i64, a0: i64, a1: i64, a2: i64, a3: i64) -> i64 {
    class_make_impl(desc_id, &[a0, a1, a2, a3])
}
#[no_mangle]
pub extern "C" fn un_class_make_5(desc_id: i64, a0: i64, a1: i64, a2: i64, a3: i64, a4: i64) -> i64 {
    class_make_impl(desc_id, &[a0, a1, a2, a3, a4])
}
#[no_mangle]
pub extern "C" fn un_class_make_6(desc_id: i64, a0: i64, a1: i64, a2: i64, a3: i64, a4: i64, a5: i64) -> i64 {
    class_make_impl(desc_id, &[a0, a1, a2, a3, a4, a5])
}
#[no_mangle]
pub extern "C" fn un_class_make_7(desc_id: i64, a0: i64, a1: i64, a2: i64, a3: i64, a4: i64, a5: i64, a6: i64) -> i64 {
    class_make_impl(desc_id, &[a0, a1, a2, a3, a4, a5, a6])
}
#[no_mangle]
pub extern "C" fn un_class_make_8(desc_id: i64, a0: i64, a1: i64, a2: i64, a3: i64, a4: i64, a5: i64, a6: i64, a7: i64) -> i64 {
    class_make_impl(desc_id, &[a0, a1, a2, a3, a4, a5, a6, a7])
}
