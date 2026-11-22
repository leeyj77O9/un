
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use super::{ess, native};

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    let mut out: Vec<(String, Obj)> = Vec::new();
    out.push(("bin".to_owned(), native(closure, "bin", vec![ess("value")], bin_impl)));
    out.push(("oct".to_owned(), native(closure, "oct", vec![ess("value")], oct_impl)));
    out.push(("hex".to_owned(), native(closure, "hex", vec![ess("value")], hex_impl)));
    out.push(("chr".to_owned(), native(closure, "chr", vec![ess("code")], chr_impl)));
    out.push(("ord".to_owned(), native(closure, "ord", vec![ess("ch")], ord_impl)));
    out.push(("ascii".to_owned(), native(closure, "ascii", vec![ess("value")], ascii_impl)));
    out
}
fn int_of(scope: &Scope, name: &str, message: &str) -> Result<i64, Obj> {
    match obj::payload(&scope::get(scope, name)) {
        Payload::Int(v) => Ok(v),
        _ => Err(obj::err(message)),
    }
}
fn bin_impl(scope: &Scope) -> Obj {
    match int_of(scope, "value", "bin: expected int") {
        Ok(v) if v < 0 => obj::str(&format!("-0b{:b}", v.wrapping_neg())),
        Ok(v) => obj::str(&format!("0b{v:b}")),
        Err(e) => e,
    }
}
fn oct_impl(scope: &Scope) -> Obj {
    match int_of(scope, "value", "oct: expected int") {
        Ok(v) if v < 0 => obj::str(&format!("-0o{:o}", v.wrapping_neg())),
        Ok(v) => obj::str(&format!("0o{v:o}")),
        Err(e) => e,
    }
}
fn hex_impl(scope: &Scope) -> Obj {
    match int_of(scope, "value", "hex: expected int") {
        Ok(v) if v < 0 => obj::str(&format!("-0x{:x}", v.wrapping_neg())),
        Ok(v) => obj::str(&format!("0x{v:x}")),
        Err(e) => e,
    }
}
fn chr_impl(scope: &Scope) -> Obj {
    let code = scope::get(scope, "code");
    let code = match obj::payload(&code) {
        Payload::Int(v) => v,
        _ => return obj::err("chr: expected int code"),
    };
    if code < 0 || code > 0x10FFFF {
        return obj::err("chr: code point out of range (0..1114111)");
    }

    match char::from_u32(code as u32) {
        Some(c) => obj::str(&c.to_string()),
        None => obj::panic(
            "error",
            "A valid UTF32 value is between 0x000000 and 0x10ffff, inclusive, and should not \
             include surrogate codepoint values (0x00d800 ~ 0x00dfff). (Parameter 'utf32')",
        ),
    }
}
fn ord_impl(scope: &Scope) -> Obj {
    let ch = scope::get(scope, "ch");
    let s = match obj::payload(&ch) {
        Payload::Str(s) => s,
        _ => return obj::err("ord: expected str"),
    };
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return obj::err("ord: expected a single character, got empty string");
    };
    if chars.next().is_some() {
        return obj::err("ord: expected a single character, got multiple characters");
    }

    obj::int(first as u32 as i64)
}
fn ascii_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    match obj::payload(&value) {
        Payload::Str(s) => match s.encode_utf16().next() {
            Some(unit) if s.encode_utf16().count() == 1 => obj::int(unit as i64),
            _ => obj::err("ascii: expected str"),
        },
        _ => obj::err("ascii: expected str"),
    }
}
