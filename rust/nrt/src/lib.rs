//! Standalone native runtime for `un build` executables.
//!
//! Implements the `unrt_*` / `un_math_*` C ABI used by Cranelift-emitted
//! object files. No dependency on the VM (`libunvm.a`) and no bytecode
//! interpreter: every value is a native object handle and every operation is
//! plain machine code.

mod abi;
mod bytes_native;
mod class;
mod closure;
mod dict;
mod dynops;
mod globals;
mod list;
mod math_native;
mod native;
mod net;
mod net_native;
mod obj;
mod regex;
// Opt-in parallel scheduler (W-2026-11-26-P05). Compiled into libnrt.a but no
// thread is spawned unless the opt-in path constructs a `Scheduler`.
pub mod scheduler;
mod trace;
mod strings;
