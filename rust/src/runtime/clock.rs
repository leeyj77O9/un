//! `runtime::clock` — the clock source every native time API reads.
//!
//! The playground builds this crate for `wasm32-unknown-unknown`, a target with no
//! operating system. There `std::time::{Instant, SystemTime}` panic on first use
//! (`RuntimeError: unreachable`), so calling any time API trapped the whole VM.
//! This module hides the split behind one place:
//!
//! - other targets: `Instant` for the monotonic source (the same role the reference's
//!   process clock plays), `SystemTime` for the wall clock;
//! - `wasm32-unknown-unknown`: a strictly increasing counter (one nanosecond per
//!   call) for the monotonic source, the Unix epoch (`0`) for the wall clock, and a
//!   no-op `sleep`. Deterministic and incapable of trapping, which is the point.

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod imp {
    use std::cell::Cell;

    thread_local! {
        static TICK: Cell<u64> = const { Cell::new(1) };
    }

    pub fn monotonic_nanos() -> u64 {
        TICK.with(|t| {
            let now = t.get();
            t.set(now + 1);
            now
        })
    }

    pub fn unix_nanos() -> u64 {
        0
    }

    pub fn sleep_ms(_milliseconds: u64) {}

    /// a seed that differs on every call, so `random()` does not repeat across calls.
    pub fn seed_nanos() -> u64 {
        TICK.with(|t| {
            let now = t.get();
            t.set(now + 1);
            now.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        })
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod imp {
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    thread_local! {
        static START: Instant = Instant::now();
    }

    pub fn monotonic_nanos() -> u64 {
        START.with(|s| s.elapsed().as_nanos() as u64)
    }

    pub fn unix_nanos() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }

    pub fn sleep_ms(milliseconds: u64) {
        std::thread::sleep(Duration::from_millis(milliseconds));
    }

    /// a value seeded from the wall clock, like the reference's `new Random()`.
    pub fn seed_nanos() -> u64 {
        unix_nanos()
    }
}

/// nanoseconds since the monotonic source started (process start or, on wasm, first call).
pub fn monotonic_nanos() -> u64 {
    imp::monotonic_nanos()
}

/// whole milliseconds since the monotonic source started.
pub fn monotonic_ms() -> i64 {
    (imp::monotonic_nanos() / 1_000_000) as i64
}

/// nanoseconds since the Unix epoch, `0` on `wasm32-unknown-unknown`.
pub fn unix_nanos() -> u64 {
    imp::unix_nanos()
}

/// whole seconds since the Unix epoch, `0` on `wasm32-unknown-unknown`.
pub fn unix_secs() -> i64 {
    (imp::unix_nanos() / 1_000_000_000) as i64
}

/// whole milliseconds since the Unix epoch, `0` on `wasm32-unknown-unknown`.
pub fn unix_millis() -> i64 {
    (imp::unix_nanos() / 1_000_000) as i64
}

/// sleep in milliseconds; a no-op on `wasm32-unknown-unknown` where nothing can be slept.
pub fn sleep_ms(milliseconds: u64) {
    imp::sleep_ms(milliseconds)
}

/// a seed that differs between calls on every target, unlike a fixed `0`.
pub fn seed_nanos() -> u64 {
    imp::seed_nanos()
}
