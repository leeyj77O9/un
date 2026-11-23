//! Standalone native runtime for `un build` executables.
//!
//! Implements the `unrt_*` / `un_math_*` C ABI used by Cranelift-emitted
//! object files. No dependency on the VM (`libunvm.a`) and no bytecode
//! interpreter: every value is a native object handle and every operation is
//! plain machine code.

mod abi;
mod class;
mod closure;
mod dict;
mod globals;
mod list;
mod math_native;
mod native;
mod obj;
mod regex;
mod trace;
mod strings;
