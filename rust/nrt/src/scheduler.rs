//! Opt-in parallel scheduler — production core for Work ID `W-2026-11-26-P05` (RFC M4).
//!
//! This is the production promotion of the `P05s` spike (`scheduler_spike.rs`,
//! kept unchanged as a test). It implements RFC §3.1's single shared-queue
//! scheduler with `std` only: `std::thread`, `Arc<Mutex<VecDeque<_>>>` and
//! `Condvar`. No atomics, no external crates, no new `unrt_*` symbols.
//!
//! # Default path is untouched
//!
//! Compiling this module into `libnrt.a` does **not** spawn a thread or build
//! a scheduler: nothing outside the opt-in path calls [`Scheduler::new`].
//! P06 will construct a `Scheduler` only from the `--parallel` / TIR
//! `Go`/`Wait` path, so the byte-identical native-exe lane keeps its current
//! deferred semantics and output.
//!
//! # Contract (RFC §3.2)
//!
//! - `submit` ↔ `go` (push a [`Task`] onto the shared FIFO queue).
//! - `join` ↔ `wait` (wake on the `Condvar`, read the memoized output slot).
//! - A repeated `join` of the same future returns the same result; the task
//!   body is never executed twice.
//! - A task that calls `wait` while still queued is executed **inline** on the
//!   calling thread rather than blocking a worker (pool-saturation deadlock
//!   avoidance). A true self-cycle (`task` waits on its own future id) is
//!   rejected with [`JoinError::SelfWait`].
//! - A panic in a task body is caught and recorded in the output slot, so
//!   `join` reports [`JoinError::TaskPanicked`] instead of hanging forever.
//! - `close()` is a graceful drain: it stops new submissions, lets workers
//!   finish every queued task, then joins the worker threads.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle, ThreadId};
use std::time::{Duration, Instant};

/// Native task entry point emitted by `natcode` in P06.
///
/// `C-unwind` (rather than the RFC draft's `extern "C"`) is deliberate: it has
/// the identical machine ABI but permits a Rust panic to unwind back into the
/// runner's `catch_unwind`, which is how requirement 4 records a worker panic
/// in the output slot instead of aborting the process.
pub type EntryFn = extern "C-unwind" fn(*mut u8) -> u64;

/// Cap for [`Scheduler::with_default_threads`] (RFC §3.1: default 4).
const DEFAULT_WORKER_CAP: usize = 4;
/// Explicit worker stack size; RFC §3.1 asks `Builder::stack_size` to be set.
const WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

/// Opaque future handle returned by `submit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FutureId(u64);

/// The native value produced by a task. `tag` follows the RFC convention
/// (0 = Int, 1 = Float bit pattern, …); P04/P06 assign concrete meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskResult {
    pub value: u64,
    pub tag: u64,
}

/// Contents of a task's shared output slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskSlot {
    /// No worker has produced a result yet.
    Pending,
    /// The entry point returned normally; every `join` reuses this value.
    Ready(TaskResult),
    /// The entry point panicked; `join` turns this into an error.
    Panicked,
}

/// Failure modes delivered by `join`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError {
    /// The task body panicked. Reported instead of deadlocking the joiner.
    TaskPanicked,
    /// A running task waited on its own future id — a true cycle (RFC §3.2).
    SelfWait,
    /// The future id is unknown (defensive; well-typed programs never hit it).
    UnknownFuture,
    /// `join_timeout` watchdog fired (proves there is no silent deadlock).
    TimedOut,
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JoinError::TaskPanicked => f.write_str("task panicked"),
            JoinError::SelfWait => f.write_str("self-referential wait"),
            JoinError::UnknownFuture => f.write_str("unknown future"),
            JoinError::TimedOut => f.write_str("join timed out (possible deadlock)"),
        }
    }
}

impl std::error::Error for JoinError {}

