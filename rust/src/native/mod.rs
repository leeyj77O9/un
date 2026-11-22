use std::rc::Rc;
use crate::runtime::context::{self, Context};
use crate::runtime::function::{self, ArgData};
use crate::runtime::modules;
use crate::runtime::obj::Obj;
use crate::runtime::scope::Scope;
use crate::source::Source;
use crate::runtime::types::Types;
use crate::runtime::world;
pub mod atomic;
pub mod builtin;
pub mod bytes;
pub mod codes;
pub mod date_members;
pub mod flow;
pub mod fs;
pub mod inspect;
pub mod io;
pub mod iter;
pub mod math;
pub mod messages;
pub mod members;
pub mod net;
pub mod os;
pub mod random;
pub mod re;
pub mod regex;
pub mod stopwatch;
pub mod stream;
pub mod sys;
pub mod time;
pub mod types;
pub mod ws;
pub mod http;
pub fn init(path: &str) {
    world::reset();
    world::set_path(path);
    modules::init_classes();

    let closure = native_closure();

    members::init_members(&closure);
    world::register_native("io", io::members(&closure));
    world::register_native("inspect", inspect::members(&closure));
    world::register_native("math", math::members(&closure));
    world::register_native("iter", iter::members(&closure));
    world::register_native("os", os::members(&closure));
    world::register_native("sys", sys::members(&closure));
    world::register_native("fs", fs::members(&closure));
    world::register_native("bytes", bytes::members(&closure));
    world::register_native("types", types::members(&closure));
    world::register_native("time", time::members(&closure));
    world::register_native("random", random::members(&closure));
    world::register_native("flow", flow::members(&closure));
    world::register_native("atomic", atomic::members(&closure));
    world::register_native("re", re::members(&closure));
    world::register_native("codes", codes::members(&closure));
    world::register_native("http", http::members(&closure));
    world::register_native("ws", ws::members(&closure));
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

/// 공통 인자 combinator. 각 native 모듈이 로컬 클로저/함수로 중복 정의하던 것을 한곳에 모은다.
/// `ess`/`opt`는 단일 `ArgData`를, `one`/`two`/`three`는 `Vec<ArgData>`를 만든다.
pub fn ess(name: &str) -> ArgData {
    arg(name, true, false, false)
}
pub fn opt(name: &str) -> ArgData {
    arg(name, false, true, false)
}
pub fn one(name: &str) -> Vec<ArgData> {
    vec![ess(name)]
}
pub fn two(a: &str, b: &str) -> Vec<ArgData> {
    vec![ess(a), ess(b)]
}
pub fn three(a: &str, b: &str, c: &str) -> Vec<ArgData> {
    vec![ess(a), ess(b), ess(c)]
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
