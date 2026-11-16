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
    let entry = crate::jit_cache::get_jit_entry(&crate::jit_cache::unbc_key(
        Rc::as_ptr(program) as usize,
        proto_index,
    ))?;
    let mut ints = Vec::with_capacity(args.values.len() + if is_class_method { 1 } else { 0 });
    if is_class_method {
        let Some(receiver) = bound_receiver else {
            return None;
        };
        ints.push(Rc::as_ptr(&receiver) as i64);
    }
    for arg in &args.values {
        match obj::payload(arg) {
            crate::runtime::obj::Payload::Int(v) => ints.push(v),
            _ => return None,
        }
    }
    if ints.len() != entry.arity as usize {
        return None;
    }

    let res = crate::vm::jit_call(entry, &ints);
    match res.tag {
        crate::jit_cache::TAG_INT => Some(obj::int(res.value)),
        crate::jit_cache::TAG_FLOAT => Some(obj::float(f64::from_bits(res.value as u64))),
        crate::jit_cache::TAG_BAIL => None, // JIT declined at runtime. Interpreter handles normally.
        crate::jit_cache::TAG_BOOL => Some(obj::boolean(res.value != 0)), // Bool-typed return
        // An out-of-contract tag means the entry is not from this JIT build;
        // fall back to the interpreter instead of panicking.
        _ => None,
    }
}