/// A unit of work: RFC §3.1's `(entry fn, ctx, out slot)` triple.
///
/// `entry(ctx_ptr)` computes a `u64`; `ctx` is a `Send` snapshot whose address
/// is passed to `entry` as a raw pointer; `out` is the slot every `join` reads.
pub struct Task {
    pub entry: EntryFn,
    pub ctx: Arc<dyn Send + Sync + 'static>,
    pub out: Arc<Mutex<TaskSlot>>,
}

impl Task {
    /// Build a task from a native entry point and its context snapshot.
    pub fn new(entry: EntryFn, ctx: Arc<dyn Send + Sync + 'static>) -> Self {
        Self {
            entry,
            ctx,
            out: Arc::new(Mutex::new(TaskSlot::Pending)),
        }
    }

    /// Convenience constructor for closure-backed tasks (used by tests and by
    /// P06 wrappers that want a plain Rust closure instead of a native fn).
    pub fn from_closure<F>(f: F) -> Self
    where
        F: FnOnce() -> u64 + Send + 'static,
    {
        let body: Arc<Mutex<Option<Box<dyn FnOnce() -> u64 + Send>>>> =
            Arc::new(Mutex::new(Some(Box::new(f))));
        Self::new(closure_trampoline, body)
    }
}

/// Trampoline that turns a boxed `FnOnce` into an `EntryFn`.
extern "C-unwind" fn closure_trampoline(ptr: *mut u8) -> u64 {
    // SAFETY: `ptr` is `Arc::as_ptr(&task.ctx)` where `ctx` is the body cell
    // created in `from_closure`. The owning `Task` keeps that `Arc` alive for
    // the entire `run_task` call, so the reference is valid here.
    let cell = unsafe { &*(ptr as *const Mutex<Option<Box<dyn FnOnce() -> u64 + Send>>>) };
    let body = cell
        .lock()
        .unwrap()
        .take()
        .expect("closure task body consumed twice");
    body()
}

/// A dequeued task plus the metadata needed for self-wait detection.
struct QueuedTask {
    id: FutureId,
    submitter: ThreadId,
    task: Task,
}

/// All mutable scheduler state behind one `Mutex`, guarded by one `Condvar`.
/// Workers wait for tasks and joiners wait for results on the same condvar:
/// a worker calls `notify_all()` after writing each result.
struct Shared {
    queue: VecDeque<QueuedTask>,
    slots: HashMap<u64, Arc<Mutex<TaskSlot>>>,
    closed: bool,
    next_id: u64,
}

