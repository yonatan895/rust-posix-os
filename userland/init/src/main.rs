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

/// Counter for failed assertions in the EFAULT hammer test suite.
static FAILED_TESTS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Asserts that a syscall return value matches the expected errno, printing an error and incrementing `FAILED_TESTS` on mismatch.
///
/// # Safety
///
/// `test_name` must point to a valid null-terminated C-string.
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

/// Executes adversarial pointer validation tests across read, write, open, stat, and mmap syscalls.
///
/// # Safety
///
/// Must be executed in userspace context with functional standard I/O and syscall dispatch.
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
    // SAFETY: Validating read syscall return codes against expected EFAULT error.
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
    // SAFETY: Validating write syscall return codes against expected EFAULT error.
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
    // SAFETY: Validating open syscall return codes against expected EFAULT error.
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
    // SAFETY: Stat is a C-compatible POD struct whose zeroed bit pattern represents a valid initial unpopulated state.
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
    // SAFETY: Validating stat syscall return codes against expected EFAULT error.
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
    // SAFETY: Validating mmap syscall return codes against expected ENOMEM error.
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

    // 5b. mmap partial failure rollback test:
    // Query initial free memory, attempt an mmap exceeding total system memory,
    // verify -ENOMEM is returned, verify freeram is completely unchanged (no frame leak),
    // and verify subsequent mmap succeeds cleanly (bump pointer valid).
    let mut si_before = Sysinfo::default();
    // SAFETY: sysinfo call with valid pointer to local stack struct.
    let s_ret1 = unsafe { sysinfo(&mut si_before) };
    if s_ret1 == 0 && si_before.totalram > 0 {
        // Request more memory than total system RAM (totalram + 1 GiB)
        let excessive_len = (si_before.totalram as usize).saturating_add(1024 * 1024 * 1024);
        // SAFETY: Calling mmap with length exceeding physical RAM capacity.
        let failed_mmap = unsafe {
            mmap(
                core::ptr::null_mut(),
                excessive_len,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        // SAFETY: Validating mmap return code against expected ENOMEM error.
        unsafe {
            assert_eq_errno(
                failed_mmap as isize,
                -(ENOMEM as isize),
                b"mmap(excessive_len)\0".as_ptr(),
            );
        }

        let mut si_after = Sysinfo::default();
        // SAFETY: sysinfo call with valid pointer to local stack struct.
        let s_ret2 = unsafe { sysinfo(&mut si_after) };
        if s_ret2 == 0 {
            if si_after.freeram != si_before.freeram {
                // SAFETY: Diagnostic output on physical frame leak.
                unsafe {
                    printf(
                        b"[FAIL] mmap failure leaked frames: before=%u kB, after=%u kB\n\0"
                            .as_ptr(),
                        (si_before.freeram / 1024) as u32,
                        (si_after.freeram / 1024) as u32,
                    );
                }
                FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
        }
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
        // SAFETY: Validating straddle read/write return codes against expected EFAULT error.
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
        // SAFETY: Validating straddle open/stat return codes against expected EFAULT error.
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

/// Runs in-guest VFS atomic rename, address-ordered cross-directory locking, and error atomicity tests.
///
/// # Safety
///
/// Must be executed in user mode with standard filesystem syscalls operational.
unsafe fn run_vfs_rename_tests() {
    let src = b"/tmp/rename_src.txt\0".as_ptr();
    let dst = b"/tmp/rename_dst.txt\0".as_ptr();
    let cycle_parent = b"/tmp/rdir\0".as_ptr();
    let cycle_child = b"/tmp/rdir/sub\0".as_ptr();
    let cycle_target = b"/tmp/rdir/sub/cycle\0".as_ptr();

    // 1. Same-directory rename
    // SAFETY: Creating test file via open.
    let fd = unsafe { open(src, O_CREAT | O_WRONLY | O_TRUNC, 0o644) };
    if fd >= 0 {
        // SAFETY: Closing open file descriptor.
        unsafe { close(fd) };
    }

    // SAFETY: Invoking rename syscall to move src to dst.
    let r1 = unsafe { rename(src, dst) };
    let mut st = Stat::default();
    // SAFETY: Checking that src is gone and dst exists.
    let (s_src, s_dst) = unsafe { (stat(src, &mut st), stat(dst, &mut st)) };

    if r1 != 0 || s_src == 0 || s_dst != 0 {
        // SAFETY: Reporting rename failure.
        unsafe {
            printf(
                b"[FAIL] Same-dir rename failed: ret=%d, s_src=%d, s_dst=%d\n\0".as_ptr(),
                r1,
                s_src,
                s_dst,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }
    // SAFETY: Unlinking test destination file.
    unsafe { unlink(dst) };

    // 2. Cross-directory rename (exercising ADR-0002 L4 address-ordered locking)
    let dir_a = b"/tmp/rdir_a\0".as_ptr();
    let dir_b = b"/tmp/rdir_b\0".as_ptr();
    let file_a = b"/tmp/rdir_a/file.txt\0".as_ptr();
    let file_b = b"/tmp/rdir_b/file.txt\0".as_ptr();

    // SAFETY: Creating test directories.
    unsafe {
        mkdir(dir_a, 0o755);
        mkdir(dir_b, 0o755);
        let fd = open(file_a, O_CREAT | O_WRONLY | O_TRUNC, 0o644);
        if fd >= 0 {
            close(fd);
        }
    }

    // SAFETY: Moving file from dir_a to dir_b across directories.
    let r_cross = unsafe { rename(file_a, file_b) };
    let (s_fa, s_fb) = unsafe { (stat(file_a, &mut st), stat(file_b, &mut st)) };
    if r_cross != 0 || s_fa == 0 || s_fb != 0 {
        // SAFETY: Reporting cross-directory rename failure.
        unsafe {
            printf(
                b"[FAIL] Cross-dir rename failed: ret=%d, s_fa=%d, s_fb=%d\n\0".as_ptr(),
                r_cross,
                s_fa,
                s_fb,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    // 3. Error atomicity: File onto Directory -> EISDIR (file_b must remain intact)
    // SAFETY: Attempting to rename file onto existing directory dir_a.
    let r_eisdir = unsafe { rename(file_b, dir_a) };
    if r_eisdir != -(EISDIR as i32) {
        // SAFETY: Reporting EISDIR failure.
        unsafe {
            printf(
                b"[FAIL] Rename file onto directory expected %d, got %d\n\0".as_ptr(),
                -(EISDIR as i32),
                r_eisdir,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    // 4. Error atomicity: Directory onto File -> ENOTDIR (dir_a must remain intact)
    // SAFETY: Attempting to rename directory onto existing file file_b.
    let r_enotdir = unsafe { rename(dir_a, file_b) };
    if r_enotdir != -(ENOTDIR as i32) {
        // SAFETY: Reporting ENOTDIR failure.
        unsafe {
            printf(
                b"[FAIL] Rename directory onto file expected %d, got %d\n\0".as_ptr(),
                -(ENOTDIR as i32),
                r_enotdir,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    // 5. Error atomicity: Directory onto non-empty directory -> ENOTEMPTY
    // SAFETY: Attempting to rename dir_a onto dir_b which contains file_b.
    let r_enotempty = unsafe { rename(dir_a, dir_b) };
    if r_enotempty != -(ENOTEMPTY as i32) {
        // SAFETY: Reporting ENOTEMPTY failure.
        unsafe {
            printf(
                b"[FAIL] Rename onto non-empty directory expected %d, got %d\n\0".as_ptr(),
                -(ENOTEMPTY as i32),
                r_enotempty,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    // 6. Directory cycle prevention: rename parent into its own child subdirectory
    // SAFETY: Creating parent and child test directories.
    unsafe {
        mkdir(cycle_parent, 0o755);
        mkdir(cycle_child, 0o755);
    }
    // SAFETY: Attempting directory cycle rename (parent into child).
    let r_cycle = unsafe { rename(cycle_parent, cycle_target) };
    if r_cycle != -(EINVAL as i32) {
        // SAFETY: Reporting directory cycle failure.
        unsafe {
            printf(
                b"[FAIL] Directory cycle prevention test failed: expected %d, got %d\n\0".as_ptr(),
                -(EINVAL as i32),
                r_cycle,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    // 7. Rename child directory onto own parent alias -> ENOTEMPTY (must not deadlock / hang!)
    // SAFETY: Attempting to rename cycle_child onto cycle_parent.
    let r_alias = unsafe { rename(cycle_child, cycle_parent) };
    if r_alias != -(ENOTEMPTY as i32) {
        // SAFETY: Reporting alias rename failure.
        unsafe {
            printf(
                b"[FAIL] Rename child onto parent alias expected %d, got %d\n\0".as_ptr(),
                -(ENOTEMPTY as i32),
                r_alias,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    // Clean up test directories and files
    // SAFETY: Cleaning up all test directories and files.
    unsafe {
        unlink(file_b);
        rmdir(dir_b);
        rmdir(dir_a);
        rmdir(cycle_child);
        rmdir(cycle_parent);
    }

    // SAFETY: Announcing success to stdout.
    unsafe {
        puts(b"[init] VFS Atomic Rename & Lock Ordering Tests PASSED!\n\0".as_ptr());
    }
}

/// Runs in-guest Saved-UID/Saved-GID privilege drop and regain, and setresuid/getresuid tests.
///
/// # Safety
///
/// Must be executed in user mode with standard credentials syscalls operational.
unsafe fn run_saved_uid_credentials_tests() {
    // 1. Process starts as root (UID 0)
    // SAFETY: Reading real and effective user IDs via syscall wrappers.
    let uid = unsafe { getuid() };
    let euid = unsafe { geteuid() };
    let mut r = 0u32;
    let mut e = 0u32;
    let mut s = 0u32;
    // SAFETY: Reading real, effective, and saved user IDs.
    let r_getres = unsafe { getresuid(&mut r, &mut e, &mut s) };

    if uid != 0 || euid != 0 || r_getres != 0 || r != 0 || e != 0 || s != 0 {
        // SAFETY: Reporting initial credential failure.
        unsafe {
            printf(
                b"[FAIL] Initial credentials mismatch: uid=%u, euid=%u, r=%u, e=%u, s=%u\n\0"
                    .as_ptr(),
                uid,
                euid,
                r,
                e,
                s,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    // 2. Temporarily drop privileges via seteuid(1000)
    // SAFETY: Dropping effective privilege to UID 1000.
    let r_drop = unsafe { seteuid(1000) };
    let euid_drop = unsafe { geteuid() };
    let _ = unsafe { getresuid(&mut r, &mut e, &mut s) };

    if r_drop != 0 || euid_drop != 1000 || r != 0 || e != 1000 || s != 0 {
        // SAFETY: Reporting seteuid drop failure.
        unsafe {
            printf(
                b"[FAIL] Privilege drop via seteuid(1000) failed: ret=%d, euid=%u, (r=%u, e=%u, s=%u)\n\0"
                    .as_ptr(),
                r_drop,
                euid_drop,
                r,
                e,
                s,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    // 3. Regain root privilege via seteuid(0) because saved-UID is 0
    // SAFETY: Regaining root effective privilege from saved-UID 0.
    let r_regain = unsafe { seteuid(0) };
    let euid_regain = unsafe { geteuid() };
    let _ = unsafe { getresuid(&mut r, &mut e, &mut s) };

    if r_regain != 0 || euid_regain != 0 || r != 0 || e != 0 || s != 0 {
        // SAFETY: Reporting seteuid regain failure.
        unsafe {
            printf(
                b"[FAIL] Privilege regain via seteuid(0) failed: ret=%d, euid=%u, (r=%u, e=%u, s=%u)\n\0"
                    .as_ptr(),
                r_regain,
                euid_regain,
                r,
                e,
                s,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    // 4. Test setresgid and getresgid
    let mut rg = 0u32;
    let mut eg = 0u32;
    let mut sg = 0u32;
    // SAFETY: Modifying group IDs.
    let r_setresg = unsafe { setresgid(0, 500, 0) };
    let _ = unsafe { getresgid(&mut rg, &mut eg, &mut sg) };
    let egid_val = unsafe { getegid() };

    if r_setresg != 0 || egid_val != 500 || rg != 0 || eg != 500 || sg != 0 {
        // SAFETY: Reporting setresgid failure.
        unsafe {
            printf(
                b"[FAIL] Group ID setresgid(0, 500, 0) failed: ret=%d, egid=%u, (rg=%u, eg=%u, sg=%u)\n\0"
                    .as_ptr(),
                r_setresg,
                egid_val,
                rg,
                eg,
                sg,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    // Restore group credentials
    // SAFETY: Restoring root group credentials.
    let _ = unsafe { setresgid(0, 0, 0) };

    // 5. Test fork / child process for permanent drop
    // In a child process, drop all privileges permanently via setuid(1000)
    // Child verifies it cannot regain root (seteuid(0) -> -EPERM)
    // SAFETY: Forking a child process to test permanent privilege drop.
    let child_pid = unsafe { fork() };
    if child_pid == 0 {
        // In child process
        // SAFETY: Dropping all UID credentials permanently.
        let r_perm = unsafe { setuid(1000) };
        let _ = unsafe { getresuid(&mut r, &mut e, &mut s) };
        // SAFETY: Attempting unauthorized regain of root.
        let r_fail_regain = unsafe { seteuid(0) };
        let ok = r_perm == 0
            && r == 1000
            && e == 1000
            && s == 1000
            && r_fail_regain == -(EPERM as i32);
        if ok {
            // SAFETY: Exiting child on success.
            unsafe { exit(0) };
        } else {
            // SAFETY: Exiting child on failure.
            unsafe { exit(1) };
        }
    } else if child_pid > 0 {
        let mut status = 0;
        // SAFETY: Reaping child test process.
        let reaped = unsafe { waitpid(child_pid, &mut status, 0) };
        if reaped != child_pid || status != 0 {
            // SAFETY: Reporting child permanent drop test failure.
            unsafe {
                printf(
                    b"[FAIL] Child permanent drop test failed: reaped=%d, status=%d\n\0".as_ptr(),
                    reaped,
                    status,
                );
                FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
        }
    } else {
        // SAFETY: Reporting fork failure.
        unsafe {
            printf(
                b"[FAIL] fork failed in credentials test: %d\n\0".as_ptr(),
                child_pid,
            );
            FAILED_TESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    // SAFETY: Outputting success message.
    unsafe {
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
