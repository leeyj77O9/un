//! the `Stopwatch` member table — `Un.Object.Util.Stopwatch`'s `[Native]` members.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload, StopwatchState};
use crate::runtime::scope::{self, Scope};

use super::native;

/// the .NET tick unit is 100 ns.
const TICKS_PER_SECOND: i64 = 10_000_000;

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "start".to_owned(),
            native(closure, "start", Vec::new(), start),
        ),
        (
            "stop".to_owned(),
            native(closure, "stop", Vec::new(), stop),
        ),
        (
            "reset".to_owned(),
            native(closure, "reset", Vec::new(), reset),
        ),
        (
            "lap".to_owned(),
            native(closure, "lap", Vec::new(), lap),
        ),
        (
            "restart".to_owned(),
            native(closure, "restart", Vec::new(), restart),
        ),
        (
            "is_running".to_owned(),
            native(closure, "is_running", Vec::new(), is_running),
        ),
        (
            "tick".to_owned(),
            native(closure, "tick", Vec::new(), tick),
        ),
        ("ms".to_owned(), native(closure, "ms", Vec::new(), ms)),
        (
            "seconds".to_owned(),
            native(closure, "seconds", Vec::new(), seconds),
        ),
        (
            "minutes".to_owned(),
            native(closure, "minutes", Vec::new(), minutes),
        ),
        (
            "hours".to_owned(),
            native(closure, "hours", Vec::new(), hours),
        ),
        (
            "elapsed".to_owned(),
            native(closure, "elapsed", Vec::new(), elapsed),
        ),
    ]
}

/// a new stopwatch object, the reference's `new Stopwatch()`.
pub fn new_stopwatch() -> Obj {
    obj::obj_new(
        "stopwatch",
        Payload::Stopwatch(Rc::new(RefCell::new(StopwatchState::default()))),
    )
}

/// the receiver of a member call, as a `Stopwatch` payload.
fn watch_self(scope: &Scope) -> Option<Rc<RefCell<StopwatchState>>> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::Stopwatch(state) => Some(state.clone()),
        _ => None,
    }
}

/// the elapsed ticks of the timer, the running run included.
fn elapsed_ticks(state: &StopwatchState) -> i64 {
    if state.running {
        state.elapsed + (now_ticks() - state.started)
    } else {
        state.elapsed
    }
}

/// the current tick count, the .NET ticks are 100 ns.
fn now_ticks() -> i64 {
    let started = START.with(|s| *s.borrow());
    started.elapsed().as_nanos() as i64 / 100
}

thread_local! {
    /// the moment the process started, the tick counts are relative to it
    static START: RefCell<Instant> = RefCell::new(Instant::now());
}

fn start(scope: &Scope) -> Obj {
    let Some(state) = watch_self(scope) else {
        return obj::err("expected 'stopwatch' object");
    };

    let mut state = state.borrow_mut();
    state.elapsed = 0;
    state.started = now_ticks();
    state.running = true;

    obj::none()
}

fn stop(scope: &Scope) -> Obj {
    let Some(state) = watch_self(scope) else {
        return obj::err("expected 'stopwatch' object");
    };

    let mut state = state.borrow_mut();
    if state.running {
        state.elapsed += now_ticks() - state.started;
        state.running = false;
    }

    obj::none()
}

fn reset(scope: &Scope) -> Obj {
    let Some(state) = watch_self(scope) else {
        return obj::err("expected 'stopwatch' object");
    };

    let mut state = state.borrow_mut();
    state.elapsed = 0;
    state.started = 0;
    state.running = false;
    state.lap = 0;

    obj::none()
}

fn lap(scope: &Scope) -> Obj {
    let Some(state) = watch_self(scope) else {
        return obj::err("expected 'stopwatch' object");
    };

    let mut state = state.borrow_mut();
    let now = elapsed_ticks(&state);
    let diff = now - state.lap;
    state.lap = now;

    obj::int(diff)
}

fn restart(scope: &Scope) -> Obj {
    let Some(state) = watch_self(scope) else {
        return obj::err("expected 'stopwatch' object");
    };

    let mut state = state.borrow_mut();
    state.elapsed = 0;
    state.started = now_ticks();
    state.running = true;

    obj::none()
}

fn is_running(scope: &Scope) -> Obj {
    let Some(state) = watch_self(scope) else {
        return obj::err("expected 'stopwatch' object");
    };

    let running = state.borrow().running;
    obj::boolean(running)
}

fn tick(scope: &Scope) -> Obj {
    let Some(state) = watch_self(scope) else {
        return obj::err("expected 'stopwatch' object");
    };

    let ticks = elapsed_ticks(&state.borrow());
    obj::int(ticks)
}

fn ms(scope: &Scope) -> Obj {
    let Some(state) = watch_self(scope) else {
        return obj::err("expected 'stopwatch' object");
    };

    let ticks = elapsed_ticks(&state.borrow());
    obj::int(ticks / 10_000)
}

fn seconds(scope: &Scope) -> Obj {
    let Some(state) = watch_self(scope) else {
        return obj::err("expected 'stopwatch' object");
    };

    let ticks = elapsed_ticks(&state.borrow());
    obj::float(ticks as f64 / TICKS_PER_SECOND as f64)
}

fn minutes(scope: &Scope) -> Obj {
    let Some(state) = watch_self(scope) else {
        return obj::err("expected 'stopwatch' object");
    };

    let ticks = elapsed_ticks(&state.borrow());
    obj::float(ticks as f64 / TICKS_PER_SECOND as f64 / 60.0)
}

fn hours(scope: &Scope) -> Obj {
    let Some(state) = watch_self(scope) else {
        return obj::err("expected 'stopwatch' object");
    };

    let ticks = elapsed_ticks(&state.borrow());
    obj::float(ticks as f64 / TICKS_PER_SECOND as f64 / 3600.0)
}

fn elapsed(scope: &Scope) -> Obj {
    let Some(state) = watch_self(scope) else {
        return obj::err("expected 'stopwatch' object");
    };

    let ticks = elapsed_ticks(&state.borrow());
    obj::date_obj(ticks)
}