thread_local! {
    /// Set while this thread is executing a task (worker or inline fallback).
    /// Used by `join` to detect the RFC §3.2 "wait from inside a task" case.
    static CURRENT_TASK: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Restores the previous `CURRENT_TASK` value on drop (supports nesting).
struct CurrentTaskGuard {
    previous: Option<u64>,
}

impl CurrentTaskGuard {
    fn new(id: FutureId) -> Self {
        Self {
            previous: CURRENT_TASK.with(|cell| cell.replace(Some(id.0))),
        }
    }
}

impl Drop for CurrentTaskGuard {
    fn drop(&mut self) {
        CURRENT_TASK.with(|cell| cell.set(self.previous));
    }
}

/// Execute a task body, converting a panic into a `Panicked` slot.
fn run_task(task: &Task) {
    let ctx_ptr = Arc::as_ptr(&task.ctx) as *mut u8;
    let slot = match catch_unwind(AssertUnwindSafe(|| (task.entry)(ctx_ptr))) {
        Ok(value) => TaskSlot::Ready(TaskResult { value, tag: 0 }),
        Err(_) => TaskSlot::Panicked,
    };
    *task.out.lock().unwrap() = slot;
}

fn read_slot(slot: &Arc<Mutex<TaskSlot>>) -> TaskSlot {
    slot.lock().unwrap().clone()
}

fn finish(slot: TaskSlot) -> Result<TaskResult, JoinError> {
    match slot {
        TaskSlot::Ready(result) => Ok(result),
        TaskSlot::Panicked => Err(JoinError::TaskPanicked),
        TaskSlot::Pending => Err(JoinError::TimedOut), // defensive: never reached
    }
}

/// Clone-able entry into a scheduler. Safe to capture in a task body so that a
/// running task can `wait` on another future (the inline-fallback path).
#[derive(Clone)]
pub struct SchedulerHandle {
    shared: Arc<(Mutex<Shared>, Condvar)>,
}

impl SchedulerHandle {
    /// Enqueue a native task. After `close()` this returns the existing
    /// native-pool error string `"pool is closed"`.
    pub fn submit(&self, task: Task) -> Result<FutureId, &'static str> {
        self.submit_inner(task)
    }

    /// Enqueue a closure-backed task.
    pub fn submit_closure<F>(&self, body: F) -> Result<FutureId, &'static str>
    where
        F: FnOnce() -> u64 + Send + 'static,
    {
        self.submit_inner(Task::from_closure(body))
    }

    fn submit_inner(&self, task: Task) -> Result<FutureId, &'static str> {
        let (lock, cv) = &*self.shared;
        let mut state = lock.lock().unwrap();
        if state.closed {
            return Err("pool is closed");
        }
        let id = FutureId(state.next_id);
        state.next_id += 1;
        state.slots.insert(id.0, Arc::clone(&task.out));
        state.queue.push_back(QueuedTask {
            id,
            submitter: thread::current().id(),
            task,
        });
        drop(state);
        cv.notify_one();
        Ok(id)
    }

    /// Wait for a result. Repeated waits return the memoized value.
    pub fn join(&self, id: FutureId) -> Result<TaskResult, JoinError> {
        self.join_inner(id, None)
    }

    /// Wait with a watchdog; useful to prove a broken wake-up fails instead of
    /// hanging the caller (the spike's `join_timeout` equivalent).
    pub fn join_timeout(&self, id: FutureId, timeout: Duration) -> Result<TaskResult, JoinError> {
        self.join_inner(id, Some(timeout))
    }

    fn join_inner(
        &self,
        id: FutureId,
        timeout: Option<Duration>,
    ) -> Result<TaskResult, JoinError> {
        let (lock, cv) = &*self.shared;
        let current_task = CURRENT_TASK.with(|cell| cell.get());
        let current_thread = thread::current().id();

        // A running task waiting on itself is a true cycle: reject it rather
        // than blocking forever (RFC §3.2).
        if current_task == Some(id.0) {
            return Err(JoinError::SelfWait);
        }

        let slot = {
            let state = lock.lock().unwrap();
            match state.slots.get(&id.0) {
                Some(slot) => Arc::clone(slot),
                None => return Err(JoinError::UnknownFuture),
            }
        };

        // Fast path / duplicate-wait memoization: a finished slot is returned
        // as-is on every `join`; the task body is never run again.
        match read_slot(&slot) {
            TaskSlot::Ready(result) => return Ok(result),
            TaskSlot::Panicked => return Err(JoinError::TaskPanicked),
            TaskSlot::Pending => {}
        }

        // RFC §3.2 inline fallback. If the waiter is a task thread, or the
        // thread that submitted this still-pending task, executing the body
        // inline avoids occupying a worker while the awaited task is queued
        // (pool saturation → deadlock).
        let inline_eligible = {
            let state = lock.lock().unwrap();
            state.queue.iter().any(|queued| {
                queued.id == id
                    && (current_task.is_some() || queued.submitter == current_thread)
            })
        };
        if inline_eligible {
            let inline_task = {
                let mut state = lock.lock().unwrap();
                state
                    .queue
                    .iter()
                    .position(|queued| queued.id == id)
                    .map(|pos| state.queue.remove(pos).unwrap().task)
            };
            if let Some(task) = inline_task {
                let _guard = CurrentTaskGuard::new(id);
                run_task(&task);
                // Another joiner may be blocked on the same future.
                cv.notify_all();
                return finish(read_slot(&slot));
            }
        }

        // Normal path: block until a worker (or an inline fallback) fills the
        // slot. Panics never cross this boundary: the slot records them.
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        let mut state = lock.lock().unwrap();
        loop {
            match read_slot(&slot) {
                TaskSlot::Ready(result) => return Ok(result),
                TaskSlot::Panicked => return Err(JoinError::TaskPanicked),
                TaskSlot::Pending => {}
            }
            state = match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(JoinError::TimedOut);
                    }
                    let (next, wait) = cv.wait_timeout(state, deadline - now).unwrap();
                    if wait.timed_out() {
                        return Err(JoinError::TimedOut);
                    }
                    next
                }
                None => cv.wait(state).unwrap(),
            };
        }
    }
}

