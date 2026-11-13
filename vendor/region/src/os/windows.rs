// fork: x86_64-pc-windows-gnu needs no dlltool — use raw FFI declarations of
// the four kernel32 functions region 3.0.2's Windows backend used.
use crate::{Error, Protection, Region, Result};
use std::cmp::{max, min};
use std::ffi::c_void;
use std::io;
use std::mem::MaybeUninit;
use std::sync::OnceLock;

#[link(name = "kernel32")]
extern "C" {
    fn VirtualAlloc(
        base_address: *mut c_void,
        size: usize,
        allocation_type: u32,
        protection: u32,
    ) -> *mut c_void;
    fn VirtualFree(base_address: *mut c_void, size: usize, free_type: u32) -> i32;
    fn VirtualProtect(
        base_address: *mut c_void,
        size: usize,
        new_protect: u32,
        old_protect: *mut u32,
    ) -> i32;
    fn VirtualQuery(base_address: *mut c_void, buffer: *mut MEMORY_BASIC_INFORMATION, length: usize) -> usize;
    fn VirtualLock(base_address: *mut c_void, size: usize) -> i32;
    fn VirtualUnlock(base_address: *mut c_void, size: usize) -> i32;
    fn GetNativeSystemInfo(system_info: *mut SYSTEM_INFO);
}

const MEM_COMMIT: u32 = 0x00001000;
const MEM_RESERVE: u32 = 0x00002000;
const MEM_RELEASE: u32 = 0x00008000;
const MEM_PRIVATE: u32 = 0x00020000;

const PAGE_NOACCESS: u32 = 0x01;
const PAGE_READONLY: u32 = 0x02;
const PAGE_READWRITE: u32 = 0x04;
const PAGE_WRITECOPY: u32 = 0x08;
const PAGE_EXECUTE: u32 = 0x10;
const PAGE_EXECUTE_READ: u32 = 0x20;
const PAGE_EXECUTE_READWRITE: u32 = 0x40;
const PAGE_EXECUTE_WRITECOPY: u32 = 0x80;
const PAGE_GUARD: u32 = 0x100;
const PAGE_NOCACHE: u32 = 0x200;
const PAGE_WRITECOMBINE: u32 = 0x400;

#[repr(C)]
#[derive(Clone, Copy)]
struct SYSTEM_INFO {
    w_processor_architecture: u16,
    w_reserved: u16,
    dw_page_size: u32,
    lp_minimum_application_address: usize,
    lp_maximum_application_address: usize,
    dw_active_processor_mask: usize,
    dw_number_of_processors: u32,
    dw_processor_type: u32,
    dw_allocation_granularity: u32,
    w_processor_level: u16,
    w_processor_revision: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MEMORY_BASIC_INFORMATION {
    base_address: *mut c_void,
    allocation_base: *mut c_void,
    allocation_protect: u32,
    are_reserved1: u16,
    are_reserved2: u16,
    region_size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}

pub struct QueryIter {
    region_address: usize,
    upper_bound: usize,
}

impl QueryIter {
    pub fn new(origin: *const (), size: usize) -> Result<QueryIter> {
        let system = system_info();

        Ok(QueryIter {
            region_address: max(origin as usize, system.lp_minimum_application_address),
            upper_bound: min(
                (origin as usize).saturating_add(size),
                system.lp_maximum_application_address,
            ),
        })
    }

    pub fn upper_bound(&self) -> usize {
        self.upper_bound
    }
}

impl Iterator for QueryIter {
    type Item = Result<Region>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut info = unsafe { std::mem::zeroed::<MEMORY_BASIC_INFORMATION>() };

