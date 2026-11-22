//! Native-module dispatch. Reimplements the `unrt_call_native` bridge without
//! the VM: each module/member pair receives boxed argument handles and returns
//! a boxed handle, matching `un run` byte-for-byte for the supported surface.

use std::fs;
use std::io::Write;

use crate::obj::{
    display, err, obj, unrt_obj_bool, unrt_obj_float, unrt_obj_int, unrt_obj_none,
};

/// Format shared by `sys.version()` and `os.version()`. The VM builds these
/// with the same pieces (`rust/src/native/sys.rs` and `native/os.rs`). Both
/// crates currently declare version 0.1.0, so plain-build output stays
/// byte-identical with `un run`; keep the two Cargo.toml versions in lockstep.
const UN_VERSION: &str = concat!("Un ", env!("CARGO_PKG_VERSION"));

// Plain builds are shipped for x86_64-pc-windows-gnu today. These constants
// name the values the Windows branches already produced; using them does not
// change any non-Windows branch. Widening platform coverage is a separate
// decision (see the NRT-06 report) because it would change non-Windows output.
const OS_NAME_WINDOWS: &str = "windows";
const ARCH_X64: &str = "X64";
const SEP_WINDOWS: &str = "\\";
const PATHSEP_WINDOWS: &str = ";";
const LINESEP_WINDOWS: &str = "\r\n";

fn as_i64(p: u64) -> Option<i64> {
    unsafe { if obj(p).tag == 0 { Some(obj(p).a) } else { None } }
}
fn as_f64(p: u64) -> Option<f64> {
    unsafe {
        match obj(p).tag {
            0 => Some(obj(p).a as f64),
            1 => Some(f64::from_bits(obj(p).a as u64)),
            _ => None,
        }
    }
}
fn as_number(p: u64) -> Option<f64> {
    as_f64(p)
}
fn as_str(p: u64) -> Option<String> {
    unsafe { if obj(p).tag == 6 { Some(crate::strings::str_of(p).clone()) } else { None } }
}
fn int(v: i64) -> u64 {
    unrt_obj_int(v)
}
fn float(v: f64) -> u64 {
    unrt_obj_float(v)
}
fn boolean(v: bool) -> u64 {
    unrt_obj_bool(v as i64)
}
fn string(v: &str) -> u64 {
    crate::strings::str_new(v.to_string())
}
fn none() -> u64 {
    unrt_obj_none()
}
fn list(v: Vec<u64>) -> u64 {
    crate::list::list_new(v)
}
fn str_arg(args: &[u64], i: usize) -> Result<String, u64> {
    args.get(i).and_then(|p| as_str(*p)).ok_or_else(|| err("expected str"))
}
fn int_arg(args: &[u64], i: usize) -> Result<i64, u64> {
    args.get(i).and_then(|p| as_i64(*p)).ok_or_else(|| err("expected int"))
}
fn num_arg(args: &[u64], i: usize) -> Result<f64, u64> {
    args.get(i).and_then(|p| as_number(*p)).ok_or_else(|| err("expected number"))
}

fn regex_compile(pattern: &str) -> Result<crate::regex::Regex, u64> {
    crate::regex::Regex::new(pattern).map_err(|m| err(&format!("invalid regex pattern: {m}")))
}

fn fs_path(s: &str) -> std::path::PathBuf {
    std::path::Path::new(s).to_path_buf()
}

fn platform_path(s: &str) -> String {
    if cfg!(windows) { s.replace('/', "\\") } else { s.to_string() }
}

/// ISO-8601 timestamp matching the VM's `runtime::date` formatting.
fn iso_time(time: std::time::SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    const EPOCH_TICKS: i128 = 621_355_968_000_000_000i128;
    const TICKS_PER_DAY: i128 = 864_000_000_000;
    let ticks = match time.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128 / 100 + EPOCH_TICKS,
        Err(d) => EPOCH_TICKS - d.duration().as_nanos() as i128 / 100,
    };
    // Howard Hinnant's civil_from_days algorithm.
    let days = ticks.div_euclid(TICKS_PER_DAY);
    let rem = ticks.rem_euclid(TICKS_PER_DAY);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    let hh = rem / 36_000_000_000;
    let mm = (rem / 600_000_000) % 60;
    let ss = (rem / 10_000_000) % 60;
    let ms = (rem / 10_000) % 1000;
    format!("{year:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{ms:03}Z")
}

