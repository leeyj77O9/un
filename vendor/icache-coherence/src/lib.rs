//! This crate provides utilities for instruction cache maintenance for JIT authors.
//!
//! See docs in the original crate for details. x86_64_pc_windows_gnu fork:
//! `clear_cache` uses `FlushInstructionCache` via raw FFI; `pipeline_flush_mt`
//! is a no-op on x86.
//! <div class="example-wrap" style="display:inline-block"><pre class="compile_fail" style="white-space:normal;font:inherit;">
//! **Warning**: In order to correctly use this interface you should always call [clear_cache].
//! A followup call to [pipeline_flush_mt] is required if you are running in a multi-threaded environment.
//! </pre></div>

use std::ffi::c_void;
use wasmtime_core::error::Result;

#[cfg(target_os = "windows")]
mod win;

#[cfg(target_os = "windows")]
use win as imp;

#[cfg(not(target_os = "windows"))]
mod noop {
    use super::*;
    pub(crate) fn pipeline_flush_mt() -> Result<()> {
        Ok(())
    }
    pub(crate) fn clear_cache(_ptr: *const c_void, _len: usize) -> Result<()> {
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
use noop as imp;

/// Flushes instructions in the processor pipeline
///
/// This pipeline flush is broadcast to all processors that are executing threads in the current process.
///
/// Calling [pipeline_flush_mt] is only required for multi-threaded programs and it *must* be called
/// after all calls to [clear_cache].
///
/// If the architecture does not require a pipeline flush, this function does nothing.
pub fn pipeline_flush_mt() -> Result<()> {
    imp::pipeline_flush_mt()
}

/// Flushes the instruction cache for a region of memory.
///
/// If the architecture does not require an instruction cache flush, this function does nothing.
///
/// # Unsafe
///
/// It is necessary to call [pipeline_flush_mt] after this function if you are running in a multi-threaded
/// environment.
pub unsafe fn clear_cache(ptr: *const c_void, len: usize) -> Result<()> {
    imp::clear_cache(ptr, len)
}
