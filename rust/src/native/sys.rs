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
fn pid_impl(scope: &Scope) -> Obj {
    super::os::pid_impl(scope)
}
fn name_impl(scope: &Scope) -> Obj {
    super::os::name_impl(scope)
}
fn arch_impl(scope: &Scope) -> Obj {
    super::os::arch_impl(scope)
}
fn version_impl(_scope: &Scope) -> Obj {
    // os.version()은 "<OS> (Un <version>)" 형식이지만 sys.version()은 "Un <version>"이다.
    // 형식이 다르므로 위임하지 않고 VM 현재 출력을 그대로 유지한다.
    obj::str(&format!("Un {}", env!("CARGO_PKG_VERSION")))
}
fn cpu_count_impl(scope: &Scope) -> Obj {
    super::os::cpu_count_impl(scope)
}
fn is_64bit_impl(scope: &Scope) -> Obj {
    super::os::is_64bit_impl(scope)
}
fn hostname_impl(scope: &Scope) -> Obj {
    super::os::hostname_impl(scope)
}
fn username_impl(scope: &Scope) -> Obj {
    super::os::username_impl(scope)
}
fn args_impl(scope: &Scope) -> Obj {
    super::os::args_impl(scope)
}
fn sep_impl(scope: &Scope) -> Obj {
    super::os::sep_impl(scope)
}
fn pathsep_impl(scope: &Scope) -> Obj {
    super::os::pathsep_impl(scope)
}
fn linesep_impl(scope: &Scope) -> Obj {
    super::os::linesep_impl(scope)
}
fn tick_count_impl(scope: &Scope) -> Obj {
    super::os::tick_count_impl(scope)
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
