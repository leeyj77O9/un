//! Tagged-pointer representation of primitives for the new runtime path.
//!
//! The existing runtime uses `Rc<RefCell<ObjData>>` for everything. For
//! Int-heavy workloads that representation doubles allocation traffic. The
//! new representation stores small integers and boolean/none/null values
//! directly in a 64-bit tagged word, and only falls back to an Rc when a
//! heap object is genuinely needed.
//!
//! Bit layout (64 bits):
//!
//!   tag 0x0 : pointer to heap-allocated `Rc<RefCell<HeapData>>`
//!   tag 0x1 : 61-bit signed Int, sign-extended in lower 61 bits
//!   tag 0x2 : Float (NaN-boxed); exponent field is 0x7FF
//!   tag 0x3 : Bool; value 0 = false, 1 = true
//!   tag 0x4 : None singleton
//!   tag 0x5 : Null singleton
//!   tag 0x6 : reserved (stack-allocated marker)
//!   tag 0x7 : reserved (future Int-expanded variants, e.g. char)
//!
//! Predicate `is_int` = `(bits & 0x7) == 0x1`, etc.
//!
//! Heap objects keep `Rc<RefCell<HeapData>>` layout; the tag detects them
//! for free because reference counts usually end in the low bits ??but more
//! precisely, pointer tagging uses the low 3 bits which are 0 for Rust's
//! allocator alignment of at least 8 bytes on x86_64.
//!
//! Stage 1 goal: prove this encodes Int/Bool/None/null correctly and that
//! arithmetic works correctly. Full runtime port follows in later stages.

use std::rc::Rc;

// -----------------------------------------------------------------------------
// Tagged word
// -----------------------------------------------------------------------------

/// A word-sized tagged value. Opaque to the outside world.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Value(u64);

pub const TAG_HEAPOBJ: u64 = 0;
pub const TAG_INT: u64 = 1;
pub const TAG_FLOAT: u64 = 2;
pub const TAG_BOOL: u64 = 3;
pub const TAG_NONE: u64 = 4;
pub const TAG_NULL: u64 = 5;

const TAG_MASK: u64 = 0x7;
const TAG_NONE_VALUE: u64 = TAG_NONE;
const TAG_NULL_VALUE: u64 = TAG_NULL;

/// Largest positive signed Int that fits in 61 bits (inclusive).
pub const INT_MAX: i64 = (1i64 << 60) - 1;
/// Smallest negative signed Int that fits in 61 bits (inclusive).
pub const INT_MIN: i64 = -(1i64 << 60) + 1; // keep symmetric with INT_MAX

impl Value {
    #[inline]
    pub const fn from_word(bits: u64) -> Self {
        Self(bits)
    }

    #[inline]
    pub const fn into_word(self) -> u64 {
        self.0
    }

    #[inline]
    pub fn tag(&self) -> u64 {
        self.0 & TAG_MASK
    }

    #[inline]
    pub fn is_int(&self) -> bool {
        self.tag() == TAG_INT
    }

    #[inline]
    pub fn is_float(&self) -> bool {
        self.tag() == TAG_FLOAT
    }

    #[inline]
    pub fn is_bool(&self) -> bool {
        self.tag() == TAG_BOOL
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        self.tag() == TAG_NONE
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.tag() == TAG_NULL
    }

    #[inline]
    pub fn is_heap(&self) -> bool {
        self.tag() == TAG_HEAPOBJ
    }

    // ------ Int ------

    #[inline]
    pub const fn from_int(n: i64) -> Self {
        assert!(n >= INT_MIN && n <= INT_MAX, "int out of range");
        // value biased by 0 so negative numbers are preserved bit-pattem
        let shifted = (n as u64) << 3;
        Self(shifted | TAG_INT)
    }

    #[inline]
    pub fn as_int(&self) -> Option<i64> {
        if self.is_int() {
            // Sign-extend the 61-bit payload: bit 60 is sign
            let raw = self.0 & !TAG_MASK;
            Some((raw as i64) >> 3)
        } else {
            None
        }
    }

    // ------ Int arithmetic (tagged representation) ------

