//! `types` — fixed-width integer and floating-point constructors.
//!
//! Un has a single `int` (i64) and a single `float` (f64) payload. The
//! `types` module exposes checked conversions to fixed-width ranges instead
//! of adding a new payload kind: range checks run at conversion time only,
//! and the result is an ordinary Int/Float. Arithmetic afterwards is the
//! usual wrapping i64/f64 behaviour, identical to `int`/`float`.
//!
//! Constructors accept Int, Float (truncated toward zero) and Bool. Range
//! violations and non-numeric input produce an error object.
//!
//! - `u8`, `u16`, `u32`, `u64` — 0..=255 / 0..=65535 / 0..=4294967295 /
//!   0..=9223372036854775807 (Un ints are signed 64-bit, so `u64` caps at
//!   `i64::MAX`; a wider unsigned integer type is not available).
//! - `i8`, `i16`, `i32`, `i64` — -128..=127, -32768..=32767,
//!   -2147483648..=2147483647, -9223372036854775808..=9223372036854775807.
//! - `f32` — Int/Float/Bool converted to the f32 value set (round to nearest
//!   even); values that do not fit become ±inf. Non-finite input stays
//!   non-finite.
//! - `f64` — Int/Float/Bool converted to f64; Int values above 2^53 lose
//!   precision silently (the usual f64 limit).

use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use super::{arg, native};

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    let one = |name: &str| vec![arg(name, true, false, false)];
    vec![
        ("u8".to_owned(), native(closure, "u8", one("value"), u8_impl)),
        ("u16".to_owned(), native(closure, "u16", one("value"), u16_impl)),
        ("u32".to_owned(), native(closure, "u32", one("value"), u32_impl)),
        ("u64".to_owned(), native(closure, "u64", one("value"), u64_impl)),
        ("i8".to_owned(), native(closure, "i8", one("value"), i8_impl)),
        ("i16".to_owned(), native(closure, "i16", one("value"), i16_impl)),
        ("i32".to_owned(), native(closure, "i32", one("value"), i32_impl)),
        ("i64".to_owned(), native(closure, "i64", one("value"), i64_impl)),
        ("f32".to_owned(), native(closure, "f32", one("value"), f32_impl)),
        ("f64".to_owned(), native(closure, "f64", one("value"), f64_impl)),
    ]
}

/// Extract the input as an i64. Floats truncate toward zero; values that do
/// not fit i64 (or are NaN) are rejected before the cast (`as` would
/// saturate instead of failing).
fn int_value(scope: &Scope) -> Result<i64, Obj> {
    let value = scope::get(scope, "value");
    match obj::payload(&value) {
        Payload::Int(v) => Ok(v),
        Payload::Bool(b) => Ok(if b { 1 } else { 0 }),
        Payload::Float(f) => {
            let t = f.trunc();
            // 2^63 == 9223372036854775808.0 is exactly representable in f64.
            if t >= 9223372036854775808.0 || t < -9223372036854775808.0 || t.is_nan() {
                return Err(obj::err("types: cannot convert NaN or infinite float to an integer"));
            }
            Ok(t as i64)
        }
        _ => Err(obj::err("types: expected an integer or floating-point value")),
    }
}

fn int_ctor(name: &str, scope: &Scope, min: i64, max: i64) -> Obj {
    match int_value(scope) {
        Ok(v) => {
            if v < min || v > max {
                return obj::err(format!(
                    "{name}: value {v} out of range ({min}..={max})"
                ));
            }
            obj::int(v)
        }
        Err(e) => e,
    }
}

fn u8_impl(scope: &Scope) -> Obj {
    int_ctor("u8", scope, 0, 255)
}
fn u16_impl(scope: &Scope) -> Obj {
    int_ctor("u16", scope, 0, 65535)
}
fn u32_impl(scope: &Scope) -> Obj {
    int_ctor("u32", scope, 0, 4294967295)
}
fn u64_impl(scope: &Scope) -> Obj {
    // Un ints are signed 64-bit, so the u64 range is capped at i64::MAX
    // (see the module docs).
    int_ctor("u64", scope, 0, i64::MAX)
}
fn i8_impl(scope: &Scope) -> Obj {
    int_ctor("i8", scope, -128, 127)
}
fn i16_impl(scope: &Scope) -> Obj {
    int_ctor("i16", scope, -32768, 32767)
}
fn i32_impl(scope: &Scope) -> Obj {
    int_ctor("i32", scope, -2147483648, 2147483647)
}
fn i64_impl(scope: &Scope) -> Obj {
    int_ctor("i64", scope, i64::MIN, i64::MAX)
}

fn float_value(scope: &Scope) -> Result<f64, Obj> {
    let value = scope::get(scope, "value");
    match obj::payload(&value) {
        Payload::Int(v) => Ok(v as f64),
        Payload::Bool(b) => Ok(if b { 1.0 } else { 0.0 }),
        Payload::Float(v) => Ok(v),
        _ => Err(obj::err("types: expected an integer or floating-point value")),
    }
}

fn f32_impl(scope: &Scope) -> Obj {
    match float_value(scope) {
        // f64 -> f32 -> f64 round-trip rounds to the f32 value set.
        Ok(v) => obj::float(v as f32 as f64),
        Err(e) => e,
    }
}

fn f64_impl(scope: &Scope) -> Obj {
    match float_value(scope) {
        Ok(v) => obj::float(v),
        Err(e) => e,
    }
}
