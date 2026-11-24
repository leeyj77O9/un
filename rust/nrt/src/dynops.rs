//! Tag-based dispatch for dynamic (`any` / union) values.
//!
//! Dynamic values never enter a scalar register: they are ordinary NObj
//! handles (the same tagged representation every NRT value already uses).
//! The code generator boxes scalars with `unrt_obj_*` before a dynamic
//! operation and calls the two entry points below; results stay boxed so a
//! chain of dynamic operations can keep inspecting the runtime tag.
//!
//! Op codes are shared with `natcode.rs` (both sides use the same numbers):
//! binary `0..=10` = Add/Sub/Mul/Div/IDiv/Mod/BAnd/BOr/BXor/Shl/Shr,
//! `20..=25` = Lt/Le/Gt/Ge/Eq/Ne; unary `0` = Neg.

use crate::obj::{alloc, err, obj, panic_err, NObj};

const OP_ADD: i64 = 0;
const OP_SUB: i64 = 1;
const OP_MUL: i64 = 2;
const OP_DIV: i64 = 3;
const OP_IDIV: i64 = 4;
const OP_MOD: i64 = 5;
const OP_BAND: i64 = 6;
const OP_BOR: i64 = 7;
const OP_BXOR: i64 = 8;
const OP_SHL: i64 = 9;
const OP_SHR: i64 = 10;
const OP_LT: i64 = 20;
const OP_LE: i64 = 21;
const OP_GT: i64 = 22;
const OP_GE: i64 = 23;
const OP_EQ: i64 = 24;
const OP_NE: i64 = 25;

#[derive(Clone, Copy)]
enum Num {
    Int(i64),
    Float(f64),
}

fn tag(p: u64) -> i64 {
    unsafe { obj(p).tag }
}

fn field(p: u64) -> i64 {
    unsafe { obj(p).a }
}

pub(crate) fn type_name(p: u64) -> &'static str {
    match tag(p) {
        0 => "int",
        1 => "float",
        2 => "bool",
        3 => "list",
        4 => "none",
        5 | 10 => "error",
        6 => "str",
        7 => "dict",
        8 => "fn",
        9 => "class",
        _ => "value",
    }
}

fn num_of(p: u64) -> Option<Num> {
    match tag(p) {
        0 => Some(Num::Int(field(p))),
        1 => Some(Num::Float(f64::from_bits(field(p) as u64))),
        2 => Some(Num::Int((field(p) != 0) as i64)),
        _ => None,
    }
}

fn box_int(v: i64) -> u64 {
    unsafe { alloc(NObj { tag: 0, a: v, b: 0 }) }
}

fn box_float(v: f64) -> u64 {
    unsafe { alloc(NObj { tag: 1, a: v.to_bits() as i64, b: 0 }) }
}

fn box_bool(v: bool) -> u64 {
    unsafe { alloc(NObj { tag: 2, a: v as i64, b: 0 }) }
}

fn as_f64(n: Num) -> f64 {
    match n {
        Num::Int(v) => v as f64,
        Num::Float(v) => v,
    }
}

fn op_symbol(op: i64) -> &'static str {
    match op {
        OP_ADD => "+",
        OP_SUB => "-",
        OP_MUL => "*",
        OP_DIV => "/",
        OP_IDIV => "//",
        OP_MOD => "%",
        OP_BAND => "&",
        OP_BOR => "|",
        OP_BXOR => "^",
        OP_SHL => "<<",
        OP_SHR => ">>",
        OP_LT => "<",
        OP_LE => "<=",
        OP_GT => ">",
        OP_GE => ">=",
        OP_EQ => "==",
        OP_NE => "!=",
        _ => "?",
    }
}

fn unsupported(op: i64, a: u64, b: u64) -> u64 {
    err(&format!(
        "unsupported operand type(s) for {}: '{}' and '{}'",
        op_symbol(op),
        type_name(a),
        type_name(b)
    ))
}