    #[inline]
    pub fn add(&self, other: &Self) -> Option<Self> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Some(Self::from_int(a.wrapping_add(b))),
            _ => None,
        }
    }

    #[inline]
    pub fn sub(&self, other: &Self) -> Option<Self> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Some(Self::from_int(a.wrapping_sub(b))),
            _ => None,
        }
    }

    #[inline]
    pub fn mul(&self, other: &Self) -> Option<Self> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Some(Self::from_int(a.wrapping_mul(b))),
            _ => None,
        }
    }

    #[inline]
    pub fn idiv(&self, other: &Self) -> Option<Self> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) if b != 0 => Some(Self::from_int(a.wrapping_div(b))),
            _ => None,
        }
    }

    #[inline]
    pub fn modulo(&self, other: &Self) -> Option<Self> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) if b != 0 => Some(Self::from_int(a.wrapping_rem(b))),
            _ => None,
        }
    }

    #[inline]
    pub fn lt(&self, other: &Self) -> Option<Self> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Some(Self::from_bool(a < b)),
            _ => None,
        }
    }

    #[inline]
    pub fn le(&self, other: &Self) -> Option<Self> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Some(Self::from_bool(a <= b)),
            _ => None,
        }
    }

    #[inline]
    pub fn gt(&self, other: &Self) -> Option<Self> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Some(Self::from_bool(a > b)),
            _ => None,
        }
    }

    #[inline]
    pub fn ge(&self, other: &Self) -> Option<Self> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Some(Self::from_bool(a >= b)),
            _ => None,
        }
    }

    #[inline]
    pub fn eq(&self, other: &Self) -> Option<Self> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Some(Self::from_bool(a == b)),
            _ => None,
        }
    }

    #[inline]
    pub fn ne(&self, other: &Self) -> Option<Self> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Some(Self::from_bool(a != b)),
            _ => None,
        }
    }

    // ------ Bool ------

    #[inline]
    pub const fn from_bool(b: bool) -> Self {
        Self((if b { 1u64 } else { 0u64 }) << 3 | TAG_BOOL)
    }

    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        if self.is_bool() {
            Some((self.0 & 0x8) != 0)
        } else {
            None
        }
    }

    // ------ None / Null ------

    #[inline]
    pub const fn none() -> Self {
        Self(TAG_NONE_VALUE)
    }

    #[inline]
    pub const fn null() -> Self {
        Self(TAG_NULL_VALUE)
    }
}

// -----------------------------------------------------------------------------
// Heap object
// -----------------------------------------------------------------------------

/// A heap payload. Strings, lists, dicts, etc. live here.
///
/// Each variant stores `Rc<RefCell<??>` so that the existing `Obj` becomes a
/// thin wrapper around this. Stage 1 keeps this out of `Rc<RefCell<??>` for
/// simplicity ??later stages can special-case lists/dicts.
#[derive(Clone, Debug)]
pub struct HeapData {
    pub type_name: Rc<str>,
}

pub type HeapObj = Rc<std::cell::RefCell<HeapData>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_roundtrip() {
        for n in [0i64, 1, 2, 100, 1000, INT_MAX, INT_MIN, -1, -100, -1000, INT_MAX / 2, INT_MIN / 2] {
            let v = Value::from_int(n);
            assert!(v.is_int());
            assert_eq!(v.as_int().unwrap(), n);
        }
    }

    #[test]
    fn int_max_boundary() {
        let max = Value::from_int(INT_MAX);
        assert!(max.is_int());
        assert_eq!(max.as_int().unwrap(), INT_MAX);
        let min = Value::from_int(INT_MIN);
        assert!(min.is_int());
        assert_eq!(min.as_int().unwrap(), INT_MIN);
    }

    #[test]
    fn bool_roundtrip() {
        assert!(Value::from_bool(true).is_bool());
        assert_eq!(Value::from_bool(true).as_bool(), Some(true));
        assert!(Value::from_bool(false).is_bool());
        assert_eq!(Value::from_bool(false).as_bool(), Some(false));
    }

    #[test]
    fn none_null_distinguised() {
        assert!(Value::none().is_none());
        assert!(!Value::none().is_null());
        assert!(Value::null().is_null());
        assert!(!Value::null().is_none());
    }

    #[test]
    fn tags_do_not_collide() {
        assert_eq!(Value::from_int(0).tag(), TAG_INT);
        assert_eq!(Value::from_bool(true).tag(), TAG_BOOL);
        assert_eq!(Value::none().tag(), TAG_NONE);
        assert_eq!(Value::null().tag(), TAG_NULL);
    }
}


#[cfg(test)]
mod tests_arithmetic {
    use super::*;

    #[test]
    fn add_sub_mul() {
        let two = Value::from_int(2);
        let three = Value::from_int(3);
        assert_eq!(two.add(&three).and_then(|v| v.as_int()), Some(5));
        assert_eq!(three.sub(&two).and_then(|v| v.as_int()), Some(1));
        assert_eq!(two.mul(&three).and_then(|v| v.as_int()), Some(6));
    }

    #[test]
    fn idiv_mod() {
        let ten = Value::from_int(10);
        let three = Value::from_int(3);
        assert_eq!(ten.idiv(&three).and_then(|v| v.as_int()), Some(3));
        assert_eq!(ten.modulo(&three).and_then(|v| v.as_int()), Some(1));
    }

    #[test]
    fn div_by_zero() {
        let ten = Value::from_int(10);
        let zero = Value::from_int(0);
        assert!(ten.idiv(&zero).is_none());
        assert!(ten.modulo(&zero).is_none());
    }

    #[test]
    fn comparisons() {
        let two = Value::from_int(2);
        let three = Value::from_int(3);
        assert_eq!(two.lt(&three).and_then(|v| v.as_bool()), Some(true));
        assert_eq!(two.le(&three).and_then(|v| v.as_bool()), Some(true));
        assert_eq!(three.gt(&two).and_then(|v| v.as_bool()), Some(true));
        assert_eq!(three.ge(&two).and_then(|v| v.as_bool()), Some(true));
        assert_eq!(two.eq(&two).and_then(|v| v.as_bool()), Some(true));
        assert_eq!(two.ne(&two).and_then(|v| v.as_bool()), Some(false));
    }

    #[test]
    fn bool_foo() {
        let t = Value::from_bool(true);
        let f = Value::from_bool(false);
        assert_ne!(t, f);
    }
}
