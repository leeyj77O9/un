//! `Un.Native.Time` — the native `time` module.

use crate::runtime::clock;
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
            "utc_now".to_owned(),
            native(closure, "utc_now", Vec::new(), utc_now_impl),
        ),
        ("epoch".to_owned(), native(closure, "epoch", Vec::new(), epoch_impl)),
        (
            "epoch_ms".to_owned(),
            native(closure, "epoch_ms", Vec::new(), epoch_ms_impl),
        ),
        (
            "from_epoch".to_owned(),
            native(
                closure,
                "from_epoch",
                vec![arg("seconds", true, false, false)],
                from_epoch_impl,
            ),
        ),
        (
            "from_epoch_ms".to_owned(),
            native(
                closure,
                "from_epoch_ms",
                vec![arg("milliseconds", true, false, false)],
                from_epoch_ms_impl,
            ),
        ),
        (
            "monotonic".to_owned(),
            native(closure, "monotonic", Vec::new(), monotonic_impl),
        ),
        (
            "monotonic_ms".to_owned(),
            native(closure, "monotonic_ms", Vec::new(), monotonic_ms_impl),
        ),
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

    clock::sleep_ms(ms.max(0) as u64);
    obj::none()
}

fn now_impl(_scope: &Scope) -> Obj {
    // the .NET ticks count from 0001-01-01
    let ticks = date::unix_epoch_ticks() + (clock::unix_nanos() / 100) as i64;

    obj::date_obj(ticks)
}

/// `Time.UtcNow()` — `new Date(DateTime.UtcNow)`. the `Date` payload is the .NET tick
/// count of the instant, which is what `now_impl` already stores, so `utc_now` prints
/// exactly the reference's UTC wall clock. `now` is the reference's local wall clock,
/// and keeping the UTC instant is the recorded difference (D-032).
fn utc_now_impl(scope: &Scope) -> Obj {
    now_impl(scope)
}

/// `Time.Epoch()` — `DateTimeOffset.UtcNow.ToUnixTimeSeconds()`.
fn epoch_impl(_scope: &Scope) -> Obj {
    obj::int(clock::unix_secs())
}

/// `Time.EpochMs()` — `DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()`.
fn epoch_ms_impl(_scope: &Scope) -> Obj {
    obj::int(clock::unix_millis())
}

/// `Time.FromEpoch(seconds)` — `DateTimeOffset.FromUnixTimeSeconds(s).LocalDateTime`.
/// the reference lands on the machine's local calendar, the runtime keeps the UTC
/// instant (D-032), so the ticks are `unix epoch + seconds` and `timestamp()` of the
/// result round-trips the input.
fn from_epoch_impl(scope: &Scope) -> Obj {
    let seconds = scope::get(scope, "seconds");

    let s = match obj::payload(&seconds) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'seconds' argument to be of type 'int'"),
    };

    obj::date_obj(date::unix_epoch_ticks() + s * date::TICKS_PER_SEC)
}

/// `Time.FromEpochMs(ms)` — `DateTimeOffset.FromUnixTimeMilliseconds(ms).LocalDateTime`.
fn from_epoch_ms_impl(scope: &Scope) -> Obj {
    let milliseconds = scope::get(scope, "milliseconds");

    let ms = match obj::payload(&milliseconds) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'milliseconds' argument to be of type 'int'"),
    };

    obj::date_obj(date::unix_epoch_ticks() + ms * date::TICKS_PER_MS)
}

/// `Time.Monotonic()` — `Environment.TickCount64 / 1000.0`. the tick count is the
/// milliseconds since the process started, so the runtime's process clock (the same
/// clock the stopwatch measures from) is the monotonic source.
fn monotonic_impl(_scope: &Scope) -> Obj {
    obj::float(clock::monotonic_ms() as f64 / 1000.0)
}

/// `Time.MonotonicMs()` — `Int.From(Environment.TickCount64)`.
fn monotonic_ms_impl(_scope: &Scope) -> Obj {
    obj::int(clock::monotonic_ms())
}

/// the `Stopwatch` object of the reference: it starts when it is created.
fn stopwatch_impl(_scope: &Scope) -> Obj {
    crate::native::stopwatch::new_stopwatch()
}
