//! `bytes` 값 생성/조회용 native 모듈과, 파일 바이너리 I/O가 공유하는 어댑터.
//!
//! ## D-098 계약 (RT-03 / CMP-05 연동)
//!
//! 이 모듈은 `bytes` 값의 런타임 표현을 **`Payload::Bytes(Vec<u8>)`** 로 만든다.
//! `bytes_obj` / `as_bytes` / `is_bytes` 세 어댑터가 표현 변환을 전담하고,
//! 나머지 12개 native 함수와 `fs.read_bytes` / `write_bytes`는 어댑터만 호출해
//! 표현과 무관하게 동작한다.
//!
//! - 생성: `bytes_obj`는 항상 `Payload::Bytes`를 만든다(`type_name = "bytes"`).
//! - 조회: `as_bytes`는 `Payload::Bytes`를 우선 읽고, 점진 전환을 위해 구 표현
//!   (`Payload::List` + `Int(0..=255)`)과 편의용 `Str`(UTF-8)도 그대로 받는다.
//! - 판정: `is_bytes`는 `Payload::Bytes`를 우선하고, 구 표현의 `type_name`도
//!   인정해 전환 전후 동작이 같다.
//!
//! RT-03이 `runtime/obj.rs`에 둔 브릿지 헬퍼(`bytes_view` / `bytes_cells` 등)는
//! 새 표현과 구 표현을 동시에 읽으므로, `b"..."` 리터럴의 `list[int]` 표현도
//! 계속 이 어댑터를 통해 안전하게 다룰 수 있다.

use crate::runtime::collections;
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};

use super::{arg, native, one, two};

/// `bytes` 값의 type name (`Payload::Bytes` 및 전환 전 구 표현 모두 공유).
pub const TYPE_NAME: &str = "bytes";

// ---------------------------------------------------------------------------
// 어댑터 (D-098: `Payload::Bytes` 우선, 구 표현 호환)
// ---------------------------------------------------------------------------

/// `Vec<u8>` → `bytes` Obj (`Payload::Bytes`).
pub fn bytes_obj(bytes: Vec<u8>) -> Obj {
    obj::obj_new(TYPE_NAME, Payload::Bytes(bytes))
}

/// `bytes` Obj → `Vec<u8>`.
///
/// `Payload::Bytes`를 우선 읽고, 점진 전환을 위해 구 표현(`Int(0..=255)` 리스트)
/// 과 편의용 문자열(UTF-8)도 그대로 받는다.
pub fn as_bytes(value: &Obj) -> Option<Vec<u8>> {
    match obj::payload(value) {
        Payload::Bytes(v) => Some(v.clone()),
        Payload::List(cells) => cells
            .borrow()
            .iter()
            .map(|item| match obj::payload(item) {
                Payload::Int(v) if (0..=255).contains(&v) => Some(v as u8),
                _ => None,
            })
            .collect(),
        Payload::Str(s) => Some(s.as_bytes().to_vec()),
        _ => None,
    }
}

/// 값이 `bytes`인지 판정.
///
/// `Payload::Bytes`면 참이고, 전환 전 구 표현(`type_name = "bytes"`)도 인정한다.
pub fn is_bytes(value: &Obj) -> bool {
    matches!(obj::payload(value), Payload::Bytes(_)) || &*obj::type_name(value) == TYPE_NAME
}

// ---------------------------------------------------------------------------
// 등록
// ---------------------------------------------------------------------------

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "from_str".to_owned(),
            native(closure, "from_str", one("text"), from_str_impl),
        ),
        (
            "from_list".to_owned(),
            native(closure, "from_list", one("values"), from_list_impl),
        ),
        (
            "from_hex".to_owned(),
            native(closure, "from_hex", one("hex"), from_hex_impl),
        ),
        (
            "to_str".to_owned(),
            native(closure, "to_str", one("bytes"), to_str_impl),
        ),
        (
            "to_list".to_owned(),
            native(closure, "to_list", one("bytes"), to_list_impl),
        ),
        (
            "to_hex".to_owned(),
            native(closure, "to_hex", one("bytes"), to_hex_impl),
        ),
        (
            "len".to_owned(),
            native(closure, "len", one("bytes"), len_impl),
        ),
        (
            "get".to_owned(),
            native(closure, "get", two("bytes", "index"), get_impl),
        ),
        (
            "slice".to_owned(),
            native(
                closure,
                "slice",
                vec![
                    arg("bytes", true, false, false),
                    arg("start", true, false, false),
                    arg("end", false, true, false),
                ],
                slice_impl,
            ),
        ),
        (
            "concat".to_owned(),
            native(closure, "concat", two("a", "b"), concat_impl),
        ),
        (
            "eq".to_owned(),
            native(closure, "eq", two("a", "b"), eq_impl),
        ),
        (
            "is_bytes".to_owned(),
            native(closure, "is_bytes", one("value"), is_bytes_impl),
        ),
    ]
}

// ---------------------------------------------------------------------------
// 인자 helpers
// ---------------------------------------------------------------------------

fn bytes_arg(scope: &Scope, name: &str) -> Result<Vec<u8>, Obj> {
    as_bytes(&scope::get(scope, name))
        .ok_or_else(|| obj::err(&format!("expected '{name}' is bytes")))
}

fn int_arg(scope: &Scope, name: &str) -> Result<i64, Obj> {
    match obj::payload(&scope::get(scope, name)) {
        Payload::Int(v) => Ok(v),
        _ => Err(obj::err(&format!("expected '{name}' is integer"))),
    }
}

fn normalize_index(index: i64, len: usize) -> Option<usize> {
    let shifted = if index < 0 { len as i64 + index } else { index };
    if shifted >= 0 && (shifted as usize) < len {
        Some(shifted as usize)
    } else {
        None
    }
}

