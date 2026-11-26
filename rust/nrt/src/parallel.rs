//! Opt-in `go`/`wait` bridge for no-VM executables (Work ID `W-2026-11-27-P06-NRT`).
//!
//! Implements the three approved C symbols:
//!
//! ```c
//! uint64_t unrt_go (int64_t func_index, const uint64_t *caps, uintptr_t cap_count);
//! int64_t  unrt_wait(uint64_t handle, uint64_t *out_value, uint64_t *out_tag);
//! uint64_t unrt_trace(uint64_t handle, uint8_t *buf, uintptr_t cap);
//! ```
//!
//! # Default path is untouched
//!
//! `unrt_go` is the **only** function that constructs a [`Scheduler`]. The
//! scheduler is a lazy global built on first `unrt_go`, so a program that never
//! uses the opt-in parallel path spawns zero threads and keeps its byte-for-byte
//! deferred `go`/`wait` behaviour. `is_parallel_future` — the only helper called
//! from the default `type()` path — touches a separate empty `HashMap`, never
//! the scheduler.
//!
//! # Value and trace flow
//!
//! `unrt_go` builds the native closure exactly as the deferred path would
//! (`unrt_obj_closure`), submits it to the scheduler, and returns the closure
//! **object handle** as the future handle. Keeping a real NRT handle (rather
//! than a bare slot index) means `type(future)` stays safe and can recognise it
//! through [`is_parallel_future`].
//!
//! A worker clears its task-local trace at entry, invokes
//! `unrt_call_obj(closure, NULL, 0)`, and stores `(value, tag, trace, error)` in
//! the shared slot. `unrt_wait` joins that slot and reinstalls the worker's
//! trace, error site and pending error into the **waiter's** thread-local state
//! before returning, so a rethrown error produces the original bytes at the
//! wait point (RFC §3.2). `unrt_trace` serializes the same snapshot without
//! requiring a wait.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::closure::unrt_call_obj;
use crate::obj::obj;
use crate::scheduler::{
    FutureId, JoinError, JoinFailure, Scheduler, SchedulerHandle, TagMode, Task, TaskResult,
    TaskSlot,
};
use crate::trace::TraceSnapshot;

/// The one and only scheduler instance. Built on first `unrt_go`; a plain
/// build that never calls it never spawns a worker thread.
struct Globals {
    /// Owns the worker threads. Locked only for `submit` — never across
    /// `join`, because a running task may itself call `unrt_wait`.
    scheduler: Mutex<Scheduler>,
    /// Clone-able join handle, so waiting does not contend on `scheduler`.
    handle: SchedulerHandle,
}

static GLOBALS: OnceLock<Globals> = OnceLock::new();

fn globals() -> &'static Globals {
    GLOBALS.get_or_init(|| {
        let scheduler = Scheduler::with_default_threads();
        let handle = scheduler.handle();
        Globals { scheduler: Mutex::new(scheduler), handle }
    })
}

// ---- W-2026-11-27-P07: opt-in signal for flow/pool/atomic ------------------
//
// `un build --parallel` is the single opt-in entry point (plan §1.1). The
// generated plain-exe driver sets `UN_PARALLEL=1` before calling `un_main`;
// NRT reads it exactly once, on the first flow/atomic handle construction.
// No new C ABI symbol is needed: the existing `unrt_call_native` bridge is the
// only path the flow shim uses, and P06's three symbols stay unchanged.
//
// A build that never sets the variable keeps the cooperative `RefCell` shim
// byte-for-byte, so every default gate (exe-diff, test.ps1, ...) is untouched.
static FLOW_PARALLEL: OnceLock<bool> = OnceLock::new();

