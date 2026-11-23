//! Native dict objects (tag 7): insertion-ordered key/value pairs with a
//! value-based hash index. Keys remain opaque object handles at the C ABI;
//! the index only speeds up lookup, while `eq_obj` stays authoritative.

use crate::obj::{alloc, eq_obj, err, obj, NObj};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};

/// Deterministic, allocation-free hasher for the internal integer-keyed
/// indexes. `RandomState`'s SipHash setup dominates short dict lookups; the
/// in-process index only needs good distribution, not DoS resistance, and
/// collisions are still resolved by `eq_obj` / exact i64 comparison.
#[derive(Default)]
struct FastHasher(u64);

impl Hasher for FastHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        // Generic fallback (only used if a non-integer key reaches an index).
        let mut h = self.0 ^ 0x9e3779b97f4a7c15;
        for &b in bytes {
            h = (h ^ b as u64).wrapping_mul(0xff51afd7ed558ccd);
        }
        self.0 = splitmix64(h);
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.0 = splitmix64(i);
    }
    #[inline]
    fn write_i64(&mut self, i: i64) {
        self.0 = splitmix64(i as u64);
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.0 = splitmix64(i as u64);
    }
    #[inline]
    fn write_i32(&mut self, i: i32) {
        self.0 = splitmix64(i as u64);
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.0 = splitmix64(i as u64);
    }
}

type FastIndex = HashMap<u64, Vec<usize>, BuildHasherDefault<FastHasher>>;
type FastIntMap = HashMap<i64, (i64, usize), BuildHasherDefault<FastHasher>>;

