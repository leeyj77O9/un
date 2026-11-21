//! Math intrinsics matching `rust/src/native/math.rs`.

#[no_mangle] pub extern "C" fn un_math_sin(x: f64) -> f64 { x.sin() }
#[no_mangle] pub extern "C" fn un_math_cos(x: f64) -> f64 { x.cos() }
#[no_mangle] pub extern "C" fn un_math_tan(x: f64) -> f64 { x.tan() }
#[no_mangle] pub extern "C" fn un_math_asin(x: f64) -> f64 { x.asin() }
#[no_mangle] pub extern "C" fn un_math_acos(x: f64) -> f64 { x.acos() }
#[no_mangle] pub extern "C" fn un_math_atan(x: f64) -> f64 { x.atan() }
#[no_mangle] pub extern "C" fn un_math_sqrt(x: f64) -> f64 { x.sqrt() }
#[no_mangle] pub extern "C" fn un_math_cbrt(x: f64) -> f64 { x.cbrt() }
#[no_mangle] pub extern "C" fn un_math_exp(x: f64) -> f64 { x.exp() }
#[no_mangle] pub extern "C" fn un_math_ln(x: f64) -> f64 { x.ln() }
#[no_mangle] pub extern "C" fn un_math_log2(x: f64) -> f64 { x.log2() }
#[no_mangle] pub extern "C" fn un_math_log10(x: f64) -> f64 { x.log10() }
#[no_mangle] pub extern "C" fn un_math_floor(x: f64) -> f64 { x.floor() }
#[no_mangle] pub extern "C" fn un_math_ceil(x: f64) -> f64 { x.ceil() }
#[no_mangle] pub extern "C" fn un_math_trunc(x: f64) -> f64 { x.trunc() }
#[no_mangle] pub extern "C" fn un_math_abs(x: f64) -> f64 { x.abs() }
#[no_mangle] pub extern "C" fn un_math_powf(x: f64, y: f64) -> f64 { x.powf(y) }
#[no_mangle] pub extern "C" fn un_math_log(x: f64, base: f64) -> f64 { x.ln() / base.ln() }
