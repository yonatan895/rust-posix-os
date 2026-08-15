//! POSIX Process Wait Operations (sys/wait.h).

use crate::syscall::*;
use posix_abi::*;

/// Waits for any child process to change state.
///
/// Equivalent to `waitpid(-1, wstatus, 0)`.
///
/// # Safety
///
/// `wstatus` must either be null or point to a valid writable `i32` location.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wait(wstatus: *mut i32) -> i32 {
    // SAFETY: Delegates to waitpid with pid=-1 and options=0.
    unsafe { waitpid(-1, wstatus, 0) }
}

/// Waits for state changes in a specific child process.
///
/// Returns the process ID of the child whose state changed, 0 if `WNOHANG` was specified
/// and no child was ready, or `-1` on error.
///
/// # Safety
///
/// `wstatus` must either be null or point to a valid writable `i32` location.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn waitpid(pid: i32, wstatus: *mut i32, options: i32) -> i32 {
    // SAFETY: Issues SYS_WAIT4 syscall with specified PID, wstatus pointer, options, and rusage null pointer (0).
    unsafe {
        syscall4(
            SYS_WAIT4,
            pid as usize,
            wstatus as usize,
            options as usize,
            0,
        ) as i32
    }
}
