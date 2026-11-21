//! Native dict objects (tag 7): insertion-ordered key/value handle pairs.

use crate::obj::{alloc, eq_obj, err, obj, NObj};

unsafe fn pairs(p: u64) -> &'static mut Vec<(u64, u64)> {
    &mut *(obj(p).b as *mut Vec<(u64, u64)>)
}
pub(crate) fn len(p: u64) -> i64 {
    unsafe { pairs(p).len() as i64 }
}
pub(crate) fn display_dict(p: u64) -> String {
    unsafe {
        let parts: Vec<String> = pairs(p)
            .iter()
            .map(|(k, v)| format!("{}:{}", crate::obj::display(*k), crate::obj::display(*v)))
            .collect();
        format!("{{{}}}", parts.join(", "))
    }
}
fn dict_new(pairs_vec: Vec<(u64, u64)>) -> u64 {
    let p = Box::into_raw(Box::new(pairs_vec));
    unsafe { alloc(NObj { tag: 7, a: 0, b: p as i64 }) }
}

#[no_mangle]
pub extern "C" fn unrt_dict_from_pairs(ptr: *const u64, len: i64) -> u64 {
    if ptr.is_null() || len < 0 {
        return err("unrt: bad dict pairs pointer");
    }
    let raw = unsafe { std::slice::from_raw_parts(ptr, (len * 2) as usize) };
    let mut out = Vec::with_capacity(len as usize);
    for chunk in raw.chunks_exact(2) {
        if chunk[0] == 0 || chunk[1] == 0 {
            return err("unrt: dict key or value is not an object");
        }
        out.push((chunk[0], chunk[1]));
    }
    dict_new(out)
}

#[no_mangle]
pub extern "C" fn unrt_dict_get(dict: u64, key: u64) -> u64 {
    unsafe {
        if obj(dict).tag != 7 {
            return err("unrt: dict argument is not a dict");
        }
        if key == 0 {
            return err("unrt: dict key is not an object");
        }
        match pairs(dict).iter().find(|(k, _)| eq_obj(*k, key)) {
            Some((_, v)) => *v,
            None => err(&format!(
                "key '{}' not found in dictionary",
                crate::obj::display(key)
            )),
        }
    }
}

#[no_mangle]
pub extern "C" fn unrt_dict_set(dict: u64, key: u64, value: u64) -> u64 {
    unsafe {
        if obj(dict).tag != 7 {
            return err("unrt: dict argument is not a dict");
        }
        if key == 0 || value == 0 {
            return err("unrt: dict key or value is not an object");
        }
        let v = pairs(dict);
        if let Some(entry) = v.iter_mut().find(|(k, _)| eq_obj(*k, key)) {
            entry.1 = value;
        } else {
            v.push((key, value));
        }
    }
    dict
}