/// Boxed binary operation. `op` selects the arithmetic/bitwise/comparison
/// operation; the returned handle carries a tag (Int/Float/Str/Bool/Error).
#[no_mangle]
pub extern "C" fn unrt_dyn_bin(op: i64, a: u64, b: u64) -> u64 {
    if op >= OP_LT {
        return compare(op, a, b);
    }
    // Str concat (and Str subtraction, matching the VM) converts the other
    // operand to its display string. Errors are never stringified.
    if (tag(a) == 6 || tag(b) == 6) && op == OP_ADD {
        // Error (5) and panic (10) handles are never stringified; Str (6),
        // collections and classes convert through the display helper.
        if matches!(tag(a), 5 | 10) || matches!(tag(b), 5 | 10) {
            return err("unsupported operand type(s) for str()");
        }
        let joined = format!("{}{}", crate::obj::display(a), crate::obj::display(b));
        return crate::strings::str_new(joined);
    }
    let (na, nb) = match (num_of(a), num_of(b)) {
        (Some(x), Some(y)) => (x, y),
        _ => return unsupported(op, a, b),
    };
    match op {
        OP_ADD | OP_SUB | OP_MUL => match (na, nb) {
            (Num::Int(x), Num::Int(y)) => box_int(match op {
                OP_ADD => x.wrapping_add(y),
                OP_SUB => x.wrapping_sub(y),
                _ => x.wrapping_mul(y),
            }),
            _ => {
                let (x, y) = (as_f64(na), as_f64(nb));
                box_float(match op {
                    OP_ADD => x + y,
                    OP_SUB => x - y,
                    _ => x * y,
                })
            }
        },
        OP_DIV => {
            if let (Num::Int(x), Num::Int(y)) = (na, nb) {
                if y == 0 {
                    return err("int division by zero");
                }
                box_float(x as f64 / y as f64)
            } else {
                let y = as_f64(nb);
                if y == 0.0 {
                    return err("float division by zero");
                }
                box_float(as_f64(na) / y)
            }
        }
        OP_IDIV | OP_MOD => {
            let (x, y) = match (na, nb) {
                (Num::Int(x), Num::Int(y)) => (x, y),
                _ => return unsupported(op, a, b),
            };
            if y == 0 {
                return panic_err("Attempted to divide by zero.");
            }
            box_int(if op == OP_IDIV { x / y } else { x % y })
        }
        OP_BAND | OP_BOR | OP_BXOR | OP_SHL | OP_SHR => {
            let (Num::Int(x), Num::Int(y)) = (na, nb) else {
                return unsupported(op, a, b);
            };
            box_int(match op {
                OP_BAND => x & y,
                OP_BOR => x | y,
                OP_BXOR => x ^ y,
                OP_SHL => x.wrapping_shl(y as u32),
                _ => x.wrapping_shr(y as u32),
            })
        }
        _ => unsupported(op, a, b),
    }
}

fn compare(op: i64, a: u64, b: u64) -> u64 {
    let ta = tag(a);
    let tb = tag(b);
    let eq = if ta == tb && ta != 5 && ta != 10 {
        // Collections/closures/instances compare by identity; scalars reuse
        // the NRT comparator (which handles numeric and string payloads).
        if matches!(ta, 3 | 7 | 8 | 9) {
            a == b
        } else {
            crate::obj::cmp_obj(a, b) == std::cmp::Ordering::Equal
        }
    } else if matches!(ta, 0 | 1 | 2) && matches!(tb, 0 | 1 | 2) {
        // Numeric cross-type equality promotes to float.
        match (num_of(a), num_of(b)) {
            (Some(x), Some(y)) => as_f64(x) == as_f64(y),
            _ => false,
        }
    } else if ta == 4 || tb == 4 {
        // The VM compares `none` equal only to itself.
        ta == tb
    } else {
        return unsupported(op, a, b);
    };
    if op == OP_EQ {
        return box_bool(eq);
    }
    if op == OP_NE {
        return box_bool(!eq);
    }
    // Ordering: numbers or two strings.
    let ord = if matches!(ta, 0 | 1 | 2) && matches!(tb, 0 | 1 | 2) {
        match (num_of(a), num_of(b)) {
            (Some(x), Some(y)) => as_f64(x)
                .partial_cmp(&as_f64(y))
                .unwrap_or(std::cmp::Ordering::Equal),
            _ => std::cmp::Ordering::Equal,
        }
    } else if ta == 6 && tb == 6 {
        crate::obj::display(a).cmp(&crate::obj::display(b))
    } else {
        return unsupported(op, a, b);
    };
    box_bool(match op {
        OP_LT => ord == std::cmp::Ordering::Less,
        OP_LE => ord != std::cmp::Ordering::Greater,
        OP_GT => ord == std::cmp::Ordering::Greater,
        _ => ord != std::cmp::Ordering::Less,
    })
}

/// Boxed unary operation. Only `0` (Neg) is defined today; other codes produce
/// a catchable error, keeping the dispatch total for all TIR unary operators.
#[no_mangle]
pub extern "C" fn unrt_dyn_un(op: i64, a: u64) -> u64 {
    if op != 0 {
        return err("unsupported dynamic unary operation");
    }
    match num_of(a) {
        Some(Num::Int(v)) => box_int(v.wrapping_neg()),
        Some(Num::Float(v)) => box_float(-v),
        None => err(&format!(
            "unsupported operand type(s) for -: '{}'",
            type_name(a)
        )),
    }
}
