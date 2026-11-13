pub fn fmt_double(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_owned();
    }
    if v.is_infinite() {
        return if v > 0.0 { "∞".to_owned() } else { "-∞".to_owned() };
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0".to_owned() } else { "0".to_owned() };
    }
    let neg = v < 0.0;
    let abs = v.abs();
    let sci = format!("{:e}", abs);
    let (mant, exp) = match sci.split_once('e') {
        Some((m, e)) => (m, e.parse::<i64>().unwrap_or(0)),
        None => (sci.as_str(), 0),
    };
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let body = if exp >= 17 || exp <= -5 {
        let sign = if exp < 0 { "-" } else { "+" };
        let ae = exp.unsigned_abs();
        let pad = if ae < 10 { format!("0{ae}") } else { ae.to_string() };
        if digits.len() <= 1 {
            format!("{digits}E{sign}{pad}")
        } else {
            format!("{}.{}E{sign}{pad}", &digits[..1], &digits[1..])
        }
    } else {

        format!("{abs}")
    };
    if neg {
        format!("-{body}")
    } else {
        body
    }
}
pub fn cast_long(v: f64) -> i64 {
    if v.is_nan() {
        return i64::MIN;
    }
    if v >= i64::MAX as f64 {
        return i64::MIN;
    }
    if v <= i64::MIN as f64 {
        return i64::MIN;
    }
    v.trunc() as i64
}
pub fn str_hash(s: &str) -> i32 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for unit in s.encode_utf16() {
        hash ^= unit as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash as i32
}
#[cfg(test)]
mod tests {
    use super::fmt_double;

    fn case(v: f64, expected: &str) {
        assert_eq!(fmt_double(v), expected, "{v}");
    }
    #[test]
    fn double_display_matches_reference() {
        case(1.0, "1");
        case(1.5, "1.5");
        case(0.0, "0");
        case(-0.0, "-0");
        case(3.0, "3");
        case(1e14, "100000000000000");
        case(1e15, "1000000000000000");
        case(1e16, "10000000000000000");
        case(9.9e15, "9900000000000000");
        case(1.2345678901234567e16, "12345678901234568");
        case(1e17, "1E+17");
        case(1.2345678901234568e20, "1.2345678901234568E+20");
        case(1e-4, "0.0001");
        case(9.9e-5, "9.9E-05");
        case(1e-5, "1E-05");
        case(1.5e-10, "1.5E-10");
        case(1.0000000000000002, "1.0000000000000002");
        case(f64::INFINITY, "∞");
        case(f64::NEG_INFINITY, "-∞");
        case(f64::NAN, "NaN");
    }
    #[test]
    fn str_hash_is_deterministic() {
        assert_eq!(super::str_hash("abc"), super::str_hash("abc"));
        assert_ne!(super::str_hash("abc"), super::str_hash("abd"));
    }
}
