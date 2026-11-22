
use std::rc::Rc;

use crate::bytecode::Program;
use crate::runtime::function::Args;
use crate::runtime::obj::{self, Obj};

/// Convert the scalar tags of a `(value, tag)` JIT result into an `Obj`.
///
/// `TAG_INTLIST` is delegated to `on_intlist`: the `.unbc` path must snapshot
/// and free the returned handle (and may reuse a borrowed argument's identity),
/// while the TIR path has no list handles at all. Keeping that policy in the
/// caller is what lets both paths share this conversion.
fn jit_result_to_obj(
    res: crate::vm::UnJitResult,
    on_intlist: impl FnOnce(i64) -> Option<Obj>,
) -> Option<Obj> {
    match res.tag {
        crate::jit_cache::TAG_INT => Some(obj::int(res.value)),
        crate::jit_cache::TAG_FLOAT => Some(obj::float(f64::from_bits(res.value as u64))),
        crate::jit_cache::TAG_BOOL => Some(obj::boolean(res.value != 0)),
        crate::jit_cache::TAG_NONE => Some(obj::none()),
        crate::jit_cache::TAG_INTLIST => on_intlist(res.value),
        // `TAG_BAIL` and any unknown tag mean "fall back to the interpreter".
        _ => None,
    }
}

fn try_call_tir_entry(
    entry: crate::jit_cache::JitEntry,
    args: &Args,
    proto_index: usize,
    program: &Rc<Program>,
) -> Option<Obj> {
    if proto_index >= program.protos.len() {
        return None;
    }
    if !args.names.iter().all(|n| n.is_empty()) {
        return None;
    }
    if entry.arg_kinds.len() != args.values.len() {
        return None;
    }
    let mut ints = Vec::with_capacity(args.values.len());
    let mut param_tags = 0i64;
    for (pos, (kind, arg)) in entry.arg_kinds.iter().zip(&args.values).enumerate() {
        match (kind, obj::payload(arg)) {
            (crate::jit_cache::ArgKind::Int, crate::runtime::obj::Payload::Int(v)) => {
                ints.push(v);
            }
            (crate::jit_cache::ArgKind::Float, crate::runtime::obj::Payload::Float(v)) => {
                param_tags |= 1i64 << (crate::jit_cache::PARAM_MASK_FLOAT_BASE + pos);
                ints.push(v.to_bits() as i64);
            }
            (crate::jit_cache::ArgKind::Float, crate::runtime::obj::Payload::Int(v)) => {
                param_tags |= 1i64 << (crate::jit_cache::PARAM_MASK_FLOAT_BASE + pos);
                ints.push((v as f64).to_bits() as i64);
            }
            (crate::jit_cache::ArgKind::Bool, crate::runtime::obj::Payload::Bool(b)) => {
                param_tags |= 1i64 << (crate::jit_cache::PARAM_MASK_BOOL_BASE + pos);
                ints.push(if b { 1 } else { 0 });
            }
            _ => return None,
        }
    }
    if ints.len() != entry.arity as usize {
        return None;
    }
    let res = crate::vm::jit_call(entry, &ints, param_tags);
    // TIR entries never return an IntList handle, so there is nothing to free.
    jit_result_to_obj(res, |_| None)
}

pub(crate) fn try_call_jit(
    program: &Rc<Program>,
    proto_index: usize,
    args: &Args,
    bound_receiver: Option<Obj>,
) -> Option<Obj> {
    if !args.names.iter().all(|n| n.is_empty()) {
        return None;
    }
    let is_class_method = program
        .protos
        .get(proto_index)
        .and_then(|p| p.ext_type)
        .is_some();
    let tir_entry = crate::jit_cache::get_tir_entry(program.id, proto_index as u32);
    if let Some(entry) = tir_entry {
        return try_call_tir_entry(entry, args, proto_index, program);
    }
    let entry = match crate::jit_cache::get_jit_entry(program.id, proto_index as u32) {
        Some(entry) => Some(entry),
        None => {
            if crate::jit_cache::jit_lazy_attempted(program.id) {
                None
            } else {
                crate::jit_cache::mark_jit_lazy_attempted(program.id);
                crate::jit_bc::register_eligible(program, program.id);
                crate::jit_cache::get_jit_entry(program.id, proto_index as u32)
            }
        }
    };
    let entry = entry?;
    let mut ints = Vec::with_capacity(args.values.len() + if is_class_method { 1 } else { 0 });
    // Low bits `1 << pos` mark handle-valued (unsafe) parameters and are read
    // by `jit_bc`'s unsafe-param mask. For class methods `pos` already includes
    // the receiver slot (`PARAM_MASK_UNSAFE_SHIFT`). Float/Bool tags occupy the
    // disjoint high-bit space (`PARAM_MASK_FLOAT_BASE` / `_BOOL_BASE`) set in
    // `try_call_tir_entry`; never conflate the two ranges.
    let mut param_tags: i64 = 0;
    let mut list_args: Vec<(Obj, i64)> = Vec::new();
    fn drain(list_args: Vec<(Obj, i64)>) {
        for (_, handle) in list_args {
            crate::runtime::int_list::un_list_int_free(handle);
        }
    }
    if is_class_method {
        let Some(receiver) = bound_receiver else {
            return None;
        };
        ints.push(Rc::as_ptr(&receiver) as i64);
    }
    for arg in &args.values {
        let pos = ints.len();
        match obj::payload(arg) {
            crate::runtime::obj::Payload::Int(v) => {
                if v >= crate::runtime::int_list::id_base() {
                    return None;
                }
                ints.push(v);
            }
            crate::runtime::obj::Payload::List(_) => {
                match crate::runtime::int_list::materialize_nested(arg) {
                    Some(handle) => {
                        param_tags |= 1i64 << pos;
                        list_args.push((arg.clone(), handle));
                        ints.push(handle);
                    }
                    None => match crate::runtime::int_list::materialize(arg) {
                        Some(handle) => {
                            param_tags |= 1i64 << pos;
                            list_args.push((arg.clone(), handle));
                            ints.push(handle);
                        }
                        None => {
                            drain(list_args);
                            return None;
                        }
                    },
                }
            }
            _ => {
                drain(list_args);
                return None;
            }
        }
    }
    if ints.len() != entry.arity as usize {
        drain(list_args);
        return None;
    }

    let res = crate::vm::jit_call(entry, &ints, param_tags);
    let mut returned_arg_identity: Option<Obj> = None;
    if res.tag != crate::jit_cache::TAG_BAIL && !list_args.is_empty() {
        let all_ok = list_args
            .iter()
            .all(|(_, handle)| crate::runtime::int_list::snapshot(*handle).is_some());
        if !all_ok {
            drain(list_args);
            return None;
        }
        for (original, handle) in list_args {
            let returned = res.tag == crate::jit_cache::TAG_INTLIST && res.value == handle;
            let _ = crate::runtime::int_list::sync_into(&original, handle);
            if returned {
                returned_arg_identity = Some(original.clone());
            }
            crate::runtime::int_list::un_list_int_free(handle);
        }
    } else {
        drain(list_args);
    }
    jit_result_to_obj(res, move |handle| {
        if let Some(identity) = returned_arg_identity {
            return Some(identity);
        }
        let items = crate::runtime::int_list::snapshot(handle)?;
        crate::runtime::int_list::un_list_int_free(handle);
        Some(crate::runtime::collections::list(items))
    })
}