fn dispatch(m: &str, name: &str, args: &[u64]) -> u64 {
    match (m, name) {
        ("io", "write") => {
            let parts: Vec<String> = args.iter().map(|p| {
                let tag = unsafe { obj(*p).tag };
                if tag == 5 || tag == 10 { format!("error: {}", display(*p)) } else { display(*p) }
            }).collect();
            let line = format!("{}\n", parts.join(" "));
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(line.as_bytes());
            none()
        }
        ("io", "read") => {
            let mut line = String::new();
            match std::io::stdin().read_line(&mut line) {
                Ok(_) => string(line.trim_end_matches(['\r', '\n'])),
                Err(e) => err(&e.to_string()),
            }
        }

        ("math", "sign") => match num_arg(args, 0) {
            Ok(v) => int(if v.is_nan() || v == 0.0 { 0 } else if v > 0.0 { 1 } else { -1 }),
            Err(e) => e,
        },
        ("math", "isqrt") => match int_arg(args, 0) {
            Ok(v) if v >= 0 => int(v.isqrt()),
            Ok(_) => err("cannot compute isqrt of a negative number"),
            Err(e) => e,
        },
        ("math", "is_nan") => match num_arg(args, 0) {
            Ok(v) => boolean(v.is_nan()),
            Err(e) => e,
        },
        ("math", "is_infinite") => match num_arg(args, 0) {
            Ok(v) => boolean(v.is_infinite()),
            Err(e) => e,
        },
        ("math", "is_finite") => match num_arg(args, 0) {
            Ok(v) => boolean(v.is_finite()),
            Err(e) => e,
        },
        ("math", "pi") => float(std::f64::consts::PI),
        ("math", "e") => float(std::f64::consts::E),
        ("math", "tau") => float(std::f64::consts::TAU),
        ("math", "abs") => match args.first().copied() {
            Some(v) if unsafe { obj(v).tag == 0 } => {
                let i = unsafe { obj(v).a };
                if i == i64::MIN { float(9223372036854775808.0) } else { int(i.unsigned_abs() as i64) }
            }
            _ => match num_arg(args, 0) { Ok(v) => float(v.abs()), Err(e) => e },
        },
        ("math", "round") => match num_arg(args, 0) {
            Ok(v) => {
                let r = v.round_ties_even();
                if r.fract() == 0.0 && r >= i64::MIN as f64 && r <= i64::MAX as f64 {
                    int(r as i64)
                } else {
                    float(r)
                }
            }
            Err(e) => e,
        },
        ("math", "sqrt") => match num_arg(args, 0) {
            Ok(v) if v < 0.0 => err("cannot compute sqrt of a negative number"),
            Ok(v) => float(v.sqrt()),
            Err(e) => e,
        },
        ("math", "pow") | ("math", "atan2") | ("math", "hypot") | ("math", "log") => {
            if name == "log" {
                match num_arg(args, 0) {
                    Ok(v) if v <= 0.0 => return err("expected value greater than 0"),
                    _ => {}
                }
            }
            match (num_arg(args, 0), num_arg(args, 1)) {
                (Ok(a), Ok(b)) => {
                    let r = match name {
                        "pow" => a.powf(b),
                        "atan2" => a.atan2(b),
                        "hypot" => a.hypot(b),
                        _ => a.ln() / b.ln(),
                    };
                    if name == "pow" && args.len() >= 2
                        && unsafe { obj(args[0]).tag == 0 && obj(args[1]).tag == 0 }
                        && r.fract() == 0.0 && r >= i64::MIN as f64 && r <= i64::MAX as f64
                    {
                        int(r as i64)
                    } else { float(r) }
                }
                (Err(e), _) | (_, Err(e)) => e,
            }
        }
        ("math", "gcd") | ("math", "lcm") => match (int_arg(args, 0), int_arg(args, 1)) {
            (Ok(a), Ok(b)) => {
                let mut x = a.unsigned_abs();
                let mut y = b.unsigned_abs();
                if name == "gcd" {
                    while y != 0 { let t = x % y; x = y; y = t; }
                    int(x as i64)
                } else {
                    let g = { let (mut p, mut q) = (x, y); while q != 0 { let t = p % q; p = q; q = t; } p };
                    int((x / g * y) as i64)
                }
            }
            (Err(e), _) | (_, Err(e)) => e,
        },
        ("math", "clamp") => match (num_arg(args, 0), num_arg(args, 1), num_arg(args, 2)) {
            (Ok(v), Ok(lo), Ok(hi)) => float(v.clamp(lo, hi)),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => e,
        },

        ("iter", "map") | ("iter", "filter") => {
            if args.len() == 2 && crate::closure::unrt_is_callable(args[0]) == 1
                && unsafe { obj(args[1]).tag == 3 }
            {
                let f = args[0];
                let items = unsafe { crate::list::items(args[1]).clone() };
                let mut out = Vec::new();
                for item in items {
                    let r = crate::closure::unrt_call_obj(f, &item, 1);
                    if crate::obj::unrt_obj_is_err(r) != 0 {
                        return r;
                    }
                    if name == "map" {
                        out.push(r);
                    } else if crate::obj::unrt_bool_from_obj(r) != 0 {
                        out.push(item);
                    }
                }
                return list(out);
            }
            err(&format!("native module 'iter.{name}' requires a callable and a list"))
        }
        ("iter", "min") | ("iter", "max") => {
            let items: Vec<u64> = if args.len() == 1 && unsafe { obj(args[0]).tag == 3 } {
                unsafe { crate::list::items(args[0]).clone() }
            } else {
                args.to_vec()
            };
            if items.is_empty() { return err("expected 'iterable' argument to have at least one value"); }
            let mut best = items[0];
            for item in items.iter().skip(1) {
                let ord = crate::obj::cmp_obj(best, *item);
                if (name == "min" && ord == std::cmp::Ordering::Greater)
                    || (name == "max" && ord == std::cmp::Ordering::Less) { best = *item; }
            }
            best
        }
        ("iter", "sum") => {
            let items: Vec<u64> = if args.len() == 1 && unsafe { obj(args[0]).tag == 3 } {
                unsafe { crate::list::items(args[0]).clone() }
            } else { args.to_vec() };
            let mut total = 0.0;
            let mut is_int = true;
            for p in &items {
                match as_i64(*p) { Some(v) => total += v as f64, None => { is_int = false; total += as_f64(*p).unwrap_or(0.0); } }
            }
            if is_int { int(total as i64) } else { float(total) }
        }
        ("iter", "sorted") => {
            let items: Vec<u64> = if args.len() == 1 && unsafe { obj(args[0]).tag == 3 } {
                unsafe { crate::list::items(args[0]).clone() }
            } else { args.to_vec() };
            let mut v = items;
            v.sort_by(|a, b| crate::obj::cmp_obj(*a, *b));
            list(v)
        }
        ("iter", "reverse") => {
            let items: Vec<u64> = if args.len() == 1 && unsafe { obj(args[0]).tag == 3 } {
                unsafe { crate::list::items(args[0]).clone() }
            } else { args.to_vec() };
            let mut v = items;
            v.reverse();
            list(v)
        }

        ("codes", "bin") => match args.first().copied().and_then(as_i64) {
            None => err("bin: expected int"),
            Some(v) if v < 0 => string(&format!("-0b{:b}", v.wrapping_neg())),
            Some(v) => string(&format!("0b{v:b}")),
        },
        ("codes", "oct") => match args.first().copied().and_then(as_i64) {
            None => err("oct: expected int"),
            Some(v) if v < 0 => string(&format!("-0o{:o}", v.wrapping_neg())),
            Some(v) => string(&format!("0o{v:o}")),
        },
        ("codes", "hex") => match args.first().copied().and_then(as_i64) {
            None => err("hex: expected int"),
            Some(v) if v < 0 => string(&format!("-0x{:x}", v.wrapping_neg())),
            Some(v) => string(&format!("0x{v:x}")),
        },
        ("codes", "chr") => match args.first().copied().and_then(as_i64) {
            None => err("chr: expected int code"),
            Some(v) if !(0..=0x10FFFF).contains(&v) => err("chr: code point out of range (0..1114111)"),
            Some(v) => char::from_u32(v as u32)
                .map(|c| string(&c.to_string()))
                .unwrap_or_else(|| crate::obj::panic_err("A valid UTF32 value is between 0x000000 and 0x10ffff, inclusive, and should not include surrogate codepoint values (0x00d800 ~ 0x00dfff). (Parameter 'utf32')")),
        },
        ("codes", "ord") => match args.first().copied().and_then(as_str) {
            None => err("ord: expected str"),
            Some(s) => {
                let mut it = s.chars();
                match (it.next(), it.next()) {
                    (Some(c), None) => int(c as u32 as i64),
                    (None, _) => err("ord: expected a single character, got empty string"),
                    _ => err("ord: expected a single character, got multiple characters"),
                }
            }
        },
        ("codes", "ascii") => match args.first().copied().and_then(as_str) {
            None => err("ascii: expected str"),
            Some(s) => {
                let mut it = s.encode_utf16();
                match (it.next(), it.next()) {
                    (Some(u), None) => int(u as i64),
                    _ => err("ascii: expected str"),
                }
            }
        },

        ("os", "pid") | ("sys", "pid") => int(std::process::id() as i64),
        ("os", "cpuCount") | ("sys", "cpuCount") => {
            int(std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1))
        }
        ("os", "is64bit") | ("sys", "is64bit") => boolean(cfg!(target_pointer_width = "64")),
        ("os", "name") | ("sys", "name") => string(if cfg!(windows) { OS_NAME_WINDOWS } else { "unknown" }),
        ("os", "arch") | ("sys", "arch") => string(ARCH_X64),
        ("os", "hostname") | ("sys", "hostname") => string(
            &std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default(),
        ),
        ("os", "username") | ("sys", "username") => string(
            &std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default(),
        ),
        ("os", "version") => string(&format!("{} ({UN_VERSION})", std::env::consts::OS)),
        ("sys", "version") => string(UN_VERSION),
        ("os", "tickCount") | ("sys", "tickCount") => int(
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64).unwrap_or(0),
        ),
        ("sys", "getcalldepth") | ("sys", "getrecursionlimit") => int(1000),
        ("os", "sep") | ("sys", "sep") => string(SEP_WINDOWS),
        ("os", "pathsep") | ("sys", "pathsep") => string(PATHSEP_WINDOWS),
        ("os", "linesep") | ("sys", "linesep") => string(LINESEP_WINDOWS),

        ("fs", "exists") => match str_arg(args, 0) { Ok(s) => boolean(fs_path(&s).exists()), Err(e) => e },
        ("fs", "file") => match str_arg(args, 0) { Ok(s) => boolean(fs_path(&s).is_file()), Err(e) => e },
        ("fs", "dir") => match str_arg(args, 0) { Ok(s) => boolean(fs_path(&s).is_dir()), Err(e) => e },
        ("fs", "readonly") => match str_arg(args, 0) {
            Ok(s) => match fs::metadata(&s) {
                Ok(m) => boolean(m.permissions().readonly()),
                Err(_) => err("path not found"),
            },
            Err(e) => e,
        },
        ("fs", "hidden") => match str_arg(args, 0) {
            Ok(s) => {
                if !fs_path(&s).exists() { return err("file not found"); }
                boolean(fs_path(&s).file_name()
                    .and_then(|n| n.to_str()).map(|n| n.starts_with('.')).unwrap_or(false))
            }
            Err(e) => e,
        },
        ("fs", "size") => match str_arg(args, 0) {
            Ok(s) => match fs::metadata(&s) {
                Ok(m) if m.is_file() => int(m.len() as i64),
                _ => err("file not found"),
            },
            Err(e) => e,
        },
        ("fs", "join") => {
            let mut it = args.iter();
            let mut p = match it.next().and_then(|p| as_str(*p)) { Some(s) => fs_path(&s), None => return err("fs.join: expected str") };
            for part in it {
                if let Some(s) = as_str(*part) { p.push(s); }
            }
            string(&p.to_string_lossy())
        }
        ("fs", "parent") => match str_arg(args, 0) {
            Ok(s) => string(&platform_path(&fs_path(&s).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())),
            Err(e) => e,
        },
        ("fs", "name") => match str_arg(args, 0) {
            Ok(s) => string(&platform_path(&fs_path(&s).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())),
            Err(e) => e,
        },
        ("fs", "stem") => match str_arg(args, 0) {
            Ok(s) => string(&fs_path(&s).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
            Err(e) => e,
        },
        ("fs", "ext") => match str_arg(args, 0) {
            Ok(s) => string(&fs_path(&s).extension()
                .map(|n| format!(".{}", n.to_string_lossy())).unwrap_or_default()),
            Err(e) => e,
        },
        ("fs", "cwd") => string(&std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()),
        ("fs", "temp") => string(&std::env::temp_dir().to_string_lossy()),
        ("fs", "home") => string(&std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default()),
        ("fs", "exe") => string(&std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()),
        ("fs", "app") => string(&std::env::current_exe().map(|p| p.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()).unwrap_or_default()),
        ("fs", "absolute") => match str_arg(args, 0) {
            Ok(s) => {
                let p = fs_path(&s);
                let full = if p.is_absolute() { p } else {
                    std::env::current_dir().unwrap_or_default().join(p)
                };
                string(&platform_path(&std::fs::canonicalize(&full).unwrap_or(full).to_string_lossy()))
            }
            Err(e) => e,
        },
        ("fs", "normalize") => match str_arg(args, 0) {
            Ok(s) => {
                let p = fs_path(&s);
                let full = if p.is_absolute() { p } else { std::env::current_dir().unwrap_or_default().join(p) };
                let mut parts: Vec<String> = Vec::new();
                use std::path::Component;
                for c in full.components() {
                    match c {
                        Component::ParentDir => { parts.pop(); }
                        Component::CurDir => {}
                        other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
                    }
                }
                let sep = if cfg!(windows) { "\\" } else { "/" };
                string(&parts.join(sep))
            }
            Err(e) => e,
        },
        ("fs", "relative") => {
            let a = match str_arg(args, 0) { Ok(s) => s, Err(e) => return e };
            let b = match str_arg(args, 1) { Ok(s) => s, Err(e) => return e };
            let from = std::fs::canonicalize(&a).unwrap_or_else(|_| fs_path(&a));
            let to = std::fs::canonicalize(&b).unwrap_or_else(|_| fs_path(&b));
            let rest = to.strip_prefix(&from).unwrap_or(&to);
            string(&platform_path(&rest.to_string_lossy()))
        }
        ("fs", "created") | ("fs", "modified") | ("fs", "accessed") => match str_arg(args, 0) {
            Ok(s) => {
                let mt = match fs::metadata(&s) {
                    Ok(m) => m,
                    Err(_) => return err("file not found"),
                };
                let t = if name == "created" { mt.created() }
                    else if name == "modified" { mt.modified() }
                    else { mt.modified() };
                match t {
                    Ok(time) => string(&iso_time(time)),
                    Err(_) => err("file not found"),
                }
            }
            Err(e) => e,
        },

        ("re", "escape") => match str_arg(args, 0) { Ok(s) => string(&crate::regex::escape(&s)), Err(e) => e },
        ("re", "test") | ("re", "search") | ("re", "match") => {
            match (str_arg(args, 0), str_arg(args, 1)) {
                (Ok(p), Ok(t)) => match regex_compile(&p) {
                    Ok(re) => {
                        let chars: Vec<char> = t.chars().collect();
                        match re.find_at(&chars, 0) {
                            Ok(found) => match found {
                                None => {
                                    if name == "test" { boolean(false) }
                                    else { crate::obj::unrt_obj_none() }
                                }
                                Some(m) => {
                                    if name == "test" {
                                        boolean(true)
                                    } else if name == "match" && m.start != 0 {
                                        crate::obj::unrt_obj_none()
                                    } else {
                                        string(&chars[m.start..m.end].iter().collect::<String>())
                                    }
                                }
                            },
                            Err(_) => err("regex backtracking limit exceeded"),
                        }
                    }
                    Err(e) => e,
                },
                (Err(e), _) | (_, Err(e)) => e,
            }
        }
        ("re", "find_all") => match (str_arg(args, 0), str_arg(args, 1)) {
            (Ok(p), Ok(t)) => match regex_compile(&p) {
                Ok(re) => {
                    let chars: Vec<char> = t.chars().collect();
                    match re.find_all(&chars) {
                        Ok(ms) => list(ms.iter().map(|m| string(&chars[m.start..m.end].iter().collect::<String>())).collect()),
                        Err(_) => err("regex backtracking limit exceeded"),
                    }
                }
                Err(e) => e,
            },
            (Err(e), _) | (_, Err(e)) => e,
        },
        ("re", "replace") | ("re", "replace_all") => {
            match (str_arg(args, 0), str_arg(args, 1), str_arg(args, 2)) {
                (Ok(p), Ok(t), Ok(r)) => match regex_compile(&p) {
                    Ok(re) => {
                        let chars: Vec<char> = t.chars().collect();
                        match re.find_all(&chars) {
                            Ok(ms) => {
                                let mut out = String::new();
                                let mut pos = 0;
                                let limit = if name == "replace" { 1 } else { usize::MAX };
                                for m in ms.iter().take(limit) {
                                    out.extend(&chars[pos..m.start]);
                                    out.push_str(&r);
                                    pos = m.end;
                                }
                                out.extend(&chars[pos..]);
                                string(&out)
                            }
                            Err(_) => err("regex backtracking limit exceeded"),
                        }
                    }
                    Err(e) => e,
                },
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => e,
            }
        }
        ("re", "split") => match (str_arg(args, 0), str_arg(args, 1)) {
            (Ok(p), Ok(t)) => match regex_compile(&p) {
                Ok(re) => {
                    let chars: Vec<char> = t.chars().collect();
                    match re.find_all(&chars) {
                        Ok(ms) => {
                            let mut out = Vec::new();
                            let mut pos = 0;
                            for m in &ms {
                                out.push(string(&chars[pos..m.start].iter().collect::<String>()));
                                pos = m.end;
                            }
                            out.push(string(&chars[pos..].iter().collect::<String>()));
                            list(out)
                        }
                        Err(_) => err("regex backtracking limit exceeded"),
                    }
                }
                Err(e) => e,
            },
            (Err(e), _) | (_, Err(e)) => e,
        },

        ("time", "monotonic") | ("time", "monotonic_ms") | ("time", "epoch") | ("time", "epoch_ms")
        | ("time", "now") => {
            static START: std::sync::LazyLock<std::time::Instant> = std::sync::LazyLock::new(std::time::Instant::now);
            let mono_ms = START.elapsed().as_millis() as i64;
            let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64).unwrap_or(0);
            match name {
                "monotonic" => float(START.elapsed().as_micros() as f64 / 1_000_000.0),
                "monotonic_ms" => int(mono_ms),
                "epoch_ms" => int(ms),
                _ => int(ms / 1000),
            }
        }

        ("types", n @ ("u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64")) => {
            match int_arg(args, 0) {
                Ok(v) => {
                    let ok = match n {
                        "u8" => (0..=u8::MAX as i64).contains(&v),
                        "u16" => (0..=u16::MAX as i64).contains(&v),
                        "u32" => (0..=u32::MAX as i64).contains(&v),
                        "u64" => v >= 0,
                        "i8" => (i8::MIN as i64..=i8::MAX as i64).contains(&v),
                        "i16" => (i16::MIN as i64..=i16::MAX as i64).contains(&v),
                        "i32" => (i32::MIN as i64..=i32::MAX as i64).contains(&v),
                        _ => true,
                    };
                    if ok { int(v) } else {
                        let (min, max) = match n {
                            "u8" => (0, u8::MAX as i64), "u16" => (0, u16::MAX as i64),
                            "u32" => (0, u32::MAX as i64), "u64" => (0, i64::MAX),
                            "i8" => (i8::MIN as i64, i8::MAX as i64),
                            "i16" => (i16::MIN as i64, i16::MAX as i64),
                            "i32" => (i32::MIN as i64, i32::MAX as i64),
                            _ => (i64::MIN, i64::MAX),
                        };
                        crate::obj::panic_err(&format!("{n}: value {v} out of range ({min}..={max})"))
                    }
                }
                Err(e) => e,
            }
        }
        ("types", "f32") => match num_arg(args, 0) { Ok(v) => float(v as f32 as f64), Err(e) => e },
        ("types", "f64") => match num_arg(args, 0) { Ok(v) => float(v), Err(e) => e },

        _ => err(&format!("native module '{m}.{name}' is not supported in a no-VM executable")),
    }
}

#[no_mangle]
pub extern "C" fn unrt_call_native(
    module: *const u8,
    module_len: usize,
    member: *const u8,
    member_len: usize,
    args: *const u64,
    argc: usize,
) -> u64 {
    let (m, name) = unsafe {
        (
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(module, module_len)),
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(member, member_len)),
        )
    };
    let vals = unsafe { std::slice::from_raw_parts(args, argc) };
    dispatch(m, name, vals)
}
