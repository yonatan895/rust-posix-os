//! POSIX Memory Management Declarations (sys/mman.h).

use crate::syscall::*;
use posix_abi::*;

#[no_mangle]
pub unsafe extern "C" fn mmap(
    addr: *mut u8,
    length: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: i64,
) -> *mut u8 {
    let ret = syscall6(
        SYS_MMAP,
        addr as usize,
        length,
        prot as usize,
        flags as usize,
        fd as usize,
        offset as usize,
    );
    ret as *mut u8
}

#[no_mangle]
pub unsafe extern "C" fn munmap(addr: *mut u8, length: usize) -> i32 {
    syscall2(SYS_MUNMAP, addr as usize, length) as i32
}

#[no_mangle]
pub unsafe extern "C" fn mprotect(addr: *mut u8, len: usize, prot: i32) -> i32 {
    syscall3(SYS_MPROTECT, addr as usize, len, prot as usize) as i32
}
