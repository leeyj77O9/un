#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod imp {
    use std::cell::Cell;

    #[link(wasm_import_module = "env")]
    extern "C" {
        fn unvm_monotonic_nanos() -> f64;
        fn unvm_unix_nanos() -> f64;
    }
    thread_local! {
        static TICK: Cell<u64> = const { Cell::new(1) };
    }
    fn counter_nanos() -> u64 {
        TICK.with(|t| {
            let now = t.get();
            t.set(now + 1);
            now
        })
    }
    fn host(f: unsafe extern "C" fn() -> f64) -> Option<u64> {
        let value = unsafe { f() };
        if value.is_finite() && value > 0.0 {
            Some(value as u64)
        } else {
            None
        }
    }
    pub fn monotonic_nanos() -> u64 {
        host(unvm_monotonic_nanos).unwrap_or_else(counter_nanos)
    }
    pub fn unix_nanos() -> u64 {
        host(unvm_unix_nanos).unwrap_or(0)
    }
    pub fn sleep_ms(_milliseconds: u64) {}

    pub fn seed_nanos() -> u64 {
        monotonic_nanos().wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(counter_nanos())
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

    pub fn seed_nanos() -> u64 {
        unix_nanos()
    }
}
pub fn monotonic_nanos() -> u64 {
    imp::monotonic_nanos()
}
pub fn monotonic_ms() -> i64 {
    (imp::monotonic_nanos() / 1_000_000) as i64
}
pub fn unix_nanos() -> u64 {
    imp::unix_nanos()
}
pub fn unix_secs() -> i64 {
    (imp::unix_nanos() / 1_000_000_000) as i64
}
pub fn unix_millis() -> i64 {
    (imp::unix_nanos() / 1_000_000) as i64
}
pub fn sleep_ms(milliseconds: u64) {
    imp::sleep_ms(milliseconds)
}
pub fn seed_nanos() -> u64 {
    imp::seed_nanos()
}