fn clamp_bound(bound: i64, len: usize) -> usize {
    let shifted = if bound < 0 { len as i64 + bound } else { bound };
    if shifted < 0 {
        0
    } else if shifted as usize > len {
        len
    } else {
        shifted as usize
    }
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 구현
// ---------------------------------------------------------------------------

fn from_str_impl(scope: &Scope) -> Obj {
    match obj::payload(&scope::get(scope, "text")) {
        Payload::Str(s) => bytes_obj(s.as_bytes().to_vec()),
        _ => obj::err("expected 'text' is string"),
    }
}

fn from_list_impl(scope: &Scope) -> Obj {
    let values = scope::get(scope, "values");
    let items = match obj::payload(&values) {
        Payload::List(cells) => cells.borrow().clone(),
        Payload::Tup(v, _) => v.as_ref().clone(),
        _ => return obj::err("expected 'values' is list"),
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        match obj::payload(&item) {
            Payload::Int(v) if (0..=255).contains(&v) => out.push(v as u8),
            _ => return obj::err("expected byte values (integer 0..255)"),
        }
    }
    bytes_obj(out)
}

fn from_hex_impl(scope: &Scope) -> Obj {
    let hex = match obj::payload(&scope::get(scope, "hex")) {
        Payload::Str(s) => s.trim().to_owned(),
        _ => return obj::err("expected 'hex' is string"),
    };
    if hex.len() % 2 != 0 {
        return obj::err("hex string must have even length");
    }
    let raw = hex.as_bytes();
    let mut out = Vec::with_capacity(raw.len() / 2);
    let mut i = 0;
    while i < raw.len() {
        match (hex_val(raw[i]), hex_val(raw[i + 1])) {
            (Some(hi), Some(lo)) => out.push((hi << 4) | lo),
            _ => return obj::err("invalid hex string"),
        }
        i += 2;
    }
    bytes_obj(out)
}

fn to_str_impl(scope: &Scope) -> Obj {
    let bytes = match bytes_arg(scope, "bytes") {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::str(&String::from_utf8_lossy(&bytes))
}

fn to_list_impl(scope: &Scope) -> Obj {
    let bytes = match bytes_arg(scope, "bytes") {
        Ok(v) => v,
        Err(e) => return e,
    };
    collections::list(bytes.into_iter().map(|b| obj::int(b as i64)).collect())
}

fn to_hex_impl(scope: &Scope) -> Obj {
    let bytes = match bytes_arg(scope, "bytes") {
        Ok(v) => v,
        Err(e) => return e,
    };
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    obj::str(&out)
}

fn len_impl(scope: &Scope) -> Obj {
    let bytes = match bytes_arg(scope, "bytes") {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::int(bytes.len() as i64)
}

fn get_impl(scope: &Scope) -> Obj {
    let bytes = match bytes_arg(scope, "bytes") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let index = match int_arg(scope, "index") {
        Ok(v) => v,
        Err(e) => return e,
    };
    match normalize_index(index, bytes.len()) {
        Some(i) => obj::int(bytes[i] as i64),
        None => obj::err("byte index out of range"),
    }
}

fn slice_impl(scope: &Scope) -> Obj {
    let bytes = match bytes_arg(scope, "bytes") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let start = match int_arg(scope, "start") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let end_value = scope::get(scope, "end");
    let end = if obj::is_none(&end_value) {
        bytes.len() as i64
    } else {
        match obj::payload(&end_value) {
            Payload::Int(v) => v,
            _ => return obj::err("expected 'end' is integer"),
        }
    };
    let lo = clamp_bound(start, bytes.len());
    let hi = clamp_bound(end, bytes.len());
    if lo > hi {
        return bytes_obj(Vec::new());
    }
    bytes_obj(bytes[lo..hi].to_vec())
}

fn concat_impl(scope: &Scope) -> Obj {
    let a = match bytes_arg(scope, "a") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let b = match bytes_arg(scope, "b") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut out = Vec::with_capacity(a.len() + b.len());
    out.extend_from_slice(&a);
    out.extend_from_slice(&b);
    bytes_obj(out)
}

fn eq_impl(scope: &Scope) -> Obj {
    let a = match bytes_arg(scope, "a") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let b = match bytes_arg(scope, "b") {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::boolean(a == b)
}

fn is_bytes_impl(scope: &Scope) -> Obj {
    obj::boolean(is_bytes(&scope::get(scope, "value")))
}

#[cfg(test)]
mod tests {
    use super::{clamp_bound, hex_val, normalize_index};

    #[test]
    fn index_supports_negative_and_rejects_out_of_range() {
        assert_eq!(normalize_index(0, 3), Some(0));
        assert_eq!(normalize_index(2, 3), Some(2));
        assert_eq!(normalize_index(-1, 3), Some(2));
        assert_eq!(normalize_index(-3, 3), Some(0));
        assert_eq!(normalize_index(3, 3), None);
        assert_eq!(normalize_index(-4, 3), None);
    }

    #[test]
    fn slice_bounds_are_clamped() {
        assert_eq!(clamp_bound(1, 4), 1);
        assert_eq!(clamp_bound(-1, 4), 3);
        assert_eq!(clamp_bound(-10, 4), 0);
        assert_eq!(clamp_bound(99, 4), 4);
    }

    #[test]
    fn hex_nibble_parsing() {
        assert_eq!(hex_val(b'0'), Some(0));
        assert_eq!(hex_val(b'9'), Some(9));
        assert_eq!(hex_val(b'a'), Some(10));
        assert_eq!(hex_val(b'F'), Some(15));
        assert_eq!(hex_val(b'g'), None);
    }
}
