//! Opt-in parallel scheduler spike — Work ID `W-2026-11-25-P05S`.
//!
//! Question under test (implementation plan §6 C-3 / RFC §3.1):
//! "Can an opt-in parallel scheduler be built with `std` alone, without crossbeam?"
//!
//! This file is a **test-only** prototype. It is registered as a `[[test]]`
//! target in `Cargo.toml`, so it is never compiled into `libnrt.a` and never
//! changes the byte-identical native-exe path. Only `std::thread`,
//! `Arc<Mutex<_>>` and `Condvar` are used for synchronisation (no atomics,
//! no external crates).
//!
//! Mapping to the intended P05 scheduler:
//! - `submit` ↔ `go` (push a `Task` onto the shared `VecDeque`)
//! - `join`   ↔ `wait` (wake on `Condvar`, read memoized `(value)` buffer)
//! - `results: HashMap<FutureId, i64>` ↔ the future's result cell; a second
//!   `join` returns the same value and never re-runs the task.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// The spike's task payload: a `Send` closure yielding a single i64 value
/// (standing in for the RFC's `(value, tag, trace, exception)` tuple).
type Job = Box<dyn FnOnce() -> i64 + Send>;

/// Opaque future handle returned by [`Scheduler::submit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FutureId(u64);

/// Failure modes of a `join`. `TimedOut` is a spike-only watchdog so a broken
/// wake-up fails the test instead of hanging the test runner forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError {
    /// The result did not arrive within the supplied timeout (would be a
    /// deadlock in production; the RFC requires self-wait to be rejected).
    TimedOut,
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JoinError::TimedOut => f.write_str("join timed out (possible deadlock)"),
        }
    }
}

impl std::error::Error for JoinError {}

struct Task {
    id: u64,
    job: Job,
}

/// All mutable scheduler state lives behind one `Mutex` guarded by one
/// `Condvar`. Workers wait for tasks *and* joiners wait for results on the
/// same condvar: a worker calls `notify_all()` after storing each result.
struct Shared {
    queue: VecDeque<Task>,
    results: HashMap<u64, i64>,
    closed: bool,
    next_id: u64,
    /// Instrumentation for the "each task runs exactly once" assertion.
    executed: usize,
}

/// A fixed-size pool draining a single shared FIFO queue.
pub struct Scheduler {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    workers: Vec<JoinHandle<()>>,
}

impl Scheduler {
    /// Spawn `threads` workers. `threads == 0` is rejected: with no worker a
    /// `join` could never make progress (the real scheduler must clamp to
    /// `hardware_concurrency()` instead).
    pub fn new(threads: usize) -> Self {
        assert!(threads >= 1, "scheduler needs at least one worker thread");
        let shared = Arc::new((
            Mutex::new(Shared {
                queue: VecDeque::new(),
                results: HashMap::new(),
                closed: false,
                next_id: 0,
                executed: 0,
            }),
            Condvar::new(),
        ));
        let mut workers = Vec::with_capacity(threads);
        for idx in 0..threads {
            let shared = Arc::clone(&shared);
            let handle = thread::Builder::new()
                .name(format!("nrt-spike-worker-{idx}"))
                .spawn(move || worker_loop(shared))
                .expect("spike: failed to spawn worker thread");
            workers.push(handle);
        }
        Self { shared, workers }
    }

    /// Enqueue a task and wake one idle worker. Returns a future handle.
    /// After [`Scheduler::close`] this mirrors the existing native-pool
    /// error string exactly: `"pool is closed"`.
    pub fn submit<F>(&self, job: F) -> Result<FutureId, &'static str>
    where
        F: FnOnce() -> i64 + Send + 'static,
    {
        let (lock, cv) = &*self.shared;
        let mut state = lock.lock().unwrap();
        if state.closed {
            return Err("pool is closed");
        }
        let id = state.next_id;
        state.next_id += 1;
        state.queue.push_back(Task {
            id,
            job: Box::new(job),
        });
        drop(state);
        cv.notify_one();
        Ok(FutureId(id))
    }

    /// Wait for a result with the spike's generous watchdog (30 s).
    pub fn join(&self, id: FutureId) -> Result<i64, JoinError> {
        self.join_timeout(id, Duration::from_secs(30))
    }

    /// Wait until the result memo is populated, with an explicit timeout.
    /// Repeated waits on the same future return the memoized value without
    /// re-executing the task.
    pub fn join_timeout(&self, id: FutureId, timeout: Duration) -> Result<i64, JoinError> {
        let (lock, cv) = &*self.shared;
        let mut state = lock.lock().unwrap();
        loop {
            if let Some(value) = state.results.get(&id.0) {
                return Ok(*value);
            }
            let (next, wait) = cv.wait_timeout(state, timeout).unwrap();
            state = next;
            if wait.timed_out() {
                return Err(JoinError::TimedOut);
            }
        }
    }

