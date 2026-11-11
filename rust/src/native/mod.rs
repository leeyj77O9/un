//! `Global.Init` — the native module registry and the global builtin functions.
//!
//! `Global.Init(path)` resets `PATH`, registers every builtin function into the global
//! scope, registers the builtin classes, and finally binds `__name__ = "__main__"`.
//! `[BuiltinModule]` is not `[NativeModule]`, so `builtin` is not a module `use` can bind:
//! `use builtin` falls through to `Global.Import` and reports a missing file.

use std::rc::Rc;

use crate::runtime::context::{self, Context};
use crate::runtime::function::{self, ArgData};
use crate::runtime::global;
use crate::runtime::obj::Obj;
use crate::runtime::scope::Scope;
use crate::source::Source;
use crate::runtime::types::Types;
use crate::runtime::world;

pub mod builtin;
pub mod date_members;
pub mod fs;
pub mod inspect;
pub mod io;
pub mod iter;
pub mod math;
pub mod members;
pub mod os;
pub mod stopwatch;
pub mod sys;
pub mod time;

/// `Global.Init(path)`
pub fn init(path: &str) {
    world::reset();
    world::set_path(path);

    global::init_classes();

    // the reference builds every native on one `Context(Global.GetGlobalScope(), null, null)`,
    // so `CallDepth` is shared state across all native calls
    let closure = native_closure();

    // the `[Native]` members of the builtin types live in `Global.originalClasses`
    members::init_members(&closure);

    world::register_native("io", io::members(&closure));
    world::register_native("inspect", inspect::members(&closure));
    world::register_native("math", math::members(&closure));
    world::register_native("iter", iter::members(&closure));
    world::register_native("os", os::members(&closure));
    world::register_native("sys", sys::members(&closure));
    world::register_native("fs", fs::members(&closure));
    world::register_native("time", time::members(&closure));

    for (name, f) in builtin::members(&closure) {
        world::set_global(&name, f);
    }

    world::set_global("__name__", world::str_pooled("__main__"));
}

fn native_closure() -> Context {
    context::new(
        world::global_scope(),
        Rc::new(Source::new("", "")),
        context::new_frames(),
    )
}

/// `new Arg(info.Name) { IsEssential, IsOptional, IsPositional }`. `DefaultValue` is unset
/// for native arguments, so an unbound parameter binds `none` and the host checks `is_none`
/// in place of the reference's `??=`.
pub fn arg(name: &str, essential: bool, optional: bool, positional: bool) -> ArgData {
    ArgData {
        name: name.to_owned(),
        type_name: Types::single("any"),
        essential,
        optional,
        positional,
        keyword: false,
        default: None,
    }
}

pub fn native(
    closure: &Context,
    name: &str,
    args: Vec<ArgData>,
    host: fn(&Scope) -> Obj,
) -> Obj {
    function::nfn_in(
        name,
        args,
        closure.clone(),
        Rc::new(host) as Rc<dyn Fn(&Scope) -> Obj>,
    )
}
