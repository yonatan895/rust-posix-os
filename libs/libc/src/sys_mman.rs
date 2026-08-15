//! POSIX Memory Management Declarations (sys/mman.h).

use crate::syscall::*;
use posix_abi::*;

/// Establishes a memory mapping in the process virtual address space.
///
/// Returns pointer to the mapped region on success, or `MAP_FAILED` (`!0 as *mut u8`) on failure.
///
/// # Safety
///
/// Direct memory management system call. `addr` and `length` must satisfy system alignment constraints.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmap(
    addr: *mut u8,
    length: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: i64,
) -> *mut u8 {
    // SAFETY: Issues SYS_MMAP syscall with address, length, protections, flags, fd, and offset.
    let ret = unsafe {
        syscall6(
            SYS_MMAP,
            addr as usize,
            length,
            prot as usize,
            flags as usize,
            fd as usize,
            offset as usize,
        )
    };
    ret as *mut u8
}

/// Unmaps a previously mapped memory region.
///
/// Returns 0 on success, or `-1` on error.
///
/// # Safety
///
/// `addr` must be page-aligned and reference a valid memory mapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn munmap(addr: *mut u8, length: usize) -> i32 {
    // SAFETY: Issues SYS_MUNMAP syscall with base address pointer and byte length.
    unsafe { syscall2(SYS_MUNMAP, addr as usize, length) as i32 }
}

/// Changes the access protections for a region of memory pages.
///
/// Returns 0 on success, or `-1` on error.
///
/// # Safety
///
/// `addr` must be page-aligned and point to allocated memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mprotect(addr: *mut u8, len: usize, prot: i32) -> i32 {
    // SAFETY: Issues SYS_MPROTECT syscall with base address pointer, length, and protection flags.
    unsafe { syscall3(SYS_MPROTECT, addr as usize, len, prot as usize) as i32 }
}
