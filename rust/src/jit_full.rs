//! Obj* ABI for the full Cranelift JIT.
//!
//! `un2_abi_*` names are public symbols linked from the JIT module. Each
//! stage registers these via `JITBuilder::symbol(name, fn_ptr)`. Cranelift-
//! generated code calls them through the normal `call` instruction with
//! `IconstInterface` — the JIT just has to pass `Linkage::Import` and the
//! symbol table point.
//!
//! Stage 1: minimal ABI — enough to compile Int-only protos (sum loop).

use std::rc::Rc;

use crate::bytecode::Program;
use crate::runtime::obj::Obj;

/// Opaque pointer-sized word passed across the FFI boundary.
pub type Word = i64;
pub type ObjPtr = *const Obj;

/// Return-code sentinels for the flow code passed out through the out register.
pub const FLOW_OK: i64 = 0;
pub const FLOW_RETURN: i64 = 0;
pub const FLOW_PROPAGATE: i64 = 1;
pub const FLOW_ERROR: i64 = 2;
pub const FLOW_PANIC: i64 = 3;

/// Helpers exported to the JIT. Each minimal example returns a fresh Rc,
/// which the JIT's generated code can pass to `obj::int`. Once enough
/// helpers are in place, the .unbc → JIT pipeline becomes feasible.

/// Construct an Int Obj.
///
/// Returning a fresh Rc gives the raw caller ownership of the Rc's pointer;
/// the caller handles `Rc::into_raw` semantics.
#[no_mangle]
pub extern "C" fn un2_const_int(v: i64) -> Word {
    let o: Obj = crate::runtime::obj::int(v);
    Rc::into_raw(o) as Word
}

/// Construct `true` / `false`.
#[no_mangle]
pub extern "C" fn un2_const_bool(v: i64) -> Word {
    let o: Obj = crate::runtime::obj::boolean(v != 0);
    Rc::into_raw(o) as Word
}
