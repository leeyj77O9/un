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

/// Attempt to dispatch `(program, proto_index)` through the JIT. Returns the
/// produced Obj on success, `None` when the call must run in the interpreter.
pub(crate) fn try_call_jit(program: &Rc<Program>, proto_index: usize, args: &Args) -> Option<Obj> {
    if !args.names.iter().all(|n| n.is_empty()) {
        return None;
    }
    let entry = crate::jit_cache::get_jit_entry(&crate::jit_cache::unbc_key(
        Rc::as_ptr(program) as usize,
        proto_index,
    ))?;
    if args.values.len() != entry.arity as usize {
        return None;
    }
    let mut ints = Vec::with_capacity(args.values.len());
    for arg in &args.values {
        match obj::payload(arg) {
            crate::runtime::obj::Payload::Int(v) => ints.push(v),
            _ => return None,
        }
    }
    let res = crate::vm::jit_call(entry, &ints);
    match res.tag {
        0 => Some(obj::int(res.value)),
        1 => Some(obj::float(f64::from_bits(res.value as u64))),
        2 => None, // JIT declined at runtime. Interpreter handles normally.
        3 => Some(obj::boolean(res.value != 0)), // Bool-typed return
        _ => unreachable!("jit_call unexpected tag {}", res.tag),
    }
}