/// Whether this exe was built with `un build --parallel`.
pub(crate) fn parallel_enabled() -> bool {
    *FLOW_PARALLEL.get_or_init(|| {
        std::env::var("UN_PARALLEL")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// Submit a task from the P07 flow shim. Returns the future id plus a clone of
/// the shared output slot; the shim currently joins by id (which also
/// installs the worker's trace on the waiter) and ignores the slot clone.
pub(crate) fn submit_flow_task(
    task: Task,
) -> Result<(FutureId, Arc<Mutex<TaskSlot>>), &'static str> {
    let out = Arc::clone(&task.out);
    let id = globals().scheduler.lock().unwrap().submit_task(task)?;
    Ok((id, out))
}

/// Join a P07 flow task. Memoized: repeated joins reuse the same result, and
/// the producing worker's trace/error state is installed on the waiter.
pub(crate) fn join_flow_task(id: FutureId) -> Result<TaskResult, JoinFailure> {
    globals().handle.join_full(id)
}

#[derive(Clone)]
struct FutureEntry {
    id: FutureId,
    out: Arc<Mutex<TaskSlot>>,
}

/// Future handle (an NRT object handle) → scheduler slot. Kept separate from
/// [`GLOBALS`] so the default `type()` path cannot accidentally create a pool.
static FUTURES: OnceLock<Mutex<HashMap<u64, FutureEntry>>> = OnceLock::new();

fn futures() -> &'static Mutex<HashMap<u64, FutureEntry>> {
    FUTURES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `true` when `p` is a parallel future handle. Used by `native::go_future_of`
/// as an additive check — the existing deferred-future probe is unchanged.
pub(crate) fn is_parallel_future(p: u64) -> bool {
    futures().lock().unwrap().contains_key(&p)
}

/// Task entry point: the context is an `Arc<u64>` holding the closure handle.
extern "C-unwind" fn task_entry(ptr: *mut u8) -> u64 {
    // SAFETY: `ptr` is `Arc::as_ptr(&task.ctx)`, where `ctx` is the `Arc<u64>`
    // created in `unrt_go` and kept alive by the queued `Task` for this call.
    let closure = unsafe { *(ptr as *const u64) };
    // `unrt_call_obj` builds a slice from this pointer; a non-null (though
    // zero-length) pointer is required even when there are no call arguments.
    let no_args = [0u64; 1];
    unrt_call_obj(closure, no_args.as_ptr(), 0)
}

/// Create a parallel future from a registered lambda and its boxed captures.
///
/// Returns the closure object handle, or 0 when the lambda is not registered
/// or the pool has been closed.
#[no_mangle]
pub extern "C" fn unrt_go(func_index: i64, caps: *const u64, cap_count: usize) -> u64 {
    let closure = crate::closure::unrt_obj_closure(func_index, caps, cap_count);
    // `unrt_obj_closure` returns an Error handle on failure; that is not a
    // callable object and must not escape as a future handle.
    if closure == 0 || unsafe { obj(closure).tag != 8 } {
        return 0;
    }

    let out = Arc::new(Mutex::new(TaskSlot::Pending));
    let task = Task {
        entry: task_entry,
        ctx: Arc::new(closure) as Arc<dyn Send + Sync + 'static>,
        out: Arc::clone(&out),
        tag_mode: TagMode::Obj,
    };
    let id = match globals().scheduler.lock().unwrap().submit_task(task) {
        Ok(id) => id,
        // Preserves the existing "pool is closed" behaviour from P05.
        Err(_) => return 0,
    };
    futures().lock().unwrap().insert(closure, FutureEntry { id, out });
    closure
}

/// Wait for a parallel future and deliver its result to `out_value`/`out_tag`.
///
/// Return codes: 0 ok, 1 panic, 2 self-wait, 3 unknown handle, 4 closed/no
/// result. On *every* path the producing task's trace, error site and pending
/// error are reinstalled into the waiting thread before returning.
#[no_mangle]
pub extern "C" fn unrt_wait(handle: u64, out_value: *mut u64, out_tag: *mut u64) -> i64 {
    unsafe {
        if !out_value.is_null() { *out_value = 0; }
        if !out_tag.is_null() { *out_tag = 0; }
    }
    let Some(entry) = futures().lock().unwrap().get(&handle).cloned() else {
        return 3;
    };

    match globals().handle.join_full(entry.id) {
        Ok(result) => {
            crate::trace::install(&result.trace);
            let err = crate::obj::ensure_error_obj(result.error);
            crate::trace::pending_error_set(err);
            // A task that `throw`s does not panic the worker: `unrt_call_obj`
            // returns an error handle and parks the error in the task-local
            // cell. Report failure and hand codegen the error object itself so
            // `emit_error_gate` can dispatch it without clearing the worker's
            // trace frames installed above.
            if err != 0 {
                unsafe { if !out_value.is_null() { *out_value = err; } }
                return 1;
            }
            unsafe {
                if !out_value.is_null() { *out_value = result.value; }
                if !out_tag.is_null() { *out_tag = result.tag; }
            }
            0
        }
        Err(failure) => {
            crate::trace::install(&failure.trace);
            let err = crate::obj::ensure_error_obj(failure.error);
            crate::trace::pending_error_set(err);
            unsafe { if !out_value.is_null() { *out_value = err; } }
            match failure.kind {
                JoinError::TaskPanicked => 1,
                JoinError::SelfWait => 2,
                JoinError::UnknownFuture => 3,
                // `join` has no timeout; this code is the ABI's "closed/no
                // result" bucket if a watchdog path is ever wired in.
                JoinError::TimedOut => 4,
            }
        }
    }
}

/// Serialize a future's traceback into `buf`.
///
/// Returns the number of bytes needed (without a NUL terminator, matching the
/// explicit-length convention used by every other `nrt_register_*` ABI call).
/// When `buf` is null, only the length is computed; a short buffer receives a
/// truncation but still reports the full length.
#[no_mangle]
pub extern "C" fn unrt_trace(handle: u64, buf: *mut u8, cap: usize) -> u64 {
    let Some(entry) = futures().lock().unwrap().get(&handle).cloned() else {
        return 0;
    };
    let snapshot = match entry.out.lock().unwrap().clone() {
        TaskSlot::Ready(result) => result.trace,
        TaskSlot::Panicked { trace, .. } => trace,
        TaskSlot::Pending => TraceSnapshot::default(),
    };
    let text = crate::trace::traceback_text(&snapshot);
    let needed = text.len() as u64;
    if !buf.is_null() && cap > 0 {
        let copied = (needed as usize).min(cap);
        // SAFETY: `text` outlives the copy and `buf` is caller-owned with at
        // least `cap` writable bytes.
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), buf, copied) };
    }
    needed
}

