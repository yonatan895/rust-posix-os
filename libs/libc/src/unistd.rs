//! POSIX Standard Symbolic Constants & Types (unistd).

use crate::syscall::*;
use posix_abi::*;

/// Reads up to `count` bytes from file descriptor `fd` into `buf`.
///
/// Returns the number of bytes read, 0 at end-of-file, or a negative error code.
///
/// # Safety
///
/// `buf` must point to a buffer of at least `count` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn read(fd: i32, buf: *mut u8, count: usize) -> isize {
    // SAFETY: Issues SYS_READ syscall with specified file descriptor, buffer pointer, and byte count.
    unsafe { syscall3(SYS_READ, fd as usize, buf as usize, count) as isize }
}

/// Writes up to `count` bytes from `buf` to the file descriptor `fd`.
///
/// Returns the number of bytes written, or a negative error code.
///
/// # Safety
///
/// `buf` must point to a buffer of at least `count` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn write(fd: i32, buf: *const u8, count: usize) -> isize {
    // SAFETY: Issues SYS_WRITE syscall with specified file descriptor, buffer pointer, and byte count.
    unsafe { syscall3(SYS_WRITE, fd as usize, buf as usize, count) as isize }
}

/// Opens the file specified by `path` with flags `flags` and permission mode `mode`.
///
/// Returns the new file descriptor on success, or a negative error code.
///
/// # Safety
///
/// `path` must point to a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn open(path: *const u8, flags: i32, mode: u32) -> i32 {
    // SAFETY: Issues SYS_OPEN syscall with null-terminated pathname, open flags, and permission mode.
    unsafe { syscall3(SYS_OPEN, path as usize, flags as usize, mode as usize) as i32 }
}

/// Closes a file descriptor `fd`.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn close(fd: i32) -> i32 {
    // SAFETY: Issues SYS_CLOSE syscall to close the open file descriptor.
    unsafe { syscall1(SYS_CLOSE, fd as usize) as i32 }
}

/// Repositions the read/write offset of the open file descriptor `fd`.
///
/// Returns the resulting offset location in bytes, or a negative error code.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    // SAFETY: Issues SYS_LSEEK syscall to reposition the file offset.
    unsafe { syscall3(SYS_LSEEK, fd as usize, offset as usize, whence as usize) as i64 }
}

/// Duplicates an open file descriptor `oldfd`.
///
/// Returns the new file descriptor, or a negative error code.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dup(oldfd: i32) -> i32 {
    // SAFETY: Issues SYS_DUP syscall to duplicate oldfd.
    unsafe { syscall1(SYS_DUP, oldfd as usize) as i32 }
}

/// Duplicates `oldfd` to `newfd`, closing `newfd` first if open.
///
/// Returns `newfd` on success, or a negative error code.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dup2(oldfd: i32, newfd: i32) -> i32 {
    // SAFETY: Issues SYS_DUP2 syscall to duplicate oldfd into newfd.
    unsafe { syscall2(SYS_DUP2, oldfd as usize, newfd as usize) as i32 }
}

/// Creates a unidirectional data channel (pipe).
///
/// Stores read end in `pipefd[0]` and write end in `pipefd[1]`.
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// `pipefd` must point to a writable array of two `i32` integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pipe(pipefd: *mut [i32; 2]) -> i32 {
    // SAFETY: Issues SYS_PIPE syscall with pointer to a 2-element i32 array.
    unsafe { syscall1(SYS_PIPE, pipefd as usize) as i32 }
}

/// Creates a new process by duplicating the calling process.
///
/// Returns 0 in child, child PID in parent, or negative error code on failure.
///
/// # Safety
///
/// Direct system call invocation duplicating execution context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fork() -> i32 {
    // SAFETY: Issues SYS_FORK syscall to duplicate current process address space and execution context.
    unsafe { syscall0(SYS_FORK) as i32 }
}

/// Executes the program referred to by `path`.
///
/// # Safety
///
/// `path` must point to a valid null-terminated C string.
/// `argv` and `envp` must point to null-terminated arrays of null-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn execve(
    path: *const u8,
    argv: *const *const u8,
    envp: *const *const u8,
) -> i32 {
    // SAFETY: Issues SYS_EXECVE syscall with pointers to path, argv, and envp.
    unsafe { syscall3(SYS_EXECVE, path as usize, argv as usize, envp as usize) as i32 }
}

