//! Epoll Asynchronous I/O Event Notification (sys/epoll.h).

use crate::syscall::*;
use posix_abi::*;

/// Opens an epoll file descriptor with flags (e.g. `O_CLOEXEC`).
///
/// Returns the new file descriptor on success, or `-1` on error.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn epoll_create1(flags: i32) -> i32 {
    // SAFETY: Issues SYS_EPOLL_CREATE1 syscall with creation flags.
    unsafe { syscall1(SYS_EPOLL_CREATE1, flags as usize) as i32 }
}

/// Opens an epoll file descriptor (legacy interface with size hint).
///
/// Returns the new file descriptor on success, or `-1` on error.
///
/// # Safety
///
/// Direct system call invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn epoll_create(_size: i32) -> i32 {
    // SAFETY: Delegates to epoll_create1 with default flags 0.
    unsafe { epoll_create1(0) }
}

/// Controls an epoll file descriptor: adds, modifies, or removes target descriptors.
///
/// `op` is one of [`EPOLL_CTL_ADD`], [`EPOLL_CTL_MOD`], or [`EPOLL_CTL_DEL`].
///
/// # Safety
///
/// `event` must point to a valid [`EpollEvent`] structure when `op` is `EPOLL_CTL_ADD` or `EPOLL_CTL_MOD`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut EpollEvent) -> i32 {
    // SAFETY: Issues SYS_EPOLL_CTL syscall with epoll fd, op, target fd, and EpollEvent pointer.
    unsafe {
        syscall4(
            SYS_EPOLL_CTL,
            epfd as usize,
            op as usize,
            fd as usize,
            event as usize,
        ) as i32
    }
}

/// Waits for I/O events on an epoll file descriptor.
///
/// Returns the number of ready file descriptors, 0 on timeout, or `-1` on error.
///
/// # Safety
///
/// `events` must point to an array of at least `maxevents` [`EpollEvent`] elements writable by the kernel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn epoll_wait(
    epfd: i32,
    events: *mut EpollEvent,
    maxevents: i32,
    timeout: i32,
) -> i32 {
    // SAFETY: Issues SYS_EPOLL_WAIT syscall with epoll fd, events buffer pointer, maxevents count, and timeout.
    unsafe {
        syscall4(
            SYS_EPOLL_WAIT,
            epfd as usize,
            events as usize,
            maxevents as usize,
            timeout as usize,
        ) as i32
    }
}
