//! POSIX File Information (sys/stat.h).

use crate::syscall::*;
use posix_abi::*;

/// Retrieves file status information for the file named by `pathname`.
///
/// Returns 0 on success, or `-1` on error.
///
/// # Safety
///
/// `pathname` must point to a valid null-terminated C string.
/// `statbuf` must point to a valid writable [`Stat`] structure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stat(pathname: *const u8, statbuf: *mut Stat) -> i32 {
    unsafe { syscall2(SYS_STAT, pathname as usize, statbuf as usize) as i32 }
}

/// Retrieves file status information for the open file referenced by `fd`.
///
/// Returns 0 on success, or `-1` on error.
///
/// # Safety
///
/// `statbuf` must point to a valid writable [`Stat`] structure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fstat(fd: i32, statbuf: *mut Stat) -> i32 {
    // SAFETY: Performing direct fstat syscall.
    unsafe { syscall2(SYS_FSTAT, fd as usize, statbuf as usize) as i32 }
}

/// Sets the calling process's file mode creation mask to `mask`.
///
/// Returns the previous mask value.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn umask(mask: u32) -> u32 {
    // SAFETY: Performing direct umask syscall.
    unsafe { syscall1(SYS_UMASK, mask as usize) as u32 }
}
