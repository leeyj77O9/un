//! List operations.

use crate::obj::{alloc, cmp_obj, eq_obj, err, obj, obj_mut, unrt_obj_float, unrt_obj_int, NObj};

pub(crate) unsafe fn items(p: u64) -> &'static mut Vec<u64> {
    &mut *(obj(p).b as *mut Vec<u64>)
}
pub(crate) fn list_new(items: Vec<u64>) -> u64 {
    let len = items.len() as i64;
    let v = Box::into_raw(Box::new(items));
    unsafe { alloc(NObj { tag: 3, a: len, b: v as i64 }) }
}
fn index(v: &[u64], idx: i64) -> Option<usize> {
    let i = if idx < 0 { v.len() as i64 + idx } else { idx };
    if i >= 0 && (i as usize) < v.len() { Some(i as usize) } else { None }
}

#[no_mangle]
pub extern "C" fn unrt_len(o: u64) -> u64 {
    // The VM bridge returns an object handle, not a raw i64.
    let n = unsafe {
        match obj(o).tag {
            3 => obj(o).a,
            6 => crate::strings::str_of(o).chars().count() as i64,
            7 => crate::dict::len(o),
            _ => 0,
        }
    };
    unrt_obj_int(n)
}
#[no_mangle]
pub extern "C" fn unrt_list_len(list: u64) -> i64 {
    unsafe { obj(list).a }
}
#[no_mangle]
pub extern "C" fn unrt_list_new() -> u64 {
    list_new(Vec::new())
}
#[no_mangle]
pub extern "C" fn unrt_list_push(list: u64, item: u64) -> u64 {
    unsafe {
        let v = items(list);
        v.push(item);
        obj_mut(list).a = v.len() as i64;
    }
    list
}
#[no_mangle]
pub extern "C" fn unrt_list_get(list: u64, idx: i64) -> u64 {
    unsafe {
        let v = items(list);
        match index(v, idx) {
            Some(i) => v[i],
            None => err("list index out of range"),
        }
    }
}
#[no_mangle]
pub extern "C" fn unrt_list_set(list: u64, idx: i64, value: u64) -> u64 {
    unsafe {
        let v = items(list);
        match index(v, idx) {
            Some(i) => v[i] = value,
            None => return crate::obj::err("list index out of range"),
        }
    }
    list
}
#[no_mangle]
pub extern "C" fn unrt_list_pop(list: u64) -> u64 {
    unsafe {
        let v = items(list);
        let res = v.pop().unwrap_or_else(|| err("list index out of range"));
        obj_mut(list).a = v.len() as i64;
        res
    }
}
#[no_mangle]
pub extern "C" fn unrt_list_pop_at(list: u64, idx: i64) -> u64 {
    unsafe {
        let v = items(list);
        match index(v, idx) {
            Some(i) => {
                let res = v.remove(i);
                obj_mut(list).a = v.len() as i64;
                res
            }
            None => err("list index out of range"),
        }
    }
}
#[no_mangle]
pub extern "C" fn unrt_list_insert(list: u64, item: u64, idx: i64) -> u64 {
    unsafe {
        let v = items(list);
        let i = if idx < 0 { v.len() as i64 + idx } else { idx };
        let i = i.clamp(0, v.len() as i64) as usize;
        v.insert(i, item);
        obj_mut(list).a = v.len() as i64;
    }
    list
}
#[no_mangle]
pub extern "C" fn unrt_list_reverse(list: u64) -> u64 {
    unsafe { items(list).reverse(); }
    list
}
#[no_mangle]
pub extern "C" fn unrt_list_sort(list: u64) -> u64 {
    unsafe { items(list).sort_by(|a, b| cmp_obj(*a, *b)); }
    list
}
#[no_mangle]
pub extern "C" fn unrt_list_contains(list: u64, item: u64) -> i64 {
    unsafe {
        if items(list).iter().any(|p| eq_obj(*p, item)) { 1 } else { 0 }
    }
}
#[no_mangle]
pub extern "C" fn unrt_list_index_of(list: u64, item: u64) -> i64 {
    unsafe {
        items(list)
            .iter()
            .position(|p| eq_obj(*p, item))
            .map_or(-1, |i| i as i64)
    }
}
#[no_mangle]
pub extern "C" fn unrt_list_from_ints(ptr: *const i64, len: i64) -> u64 {
    if ptr.is_null() || len < 0 { return err("bad list"); }
    let items: Vec<u64> = unsafe { std::slice::from_raw_parts(ptr, len as usize) }
        .iter()
        .map(|v| unrt_obj_int(*v))
        .collect();
    list_new(items)
}
#[no_mangle]
pub extern "C" fn unrt_list_from_elems(ptr: *const u64, len: i64) -> u64 {
    if ptr.is_null() || len < 0 { return err("bad list"); }
    let items = unsafe { std::slice::from_raw_parts(ptr, len as usize) }.to_vec();
    list_new(items)
}
#[no_mangle]
pub extern "C" fn unrt_list_from_floats(ptr: *const f64, len: i64) -> u64 {
    if ptr.is_null() || len < 0 { return err("bad list"); }
    let items: Vec<u64> = unsafe { std::slice::from_raw_parts(ptr, len as usize) }
        .iter()
        .map(|v| unrt_obj_float(*v))
        .collect();
    list_new(items)
}
#[no_mangle]
pub extern "C" fn unrt_list_repeat_int(value: i64, len: i64) -> u64 {
    list_new((0..len.max(0)).map(|_| unrt_obj_int(value)).collect())
}
#[no_mangle]
pub extern "C" fn unrt_list_repeat_float(value: f64, len: i64) -> u64 {
    list_new((0..len.max(0)).map(|_| unrt_obj_float(value)).collect())
}
