#![no_std]
#![no_main]

use core::panic::PanicInfo;
use libc::*;

#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    puts(b"================================================\0".as_ptr());
    puts(b"  Rust POSIX OS - Init Daemon (PID 1) Running   \0".as_ptr());
    puts(b"================================================\0".as_ptr());

    let pid = getpid();
    printf(b"[init] Started as Process ID: %d\n\0".as_ptr(), pid);

    // Test malloc / free in userspace
    let ptr = malloc(128);
    if !ptr.is_null() {
        strcpy(
            ptr,
            b"Memory allocation via mmap-backed malloc works!\0".as_ptr(),
        );
        printf(b"[init] Userspace heap test: %s\n\0".as_ptr(), ptr);
        free(ptr);
    }

    // Launch interactive POSIX Shell
    puts(b"[init] Spawning interactive POSIX Shell...\n\0".as_ptr());
    execve(b"/bin/sh\0".as_ptr(), core::ptr::null(), core::ptr::null());

    loop {
        let mut status = 0;
        waitpid(-1, &mut status, 0);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe { exit(1) };
}
