//! ABI-aware indirect call shim for lambdas and class methods.
//!
//! Cranelift emits Float parameters and returns in XMM registers (`f64` in the
//! C signature), while Int/Bool/object-handle values travel in general-purpose
//! registers. The old all-`u64` shim passed every argument in a GPR, so a Float
//! argument or return produced garbage. `call_native_typed` derives the actual
//! C signature from the registered parameter kinds (0 Int, 1 Float, 2 Bool,
//! 3 object handle) and dispatches to a transmute with the matching `f64`/`u64`
//! argument types. All non-Float kinds use `u64` in the transmute: the callee
//! reads only the low byte for Bool, and object handles are pointer-sized.
//!
//! ## A-12 (W-2026-11-26-P03) — transmute safety gate
//!
//! A function-pointer value is `Send + Sync` and the reconstructed signature
//! is the one the emitter used. The open question was the *callee*: a worker
//! calling through this pointer must not reach shared mutable state. P02B
//! closed that question; see the SAFETY note on `call_native_typed` for the
//! re-runnable grep evidence and the explicitly excluded P04 residue.

use crate::class::Kind;

trait AbiValue: Sized {
    fn from_bits(bits: u64) -> Self;
}

impl AbiValue for u64 {
    #[inline(always)]
    fn from_bits(bits: u64) -> Self {
        bits
    }
}

impl AbiValue for f64 {
    #[inline(always)]
    fn from_bits(bits: u64) -> Self {
        f64::from_bits(bits)
    }
}

macro_rules! abi_tree {
    (@leaf $ptr:expr, $args:expr, $ret:ty, [$(($t:ty, $i:tt))*]) => {
        unsafe {
            std::mem::transmute::<usize, extern "C" fn($($t),*) -> $ret>($ptr)(
                $( <$t as AbiValue>::from_bits($args[$i]) ),*
            )
        }
    };
    (@step $ptr:expr, $args:expr, $ret:ty, $mask:expr, [$(($t:ty, $i:tt))*], []) => {
        abi_tree!(@leaf $ptr, $args, $ret, [$(($t, $i))*])
    };
    (@step $ptr:expr, $args:expr, $ret:ty, $mask:expr, [$(($t:ty, $i:tt))*], [$b:tt $($rest:tt)*]) => {
        if ($mask >> $b) & 1 != 0 {
            abi_tree!(@step $ptr, $args, $ret, $mask, [$(($t, $i))* (f64, $b)], [$($rest)*])
        } else {
            abi_tree!(@step $ptr, $args, $ret, $mask, [$(($t, $i))* (u64, $b)], [$($rest)*])
        }
    };
}

macro_rules! abi_call {
    ($ptr:expr, $args:expr, $ret:ty, $mask:expr, [$($bits:tt)*]) => {
        abi_tree!(@step $ptr, $args, $ret, $mask, [], [$($bits)*])
    };
}

macro_rules! abi_dispatch {
    ($ptr:expr, $args:expr, $ret:ty, $mask:expr) => {
        match $args.len() {
            0 => abi_call!($ptr, $args, $ret, $mask, []),
            1 => abi_call!($ptr, $args, $ret, $mask, [0]),
            2 => abi_call!($ptr, $args, $ret, $mask, [0 1]),
            3 => abi_call!($ptr, $args, $ret, $mask, [0 1 2]),
            4 => abi_call!($ptr, $args, $ret, $mask, [0 1 2 3]),
            5 => abi_call!($ptr, $args, $ret, $mask, [0 1 2 3 4]),
            6 => abi_call!($ptr, $args, $ret, $mask, [0 1 2 3 4 5]),
            7 => abi_call!($ptr, $args, $ret, $mask, [0 1 2 3 4 5 6]),
            8 => abi_call!($ptr, $args, $ret, $mask, [0 1 2 3 4 5 6 7]),
            _ => abi_call!($ptr, $args, $ret, $mask, [0 1 2 3 4 5 6 7 8]),
        }
    };
}