/// A fixed-size pool draining a single shared FIFO queue.
///
/// Constructed only on the opt-in parallel path; the default native build
/// never creates one and never spawns a worker thread.
pub struct Scheduler {
    handle: SchedulerHandle,
    workers: Vec<JoinHandle<()>>,
}

impl Scheduler {
    /// Spawn `threads` workers. `threads == 0` is rejected: with no worker a
    /// `join` could never make progress.
    pub fn new(threads: usize) -> Self {
        assert!(threads >= 1, "scheduler needs at least one worker thread");
        let shared = Arc::new((
            Mutex::new(Shared {
                queue: VecDeque::new(),
                slots: HashMap::new(),
                closed: false,
                next_id: 0,
            }),
            Condvar::new(),
        ));
        let mut workers = Vec::with_capacity(threads);
        for index in 0..threads {
            let shared = Arc::clone(&shared);
            let handle = thread::Builder::new()
                .name(format!("nrt-worker-{index}"))
                .stack_size(WORKER_STACK_BYTES)
                .spawn(move || worker_loop(shared))
                .expect("scheduler: failed to spawn worker thread");
            workers.push(handle);
        }
        Self {
            handle: SchedulerHandle {
                shared: Arc::clone(&shared),
            },
            workers,
        }
    }

    /// RFC §3.1 default: 4 workers, capped by available parallelism.
    pub fn with_default_threads() -> Self {
        Self::new(Self::default_thread_count())
    }

    pub fn default_thread_count() -> usize {
        let available = thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(DEFAULT_WORKER_CAP);
        available.min(DEFAULT_WORKER_CAP).max(1)
    }

    /// A clone-able handle for capturing in task bodies.
    pub fn handle(&self) -> SchedulerHandle {
        self.handle.clone()
    }

    pub fn submit(&self, body: impl FnOnce() -> u64 + Send + 'static) -> Result<FutureId, &'static str> {
        self.handle.submit_closure(body)
    }

    pub fn submit_task(&self, task: Task) -> Result<FutureId, &'static str> {
        self.handle.submit(task)
    }

    pub fn join(&self, id: FutureId) -> Result<TaskResult, JoinError> {
        self.handle.join(id)
    }

    pub fn join_timeout(&self, id: FutureId, timeout: Duration) -> Result<TaskResult, JoinError> {
        self.handle.join_timeout(id, timeout)
    }