/// Returns the process ID of the calling process.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpid() -> i32 {
    // SAFETY: Issues SYS_GETPID syscall.
    unsafe { syscall0(SYS_GETPID) as i32 }
}

/// Returns the process ID of the parent of the calling process.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getppid() -> i32 {
    // SAFETY: Issues SYS_GETPPID syscall.
    unsafe { syscall0(SYS_GETPPID) as i32 }
}

/// Returns the real user ID of the calling process.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getuid() -> u32 {
    // SAFETY: Issues SYS_GETUID syscall.
    unsafe { syscall0(SYS_GETUID) as u32 }
}

/// Returns the effective user ID of the calling process.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn geteuid() -> u32 {
    // SAFETY: Issues SYS_GETEUID syscall.
    unsafe { syscall0(SYS_GETEUID) as u32 }
}

/// Returns the real group ID of the calling process.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgid() -> u32 {
    // SAFETY: Issues SYS_GETGID syscall.
    unsafe { syscall0(SYS_GETGID) as u32 }
}

/// Returns the effective group ID of the calling process.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getegid() -> u32 {
    // SAFETY: Issues SYS_GETEGID syscall.
    unsafe { syscall0(SYS_GETEGID) as u32 }
}

/// Sets the real and effective user ID of the calling process.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setuid(uid: u32) -> i32 {
    // SAFETY: Issues SYS_SETUID syscall.
    unsafe { syscall1(SYS_SETUID, uid as usize) as i32 }
}

/// Sets the real and effective group ID of the calling process.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setgid(gid: u32) -> i32 {
    // SAFETY: Issues SYS_SETGID syscall.
    unsafe { syscall1(SYS_SETGID, gid as usize) as i32 }
}

/// Sets the effective user ID of the calling process.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seteuid(euid: u32) -> i32 {
    // SAFETY: Issues SYS_SETEUID syscall.
    unsafe { syscall1(SYS_SETEUID, euid as usize) as i32 }
}

/// Sets the effective group ID of the calling process.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setegid(egid: u32) -> i32 {
    // SAFETY: Issues SYS_SETEGID syscall.
    unsafe { syscall1(SYS_SETEGID, egid as usize) as i32 }
}

/// Sets the real, effective, and saved user IDs of the calling process.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setresuid(ruid: u32, euid: u32, suid: u32) -> i32 {
    // SAFETY: Issues SYS_SETRESUID syscall.
    unsafe { syscall3(SYS_SETRESUID, ruid as usize, euid as usize, suid as usize) as i32 }
}

/// Retrieves the real, effective, and saved user IDs of the calling process.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// Caller must ensure destination pointers are valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getresuid(ruid: *mut u32, euid: *mut u32, suid: *mut u32) -> i32 {
    // SAFETY: Issues SYS_GETRESUID syscall.
    unsafe { syscall3(SYS_GETRESUID, ruid as usize, euid as usize, suid as usize) as i32 }
}

/// Sets the real, effective, and saved group IDs of the calling process.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setresgid(rgid: u32, egid: u32, sgid: u32) -> i32 {
    // SAFETY: Issues SYS_SETRESGID syscall.
    unsafe { syscall3(SYS_SETRESGID, rgid as usize, egid as usize, sgid as usize) as i32 }
}

/// Retrieves the real, effective, and saved group IDs of the calling process.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// Caller must ensure destination pointers are valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getresgid(rgid: *mut u32, egid: *mut u32, sgid: *mut u32) -> i32 {
    // SAFETY: Issues SYS_GETRESGID syscall.
    unsafe { syscall3(SYS_GETRESGID, rgid as usize, egid as usize, sgid as usize) as i32 }
}

/// Tests whether a file descriptor refers to a terminal.
///
/// Returns 1 if `fd` refers to a terminal, 0 otherwise.
///
/// # Safety
///
/// Direct system call invocation via `ioctl`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn isatty(fd: i32) -> i32 {
    let mut term = Termios::default();
    // SAFETY: Issues SYS_IOCTL syscall with TCGETS request and pointer to local Termios struct.
    let res = unsafe {
        syscall3(
            SYS_IOCTL,
            fd as usize,
            0x5401, /* TCGETS */
            &mut term as *mut _ as usize,
        )
    };
    if res == 0 { 1 } else { 0 }
}