#[cfg(test)]
mod p06_tests {
    use super::*;
    use crate::obj::{display, err, unrt_obj_int, unrt_int_from_obj};

    const ADD_ID: i64 = 91_001;
    const SELF_ID: i64 = 91_002;

    extern "C" fn add_captures(a: u64, b: u64) -> u64 {
        // Captures arrive as boxed Int handles; return a new boxed Int handle
        // (ret_kind 3 passes the handle through `box_return` unchanged).
        let sum = unrt_int_from_obj(a).wrapping_add(unrt_int_from_obj(b));
        unrt_obj_int(sum)
    }

    fn add_future(a: i64, b: i64) -> u64 {
        crate::closure::nrt_register_lambda(
            ADD_ID,
            add_captures as extern "C" fn(u64, u64) -> u64 as usize,
            [3u8, 3].as_ptr(),
            2,
            3,
        );
        // `unrt_go` builds this closure internally, then gives it a scheduler
        // slot — exactly what the opt-in TIR path emits.
        unrt_go(ADD_ID, [unrt_obj_int(a), unrt_obj_int(b)].as_ptr(), 2)
    }

    #[test]
    fn go_wait_round_trip_delivers_value_and_tag() {
        let handle = add_future(7, 35);
        assert_ne!(handle, 0, "unrt_go must return a nonzero handle");
        assert!(is_parallel_future(handle));

        let mut value = 0u64;
        let mut tag = u64::MAX;
        assert_eq!(unrt_wait(handle, &mut value, &mut tag), 0);
        assert_eq!(value, 42, "parallel wait returns unboxed raw Int bits");
        assert_eq!(tag, 0, "boxed Int has tag 0");

        // type() sees a future, not an ordinary func.
        assert_eq!(display(handle), "fn() -> any");
    }

    #[test]
    fn duplicate_wait_is_memoized() {
        static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        extern "C" fn counted(a: u64, b: u64) -> u64 {
            CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let sum = unrt_int_from_obj(a).wrapping_add(unrt_int_from_obj(b));
            unrt_obj_int(sum)
        }
        crate::closure::nrt_register_lambda(
            ADD_ID,
            counted as extern "C" fn(u64, u64) -> u64 as usize,
            [3u8, 3].as_ptr(),
            2,
            3,
        );
        let handle = unrt_go(ADD_ID, [unrt_obj_int(2), unrt_obj_int(3)].as_ptr(), 2);
        let mut first = 0u64;
        let mut second = 0u64;
        assert_eq!(unrt_wait(handle, &mut first, std::ptr::null_mut()), 0);
        assert_eq!(unrt_wait(handle, &mut second, std::ptr::null_mut()), 0);
        assert_eq!(first, second);
        assert_eq!(
            CALLS.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "duplicate wait must reuse the memoized slot"
        );
    }

