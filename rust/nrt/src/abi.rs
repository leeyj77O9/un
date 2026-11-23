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