struct DictData {
    pairs: Vec<(u64, u64)>,
    /// hash -> candidate indices, in insertion order.
    index: FastIndex,
    /// Number of keys per equivalence class (numeric, str, other). Used to
    /// skip the cross-type fallback scan when all keys share the lookup class.
    class_counts: [usize; 3],
    /// Direct storage for statically typed `dict[int, int]`: key -> (value,
    /// index in `pairs` so len()/display() stay correct). Empty otherwise.
    int_map: FastIntMap,
    int_only: bool,
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
    let mut index: FastIndex = HashMap::default();
    let mut class_counts = [0usize; 3];
    let mut int_map: FastIntMap = HashMap::default();
    let mut int_only = true;
    for (i, (k, v)) in pairs_vec.iter().enumerate() {
        index.entry(hash_obj(*k)).or_default().push(i);
        class_counts[key_class(*k)] += 1;
        if unsafe { obj(*k).tag == 0 && obj(*v).tag == 0 } {
            int_map.insert(unsafe { obj(*k).a }, (unsafe { obj(*v).a }, i));
        } else {
            int_only = false;
        }
    }
    let d = Box::new(DictData { pairs: pairs_vec, index, class_counts, int_map, int_only });
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
        if data(dict).int_only && obj(key).tag == 0 {
            return match data(dict).int_map.get(&obj(key).a).copied() {
                Some((v, _)) => crate::obj::unrt_obj_int(v),
                None => err(&format!("key '{}' not found in dictionary", crate::obj::display(key))),
            };
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

/// Raw-i64 read for statically typed `dict[int, int]`.
///
/// Success writes `0` to `err_out` and returns the value directly, skipping
/// boxing/unboxing and the boxed-key hash lookup. Failure writes the exact
/// same error object `unrt_dict_get` would produce to `err_out` and returns
/// `0`; the generated call site keeps its usual span/error-site handling.
#[no_mangle]
pub extern "C" fn unrt_dict_get_int(dict: u64, key: i64, err_out: *mut u64) -> i64 {
    unsafe {
        if obj(dict).tag != 7 {
            if !err_out.is_null() {
                *err_out = err("unrt: dict argument is not a dict");
            }
            return 0;
        }
        // Same handle the boxed path receives from `wrap_as_obj(Int)`, so
        // `display()` produces a byte-identical message for missing keys.
        let key_h = crate::obj::unrt_obj_int(key);
        let found = if data(dict).int_only {
            data(dict).int_map.get(&key).copied().map(|(v, _)| v)
        } else {
            find(dict, key_h).map(|(i, _)| crate::obj::int_of(data(dict).pairs[i].1))
        };
        match found {
            Some(v) => {
                if !err_out.is_null() {
                    *err_out = 0;
                }
                v
            }
            None => {
                let e = err(&format!(
                    "key '{}' not found in dictionary",
                    crate::obj::display(key_h)
                ));
                if !err_out.is_null() {
                    *err_out = e;
                }
                0
            }
        }
    }
}

/// Fast path for statically typed `dict[int, int]`: raw i64 key/value, no
/// boxing, no SipHash, no `eq_obj`. Always returns the dict handle and never
/// produces an error object (the generated call site relies on this contract).
#[no_mangle]
pub extern "C" fn unrt_dict_set_int(dict: u64, key: i64, value: i64) -> u64 {
    unsafe {
    if data(dict).int_only {
        let d = data(dict);
        if let Some(&(_, i)) = d.int_map.get(&key) {
            d.pairs[i].1 = crate::obj::unrt_obj_int(value);
            d.int_map.insert(key, (value, i));
        } else {
            let idx = d.pairs.len();
            let key_h = crate::obj::unrt_obj_int(key);
            let value_h = crate::obj::unrt_obj_int(value);
            // `int_map` is authoritative for int-only dicts; `pairs` only
            // backs len()/display(), so the hash bucket index is unnecessary.
            d.pairs.push((key_h, value_h));
            d.int_map.insert(key, (value, idx));
        }
    } else {
        unrt_dict_set(dict, crate::obj::unrt_obj_int(key), crate::obj::unrt_obj_int(value));
    }
    dict
    }
}

/// Snapshot the dict keys into a new list, in insertion order. Mirrors the
/// VM's `dict.keys()` native method (`KeyMap.keys()`).
#[no_mangle]
pub extern "C" fn unrt_dict_keys(dict: u64) -> u64 {
    unsafe {
        if obj(dict).tag != 7 {
            return err("unrt: keys target is not a dict");
        }
        let keys: Vec<u64> = data(dict).pairs.iter().map(|(k, _)| *k).collect();
        crate::list::unrt_list_from_elems(keys.as_ptr(), keys.len() as i64)
    }
}

/// Snapshot the dict values into a new list, in insertion order. Mirrors the
/// VM's `dict.values()` native method (`KeyMap.values()`).
#[no_mangle]
pub extern "C" fn unrt_dict_values(dict: u64) -> u64 {
    unsafe {
        if obj(dict).tag != 7 {
            return err("unrt: values target is not a dict");
        }
        let values: Vec<u64> = data(dict).pairs.iter().map(|(_, v)| *v).collect();
        crate::list::unrt_list_from_elems(values.as_ptr(), values.len() as i64)
    }
}

/// Remove every entry from a dict. Returns the dict handle on success (so the
/// existing self-return error gate applies) and an error handle otherwise.
#[no_mangle]
pub extern "C" fn unrt_dict_clear(dict: u64) -> u64 {
    unsafe {
        if obj(dict).tag != 7 {
            return err("unrt: clear target is not a dict");
        }
        let d = data(dict);
        d.pairs.clear();
        d.index.clear();
        d.class_counts = [0; 3];
        d.int_map.clear();
        // Keep `int_only`: it is a property of the dict's key/value types, not
        // of its current contents, and the fast paths rely on it.
    }
    dict
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
        if data(dict).int_only && obj(key).tag == 0 && obj(value).tag == 0 {
            let idx = data(dict).pairs.len() - 1;
            data(dict).int_map.insert(obj(key).a, (obj(value).a, idx));
        }
    }
    dict
}
