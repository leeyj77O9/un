//! Stub for the `wasm32-unknown-unknown` target: there is no real virtual
//! memory at runtime in that environment. These stubs panic, which is safe
//! because the JIT is never used for playground/WASM builds.
use crate::{Protection, Result};
use std::ffi::c_void;
use std::io;

pub struct QueryIter {
    region_address: usize,
    upper_bound: usize,
}

impl QueryIter {
    pub fn new(origin: *const (), size: usize) -> Result<QueryIter> {
        Ok(QueryIter {
            region_address: origin as usize,
            upper_bound: (origin as usize).saturating_add(size),
        })
    }

    pub fn upper_bound(&self) -> usize {
        self.upper_bound
    }
}

impl Iterator for QueryIter {
    type Item = Result<crate::Region>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.region_address >= self.upper_bound {
            return None;
        }
        Some(Err(crate::Error::SystemCall(io::Error::last_os_error())))
    }
}

pub fn page_size() -> usize {
    4096
}

pub unsafe fn alloc(_base: *const (), _size: usize, _protection: Protection) -> Result<*const ()> {
    Err(crate::Error::SystemCall(io::ErrorKind::Unsupported.into()))
}

pub unsafe fn free(_base: *const (), _size: usize) -> Result<()> {
    Ok(())
}

pub unsafe fn protect(_base: *const (), _size: usize, _protection: Protection) -> Result<()> {
    Ok(())
}

pub fn lock(_base: *const (), _size: usize) -> Result<()> {
    Ok(())
}

pub fn unlock(_base: *const (), _size: usize) -> Result<()> {
    Ok(())
}

fn _force_use() {
    let _ = std::mem::size_of::<*const c_void>();
}