        while self.region_address < self.upper_bound {
            let bytes = unsafe {
                VirtualQuery(
                    self.region_address as *mut c_void,
                    &mut info,
                    size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };

            if bytes == 0 {
                return Some(Err(Error::SystemCall(io::Error::last_os_error())));
            }

            self.region_address = (info.base_address as usize).saturating_add(info.region_size);

            // Only mapped memory regions are of interest
            if info.state == 0x1000 || info.state == 0x2000 {
                let mut region = Region {
                    base: info.base_address as *const _,
                    reserved: info.state != 0x1000,
                    guarded: (info.protect & PAGE_GUARD) != 0,
                    shared: (info.kind & MEM_PRIVATE) == 0,
                    size: info.region_size as usize,
                    ..Default::default()
                };

                if region.is_committed() {
                    region.protection = Protection::from_native(info.protect);
                }

                return Some(Ok(region));
            }
        }

        None
    }
}

pub fn page_size() -> usize {
    system_info().dw_page_size as usize
}

pub unsafe fn alloc(base: *const (), size: usize, protection: Protection) -> Result<*const ()> {
    let allocation = VirtualAlloc(
        base as *mut c_void,
        size,
        MEM_COMMIT | MEM_RESERVE,
        protection.to_native(),
    );

    if allocation.is_null() {
        return Err(Error::SystemCall(io::Error::last_os_error()));
    }

    Ok(allocation as *const ())
}

pub unsafe fn free(base: *const (), _size: usize) -> Result<()> {
    match VirtualFree(base as *mut c_void, 0, MEM_RELEASE) {
        0 => Err(Error::SystemCall(io::Error::last_os_error())),
        _ => Ok(()),
    }
}

pub unsafe fn protect(base: *const (), size: usize, protection: Protection) -> Result<()> {
    let result = VirtualProtect(base as *mut c_void, size, protection.to_native(), &mut 0);

    if result == 0 {
        Err(Error::SystemCall(io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

pub fn lock(base: *const (), size: usize) -> Result<()> {
    let result = unsafe { VirtualLock(base as *mut c_void, size) };

    if result == 0 {
        Err(Error::SystemCall(io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

pub fn unlock(base: *const (), size: usize) -> Result<()> {
    let result = unsafe { VirtualUnlock(base as *mut c_void, size) };

    if result == 0 {
        Err(Error::SystemCall(io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

fn system_info() -> &'static SYSTEM_INFO {
    static INFO: OnceLock<SYSTEM_INFO> = OnceLock::new();

    INFO.get_or_init(|| unsafe {
        let mut raw: MaybeUninit<SYSTEM_INFO> = MaybeUninit::uninit();
        GetNativeSystemInfo(raw.as_mut_ptr());
        raw.assume_init()
    })
}

impl Protection {
    fn from_native(protection: u32) -> Self {
        // Ignore unsupported flags (TODO: Preserve this information?)
        let ignored = PAGE_GUARD | PAGE_NOCACHE | PAGE_WRITECOMBINE;

        match protection & !ignored {
            PAGE_EXECUTE => Protection::EXECUTE,
            PAGE_EXECUTE_READ => Protection::READ_EXECUTE,
            PAGE_EXECUTE_READWRITE => Protection::READ_WRITE_EXECUTE,
            PAGE_EXECUTE_WRITECOPY => Protection::READ_WRITE_EXECUTE,
            PAGE_NOACCESS => Protection::NONE,
            PAGE_READONLY => Protection::READ,
            PAGE_READWRITE => Protection::READ_WRITE,
            PAGE_WRITECOPY => Protection::READ_WRITE,
            _ => unreachable!("Protection: 0x{:X}", protection),
        }
    }

    pub(crate) fn to_native(self) -> u32 {
        match self {
            Protection::NONE => PAGE_NOACCESS,
            Protection::READ => PAGE_READONLY,
            Protection::EXECUTE => PAGE_EXECUTE,
            Protection::READ_EXECUTE => PAGE_EXECUTE_READ,
            Protection::READ_WRITE => PAGE_READWRITE,
            Protection::READ_WRITE_EXECUTE => PAGE_EXECUTE_READWRITE,
            Protection::WRITE_EXECUTE => PAGE_EXECUTE_READWRITE,
            _ => unreachable!("Protection: {:?}", self),
        }
    }
}
