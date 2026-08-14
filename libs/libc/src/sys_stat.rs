//! POSIX File Information (sys/stat.h).

use crate::syscall::*;
use posix_abi::*;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn stat(pathname: *const u8, statbuf: *mut Stat) -> i32 {
    unsafe { syscall2(SYS_STAT, pathname as usize, statbuf as usize) as i32 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fstat(fd: i32, statbuf: *mut Stat) -> i32 {
    unsafe { syscall2(SYS_FSTAT, fd as usize, statbuf as usize) as i32 }
}
