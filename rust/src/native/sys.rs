//! `Un.Native.Sys` — the native `sys` module. every function delegates to `os`.

use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj};
use crate::runtime::scope::{self, Scope};
use crate::runtime::world;

use super::{arg, native};

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "env".to_owned(),
            native(closure, "env", vec![arg("name", true, false, false)], env_impl),
        ),
        (
            "environ".to_owned(),
            native(closure, "environ", Vec::new(), environ_impl),
        ),
        (
            "expand_vars".to_owned(),
            native(
                closure,
                "expand_vars",
                vec![arg("text", true, false, false)],
                expand_vars_impl,
            ),
        ),
        ("pid".to_owned(), native(closure, "pid", Vec::new(), pid_impl)),
        ("name".to_owned(), native(closure, "name", Vec::new(), name_impl)),
        ("arch".to_owned(), native(closure, "arch", Vec::new(), arch_impl)),
        (
            "version".to_owned(),
            native(closure, "version", Vec::new(), version_impl),
        ),
        (
            "cpuCount".to_owned(),
            native(closure, "cpuCount", Vec::new(), cpu_count_impl),
        ),
        (
            "is64bit".to_owned(),
            native(closure, "is64bit", Vec::new(), is_64bit_impl),
        ),
        (
            "hostname".to_owned(),
            native(closure, "hostname", Vec::new(), hostname_impl),
        ),
        (
            "username".to_owned(),
            native(closure, "username", Vec::new(), username_impl),
        ),
        ("args".to_owned(), native(closure, "args", Vec::new(), args_impl)),
        ("sep".to_owned(), native(closure, "sep", Vec::new(), sep_impl)),
        (
            "pathsep".to_owned(),
            native(closure, "pathsep", Vec::new(), pathsep_impl),
        ),
        (
            "linesep".to_owned(),
            native(closure, "linesep", Vec::new(), linesep_impl),
        ),
        (
            "tickCount".to_owned(),
            native(closure, "tickCount", Vec::new(), tick_count_impl),
        ),
        (
            "getcalldepth".to_owned(),
            native(closure, "getcalldepth", Vec::new(), get_call_depth_impl),
        ),
        (
            "setcalldepth".to_owned(),
            native(
                closure,
                "setcalldepth",
                vec![arg("depth", true, false, false)],
                set_call_depth_impl,
            ),
        ),
        (
            "getrecursionlimit".to_owned(),
            native(closure, "getrecursionlimit", Vec::new(), get_call_depth_impl),
        ),
        (
            "setrecursionlimit".to_owned(),
            native(
                closure,
                "setrecursionlimit",
                vec![arg("depth", true, false, false)],
                set_call_depth_impl,
            ),
        ),
    ]
}

fn env_impl(scope: &Scope) -> Obj {
    let name = scope::get(scope, "name");
    super::os::env_of(&name)
}

fn environ_impl(_scope: &Scope) -> Obj {
    super::os::environ_of()
}

fn expand_vars_impl(scope: &Scope) -> Obj {
    let text = scope::get(scope, "text");
    super::os::expand_vars_of(&text)
}

fn pid_impl(_scope: &Scope) -> Obj {
    obj::int(std::process::id() as i64)
}

fn name_impl(_scope: &Scope) -> Obj {
    if cfg!(target_os = "windows") {
        return obj::str("windows");
    }
    if cfg!(target_os = "linux") {
        return obj::str("linux");
    }
    if cfg!(target_os = "macos") {
        return obj::str("macos");
    }
    obj::str("unknown")
}

fn arch_impl(_scope: &Scope) -> Obj {
    if cfg!(target_arch = "x86_64") {
        obj::str("X64")
    } else if cfg!(target_arch = "aarch64") {
        obj::str("Arm64")
    } else if cfg!(target_arch = "x86") {
        obj::str("X86")
    } else {
        obj::str("unknown")
    }
}

fn version_impl(_scope: &Scope) -> Obj {
    obj::str(&format!(
        "{} {}",
        if cfg!(target_os = "windows") { "Microsoft Windows NT" } else { "Unix" },
        "10.0.0"
    ))
}

fn cpu_count_impl(_scope: &Scope) -> Obj {
    obj::int(std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1))
}

fn is_64bit_impl(_scope: &Scope) -> Obj {
    obj::boolean(cfg!(target_pointer_width = "64"))
}

fn hostname_impl(_scope: &Scope) -> Obj {
    obj::str(&std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default())
}

fn username_impl(_scope: &Scope) -> Obj {
    obj::str(&std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default())
}

fn args_impl(_scope: &Scope) -> Obj {
    let args: Vec<Obj> = std::env::args().map(|a| obj::str(&a)).collect();
    crate::runtime::collections::list(args)
}

fn sep_impl(_scope: &Scope) -> Obj {
    obj::str(if cfg!(target_os = "windows") { "\\" } else { "/" })
}

fn pathsep_impl(_scope: &Scope) -> Obj {
    obj::str(if cfg!(target_os = "windows") { ";" } else { ":" })
}

fn linesep_impl(_scope: &Scope) -> Obj {
    obj::str(crate::error::nl())
}

fn tick_count_impl(_scope: &Scope) -> Obj {
    obj::int(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0))
}

fn get_call_depth_impl(_scope: &Scope) -> Obj {
    obj::int(world::max_recursion_depth() as i64)
}

fn set_call_depth_impl(scope: &Scope) -> Obj {
    let depth = scope::get(scope, "depth");

    let value = match crate::runtime::obj::payload(&depth) {
        crate::runtime::obj::Payload::Int(v) => v,
        _ => return obj::err("expected 'depth' to be int"),
    };

    if value < 1 {
        return obj::err("depth must be >= 1");
    }

    world::set_max_recursion_depth(value as usize);
    obj::none()
}
