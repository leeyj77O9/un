//! `Un.Native.Time` — the native `time` module.

use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::runtime::context::Context;
use crate::runtime::date;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};

use super::{arg, native};

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "sleep".to_owned(),
            native(closure, "sleep", vec![arg("milliseconds", true, false, false)], sleep_impl),
        ),
        ("now".to_owned(), native(closure, "now", Vec::new(), now_impl)),
        (
            "stopwatch".to_owned(),
            native(closure, "stopwatch", Vec::new(), stopwatch_impl),
        ),
    ]
}

fn sleep_impl(scope: &Scope) -> Obj {
    let milliseconds = scope::get(scope, "milliseconds");

    let ms = match obj::payload(&milliseconds) {
        Payload::Int(v) => v as i32,
        Payload::Float(v) => (v * 1000.0) as i32,
        _ => {
            let converted = obj::unary(&milliseconds, obj::UnOp::ToInt);
            match obj::payload(&converted) {
                Payload::Int(v) => v as i32,
                _ => {
                    return obj::err(
                        "expected 'time' argument to be of type 'int' or 'float' or convertible to int",
                    )
                }
            }
        }
    };

    thread::sleep(Duration::from_millis(ms as u64));
    obj::none()
}

fn now_impl(_scope: &Scope) -> Obj {
    let now = SystemTime::now();
    let duration = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    // the .NET ticks count from 0001-01-01
    let ticks = date::unix_epoch_ticks() + (duration.as_nanos() / 100) as i64;

    obj::date_obj(ticks)
}

/// the `Stopwatch` object of the reference: it starts when it is created.
fn stopwatch_impl(_scope: &Scope) -> Obj {
    crate::native::stopwatch::new_stopwatch()
}

/// the `date` helper of the runtime, the module needs it for the ticks.
pub fn _unused() {
    let _ = date::TICKS_PER_DAY;
}
