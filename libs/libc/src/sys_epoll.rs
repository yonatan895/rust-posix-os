//! Epoll Asynchronous I/O Event Notification (sys/epoll.h).

use crate::syscall::*;
use posix_abi::*;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn epoll_create1(flags: i32) -> i32 {
    unsafe { syscall1(SYS_EPOLL_CREATE1, flags as usize) as i32 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn epoll_create(_size: i32) -> i32 {
    unsafe { epoll_create1(0) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut EpollEvent) -> i32 {
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn epoll_wait(
    epfd: i32,
    events: *mut EpollEvent,
    maxevents: i32,
    timeout: i32,
) -> i32 {
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