    /// Signal shutdown, drain everything still queued, then join the workers.
    /// Idempotent: a second call is a no-op.
    pub fn close(&mut self) {
        {
            let (lock, cv) = &*self.shared;
            let mut state = lock.lock().unwrap();
            state.closed = true;
            drop(state);
            cv.notify_all(); // wake idle workers so they observe `closed`
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }

    /// Number of tasks actually executed — used to prove exactly-once.
    pub fn executed_count(&self) -> usize {
        let (lock, _) = &*self.shared;
        lock.lock().unwrap().executed
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.close();
    }
}

fn worker_loop(shared: Arc<(Mutex<Shared>, Condvar)>) {
    let (lock, cv) = &*shared;
    loop {
        let mut state = lock.lock().unwrap();
        // Wait while the queue is empty; break only when closed *and* empty
        // so shutdown drains all pending tasks before the workers exit.
        let task = loop {
            if let Some(task) = state.queue.pop_front() {
                break task;
            }
            if state.closed {
                return;
            }
            state = cv.wait(state).unwrap();
        };
        drop(state);

        let value = (task.job)();

        let mut state = lock.lock().unwrap();
        state.results.insert(task.id, value);
        state.executed += 1;
        drop(state);
        cv.notify_all(); // wake joiners (and idle workers, harmlessly)
    }
}

/// Deterministic per-task computation. The cheap `wrapping` mix produces
/// values in 0..128 so duplicates are common and a multiset comparison is
/// meaningfully stricter than a set comparison.
fn work(i: u64, iters: u64) -> i64 {
    let mut acc = i;
    for k in 0..iters {
        acc = acc
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(k + 1);
    }
    (acc % 128) as i64
}

/// Test helper: order-insensitive view of results as sorted `(value, count)`.
pub fn multiset(values: &[i64]) -> Vec<(i64, usize)> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mut out: Vec<(i64, usize)> = Vec::new();
    for value in sorted {
        match out.last_mut() {
            Some((key, count)) if *key == value => *count += 1,
            _ => out.push((value, 1)),
        }
    }
    out
}

/// Sequential reference: the same work in submission order.
fn run_sequential(n: u64, iters: u64) -> Vec<i64> {
    (0..n).map(|i| work(i, iters)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIGHT_ITERS: u64 = 2_000;

    #[test]
    fn parallel_multiset_matches_sequential_for_100_and_1000_tasks() {
        for &n in &[100u64, 1_000u64] {
            let mut sched = Scheduler::new(4);
            let futures: Vec<FutureId> = (0..n)
                .map(|i| {
                    sched
                        .submit(move || work(i, LIGHT_ITERS))
                        .expect("submit before close")
                })
                .collect();
            let mut got = Vec::with_capacity(n as usize);
            for future in futures {
                got.push(sched.join(future).expect("join must not deadlock"));
            }
            sched.close();
            assert_eq!(sched.executed_count(), n as usize, "n={n} exactly-once");
            assert_eq!(
                multiset(&got),
                multiset(&run_sequential(n, LIGHT_ITERS)),
                "n={n} multiset identity"
            );
        }
    }

    #[test]
    fn duplicate_wait_is_memoized_and_task_runs_once() {
        let mut sched = Scheduler::new(2);
        let future = sched.submit(|| work(7, LIGHT_ITERS)).unwrap();
        let first = sched.join(future).unwrap();
        assert_eq!(sched.executed_count(), 1);
        let second = sched.join(future).unwrap();
        assert_eq!(first, second);
        assert_eq!(sched.executed_count(), 1, "re-wait must not re-execute");
        sched.close();
    }

    #[test]
    fn close_drains_pending_tasks_and_rejects_new_submits() {
        const N: u64 = 50;
        let mut sched = Scheduler::new(3);
        let futures: Vec<FutureId> = (0..N)
            .map(|i| {
                sched
                    .submit(move || work(i, LIGHT_ITERS))
                    .expect("submit before close")
            })
            .collect();
        sched.close();
        assert_eq!(sched.submit(|| 1).unwrap_err(), "pool is closed");
        let got: Vec<i64> = futures
            .iter()
            .map(|future| sched.join(*future).expect("drained result ready"))
            .collect();
        assert_eq!(sched.executed_count(), N as usize);
        assert_eq!(
            multiset(&got),
            multiset(&run_sequential(N, LIGHT_ITERS)),
            "drain must keep every task's result"
        );
    }

    #[test]
    fn join_timeout_guard_proves_no_silent_deadlock() {
        let mut sched = Scheduler::new(1);
        // No task 999 was ever submitted: a buggy condvar wait would hang
        // here; the watchdog must return instead.
        let err = sched
            .join_timeout(FutureId(999), Duration::from_millis(50))
            .unwrap_err();
        assert_eq!(err, JoinError::TimedOut);
        sched.close();
    }

    #[test]
    fn thread_count_scaling_reference_timing() {
        const N: u64 = 400;
        const ITERS: u64 = 50_000;
        let expected = multiset(&run_sequential(N, ITERS));
        for &threads in &[1usize, 2, 4, 8] {
            let mut sched = Scheduler::new(threads);
            let start = Instant::now();
            let futures: Vec<FutureId> = (0..N)
                .map(|i| {
                    sched
                        .submit(move || work(i, ITERS))
                        .expect("submit before close")
                })
                .collect();
            let mut got = Vec::with_capacity(N as usize);
            for future in futures {
                got.push(sched.join(future).expect("join must not deadlock"));
            }
            let elapsed = start.elapsed();
            sched.close();
            assert_eq!(sched.executed_count(), N as usize, "threads={threads}");
            assert_eq!(
                multiset(&got), expected,
                "threads={threads} must preserve the result multiset"
            );
            eprintln!(
                "[spike] threads={threads} tasks={N} iters/task={ITERS} elapsed={elapsed:?}"
            );
        }
    }
}
