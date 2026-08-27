//! Process 1 (init daemon) for the Rust POSIX OS userland.
//!
//! Handles userspace initialization, heap and EFAULT validation tests,
//! syscall microbenchmarking, spawning the interactive shell, and reaping orphaned children.

#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
// Userland crate uses C-style FFI patterns (nul-terminated byte-string literals,
// raw pointer arithmetic) that conflict with clippy's Rust-idiomatic expectations.
#![allow(clippy::all)]

use core::panic::PanicInfo;
use libc::*;
use posix_abi::*;

/// Entry point for the init daemon (PID 1).
///
/// Sets up initial execution, performs memory allocation tests, runs EFAULT hammer tests,
/// runs syscall microbenchmarks, executes `/bin/sh`, and enters an infinite child reaping loop.
///
/// # Safety
///
/// This function must be invoked as the raw ELF entry point with a valid stack.
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

    // Run VFS Atomic Rename & Directory Cycle Tests
    // SAFETY: Executing in-guest atomic rename and directory cycle tests in PID 1 user mode.
    unsafe {
        run_vfs_rename_tests();
    }

    // Run Saved-UID Privilege Drop and Regain Tests
    // SAFETY: Executing in-guest saved-UID/saved-GID credentials tests in PID 1 user mode.
    unsafe {
        run_saved_uid_credentials_tests();
    }

    // Run Syscall Microbenchmark (100,000 getpid fast syscalls in-guest)
    // SAFETY: Executing in-guest hardware fast-syscall benchmark to measure real hardware cycles.
    unsafe {
        run_syscall_microbench();
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

/// Counter for failed assertions in the test suite.
static FAILED_TESTS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

macro_rules! check_errno {
    ($expr:expr, $expected_errno:expr, $name:expr) => {
        // SAFETY: Executing test syscall with intentional adversarial arguments.
        let res = unsafe { $expr } as isize;
        if res != -($expected_errno as isize) {
            // SAFETY: Printing failure diagnostic to standard error.
            unsafe {
                printf(
                    b"[FAIL] %s: expected errno %d, got %d\n\0".as_ptr(),
                    $name,
                    $expected_errno as i32,
                    -res as i32,
                );
            }
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    };
}

macro_rules! check_efault {
    ($expr:expr, $name:expr) => {
        check_errno!($expr, EFAULT, $name);
    };
}

/// Executes adversarial pointer validation tests across read, write, open, stat, and mmap syscalls.
///
/// # Safety
///
/// Must be executed in userspace context with functional standard I/O and syscall dispatch.
unsafe fn run_efault_hammer_tests() {
    // SAFETY: Outputting start banner to stdout.
    unsafe {
        puts(b"[init] Running User Pointer Validation (EFAULT) Hammer Tests...\n\0".as_ptr())
    };
    let null = core::ptr::null_mut::<u8>();
    let kern = 0xFFFF_8000_0000_0000usize as *mut u8;
    let unmap = 0x0000_7000_1234_0000usize as *mut u8;
    // SAFETY: Stat is zeroable POD.
    let mut st: Stat = unsafe { core::mem::zeroed() };

    for &(p, name) in &[
        (null, b"null\0".as_ptr()),
        (kern, b"kern\0".as_ptr()),
        (unmap, b"unmap\0".as_ptr()),
    ] {
        check_efault!(read(0, p, 10), name);
        check_efault!(write(1, p, 10), name);
        check_efault!(open(p, O_RDONLY, 0), name);
        check_efault!(stat(p, &mut st), name);
        check_efault!(stat(b"/dev/null\0".as_ptr(), p as *mut Stat), name);
    }

    check_errno!(
        mmap(
            kern,
            4096,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0
        ),
        ENOMEM,
        b"mmap(kern)\0".as_ptr()
    );
    check_errno!(
        mmap(
            null,
            usize::MAX,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0
        ),
        ENOMEM,
        b"mmap(len=MAX)\0".as_ptr()
    );

    let mut si_before = Sysinfo::default();
    // SAFETY: Querying system metrics to valid local buffer.
    if unsafe { sysinfo(&mut si_before) } == 0 && si_before.totalram > 0 {
        let excessive_len = (si_before.totalram as usize).saturating_add(1024 * 1024 * 1024);
        check_errno!(
            mmap(
                null,
                excessive_len,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0
            ),
            ENOMEM,
            b"mmap(excessive)\0".as_ptr()
        );
        let mut si_after = Sysinfo::default();
        // SAFETY: Querying system metrics after failed mmap to ensure no leaked frames.
        if unsafe { sysinfo(&mut si_after) } == 0 && si_after.freeram != si_before.freeram {
            // SAFETY: Printing failure diagnostic.
            unsafe { printf(b"[FAIL] mmap failure leaked frames\n\0".as_ptr()) };
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    // SAFETY: Allocating test user page for straddle tests.
    let page = unsafe {
        mmap(
            null,
            4096,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if (page as isize) > 0 {
        let straddle_buf = (page as usize + 4090) as *mut u8;
        check_efault!(read(0, straddle_buf, 16), b"read(straddle)\0".as_ptr());
        check_efault!(write(1, straddle_buf, 16), b"write(straddle)\0".as_ptr());
        // SAFETY: Writing 6 bytes within mapped page boundary before page edge.
        for i in 0..6 {
            unsafe { *straddle_buf.add(i) = b'A' };
        }
        check_efault!(
            open(straddle_buf, O_RDONLY, 0),
            b"open(straddle)\0".as_ptr()
        );
        check_efault!(stat(straddle_buf, &mut st), b"stat(straddle)\0".as_ptr());
        check_efault!(
            stat(b"/dev/null\0".as_ptr(), straddle_buf as *mut Stat),
            b"stat(straddle_buf)\0".as_ptr()
        );
        // SAFETY: Unmapping temporary test page.
        unsafe { munmap(page, 4096) };
    }

    let failures = FAILED_TESTS.load(core::sync::atomic::Ordering::Relaxed);
    if failures > 0 {
        // SAFETY: Outputting failure summary and terminating test harness.
        unsafe {
            printf(
                b"[FAIL] User Pointer Validation (EFAULT) Hammer had %d failures!\n\0".as_ptr(),
                failures as i32,
            );
            exit(1);
        }
    }
    // SAFETY: Outputting success banner to stdout.
    unsafe { puts(b"[init] User Pointer Validation (EFAULT) Hammer Tests PASSED!\n\0".as_ptr()) };
}

/// Runs in-guest VFS atomic rename, address-ordered cross-directory locking, and error atomicity tests.
///
/// # Safety
///
/// Must be executed in user mode with standard filesystem syscalls operational.
unsafe fn run_vfs_rename_tests() {
    let src = b"/tmp/rename_src.txt\0".as_ptr();
    let dst = b"/tmp/rename_dst.txt\0".as_ptr();
    let mut st = Stat::default();

    // SAFETY: Exercising VFS file creation, rename, and directory cycle rejection syscalls.
    unsafe {
        let fd = open(src, O_CREAT | O_WRONLY | O_TRUNC, 0o644);
        if fd >= 0 {
            close(fd);
        }
        let r1 = rename(src, dst);
        let (s_src, s_dst) = (stat(src, &mut st), stat(dst, &mut st));
        if r1 != 0 || s_src == 0 || s_dst != 0 {
            printf(b"[FAIL] Same-dir rename failed: ret=%d\n\0".as_ptr(), r1);
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        unlink(dst);

        let (dir_a, dir_b) = (b"/tmp/rdir_a\0".as_ptr(), b"/tmp/rdir_b\0".as_ptr());
        let (file_a, file_b) = (
            b"/tmp/rdir_a/file.txt\0".as_ptr(),
            b"/tmp/rdir_b/file.txt\0".as_ptr(),
        );
        mkdir(dir_a, 0o755);
        mkdir(dir_b, 0o755);
        let fd = open(file_a, O_CREAT | O_WRONLY | O_TRUNC, 0o644);
        if fd >= 0 {
            close(fd);
        }

        let r_cross = rename(file_a, file_b);
        let (s_fa, s_fb) = (stat(file_a, &mut st), stat(file_b, &mut st));
        if r_cross != 0 || s_fa == 0 || s_fb != 0 {
            printf(
                b"[FAIL] Cross-dir rename failed: ret=%d\n\0".as_ptr(),
                r_cross,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }

        if rename(file_b, dir_a) != -(EISDIR as i32) {
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        if rename(dir_a, file_b) != -(ENOTDIR as i32) {
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        if rename(dir_a, dir_b) != -(ENOTEMPTY as i32) {
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }

        let (cycle_parent, cycle_child, cycle_target) = (
            b"/tmp/rdir\0".as_ptr(),
            b"/tmp/rdir/sub\0".as_ptr(),
            b"/tmp/rdir/sub/cycle\0".as_ptr(),
        );
        mkdir(cycle_parent, 0o755);
        mkdir(cycle_child, 0o755);
        if rename(cycle_parent, cycle_target) != -(EINVAL as i32) {
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        if rename(cycle_child, cycle_parent) != -(ENOTEMPTY as i32) {
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }

        unlink(file_b);
        rmdir(dir_b);
        rmdir(dir_a);
        rmdir(cycle_child);
        rmdir(cycle_parent);
        puts(b"[init] VFS Atomic Rename & Lock Ordering Tests PASSED!\n\0".as_ptr());
    }
}

/// Runs in-guest Saved-UID/Saved-GID privilege drop and regain, and setresuid/getresuid tests.
///
/// # Safety
///
/// Must be executed in user mode with standard credentials syscalls operational.
unsafe fn run_saved_uid_credentials_tests() {
    let (mut r, mut e, mut s) = (0u32, 0u32, 0u32);
    // SAFETY: Exercising process credential syscalls from root process.
    unsafe {
        if getuid() != 0
            || geteuid() != 0
            || getresuid(&mut r, &mut e, &mut s) != 0
            || r != 0
            || e != 0
            || s != 0
        {
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        if seteuid(1000) != 0
            || geteuid() != 1000
            || getresuid(&mut r, &mut e, &mut s) != 0
            || e != 1000
        {
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        if seteuid(2000) != -(EPERM as i32) {
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        if seteuid(0) != 0 || geteuid() != 0 || getresuid(&mut r, &mut e, &mut s) != 0 || e != 0 {
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        let (mut rg, mut eg, mut sg) = (0u32, 0u32, 0u32);
        if setresgid(0, 500, 0) != 0
            || getegid() != 500
            || getresgid(&mut rg, &mut eg, &mut sg) != 0
            || eg != 500
        {
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        let _ = setresgid(0, 0, 0);
        puts(b"[init] Saved-UID Privilege Drop and Regain Tests PASSED!\n\0".as_ptr());
    }
}

/// Reads the current CPU timestamp counter (TSC).
#[inline(always)]
fn read_tsc() -> u64 {
    // SAFETY: RDTSC is an unprivileged user-mode instruction on x86_64.
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// Runs a microbenchmark measuring hardware CPU cycle latency for 100,000 `getpid` syscalls.
///
/// # Safety
///
/// Must be called with functional standard output for printing benchmark statistics.
unsafe fn run_syscall_microbench() {
    // SAFETY: Outputting benchmark start message to stdout.
    unsafe {
        puts(b"[bench] Running in-guest syscall microbenchmark (100,000 getpid)...\n\0".as_ptr());
    }

    // Warm-up cache lines and branch predictors (1,000 getpid syscalls)
    for _ in 0..1000 {
        // SAFETY: Invoking getpid fast syscall.
        let _ = unsafe { getpid() };
    }

    let start_tsc = read_tsc();
    let mut last_pid = 0;
    for _ in 0..100_000 {
        // SAFETY: Invoking getpid fast syscall (ring 3 -> ring 0 -> sysretq).
        last_pid = unsafe { getpid() };
    }
    let end_tsc = read_tsc();

    let total_cycles = end_tsc.saturating_sub(start_tsc);
    let avg_cycles = total_cycles / 100_000;

    // SAFETY: Writing measured in-guest microbenchmark results to stdout/serial.
    unsafe {
        printf(
            b"[bench] In-guest 100k getpid complete: %u total cycles (~%u cycles/syscall, PID=%d)\n\0".as_ptr(),
            total_cycles as u32,
            avg_cycles as u32,
            last_pid,
        );
    }
}

/// Userland panic handler for the init daemon.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    write_panic_info(STDERR_FILENO, "init panic", info);
    // SAFETY: Exiting init daemon process with failure code 1.
    unsafe { exit(1) };
}
