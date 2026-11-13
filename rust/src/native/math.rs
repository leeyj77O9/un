use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use crate::runtime::world;
use super::{arg, native};
const MAX_DIGITS: i64 = 15;
pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    let one = |name: &str| vec![arg(name, true, false, false)];
    let optional = |name: &str| vec![arg(name, false, true, false)];
    vec![
        (
            "round".to_owned(),
            native(
                closure,
                "round",
                vec![arg("value", true, false, false), arg("digit", false, true, false)],
                round_impl,
            ),
        ),
        ("abs".to_owned(), native(closure, "abs", one("value"), abs_impl)),
        ("ceil".to_owned(), native(closure, "ceil", one("value"), ceil_impl)),
        ("floor".to_owned(), native(closure, "floor", one("value"), floor_impl)),
        ("trunc".to_owned(), native(closure, "trunc", one("value"), trunc_impl)),
        ("sign".to_owned(), native(closure, "sign", one("value"), sign_impl)),
        ("sqrt".to_owned(), native(closure, "sqrt", one("value"), sqrt_impl)),
        ("cbrt".to_owned(), native(closure, "cbrt", one("value"), cbrt_impl)),
        (
            "pow".to_owned(),
            native(
                closure,
                "pow",
                vec![arg("value", true, false, false), arg("exponent", true, false, false)],
                pow_impl,
            ),
        ),
        ("exp".to_owned(), native(closure, "exp", one("value"), exp_impl)),
        (
            "log".to_owned(),
            native(
                closure,
                "log",
                vec![arg("value", true, false, false), arg("base", false, true, false)],
                log_impl,
            ),
        ),
        ("log2".to_owned(), native(closure, "log2", one("value"), log2_impl)),
        ("log10".to_owned(), native(closure, "log10", one("value"), log10_impl)),
        ("sin".to_owned(), native(closure, "sin", one("value"), sin_impl)),
        ("cos".to_owned(), native(closure, "cos", one("value"), cos_impl)),
        ("tan".to_owned(), native(closure, "tan", one("value"), tan_impl)),
        ("asin".to_owned(), native(closure, "asin", one("value"), asin_impl)),
        ("acos".to_owned(), native(closure, "acos", one("value"), acos_impl)),
        ("atan".to_owned(), native(closure, "atan", one("value"), atan_impl)),
        (
            "atan2".to_owned(),
            native(
                closure,
                "atan2",
                vec![arg("y", true, false, false), arg("x", true, false, false)],
                atan2_impl,
            ),
        ),
        (
            "hypot".to_owned(),
            native(
                closure,
                "hypot",
                vec![arg("x", true, false, false), arg("y", true, false, false)],
                hypot_impl,
            ),
        ),
        (
            "clamp".to_owned(),
            native(
                closure,
                "clamp",
                vec![
                    arg("value", true, false, false),
                    arg("min", true, false, false),
                    arg("max", true, false, false),
                ],
                clamp_impl,
            ),
        ),
        (
            "gcd".to_owned(),
            native(
                closure,
                "gcd",
                vec![arg("a", true, false, false), arg("b", true, false, false)],
                gcd_impl,
            ),
        ),
        (
            "lcm".to_owned(),
            native(
                closure,
                "lcm",
                vec![arg("a", true, false, false), arg("b", true, false, false)],
                lcm_impl,
            ),
        ),
        ("is_nan".to_owned(), native(closure, "is_nan", one("value"), is_nan_impl)),
        (
            "is_infinite".to_owned(),
            native(closure, "is_infinite", one("value"), is_infinite_impl),
        ),
        (
            "is_finite".to_owned(),
            native(closure, "is_finite", one("value"), is_finite_impl),
        ),
        (
            "pi".to_owned(),
            native(closure, "pi", optional("unused"), pi_impl),
        ),
        (
            "e".to_owned(),
            native(closure, "e", optional("unused"), e_impl),
        ),
        (
            "tau".to_owned(),
            native(closure, "tau", optional("unused"), tau_impl),
        ),
    ]
}
fn get_number(value: &Obj) -> Result<f64, Obj> {
    match obj::payload(value) {
        Payload::Int(v) => Ok(v as f64),
        Payload::Float(v) => Ok(v),
        _ => Err(obj::err("expected number type")),
    }
}
fn get_int(value: &Obj) -> Result<i64, Obj> {
    match obj::payload(value) {
        Payload::Int(v) => Ok(v),
        _ => Err(obj::err("expected integer type")),
    }
}
fn wrap(original: &Obj, value: f64) -> Obj {
    if matches!(obj::payload(original), Payload::Int(_)) {
        obj::int(value as i64)
    } else {
        obj::float(value)
    }
}
fn round_to_even(value: f64) -> f64 {
    let abs = value.abs();
    let mut rounded = abs.floor();
    if abs - rounded > 0.5 {
        rounded += 1.0;
    } else if abs - rounded == 0.5 && rounded % 2.0 != 0.0 {
        rounded += 1.0;
    }
    if value < 0.0 {
        -rounded
    } else {
        rounded
    }
}
fn round_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let digit = scope::get(scope, "digit");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut digits = 0i64;
    if !obj::is_none(&digit) {
        let d = match get_int(&digit) {
            Ok(d) => d,
            Err(_) => return obj::err("digit must be an integer between 0 and 15"),
        };
        if d > MAX_DIGITS || d < 0 {
            return obj::err("digit must be an integer between 0 and 15");
        }
        digits = d;
    }
    let rounded = if digits == 0 {
        round_to_even(v)
    } else {
        let power = 10f64.powi(digits as i32);
        round_to_even(v * power) / power
    };
    if digits == 0 || rounded.fract() == 0.0 {
        obj::int(rounded as i64)
    } else {
        obj::float(rounded)
    }
}
fn abs_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    wrap(&value, v.abs())
}
fn ceil_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if matches!(obj::payload(&value), Payload::Int(_)) {
        value
    } else {
        obj::float(v.ceil())
    }
}
fn floor_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if matches!(obj::payload(&value), Payload::Int(_)) {
        value
    } else {
        obj::float(v.floor())
    }
}
fn trunc_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if matches!(obj::payload(&value), Payload::Int(_)) {
        value
    } else {
        obj::int(v.trunc() as i64)
    }
}
fn sign_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let sign = if v.is_nan() || v == 0.0 {
        0
    } else if v > 0.0 {
        1
    } else {
        -1
    };
    obj::int(sign)
}
fn sqrt_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if v < 0.0 {
        return obj::err("cannot compute sqrt of a negative number");
    }
    obj::float(v.sqrt())
}
fn cbrt_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::float(v.cbrt())
}
fn pow_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let exponent = scope::get(scope, "exponent");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let e = match get_number(&exponent) {
        Ok(e) => e,
        Err(e) => return e,
    };
    let result = v.powf(e);
    let both_int = matches!(obj::payload(&value), Payload::Int(_))
        && matches!(obj::payload(&exponent), Payload::Int(_));
    if both_int && e >= 0.0 && result.fract() == 0.0 && result.is_finite() {
        obj::int(result as i64)
    } else {
        obj::float(result)
    }
}
fn exp_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::float(v.exp())
}
fn log_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let base = scope::get(scope, "base");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if v <= 0.0 {
        return obj::err("expected value greater than 0");
    }
    if obj::is_none(&base) {
        return obj::float(v.ln());
    }
    let b = match get_number(&base) {
        Ok(b) => b,
        Err(e) => return e,
    };
    obj::float(v.ln() / b.ln())
}
fn log2_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if v <= 0.0 {
        return obj::err("expected value greater than 0");
    }
    obj::float(v.log2())
}
fn log10_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if v <= 0.0 {
        return obj::err("expected value greater than 0");
    }
    obj::float(v.log10())
}
fn sin_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::float(v.sin())
}
fn cos_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::float(v.cos())
}
fn tan_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::float(v.tan())
}
fn asin_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::float(v.asin())
}
fn acos_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::float(v.acos())
}
fn atan_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::float(v.atan())
}
fn atan2_impl(scope: &Scope) -> Obj {
    let y = scope::get(scope, "y");
    let x = scope::get(scope, "x");
    let yv = match get_number(&y) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let xv = match get_number(&x) {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::float(yv.atan2(xv))
}
fn hypot_impl(scope: &Scope) -> Obj {
    let x = scope::get(scope, "x");
    let y = scope::get(scope, "y");
    let xv = match get_number(&x) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let yv = match get_number(&y) {
        Ok(v) => v,
        Err(e) => return e,
    };
    obj::float((xv * xv + yv * yv).sqrt())
}
fn clamp_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let min = scope::get(scope, "min");
    let max = scope::get(scope, "max");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let lo = match get_number(&min) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let hi = match get_number(&max) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if lo > hi {
        return obj::err("min must be less than or equal to max");
    }
    let clamped = if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    };
    wrap(&value, clamped)
}
fn gcd_impl(scope: &Scope) -> Obj {
    let a = scope::get(scope, "a");
    let b = scope::get(scope, "b");
    let av = match get_int(&a) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let bv = match get_int(&b) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut x = av.wrapping_abs();
    let mut y = bv.wrapping_abs();
    while y != 0 {
        let t = x % y;
        x = y;
        y = t;
    }
    obj::int(x)
}
fn lcm_impl(scope: &Scope) -> Obj {
    let a = scope::get(scope, "a");
    let b = scope::get(scope, "b");
    let av = match get_int(&a) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let bv = match get_int(&b) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if av == 0 || bv == 0 {
        return obj::int(0);
    }
    let x = av.wrapping_abs();
    let y = bv.wrapping_abs();
    let mut gcd = x;
    let mut rem = y;
    while rem != 0 {
        let t = gcd % rem;
        gcd = rem;
        rem = t;
    }
    obj::int(x / gcd * y)
}
fn is_nan_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    world::bool_obj(v.is_nan())
}
fn is_infinite_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    world::bool_obj(v.is_infinite())
}
fn is_finite_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let v = match get_number(&value) {
        Ok(v) => v,
        Err(e) => return e,
    };
    world::bool_obj(v.is_finite())
}
fn pi_impl(_scope: &Scope) -> Obj {
    obj::float(std::f64::consts::PI)
}
fn e_impl(_scope: &Scope) -> Obj {
    obj::float(std::f64::consts::E)
}
fn tau_impl(_scope: &Scope) -> Obj {
    obj::float(std::f64::consts::TAU)
}