/// Changes the current working directory of the calling process.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// `path` must point to a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chdir(path: *const u8) -> i32 {
    // SAFETY: Issues SYS_CHDIR syscall with null-terminated pathname pointer.
    unsafe { syscall1(SYS_CHDIR, path as usize) as i32 }
}

/// Gets the current working directory pathname into `buf`.
///
/// Returns `buf` on success, or null on failure.
///
/// # Safety
///
/// `buf` must point to writable memory of at least `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getcwd(buf: *mut u8, size: usize) -> *mut u8 {
    // SAFETY: Issues SYS_GETCWD syscall with destination buffer pointer and capacity.
    let res = unsafe { syscall2(SYS_GETCWD, buf as usize, size) as isize };
    if res >= 0 { buf } else { core::ptr::null_mut() }
}

/// Deletes a name from the filesystem.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// `path` must point to a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unlink(path: *const u8) -> i32 {
    // SAFETY: Issues SYS_UNLINK syscall with null-terminated pathname pointer.
    unsafe { syscall1(SYS_UNLINK, path as usize) as i32 }
}

/// Attempts to create a directory named `path` with permissions `mode`.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// `path` must point to a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mkdir(path: *const u8, mode: u32) -> i32 {
    // SAFETY: Issues SYS_MKDIR syscall with null-terminated pathname pointer and mode permissions.
    unsafe { syscall2(SYS_MKDIR, path as usize, mode as usize) as i32 }
}

/// Deletes a directory, which must be empty.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// `path` must point to a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rmdir(path: *const u8) -> i32 {
    // SAFETY: Issues SYS_RMDIR syscall with null-terminated pathname pointer.
    unsafe { syscall1(SYS_RMDIR, path as usize) as i32 }
}

/// Suspends execution of the calling process for `seconds` seconds.
///
/// Returns 0 on complete sleep, or unslept seconds remaining if interrupted.
///
/// # Safety
///
/// Direct system call invocation via `nanosleep`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sleep(seconds: u32) -> u32 {
    let req = Timespec {
        tv_sec: seconds as i64,
        tv_nsec: 0,
    };
    let mut rem = Timespec::default();
    // SAFETY: Issues SYS_NANOSLEEP syscall with local Timespec request and remainder pointers.
    unsafe {
        syscall2(
            SYS_NANOSLEEP,
            &req as *const _ as usize,
            &mut rem as *mut _ as usize,
        );
    }
    rem.tv_sec as u32
}

/// Suspends execution of the calling process for `usec` microseconds.
///
/// Returns 0 on success, or -1 on error.
///
/// # Safety
///
/// Direct system call invocation via `nanosleep`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn usleep(usec: u64) -> i32 {
    let req = Timespec {
        tv_sec: (usec / 1_000_000) as i64,
        tv_nsec: ((usec % 1_000_000) * 1000) as i64,
    };
    // SAFETY: Issues SYS_NANOSLEEP syscall with computed duration Timespec pointer.
    unsafe { syscall2(SYS_NANOSLEEP, &req as *const _ as usize, 0) as i32 }
}

/// Returns global system information.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// `info` must point to a valid writable [`Sysinfo`] structure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysinfo(info: *mut Sysinfo) -> i32 {
    // SAFETY: Issues SYS_SYSINFO syscall with pointer to writable Sysinfo struct.
    unsafe { syscall1(SYS_SYSINFO, info as usize) as i32 }
}

/// Records an audit log event in the kernel audit trail.
///
/// Returns 0 on success, or a negative error code.
///
/// # Safety
///
/// `target` and `details` must point to valid null-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn audit_log(event_type: u32, target: *const u8, details: *const u8) -> i32 {
    // SAFETY: Issues SYS_AUDIT_LOG syscall with event type and null-terminated string pointers.
    unsafe {
        syscall3(
            SYS_AUDIT_LOG,
            event_type as usize,
            target as usize,
            details as usize,
        ) as i32
    }
}

/// Captures a point-in-time system state audit snapshot.
///
/// Returns snapshot identifier on success, or negative error code on failure.
///
/// # Safety
///
/// `label` must point to a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn audit_snapshot(label: *const u8, flags: u32) -> i64 {
    // SAFETY: Issues SYS_AUDIT_SNAPSHOT syscall with label string pointer and flags.
    unsafe { syscall2(SYS_AUDIT_SNAPSHOT, label as usize, flags as usize) as i64 }
}
