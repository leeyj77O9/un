
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};

use crate::runtime::obj::{self, Obj};
use crate::runtime::world;

#[derive(Clone)]
pub struct IntClassDesc {
    pub name: String,
}

fn desc_arena() -> MutexGuard<'static, Vec<IntClassDesc>> {
    INT_CLASSES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

static INT_CLASSES: Mutex<Vec<IntClassDesc>> = Mutex::new(Vec::new());

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

pub(crate) fn clear() -> usize {
    let mut arena = desc_arena();
    let n = arena.len();
    arena.clear();
    n
}

thread_local! {
    static TEMP_INSTANCES: RefCell<Vec<Obj>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn temp_mark() -> usize {
    TEMP_INSTANCES.with(|v| v.borrow().len())
}

pub(crate) fn temp_truncate(mark: usize) {
    TEMP_INSTANCES.with(|v| v.borrow_mut().truncate(mark));
}

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
