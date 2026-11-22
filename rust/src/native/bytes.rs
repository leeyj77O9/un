//! `bytes` 값 생성/조회용 native 모듈과, 파일 바이너리 I/O가 공유하는 어댑터.
//!
//! ## RT-03 / CMP-05 연동 계약 (중요)
//!
//! 이 모듈은 `bytes` 값의 런타임 표현을 **`Payload::Bytes(Vec<u8>)`** 로
//! 기대한다. 아직 그 variant가 합의/추가되지 않았으므로, 변경 범위를
//! `rust/src/native/` 내부로 한정하기 위해 **`bytes_obj` / `as_bytes` /
//! `is_bytes` 세 함수에만** 임시 표현을 두었다. 그 밖의 모든 함수는 이
//! 어댑터만 호출하므로, RT-03이 variant를 추가한 뒤 위 세 함수의 본문만
//! 교체하면 나머지 native 코드는 수정 없이 새 ABI를 사용한다.
//!
//! 임시 표현: `Payload::List` + 각 원소 `Int(0..=255)`, `type_name = "bytes"`.
//! (`fs.read_bytes` / `bytes.from_str` 등이 생성하는 값은 모두 이 어댑터를
//! 거치므로 임시 표현과 새 표현이 섞이지 않는다.)
//!
//! RT-03 반영 후 교체본 (각 함수 주석에도 동일하게 적어 두었다):
//!
//! ```ignore
//! pub fn bytes_obj(bytes: Vec<u8>) -> Obj {
//!     obj::obj_new(TYPE_NAME, Payload::Bytes(bytes))
//! }
//! pub fn as_bytes(value: &Obj) -> Option<Vec<u8>> {
//!     match obj::payload(value) {
//!         Payload::Bytes(v) => Some(v),
//!         // 편의용 입력 (문자열 / 정수 리스트)은 그대로 유지해도 무방
//!         Payload::Str(s) => Some(s.as_bytes().to_vec()),
//!         _ => None,
//!     }
//! }
//! pub fn is_bytes(value: &Obj) -> bool {
//!     matches!(obj::payload(value), Payload::Bytes(_))
//! }
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::collections;
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};

use super::{arg, native, one, two};

/// `bytes` 값의 type name. RT-03이 전용 타입을 정하면 그 이름으로 맞춘다.
pub const TYPE_NAME: &str = "bytes";

// ---------------------------------------------------------------------------
// 어댑터 (RT-03 반영 시 이 세 함수만 교체)
// ---------------------------------------------------------------------------

/// `Vec<u8>` → `bytes` Obj.
///
/// RT-03 반영 후: `obj::obj_new(TYPE_NAME, Payload::Bytes(bytes))`.
pub fn bytes_obj(bytes: Vec<u8>) -> Obj {
    obj::obj_new(
        TYPE_NAME,
        Payload::List(Rc::new(RefCell::new(
            bytes.into_iter().map(|b| obj::int(b as i64)).collect(),
        ))),
    )
}

/// `bytes` Obj → `Vec<u8>`. 편의상 문자열(UTF-8)과 `0..=255` 정수 리스트도 받는다.
///
/// RT-03 반영 후 최우선 분기: `Payload::Bytes(v) => Some(v)`.
pub fn as_bytes(value: &Obj) -> Option<Vec<u8>> {
    match obj::payload(value) {
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
/// RT-03 반영 후: `matches!(obj::payload(value), Payload::Bytes(_))`.
pub fn is_bytes(value: &Obj) -> bool {
    &*obj::type_name(value) == TYPE_NAME
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
