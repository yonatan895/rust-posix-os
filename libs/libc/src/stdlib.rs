//! Standard General Utilities Library (stdlib.h).

use crate::string::memcpy;
use crate::sys_mman::*;
use crate::syscall::*;
use posix_abi::*;

#[repr(C)]
struct BlockHeader {
    size: usize,
    magic: usize,
}

const MAGIC: usize = 0x504F5349584D454D; // "POSIXMEM"

#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut u8 {
    if size == 0 {
        return core::ptr::null_mut();
    }
    let total_size = size + core::mem::size_of::<BlockHeader>();
    let aligned_size = (total_size + 4095) & !4095;

    let ptr = mmap(
        core::ptr::null_mut(),
        aligned_size,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANONYMOUS,
        -1,
        0,
    );

    if ptr.is_null() || (ptr as usize) >= (-(4095i64) as usize) {
        return core::ptr::null_mut();
    }

    let header = ptr as *mut BlockHeader;
    (*header).size = aligned_size;
    (*header).magic = MAGIC;

    ptr.add(core::mem::size_of::<BlockHeader>())
}

#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    let header_ptr = ptr.sub(core::mem::size_of::<BlockHeader>()) as *mut BlockHeader;
    if (*header_ptr).magic == MAGIC {
        let size = (*header_ptr).size;
        (*header_ptr).magic = 0;
        munmap(header_ptr as *mut u8, size);
    }
}

#[no_mangle]
pub unsafe extern "C" fn calloc(nmemb: usize, size: usize) -> *mut u8 {
    let total = nmemb.saturating_mul(size);
    let ptr = malloc(total);
    if !ptr.is_null() {
        crate::string::memset(ptr, 0, total);
    }
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn realloc(ptr: *mut u8, size: usize) -> *mut u8 {
    if ptr.is_null() {
        return malloc(size);
    }
    if size == 0 {
        free(ptr);
        return core::ptr::null_mut();
    }
    let header_ptr = ptr.sub(core::mem::size_of::<BlockHeader>()) as *mut BlockHeader;
    if (*header_ptr).magic != MAGIC {
        return core::ptr::null_mut();
    }
    let old_size = (*header_ptr).size - core::mem::size_of::<BlockHeader>();
    if old_size >= size {
        return ptr;
    }
    let new_ptr = malloc(size);
    if !new_ptr.is_null() {
        memcpy(new_ptr, ptr, old_size);
        free(ptr);
    }
    new_ptr
}

#[no_mangle]
pub unsafe extern "C" fn exit(status: i32) -> ! {
    syscall1(SYS_EXIT, status as usize);
    loop {
        core::arch::asm!("hlt");
    }
}

#[no_mangle]
pub unsafe extern "C" fn abort() -> ! {
    exit(134)
}

#[no_mangle]
pub unsafe extern "C" fn atoi(s: *const u8) -> i32 {
    let mut i = 0;
    let mut sign = 1;
    while *s.add(i) == b' ' || *s.add(i) == b'\t' || *s.add(i) == b'\n' {
        i += 1;
    }
    if *s.add(i) == b'-' {
        sign = -1;
        i += 1;
    } else if *s.add(i) == b'+' {
        i += 1;
    }
    let mut res = 0;
    while *s.add(i) >= b'0' && *s.add(i) <= b'9' {
        res = res * 10 + (*s.add(i) - b'0') as i32;
        i += 1;
    }
    sign * res
}

#[no_mangle]
pub unsafe extern "C" fn abs(j: i32) -> i32 {
    if j < 0 { -j } else { j }
}
