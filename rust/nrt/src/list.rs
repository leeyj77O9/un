//! List operations.

use crate::obj::{
    alloc, cmp_obj, eq_obj, err, obj, obj_mut, seq_len, unrt_obj_float, unrt_obj_int,
    unrt_obj_none, NObj,
};

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
            // Tuple/Set lengths are mirrored in `b` by `obj::seq_new`.
            14 | 15 => seq_len(o),
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
/// ARC-00: sequence probes for nested pattern matching. Mirrors the VM's
/// `obj::unpack_items` (lists and tuples are both sequences); invalid inputs
/// report failure through -1 / 0 instead of raising a native error, so a
/// pattern miss stays an ordinary non-match.
#[no_mangle]
pub extern "C" fn unrt_match_seq_len(value: u64) -> i64 {
    unsafe {
        match obj(value).tag {
            3 => obj(value).a,
            14 => crate::obj::seq_len(value),
            _ => -1,
        }
    }
}
#[no_mangle]
pub extern "C" fn unrt_match_seq_get(value: u64, idx: i64) -> u64 {
    unsafe {
        match obj(value).tag {
            3 => {
                let v = items(value);
                index(v, idx).map_or(0, |i| v[i])
            }
            14 => {
                let v = crate::obj::seq_items(value);
                index(v, idx).map_or(0, |i| v[i])
            }
            _ => 0,
        }
    }
}

