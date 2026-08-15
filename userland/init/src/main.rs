#![no_std]
#![no_main]
#![allow(unsafe_op_in_unsafe_fn)]
// Userland crate uses C-style FFI patterns (nul-terminated byte-string literals,
// raw pointer arithmetic) that conflict with clippy's Rust-idiomatic expectations.
#![allow(clippy::all)]

use core::panic::PanicInfo;
use libc::*;
use posix_abi::*;

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

    // Run User Pointer Validation (EFAULT) Hammer Tests
    // SAFETY: Executing adversarial user pointer tests in PID 1 user mode before shell spawn.
    unsafe {
        run_efault_hammer_tests();
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

static FAILED_TESTS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

unsafe fn assert_eq_errno(actual: isize, expected: isize, test_name: *const u8) {
    if actual != expected {
        // SAFETY: Writing failure diagnostics to standard output.
        unsafe {
            printf(
                b"[FAIL] EFAULT hammer %s: expected errno %d, got %d\n\0".as_ptr(),
                test_name,
                expected as i32,
                actual as i32,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }
}

unsafe fn run_efault_hammer_tests() {
    // SAFETY: Outputting start of EFAULT tests to stdout.
    unsafe {
        puts(b"[init] Running User Pointer Validation (EFAULT) Hammer Tests...\n\0".as_ptr());
    }
    let null_ptr = core::ptr::null_mut::<u8>();
    let kernel_ptr = 0xFFFF_8000_0000_0000usize as *mut u8;
    let unmapped_ptr = 0x0000_7000_1234_0000usize as *mut u8;

    // 1. read tests (null, kernel, unmapped)
    // SAFETY: Calling read with adversarial user pointers to verify kernel EFAULT handling.
    let (r1, r2, r3) = unsafe {
        (
            read(0, null_ptr, 10),
            read(0, kernel_ptr, 10),
            read(0, unmapped_ptr, 10),
        )
    };
    unsafe {
        assert_eq_errno(r1, -(EFAULT as isize), b"read(null)\0".as_ptr());
        assert_eq_errno(r2, -(EFAULT as isize), b"read(kernel_ptr)\0".as_ptr());
        assert_eq_errno(r3, -(EFAULT as isize), b"read(unmapped_ptr)\0".as_ptr());
    }

    // 2. write tests (null, kernel, unmapped)
    // SAFETY: Calling write with adversarial user pointers to verify kernel EFAULT handling.
    let (w1, w2, w3) = unsafe {
        (
            write(1, null_ptr, 10),
            write(1, kernel_ptr, 10),
            write(1, unmapped_ptr, 10),
        )
    };
    unsafe {
        assert_eq_errno(w1, -(EFAULT as isize), b"write(null)\0".as_ptr());
        assert_eq_errno(w2, -(EFAULT as isize), b"write(kernel_ptr)\0".as_ptr());
        assert_eq_errno(w3, -(EFAULT as isize), b"write(unmapped_ptr)\0".as_ptr());
    }

    // 3. open tests (null, kernel, unmapped)
    // SAFETY: Calling open with adversarial pathname pointers to verify kernel EFAULT handling.
    let (o1, o2, o3) = unsafe {
        (
            open(null_ptr, O_RDONLY, 0),
            open(kernel_ptr, O_RDONLY, 0),
            open(unmapped_ptr, O_RDONLY, 0),
        )
    };
    unsafe {
        assert_eq_errno(o1 as isize, -(EFAULT as isize), b"open(null)\0".as_ptr());
        assert_eq_errno(
            o2 as isize,
            -(EFAULT as isize),
            b"open(kernel_ptr)\0".as_ptr(),
        );
        assert_eq_errno(
            o3 as isize,
            -(EFAULT as isize),
            b"open(unmapped_ptr)\0".as_ptr(),
        );
    }

    // 4. stat tests (null, kernel, unmapped for both path and statbuf)
    let mut st: Stat = unsafe { core::mem::zeroed() };
    // SAFETY: Calling stat with adversarial pathname and statbuf pointers.
    let (s1, s2, s3, s4, s5, s6) = unsafe {
        (
            stat(null_ptr, &mut st),
            stat(b"/dev/null\0".as_ptr(), core::ptr::null_mut()),
            stat(kernel_ptr, &mut st),
            stat(b"/dev/null\0".as_ptr(), kernel_ptr as *mut Stat),
            stat(unmapped_ptr, &mut st),
            stat(b"/dev/null\0".as_ptr(), unmapped_ptr as *mut Stat),
        )
    };
    unsafe {
        assert_eq_errno(
            s1 as isize,
            -(EFAULT as isize),
            b"stat(null_path)\0".as_ptr(),
        );
        assert_eq_errno(
            s2 as isize,
            -(EFAULT as isize),
            b"stat(null_statbuf)\0".as_ptr(),
        );
        assert_eq_errno(
            s3 as isize,
            -(EFAULT as isize),
            b"stat(kernel_path)\0".as_ptr(),
        );
        assert_eq_errno(
            s4 as isize,
            -(EFAULT as isize),
            b"stat(kernel_statbuf)\0".as_ptr(),
        );
        assert_eq_errno(
            s5 as isize,
            -(EFAULT as isize),
            b"stat(unmapped_path)\0".as_ptr(),
        );
        assert_eq_errno(
            s6 as isize,
            -(EFAULT as isize),
            b"stat(unmapped_statbuf)\0".as_ptr(),
        );
    }

    // 5. mmap tests (kernel space address and overflowing lengths must be rejected with -ENOMEM)
    // SAFETY: Calling mmap with adversarial kernel address and usize::MAX length.
    let (m1, m2) = unsafe {
        (
            mmap(
                kernel_ptr,
                4096,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            ),
            mmap(
                core::ptr::null_mut(),
                usize::MAX,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            ),
        )
    };
    unsafe {
        assert_eq_errno(
            m1 as isize,
            -(ENOMEM as isize),
            b"mmap(kernel_ptr)\0".as_ptr(),
        );
        assert_eq_errno(
            m2 as isize,
            -(ENOMEM as isize),
            b"mmap(len=usize::MAX)\0".as_ptr(),
        );
    }

    // 6. Page boundary straddling test
    // Note: Relies on bump allocation in mmap_next_vaddr ensuring page+4096 is unmapped.
    // SAFETY: Allocating a single page and referencing the boundary at page + 4090.
    let page = unsafe {
        mmap(
            core::ptr::null_mut(),
            4096,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if (page as isize) > 0 {
        let straddle_buf = (page as usize + 4090) as *mut u8;
        // SAFETY: Invoking read/write with a 16-byte slice starting at page+4090 (straddling into unmapped page).
        let (r_straddle, w_straddle) =
            unsafe { (read(0, straddle_buf, 16), write(1, straddle_buf, 16)) };
        unsafe {
            assert_eq_errno(r_straddle, -(EFAULT as isize), b"read(straddle)\0".as_ptr());
            assert_eq_errno(
                w_straddle,
                -(EFAULT as isize),
                b"write(straddle)\0".as_ptr(),
            );
        }

        // Fill remaining bytes of mapped page with non-zero characters (no NUL terminator before page end)
        // SAFETY: straddle_buf is within the allocated 4096-byte page ([4090..4096)).
        unsafe {
            for i in 0..6 {
                *straddle_buf.add(i) = b'A';
            }
        }
        // SAFETY: Testing open and stat with non-NUL-terminated string straddling into unmapped page.
        let (o_straddle, s_straddle, s_buf_straddle) = unsafe {
            (
                open(straddle_buf, O_RDONLY, 0),
                stat(straddle_buf, &mut st),
                stat(b"/dev/null\0".as_ptr(), straddle_buf as *mut Stat),
            )
        };
        unsafe {
            assert_eq_errno(
                o_straddle as isize,
                -(EFAULT as isize),
                b"open(straddle)\0".as_ptr(),
            );
            assert_eq_errno(
                s_straddle as isize,
                -(EFAULT as isize),
                b"stat(straddle)\0".as_ptr(),
            );
            assert_eq_errno(
                s_buf_straddle as isize,
                -(EFAULT as isize),
                b"stat(straddle_statbuf)\0".as_ptr(),
            );
        }

        // SAFETY: Unmapping the test page after straddle testing.
        unsafe {
            munmap(page, 4096);
        }
    }

    let failures = FAILED_TESTS.load(core::sync::atomic::Ordering::Relaxed);
    if failures > 0 {
        // SAFETY: Reporting failure count and exiting init process on test failure.
        unsafe {
            printf(
                b"[FAIL] User Pointer Validation (EFAULT) Hammer had %d failures!\n\0".as_ptr(),
                failures as i32,
            );
            exit(1);
        }
    }
    // SAFETY: Announcing success to stdout.
    unsafe {
        puts(b"[init] User Pointer Validation (EFAULT) Hammer Tests PASSED!\n\0".as_ptr());
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut writer = FdWriter(STDERR_FILENO);
    let _ = core::fmt::write(&mut writer, format_args!("init panic: {}\n", info));
    // SAFETY: Exiting init daemon process with failure code 1.
    unsafe { exit(1) };
}
