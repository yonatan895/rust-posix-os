#![no_std]
#![no_main]
#![allow(unsafe_op_in_unsafe_fn)]
// Userland crate uses C-style FFI patterns (nul-terminated byte-string literals,
// raw pointer arithmetic) that conflict with clippy's Rust-idiomatic expectations.
#![allow(clippy::all)]

use core::panic::PanicInfo;
use libc::*;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    // SAFETY: Outputting static null-terminated banner strings to standard output.
    unsafe {
        puts(b"================================================\0".as_ptr());
        puts(b"  Rust POSIX OS - Init Daemon (PID 1) Running   \0".as_ptr());
        puts(b"================================================\0".as_ptr());
    }

    // SAFETY: Calling getpid syscall wrapper.
    let pid = unsafe { getpid() };
    // SAFETY: Calling printf syscall wrapper with static format string.
    unsafe {
        printf(b"[init] Started as Process ID: %d\n\0".as_ptr(), pid);
    }

    // Test malloc / free in userspace
    // SAFETY: Calling malloc to allocate 128 bytes from userspace heap.
    let ptr = unsafe { malloc(128) };
    if !ptr.is_null() {
        // SAFETY: ptr is non-null with 128-byte capacity; source is valid static C-string.
        unsafe {
            strcpy(
                ptr,
                b"Memory allocation via mmap-backed malloc works!\0".as_ptr(),
            );
            printf(b"[init] Userspace heap test: %s\n\0".as_ptr(), ptr);
            free(ptr);
        }
    }

    // Launch interactive POSIX Shell
    // SAFETY: Outputting status message to stdout.
    unsafe {
        puts(b"[init] Spawning interactive POSIX Shell...\n\0".as_ptr());
    }
    // SAFETY: Calling execve syscall to replace current process image with /bin/sh.
    unsafe {
        execve(b"/bin/sh\0".as_ptr(), core::ptr::null(), core::ptr::null());
    }

    loop {
        let mut status = 0;
        // SAFETY: Reaping child processes with valid pointer to local status variable.
        unsafe {
            waitpid(-1, &mut status, 0);
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // SAFETY: Exiting init daemon process with failure code 1.
    unsafe { exit(1) };
}
