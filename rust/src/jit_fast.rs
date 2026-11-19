//! Shared JIT fast-path helper.
//!
//! Routes Int-only Un function calls to the `.unbc` JIT translation when
//! the arguments are compatible (all positional, exactly `arity` Ints, and the
//! compiled proto had none of the rejected shapes at compile time). Bailing to
//! the interpreter on tag=2 happens here: we just return `None` and the
//! caller resumes the stock dispatch.

use std::rc::Rc;

use crate::bytecode::Program;
use crate::runtime::function::Args;
use crate::runtime::obj::{self, Obj};

/// Attempt to dispatch `(program, proto_index)` through the JIT. `bound_receiver`
/// carries a class method's bound `self` (as an Int pointer); the JIT sees this
/// as an extra hidden Int arg 0 in the signature, making class methods appear
/// as plain Int-arity functions to the JIT.
///
/// Returns the produced Obj on success, `None` when the call must run in the interpreter.
/// Call a compile-mode core entry. All argument types were statically
/// verified at the call site; the only runtime freedom is which payload
/// carries the value. Mismatches decline to the interpreter.
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
                param_tags |= 1i64 << (32 + pos);
                ints.push(v.to_bits() as i64);
            }
            (crate::jit_cache::ArgKind::Float, crate::runtime::obj::Payload::Int(v)) => {
                // int flowing into a float parameter promotes, like the interpreter.
                param_tags |= 1i64 << (32 + pos);
                ints.push((v as f64).to_bits() as i64);
            }
            (crate::jit_cache::ArgKind::Bool, crate::runtime::obj::Payload::Bool(b)) => {
                param_tags |= 1i64 << (40 + pos);
                ints.push(if b { 1 } else { 0 });
            }
            _ => return None,
        }
    }
    if ints.len() != entry.arity as usize {
        return None;
    }
    let res = crate::vm::jit_call(entry, &ints, param_tags);
    match res.tag {
        crate::jit_cache::TAG_INT => Some(obj::int(res.value)),
        crate::jit_cache::TAG_FLOAT => Some(obj::float(f64::from_bits(res.value as u64))),
        crate::jit_cache::TAG_BOOL => Some(obj::boolean(res.value != 0)),
        crate::jit_cache::TAG_NONE => Some(obj::none()),
        _ => None,
    }
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
    // Compile-mode TIR entries bypass UV_JIT: `un native` asked for them.
    let tir_entry = crate::jit_cache::get_tir_entry(program.id, proto_index as u32);
    // Two kinds of JIT entries share `jit_cache`: the bytecode JIT (lazy,
    // UV_JIT-gated) and the compile-mode TIR backend (eager, always on).
    if let Some(entry) = tir_entry {
        return try_call_tir_entry(entry, args, proto_index, program);
    }
    let entry = match crate::jit_cache::get_jit_entry(program.id, proto_index as u32) {
        Some(entry) => Some(entry),
        None => {
            // Compile lazily on first call: the eager pre-pass at
            // program-load paid the compile cost for protos that may never
            // run. Bailing to the interpreter is free because the entry
            // table simply doesn't have the function yet — but we want it to
            // have it, so try the compile once per program.
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
    // Two kinds of JIT entries share `jit_cache`: the bytecode JIT (lazy,
    // UV_JIT-gated, `arg_kinds` empty) and the compile-mode TIR backend
    // (eager, always on, `arg_kinds` present). The TIR path already returned
    // above when an entry exists, so reaching here means bytecode JIT.
    let mut ints = Vec::with_capacity(args.values.len() + if is_class_method { 1 } else { 0 });
    // Bit p of the JIT's hidden `param_tags` argument: argument p (in
    // `ints` order) is a borrowed IntVec arena handle, not a plain Int.
    let mut param_tags: i64 = 0;
    // Argument lists are materialised into the JIT's IntVec arena because the
    // interpreter cannot pass Obj references through Cranelift. The arena is
    // a copy, so any mutation the callee makes would be lost — unless we
    // copy the contents back into the caller's list objects after the call.
    // `list_args` records (original Un list object, arena handle) pairs.
    let mut list_args: Vec<(Obj, i64)> = Vec::new();
    // Helper for the early-decline paths below: free whatever was already
    // materialised so the arena stays flat. The arena never reuses ids, so
    // this is only about bounded growth, not correctness.
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
                // Ints >= ID_BASE could alias a live arena handle; seeing one
                // means the caller cannot tell JIT a list handle from a
                // user-constructed Int, so the JIT must not run.
                if v >= crate::runtime::int_list::id_base() {
                    return None;
                }
                ints.push(v);
            }
            crate::runtime::obj::Payload::List(_) => {
                // Nested homogeneous lists ([[1,2],[3,4],...]) enter the
                // Int-JIT as a KIND_OUTER arena handle; flat homogeneous
                // Int/Bool lists keep the flat path. Anything else
                // declines and the caller runs the interpreter.
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
    // Copy mutated arena lists back into the caller's Un list objects — but
    // only when the JIT actually produced a value. On a runtime bail the
    // interpreter re-runs the function from scratch against the untouched
    // originals, so back-syncing would leak partial mutations into it.
    //
    // If the callee returned an argument list handle itself (`-> xs`), the
    // identity of the result IS the original object; remember it so the
    // TAG_INTLIST arm does not rebuild it from a dead handle (which would
    // fall back to the interpreter and double-apply the mutations).
    //
    // Two phases: first validate every arena tree, then write everything
    // back. A poisoned or dead subtree in ANY argument list abandons the
    // whole call — the interpreter re-runs it on the pristine originals, so
    // semantics stay exactly the interpreter's.
    let mut returned_arg_identity: Option<Obj> = None;
    if res.tag != crate::jit_cache::TAG_BAIL && !list_args.is_empty() {
        let all_ok = list_args
            .iter()
            .all(|(_, handle)| crate::runtime::int_list::snapshot(*handle).is_some());
        if !all_ok {
            drain(list_args);
            return None; // the interpreter re-runs the function
        }
        for (original, handle) in list_args {
            let returned = res.tag == crate::jit_cache::TAG_INTLIST && res.value == handle;
            // The whole tree was validated above; write-back is infallible
            // from here on. (Belt and braces: ignore the result rather than
            // panic inside the VM.)
            let _ = crate::runtime::int_list::sync_into(&original, handle);
            if returned {
                returned_arg_identity = Some(original.clone());
            }
            crate::runtime::int_list::un_list_int_free(handle);
        }
    } else {
        drain(list_args);
    }
    match res.tag {
        crate::jit_cache::TAG_INT => Some(obj::int(res.value)),
        crate::jit_cache::TAG_FLOAT => Some(obj::float(f64::from_bits(res.value as u64))),
        crate::jit_cache::TAG_BAIL => None, // JIT declined at runtime. Interpreter handles normally.
        crate::jit_cache::TAG_BOOL => Some(obj::boolean(res.value != 0)), // Bool-typed return
        crate::jit_cache::TAG_NONE => Some(obj::none()), // implicit-None return (no value)
        // The callee returned a raw IntVec handle: materialise it back to an
        // Un list and drop the handle. If the handle vanished or turned
        // poisoned meanwhile, bail (interpreter re-runs).
        crate::jit_cache::TAG_INTLIST => {
            if let Some(identity) = returned_arg_identity {
                // The callee returned the argument list itself; the back-sync
                // above already copied its final contents into the original
                // object, and the interpreter would return the same identity.
                return Some(identity);
            }
            let items = crate::runtime::int_list::snapshot(res.value)?;
            crate::runtime::int_list::un_list_int_free(res.value);
            Some(crate::runtime::collections::list(items))
        }
        // An out-of-contract tag means the entry is not from this JIT build;
        // fall back to the interpreter instead of panicking.
        _ => None,
    }
}