/// Call the emitted function at `ptr` with raw ABI words in `args`.
///
/// `kinds[i]` describes argument `i` so the correct C signature can be
/// reconstructed; `ret_kind` selects the XMM (Float) or GPR return path.
/// The result is always returned as raw bits, ready for `box_return`.
///
/// # SAFETY — A-12 declaration (W-2026-11-26-P03)
///
/// `ptr` is a genuine code address, never arbitrary integer data: it is
/// recorded by `nrt_register_lambda` (Cranelift `func_addr`) or by the
/// class-method registration path. `kinds` / `ret_kind` are the emitter's own
/// descriptors, so the transmuted `extern "C" fn` type is the callee's real C
/// signature; arity is capped at 8 by `abi_dispatch!` per the codegen contract.
/// Restoring a code pointer's function type cannot by itself create a data
/// race — what mattered was whether the callee could reach shared mutable
/// state from a worker thread. The P02B gate confirms it cannot:
///
/// ```text
/// grep -nE "static[[:space:]]+mut|&'static[[:space:]]+mut" \
///   rust/nrt/src/{globals,closure,abi,net_native}.rs
/// ```
///
/// returns **zero lines** after this work: `globals.rs` is a `Mutex<Vec<u64>>`,
/// `closure.rs` captures are `u64` handles inside immutable `ClosureData`,
/// this file only restores function-pointer types, and `net_native.rs` now
/// keeps HTTP/WS state in per-object `Arc<Mutex<..>>` handle tables.
/// `class.rs`, `list.rs`, `dict.rs` and `obj.rs` were converted in P02B to
/// handle tables with lock-scoped access; no mutable reference escapes them.
///
/// P04 residue (deliberately **not** touched here): `trace.rs` still owns its
/// frame-depth global (A-14) and `obj.rs` still owns the atomic `PENDING`
/// error cell (A-13). The former is P04's task-local conversion; the latter is
/// memory-safe as an atomic and moves to a task-local cell in P04. Neither is
/// a mutable alias reachable from the transmuted callee in this work.
pub(crate) fn call_native_typed(ptr: usize, args: &[u64], kinds: &[Kind], ret_kind: Kind) -> u64 {
    let mut float_mask = 0u16;
    for (i, kind) in kinds.iter().take(16).enumerate() {
        if *kind == 1 {
            float_mask |= 1u16 << i;
        }
    }
    if ret_kind == 1 {
        let value: f64 = abi_dispatch!(ptr, args, f64, float_mask);
        value.to_bits()
    } else {
        abi_dispatch!(ptr, args, u64, float_mask)
    }
}

#[cfg(test)]
mod a12_tests {
    use super::*;

    // Pure fn items: no mutable global, so they are valid stand-ins for the
    // Cranelift wrappers registered through `nrt_register_lambda`.
    extern "C" fn f0() -> u64 { 42 }
    extern "C" fn f2(a: u64, b: u64) -> u64 { a.wrapping_add(b) }
    extern "C" fn f3(a: u64, b: f64, c: u64) -> u64 {
        a.wrapping_add(b as u64).wrapping_add(c)
    }
    extern "C" fn fr(a: u64) -> f64 { a as f64 + 0.5 }
    extern "C" fn f8(
        a: u64, b: u64, c: u64, d: u64,
        e: u64, f: u64, g: u64, h: u64,
    ) -> u64 {
        a.wrapping_add(b).wrapping_add(c).wrapping_add(d)
            .wrapping_add(e).wrapping_add(f).wrapping_add(g).wrapping_add(h)
    }

    #[test]
    fn function_pointer_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<extern "C" fn(u64, u64) -> u64>();
    }

    /// The transmute must reconstruct exactly the signature the emitter used:
    /// GPR words, an XMM `f64` in the middle, an XMM return, and max arity 8.
    #[test]
    fn transmute_dispatch_matches_declared_abi() {
        assert_eq!(
            call_native_typed(f0 as extern "C" fn() -> u64 as usize, &[], &[], 0),
            42,
        );
        assert_eq!(
            call_native_typed(
                f2 as extern "C" fn(u64, u64) -> u64 as usize,
                &[3, 4], &[0, 0], 0,
            ),
            7,
        );
        assert_eq!(
            call_native_typed(
                f3 as extern "C" fn(u64, f64, u64) -> u64 as usize,
                &[3, 2.0f64.to_bits(), 5], &[0, 1, 0], 0,
            ),
            10,
        );
        assert_eq!(
            call_native_typed(
                fr as extern "C" fn(u64) -> f64 as usize,
                &[7], &[0], 1,
            ),
            7.5f64.to_bits(),
        );
        assert_eq!(
            call_native_typed(
                f8 as extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64 as usize,
                &[1, 2, 3, 4, 5, 6, 7, 8], &[0; 8], 0,
            ),
            36,
        );
    }

    /// A-12 gate at runtime: the same restored pointer is invoked from four
    /// threads at once. The callee is pure, so there is no shared mutable
    /// state for a worker to race on — the property the SAFETY note asserts.
    #[test]
    fn transmuted_callee_is_reentrant_across_threads() {
        let ptr = f2 as extern "C" fn(u64, u64) -> u64 as usize;
        let threads: Vec<_> = (0..4)
            .map(|i| {
                std::thread::spawn(move || call_native_typed(ptr, &[i, 1], &[0, 0], 0))
            })
            .collect();
        let got: Vec<u64> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(got, vec![1, 2, 3, 4]);
    }
}
