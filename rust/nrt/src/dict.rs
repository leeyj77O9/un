//! Native dict objects (tag 7): insertion-ordered key/value pairs with a
//! value-based hash index. Keys remain opaque object handles at the C ABI;
//! the index only speeds up lookup, while `eq_obj` stays authoritative.

use crate::obj::{alloc, eq_obj, err, obj, NObj};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

struct DictData {
    pairs: Vec<(u64, u64)>,
    /// hash -> candidate indices, in insertion order.
    index: HashMap<u64, Vec<usize>>,
    /// Number of keys per equivalence class (numeric, str, other). Used to
    /// skip the cross-type fallback scan when all keys share the lookup class.
    class_counts: [usize; 3],
}

/// Equivalence class used by `cmp_obj`: numeric (Int/Float/Bool), Str, other.
fn key_class(p: u64) -> usize {
    unsafe {
        match obj(p).tag {
            0 | 1 | 2 => 0,
            6 => 1,
            _ => 2,
        }
    }
}

/// A value-based hash consistent with `eq_obj`: Int, Float and Bool compare
/// numerically, so they share one canonical f64 bucket. Str hashes by content;
/// everything else shares a tag bucket. `find` also falls back to a full scan
/// because `cmp_obj` has a permissive cross-type `_ => Equal` arm.
fn hash_obj(p: u64) -> u64 {
    unsafe {
        let o = obj(p);
        match o.tag {
            // Integers dominate dict benchmarks; SipHash setup per operation
            // costs more than the lookup itself. splitmix64 is plenty for this
            // in-process index (collisions are resolved by `eq_obj`).
            0 | 2 => splitmix64((o.a as f64).to_bits()),
            1 => splitmix64(o.a as u64),
            6 => {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                crate::strings::str_of(p).as_bytes().hash(&mut h);
                h.finish()
            }
            _ => splitmix64(o.tag as u64),
        }
    }
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

unsafe fn data(p: u64) -> &'static mut DictData {
    &mut *(obj(p).b as *mut DictData)
}
pub(crate) fn len(p: u64) -> i64 {
    unsafe { data(p).pairs.len() as i64 }
}
pub(crate) fn display_dict(p: u64) -> String {
    unsafe {
        let parts: Vec<String> = data(p)
            .pairs
            .iter()
            .map(|(k, v)| format!("{}:{}", crate::obj::display(*k), crate::obj::display(*v)))
            .collect();
        format!("{{{}}}", parts.join(", "))
    }
}
fn dict_new(pairs_vec: Vec<(u64, u64)>) -> u64 {
    let mut index: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut class_counts = [0usize; 3];
    for (i, (k, _)) in pairs_vec.iter().enumerate() {
        index.entry(hash_obj(*k)).or_default().push(i);
        class_counts[key_class(*k)] += 1;
    }
    let d = Box::new(DictData { pairs: pairs_vec, index, class_counts });
    unsafe { alloc(NObj { tag: 7, a: 0, b: Box::into_raw(d) as i64 }) }
}

/// Find the first entry matching `key`; returns (pair index, hash).
unsafe fn find(d: u64, key: u64) -> Option<(usize, u64)> {
    let h = hash_obj(key);
    let d = data(d);
    if let Some(candidates) = d.index.get(&h) {
        for &i in candidates {
            if eq_obj(d.pairs[i].0, key) {
                return Some((i, h));
            }
        }
    }
    // `cmp_obj` treats cross-type keys as equal in several cases. Only pay for
    // the linear fallback when the dict actually contains another key class;
    // homogeneous dicts (the common case) stay O(1).
    let cls = key_class(key);
    if (0..3).any(|c| c != cls && d.class_counts[c] > 0) {
        for (i, (k, _)) in d.pairs.iter().enumerate() {
            if eq_obj(*k, key) {
                return Some((i, h));
            }
        }
    }
    None
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
        match find(dict, key) {
            Some((i, _)) => data(dict).pairs[i].1,
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
        if let Some((i, _)) = find(dict, key) {
            data(dict).pairs[i].1 = value;
        } else {
            let d = data(dict);
            let idx = d.pairs.len();
            let h = hash_obj(key);
            d.pairs.push((key, value));
            d.index.entry(h).or_default().push(idx);
            d.class_counts[key_class(key)] += 1;
        }
    }
    dict
}