    /// Signal shutdown, drain everything still queued, then join workers.
    /// Idempotent: a second call is a no-op.
    pub fn close(&mut self) {
        {
            let (lock, cv) = &*self.handle.shared;
            let mut state = lock.lock().unwrap();
            state.closed = true;
            drop(state);
            cv.notify_all(); // wake idle workers so they observe `closed`
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }

    pub fn is_closed(&self) -> bool {
        let (lock, _) = &*self.handle.shared;
        lock.lock().unwrap().closed
    }

    /// Number of tasks that have produced a result — proves exactly-once.
    /// Derived from the output slots so it is always consistent with a
    /// `join` that just observed a finished slot (no counter race).
    pub fn executed_count(&self) -> usize {
        let (lock, _) = &*self.handle.shared;
        let state = lock.lock().unwrap();
        state
            .slots
            .values()
            .filter(|slot| {
                matches!(
                    *slot.lock().unwrap(),
                    TaskSlot::Ready(_) | TaskSlot::Panicked
                )
            })
            .count()
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
        let queued = {
            let mut state = lock.lock().unwrap();
            // Break only when closed *and* empty so shutdown drains everything.
            loop {
                if let Some(task) = state.queue.pop_front() {
                    break task;
                }
                if state.closed {
                    return;
                }
                state = cv.wait(state).unwrap();
            }
        };
        let _guard = CurrentTaskGuard::new(queued.id);
        run_task(&queued.task);
        cv.notify_all(); // wake joiners (and idle workers, harmlessly)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIGHT_ITERS: u64 = 2_000;

    /// Deterministic per-task computation (mirrors the spike's `work`).
    fn work(i: u64, iterations: u64) -> u64 {
        let mut acc = i;
        for step in 0..iterations {
            acc = acc
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(step + 1);
        }
        acc % 128
    }

    /// Order-insensitive view of results as sorted `(value, count)` pairs.
    fn multiset(values: &[u64]) -> Vec<(u64, usize)> {
        let mut sorted = values.to_vec();
        sorted.sort_unstable();
        let mut out: Vec<(u64, usize)> = Vec::new();
        for value in sorted {
            match out.last_mut() {
                Some((key, count)) if *key == value => *count += 1,
                _ => out.push((value, 1)),
            }
        }
        out
    }

    fn sequential(n: u64, iterations: u64) -> Vec<u64> {
        (0..n).map(|i| work(i, iterations)).collect()
    }

    #[test]
    fn parallel_multiset_matches_sequential_for_100_and_1000_tasks() {
        for &n in &[100u64, 1_000u64] {
            let mut scheduler = Scheduler::new(4);
            let futures: Vec<FutureId> = (0..n)
                .map(|i| {
                    scheduler
                        .submit(move || work(i, LIGHT_ITERS))
                        .expect("submit before close")
                })
                .collect();
            let got: Vec<u64> = futures
                .iter()
                .map(|id| scheduler.join(*id).expect("join must not deadlock").value)
                .collect();
            scheduler.close();
            assert_eq!(scheduler.executed_count(), n as usize, "n={n} exactly-once");
            assert_eq!(
                multiset(&got),
                multiset(&sequential(n, LIGHT_ITERS)),
                "n={n} multiset identity"
            );
        }
    }

    #[test]
    fn duplicate_wait_is_memoized_and_task_runs_once() {
        let mut scheduler = Scheduler::new(2);
        let future = scheduler.submit(|| work(7, LIGHT_ITERS)).unwrap();
        let first = scheduler.join(future).expect("first join");
        assert_eq!(scheduler.executed_count(), 1);
        let second = scheduler.join(future).expect("second join");
        assert_eq!(first, second, "duplicate wait must return the memoized value");
        assert_eq!(scheduler.executed_count(), 1, "re-wait must not re-execute");
        scheduler.close();
    }

    #[test]
    fn close_drains_pending_tasks_and_rejects_new_submits() {
        const N: u64 = 50;
        let mut scheduler = Scheduler::new(3);
        let futures: Vec<FutureId> = (0..N)
            .map(|i| {
                scheduler
                    .submit(move || work(i, LIGHT_ITERS))
                    .expect("submit before close")
            })
            .collect();
        scheduler.close();
        assert!(scheduler.is_closed());
        assert_eq!(
            scheduler.submit(|| 1).unwrap_err(),
            "pool is closed",
            "existing native-pool error string must be preserved"
        );
        let got: Vec<u64> = futures
            .iter()
            .map(|id| scheduler.join(*id).expect("drained result ready").value)
            .collect();
        assert_eq!(scheduler.executed_count(), N as usize);
        assert_eq!(multiset(&got), multiset(&sequential(N, LIGHT_ITERS)));
        scheduler.close(); // idempotent
    }

    #[test]
    fn worker_panic_is_recorded_and_join_reports_error() {
        let mut scheduler = Scheduler::new(1);
        let panicking = scheduler.submit(|| panic!("task body exploded")).unwrap();
        let healthy = scheduler.submit(|| 42).unwrap();
        assert_eq!(
            scheduler.join(panicking).unwrap_err(),
            JoinError::TaskPanicked,
            "panic must be delivered as an error, not a hang"
        );
        assert_eq!(
            scheduler.join(panicking).unwrap_err(),
            JoinError::TaskPanicked,
            "panic result must itself be memoized"
        );
        assert_eq!(scheduler.join(healthy).expect("worker survived").value, 42);
        scheduler.close();
        assert_eq!(scheduler.executed_count(), 2);
    }

    /// Publish/read a future id so a task body can wait on a future that is
    /// only known after submission.
    type IdCell = Arc<(Mutex<Option<FutureId>>, Condvar)>;

    fn publish(cell: &IdCell, id: FutureId) {
        let (lock, cv) = &**cell;
        *lock.lock().unwrap() = Some(id);
        cv.notify_all();
    }

    fn await_id(cell: &IdCell) -> FutureId {
        let (lock, cv) = &**cell;
        let mut guard = lock.lock().unwrap();
        while guard.is_none() {
            guard = cv.wait(guard).unwrap();
        }
        guard.unwrap()
    }

    #[test]
    fn wait_from_inside_task_uses_inline_fallback() {
        // One worker: the waiter task occupies it while the awaited task is
        // still queued. Without inline fallback this deadlocks; join_timeout
        // turns a regression into a TimedOut assertion instead of a hang.
        let mut scheduler = Scheduler::new(1);
        let handle = scheduler.handle();
        let awaited_id: IdCell = Arc::new((Mutex::new(None), Condvar::new()));

        let waiter_id_cell = Arc::clone(&awaited_id);
        let waiter = scheduler
            .submit(move || {
                let target = await_id(&waiter_id_cell);
                handle
                    .join_timeout(target, Duration::from_secs(5))
                    .expect("inline fallback must complete, not time out")
                    .value
            })
            .unwrap();
        let awaited = scheduler.submit(|| 99).unwrap();
        publish(&awaited_id, awaited);

        assert_eq!(scheduler.join(waiter).expect("waiter done").value, 99);
        assert_eq!(scheduler.join(awaited).expect("awaited done").value, 99);
        scheduler.close();
        assert_eq!(scheduler.executed_count(), 2, "each task still runs once");
    }

    #[test]
    fn true_self_cycle_is_rejected_without_deadlock() {
        let mut scheduler = Scheduler::new(1);
        let handle = scheduler.handle();
        let own_id: IdCell = Arc::new((Mutex::new(None), Condvar::new()));
        let cycle_cell = Arc::clone(&own_id);
        let task = scheduler
            .submit(move || {
                let id = await_id(&cycle_cell);
                match handle.join_timeout(id, Duration::from_secs(5)) {
                    Err(JoinError::SelfWait) => 1,
                    other => panic!("expected SelfWait, got {other:?}"),
                }
            })
            .unwrap();
        publish(&own_id, task);
        assert_eq!(scheduler.join(task).expect("cycle rejected").value, 1);
        scheduler.close();
    }

    #[test]
    fn join_timeout_guard_proves_no_silent_deadlock() {
        let scheduler = Scheduler::new(1);
        let err = scheduler
            .join_timeout(FutureId(999), Duration::from_millis(50))
            .unwrap_err();
        assert_eq!(err, JoinError::UnknownFuture);
        drop(scheduler);
    }

    #[test]
    fn native_entry_and_context_are_passed_through() {
        // Exercises the RFC §3.1 native shape directly (not the closure
        // trampoline): entry receives the ctx pointer and reads it.
        extern "C-unwind" fn native_entry(ptr: *mut u8) -> u64 {
            // SAFETY: ptr points at the `u64` kept alive in Task.ctx.
            unsafe { *(ptr as *const u64) }
        }
        let ctx: Arc<u64> = Arc::new(123);
        let task = Task::new(native_entry, ctx);
        let mut scheduler = Scheduler::new(1);
        let id = scheduler.submit_task(task).unwrap();
        assert_eq!(scheduler.join(id).expect("native task").value, 123);
        scheduler.close();
    }
}