    #[test]
    fn self_wait_returns_code_two_without_deadlock() {
        static OWN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        static CODE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(-1);
        extern "C" fn wait_on_self() -> u64 {
            while OWN.load(std::sync::atomic::Ordering::Acquire) == 0 {
                std::hint::spin_loop();
            }
            let handle = OWN.load(std::sync::atomic::Ordering::Acquire);
            let mut value = 0u64;
            let mut tag = 0u64;
            let code = unrt_wait(handle, &mut value, &mut tag);
            CODE.store(code, std::sync::atomic::Ordering::Release);
            // ret_kind 0 means the callee returns a raw i64; `box_return`
            // boxes it into an Int handle for us.
            code as u64
        }
        crate::closure::nrt_register_lambda(
            SELF_ID,
            wait_on_self as extern "C" fn() -> u64 as usize,
            std::ptr::null(),
            0,
            0,
        );
        let handle = unrt_go(SELF_ID, std::ptr::null(), 0);
        assert_ne!(handle, 0);
        OWN.store(handle, std::sync::atomic::Ordering::Release);

        let mut value = 0u64;
        assert_eq!(unrt_wait(handle, &mut value, std::ptr::null_mut()), 0);
        assert_eq!(value, 2, "inner wait saw code 2");
        assert_eq!(CODE.load(std::sync::atomic::Ordering::Acquire), 2);
    }

    #[test]
    fn unregistered_lambda_returns_zero() {
        assert_eq!(unrt_go(i64::MIN + 777, std::ptr::null(), 0), 0);
    }

    #[test]
    fn panic_task_wait_code_one_and_trace_installed() {
        extern "C-unwind" fn panicking(_: *mut u8) -> u64 {
            let name = "parallel_panic";
            crate::trace::take_error();
            crate::trace::nrt_push_frame(name.as_ptr(), name.len(), 91, 1);
            crate::trace::nrt_set_error_site(91, 2);
            crate::trace::pending_error_set(777);
            panic!("parallel boom")
        }
        let out = Arc::new(Mutex::new(TaskSlot::Pending));
        let task = Task {
            entry: panicking,
            ctx: Arc::new(0u64) as Arc<dyn Send + Sync + 'static>,
            out: Arc::clone(&out),
            tag_mode: TagMode::Raw,
        };
        let id = globals().scheduler.lock().unwrap().submit_task(task).unwrap();
        const KEY: u64 = 0xF00D_1001;
        futures().lock().unwrap().insert(KEY, FutureEntry { id, out });

        crate::trace::take_error();
        let mut value = u64::MAX;
        let mut tag = u64::MAX;
        assert_eq!(unrt_wait(KEY, &mut value, &mut tag), 1);
        assert_ne!(value, 0, "worker error object is handed to the waiter");
        assert_eq!(tag, 0);
        let snap = crate::trace::snapshot();
        assert_eq!(snap.frames.len(), 1, "worker frame installed on waiter");
        assert_eq!(snap.frames[0].name, "parallel_panic");
        assert_eq!(snap.frames[0].start, 91);
        assert_eq!(snap.error_site, Some((91, 2)));
        assert_ne!(snap.pending_error, 0, "raw pending error normalized to Error obj");
        crate::trace::take_error();
        futures().lock().unwrap().remove(&KEY);
    }

    #[test]
    fn trace_length_and_copy() {
        extern "C-unwind" fn failing(_: *mut u8) -> u64 {
            let name = "trace_frame";
            crate::trace::take_error();
            crate::trace::nrt_push_frame(name.as_ptr(), name.len(), 15, 1);
            crate::trace::nrt_set_error_site(15, 4);
            let e = err("parallel failure");
            crate::obj::unrt_write_error(e);
            e
        }
        let out = Arc::new(Mutex::new(TaskSlot::Pending));
        let task = Task {
            entry: failing,
            ctx: Arc::new(0u64) as Arc<dyn Send + Sync + 'static>,
            out: Arc::clone(&out),
            tag_mode: TagMode::Obj,
        };
        let id = globals().scheduler.lock().unwrap().submit_task(task).unwrap();
        const KEY: u64 = 0xF00D_1002;
        futures().lock().unwrap().insert(KEY, FutureEntry { id, out });

        let mut value = 0u64;
        assert_eq!(unrt_wait(KEY, &mut value, std::ptr::null_mut()), 1);
        assert_eq!(display(value), "parallel failure");

        let needed = unrt_trace(KEY, std::ptr::null_mut(), 0);
        assert!(needed > 0);
        let mut buf = vec![0u8; needed as usize];
        assert_eq!(unrt_trace(KEY, buf.as_mut_ptr(), buf.len()), needed);
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("error: parallel failure"), "{text}");

        // Short buffer truncates but still reports the full length.
        let mut short = vec![0u8; 4];
        assert_eq!(unrt_trace(KEY, short.as_mut_ptr(), short.len()), needed);
        futures().lock().unwrap().remove(&KEY);
    }
}