/// Fused read for statically typed `list[int]`: returns the raw i64 and skips
/// the box/unbox FFI round trip. Out-of-range writes a pending error, which the
/// runtime reports at exit just like the boxed path.
#[no_mangle]
pub extern "C" fn unrt_list_get_int(list: u64, idx: i64) -> i64 {
    unsafe {
        let v = items(list);
        match index(v, idx) {
            Some(i) => crate::obj::int_of(v[i]),
            None => {
                crate::obj::unrt_write_error(err("list index out of range"));
                0
            }
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

/// Raw-i64 write for statically typed `list[int]`: boxes the value once and
/// stores the handle directly, skipping the generated-code boxing round trip.
/// On an out-of-range index it returns the exact same error object as
/// `unrt_list_set` ("list index out of range").
#[no_mangle]
pub extern "C" fn unrt_list_set_int(list: u64, idx: i64, value: i64) -> u64 {
    unsafe {
        let v = items(list);
        match index(v, idx) {
            Some(i) => v[i] = crate::obj::unrt_obj_int(value),
            None => return err("list index out of range"),
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
    // Every element has the same value; share a single boxed object.
    let item = unrt_obj_int(value);
    list_new(vec![item; len.max(0) as usize])
}
#[no_mangle]
pub extern "C" fn unrt_list_repeat_float(value: f64, len: i64) -> u64 {
    list_new((0..len.max(0)).map(|_| unrt_obj_float(value)).collect())
}

// ---------------------------------------------------------------------------
// Slice support (plain no-VM executables).
//
// These are `pub(crate)` on purpose: exposing new `unrt_*` C ABI symbols needs
// ARC-00 approval (SESSION_ARCHITECTURE.md §4 / §8.1). Until that approval is
// granted the functionality is reachable through the *existing*
// `unrt_call_native` dispatch using module "list" (see `dispatch` below):
//
//   ("list", "slice")     [list, start, end, step]       -> new list handle
//   ("list", "set_slice") [list, value, start, end, step] -> same list handle
//   ("list", "clear")     [list]                         -> same list handle
//
// `start` / `end` / `step` are boxed handles; tag 4 (`unrt_obj_none()`) means
// "omitted", exactly like the VM's `Option<Obj> = None`.
// ---------------------------------------------------------------------------

/// Replica of `runtime::collections::slice_indices` (rust/src/runtime/collections.rs:502).
/// Negative explicit bounds get exactly one `+ len` adjustment, then the
/// result is clamped per the step direction: `[0, len]` for forward slices and
/// `[-1, len-1]` for reverse slices. Omitted reverse bounds default to
/// `len-1` / `-1`.
pub(crate) fn slice_indices(
    len: i64,
    start: u64,
    end: u64,
    step: u64,
) -> Result<(i64, i64, i64), u64> {
    unsafe {
        let st = match obj(step).tag {
            4 => 1,
            0 => obj(step).a,
            _ => return Err(err("slice step must be int")),
        };
        if st == 0 {
            return Err(err("slice step cannot be zero"));
        }

        // `None` (tag 4) == omitted bound. An explicit bound must be an Int;
        // as in the VM it is adjusted once for negatives and only clamped
        // afterwards.
        let bound = |p: u64, label: &str| -> Result<Option<i64>, u64> {
            match obj(p).tag {
                4 => Ok(None),
                0 => {
                    let mut i = obj(p).a;
                    if i < 0 {
                        i = i.wrapping_add(len);
                    }
                    Ok(Some(i))
                }
                _ => Err(err(&format!("slice {label} must be int"))),
            }
        };

        let mut a = match bound(start, "start")? {
            Some(i) => i,
            None => if st < 0 { len - 1 } else { 0 },
        };
        let mut b = match bound(end, "end")? {
            Some(i) => i,
            None => if st < 0 { -1 } else { len },
        };

        if st < 0 {
            if a > len - 1 { a = len - 1; }
            if a < -1 { a = -1; }
            if b > len - 1 { b = len - 1; }
            if b < -1 { b = -1; }
        } else {
            if a > len { a = len; }
            if a < 0 { a = 0; }
            if b > len { b = len; }
            if b < 0 { b = 0; }
        }
        Ok((a, b, st))
    }
}

/// Replica of `runtime::collections::slice_positions` (rust/src/runtime/collections.rs:582).
/// `i128` keeps the loop correct even when a bound is `i64::MIN`/`i64::MAX`.
pub(crate) fn slice_positions(len: i64, start: i64, end: i64, step: i64) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = start as i128;
    let limit = end as i128;
    let st = step as i128;
    if st > 0 {
        while i < limit {
            if i >= 0 && i < len as i128 { out.push(i as usize); }
            i += st;
        }
    } else {
        while i > limit {
            if i >= 0 && i < len as i128 { out.push(i as usize); }
            i += st;
        }
    }
    out
}

/// Slice read — replica of `collections::list_slice` (rust/src/runtime/collections.rs:605).
/// The source list is snapshotted before indexing, so the result is a fresh
/// list object and the original is untouched.
pub(crate) fn slice(list: u64, start: u64, end: u64, step: u64) -> u64 {
    unsafe {
        if obj(list).tag != 3 {
            return err(&format!("unsupported slice for '{}'", crate::dynops::type_name(list)));
        }
        let mine = items(list).clone();
        let len = mine.len() as i64;
        let (a, b, st) = match slice_indices(len, start, end, step) {
            Ok(v) => v,
            Err(e) => return e,
        };
        let out = slice_positions(len, a, b, st);
        list_new(out.into_iter().map(|i| mine[i]).collect())
    }
}

/// Snapshot the RHS of a slice assignment the way the VM's
/// `obj::to_list(value)` + `borrow().clone()` does. A Str RHS becomes a list of
/// one-character strings (identical to `iters::str_list` for valid UTF-8).
fn rhs_snapshot(value: u64) -> Option<Vec<u64>> {
    unsafe {
        match obj(value).tag {
            3 => Some(items(value).clone()),
            6 => Some(
                crate::strings::str_of(value)
                    .chars()
                    .map(|c| crate::strings::str_new(c.to_string()))
                    .collect(),
            ),
            _ => None,
        }
    }
}

/// Slice assignment — replica of `vm::VM::set_slice` (rust/src/vm/mod.rs:1553).
/// The RHS is snapshotted before any mutation, so `xs[1:3] = xs` is safe.
/// Identity is preserved: the same `list` handle is returned on success.
pub(crate) fn set_slice(list: u64, value: u64, start: u64, end: u64, step: u64) -> u64 {
    unsafe {
        if obj(list).tag != 3 {
            return err("slice assignment only supports list");
        }
        let count = items(list).len() as i64;
        let (a, mut b, st) = match slice_indices(count, start, end, step) {
            Ok(v) => v,
            Err(e) => return e,
        };
        let rhs = match rhs_snapshot(value) {
            Some(v) => v,
            None => return err("slice assignment value must be list"),
        };

        if st != 1 {
            // Extended slices may not change the list length; exactly as many
            // values as selected positions must be supplied.
            let positions = slice_positions(count, a, b, st);
            if positions.len() != rhs.len() {
                return err(&format!(
                    "attempt to assign sequence of size {} to extended slice of size {}",
                    rhs.len(),
                    positions.len()
                ));
            }
            let v = items(list);
            for (idx, item) in positions.into_iter().zip(rhs) {
                v[idx] = item;
            }
            return list;
        }

        // A reversed range is an empty insertion point, not an error.
        if b < a { b = a; }
        let slice_len = b - a;
        let common = slice_len.min(rhs.len() as i64);
        let v = items(list);
        for i in 0..common {
            v[(a + i) as usize] = rhs[i as usize];
        }
        // Splice: insert surplus values, then drop the leftover old elements.
        for (k, item) in rhs[common as usize..].iter().enumerate() {
            v.insert((a + common + k as i64) as usize, *item);
        }
        let remove_from = (a + rhs.len() as i64) as usize;
        for _ in rhs.len() as i64..slice_len {
            if remove_from < v.len() { v.remove(remove_from); }
        }
        obj_mut(list).a = v.len() as i64;
        list
    }
}

/// Replica of `dict_clear`-style member helpers: empties a list in place and
/// returns the same handle. Never raises for a genuine list.
pub(crate) fn clear(list: u64) -> u64 {
    unsafe {
        if obj(list).tag != 3 {
            return err("unrt: clear target is not a list");
        }
        items(list).clear();
        obj_mut(list).a = 0;
    }
    list
}

/// Existing-ABI entry point for slice/clear operations. See the comment above
/// `slice_indices` for the module/member/argument contract.
pub(crate) fn dispatch(module: &str, member: &str, args: &[u64]) -> Option<u64> {
    if module != "list" { return None; }
    let out = match (member, args) {
        ("slice", [list, start, end, step]) => slice(*list, *start, *end, *step),
        ("set_slice", [list, value, start, end, step]) => {
            set_slice(*list, *value, *start, *end, *step)
        }
        ("clear", [list]) => clear(*list),
        // VM `native::members::list_resize` is a no-op capacity hint that
        // validates the receiver and returns `none` (tag 4). The `size`
        // argument, if any, is ignored exactly as in the VM.
        ("resize", [list, ..]) => {
            if unsafe { obj(*list).tag } == 3 { unrt_obj_none() } else { err("expected 'list' object") }
        }
        _ => return None,
    };
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obj::{display, int_of, unrt_obj_is_err, unrt_obj_none};

    fn list_of(values: &[i64]) -> u64 {
        list_new(values.iter().map(|v| unrt_obj_int(*v)).collect())
    }
    fn elems(p: u64) -> Vec<i64> {
        unsafe { items(p).iter().map(|v| int_of(*v)).collect() }
    }
    fn none() -> u64 { unrt_obj_none() }
    fn assert_err(p: u64, msg: &str) {
        assert_eq!(unrt_obj_is_err(p), 1, "expected error: {msg}, got {}", display(p));
        assert_eq!(display(p), msg);
    }

    #[test]
    fn slice_read_matches_vm_normalisation() {
        let xs = list_of(&[10, 20, 30, 40, 50]);
        assert_eq!(elems(slice(xs, none(), none(), none())), [10, 20, 30, 40, 50]);
        assert_eq!(elems(slice(xs, unrt_obj_int(1), unrt_obj_int(4), none())), [20, 30, 40]);
        assert_eq!(elems(slice(xs, unrt_obj_int(-2), none(), none())), [40, 50]);
        assert_eq!(elems(slice(xs, none(), unrt_obj_int(-1), none())), [10, 20, 30, 40]);
        assert_eq!(elems(slice(xs, unrt_obj_int(-100), unrt_obj_int(100), none())), [10, 20, 30, 40, 50]);
        assert_eq!(elems(slice(xs, unrt_obj_int(10), none(), none())), Vec::<i64>::new());
        // step != 1 (forward and reverse)
        assert_eq!(elems(slice(xs, none(), none(), unrt_obj_int(2))), [10, 30, 50]);
        assert_eq!(elems(slice(xs, none(), none(), unrt_obj_int(-1))), [50, 40, 30, 20, 10]);
        assert_eq!(elems(slice(xs, unrt_obj_int(4), unrt_obj_int(0), unrt_obj_int(-2))), [50, 30]);
        // reading returns a fresh list; the source is unchanged
        assert_eq!(unrt_list_len(xs), 5);
        assert_eq!(elems(xs), [10, 20, 30, 40, 50]);
    }

    #[test]
    fn slice_read_errors_match_vm() {
        let xs = list_of(&[1, 2, 3]);
        assert_err(slice(xs, none(), none(), unrt_obj_int(0)), "slice step cannot be zero");
        assert_err(
            slice(xs, crate::strings::str_new("x".into()), none(), none()),
            "slice start must be int",
        );
        assert_err(
            slice(xs, none(), none(), crate::strings::str_new("x".into())),
            "slice step must be int",
        );
        assert_err(slice(unrt_obj_int(1), none(), none(), none()), "unsupported slice for 'int'");
    }

    #[test]
    fn slice_assign_step_one_splices_and_keeps_identity() {
        let xs = list_of(&[10, 20, 30, 40, 50]);
        let same = set_slice(xs, list_of(&[7, 8, 9]), unrt_obj_int(1), unrt_obj_int(3), none());
        assert_eq!(same, xs);
        assert_eq!(elems(xs), [10, 7, 8, 9, 40, 50]);
        assert_eq!(unrt_list_len(xs), 6);

        let xs = list_of(&[10, 20, 30, 40, 50]);
        set_slice(xs, list_of(&[7]), unrt_obj_int(1), unrt_obj_int(3), none());
        assert_eq!(elems(xs), [10, 7, 40, 50]);

        let xs = list_of(&[10, 20, 30]);
        set_slice(xs, list_of(&[7, 8]), unrt_obj_int(1), unrt_obj_int(1), none());
        assert_eq!(elems(xs), [10, 7, 8, 20, 30]);

        // reversed range == empty insertion point
        let xs = list_of(&[10, 20, 30]);
        set_slice(xs, list_of(&[7]), unrt_obj_int(2), unrt_obj_int(0), none());
        assert_eq!(elems(xs), [10, 20, 7, 30]);
    }

    #[test]
    fn slice_assign_extended_replaces_only_equal_length() {
        let xs = list_of(&[10, 20, 30, 40, 50]);
        set_slice(xs, list_of(&[1, 2, 3]), none(), none(), unrt_obj_int(2));
        assert_eq!(elems(xs), [1, 20, 2, 40, 3]);
        assert_eq!(unrt_list_len(xs), 5);

        let xs = list_of(&[10, 20, 30, 40, 50]);
        assert_err(
            set_slice(xs, list_of(&[1, 2]), none(), none(), unrt_obj_int(2)),
            "attempt to assign sequence of size 2 to extended slice of size 3",
        );
        assert_eq!(elems(xs), [10, 20, 30, 40, 50]);
    }

    #[test]
    fn slice_assign_self_rhs_is_snapshotted() {
        let xs = list_of(&[10, 20, 30, 40, 50]);
        let same = set_slice(xs, xs, none(), none(), none());
        assert_eq!(same, xs);
        assert_eq!(elems(xs), [10, 20, 30, 40, 50]);

        let xs = list_of(&[10, 20, 30, 40, 50]);
        set_slice(xs, xs, none(), none(), unrt_obj_int(-1));
        assert_eq!(elems(xs), [50, 40, 30, 20, 10]);

        // Exact case from test/unit/runtime/slice_assign.un: `a[1:3] = a`.
        let xs = list_of(&[1, 2, 3, 4, 5]);
        set_slice(xs, xs, unrt_obj_int(1), unrt_obj_int(3), none());
        assert_eq!(elems(xs), [1, 1, 2, 3, 4, 5, 4, 5]);
    }

    #[test]
    fn slice_bounds_stay_i64() {
        let xs = list_of(&[1, 2, 3]);
        let got = slice(xs, unrt_obj_int(10_000_000_000), unrt_obj_int(20_000_000_000), none());
        assert_eq!(elems(got), Vec::<i64>::new());
        let got = slice(xs, unrt_obj_int(-10_000_000_000), none(), none());
        assert_eq!(elems(got), [1, 2, 3]);
    }

    #[test]
    fn clear_empties_list_in_place() {
        let xs = list_of(&[1, 2, 3]);
        assert_eq!(clear(xs), xs);
        assert_eq!(elems(xs), Vec::<i64>::new());
        assert_eq!(unrt_list_len(xs), 0);
    }

    #[test]
    fn existing_abi_dispatch_reaches_slice_helpers() {
        let xs = list_of(&[1, 2, 3, 4]);
        let start = unrt_obj_int(1);
        let end = unrt_obj_int(3);
        let step = none();
        let got = crate::native::unrt_call_native(
            b"list\0".as_ptr(), 4, b"slice\0".as_ptr(), 5,
            [xs, start, end, step].as_ptr(), 4,
        );
        assert_eq!(elems(got), [2, 3]);

        let got = crate::native::unrt_call_native(
            b"list\0".as_ptr(), 4, b"clear\0".as_ptr(), 5,
            [xs].as_ptr(), 1,
        );
        assert_eq!(got, xs);
        assert_eq!(elems(xs), Vec::<i64>::new());
    }

    #[test]
    fn resize_is_a_noop_returning_tag_four_none() {
        let xs = list_of(&[1, 2, 3]);
        let got = crate::native::unrt_call_native(
            b"list\0".as_ptr(), 4, b"resize\0".as_ptr(), 6,
            [xs, unrt_obj_int(10)].as_ptr(), 2,
        );
        assert_eq!(unsafe { obj(got).tag }, 4);
        assert_eq!(elems(xs), [1, 2, 3]);
        let bad = crate::native::unrt_call_native(
            b"list\0".as_ptr(), 4, b"resize\0".as_ptr(), 6,
            [unrt_obj_int(1)].as_ptr(), 1,
        );
        assert_eq!(display(bad), "expected 'list' object");
    }

    #[test]
    fn str_concat_helper_matches_vm() {
        let a = crate::strings::str_new("ab".into());
        let b = crate::strings::str_new("cd".into());
        assert_eq!(display(crate::strings::unrt_str_concat(a, b)), "abcd");
        assert_eq!(
            display(crate::strings::unrt_str_concat(a, unrt_obj_int(1))),
            "unrt: str concat needs two Str",
        );
    }
}
