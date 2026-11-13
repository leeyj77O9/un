// fork: x86_64-pc-windows-gnu needs no dlltool — use raw FFI declarations of
// the two kernel32 functions this crate uses on Windows.
use std::ffi::c_void;
use std::io::Error;
use wasmtime_core::error::Result;

#[link(name = "kernel32")]
unsafe extern "C" {
    fn FlushInstructionCache(process: *mut c_void, base: *const c_void, size: usize) -> i32;
    fn GetCurrentProcess() -> *mut c_void;
}

/// See docs on [crate::pipeline_flush_mt] for a description of what this function is trying to do.
#[inline]
pub(crate) fn pipeline_flush_mt() -> Result<()> {
    // On all coherent architectures this needs nothing
    Ok(())
}

/// See docs on [crate::clear_cache] for a description of what this function is trying to do.
#[inline]
pub(crate) fn clear_cache(ptr: *const c_void, len: usize) -> Result<()> {
    // See:
    //   * https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-flushinstructioncache
    //   * https://devblogs.microsoft.com/oldnewthing/20190902-00/?p=102828
    unsafe {
        let res = FlushInstructionCache(GetCurrentProcess(), ptr, len);
        if res == 0 {
            return Err(Error::last_os_error().into());
        }
    }

    Ok(())
}
