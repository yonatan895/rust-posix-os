//! POSIX Epoll Non-blocking Event Multiplexing System Calls.

use alloc::sync::Arc;
use posix_abi::*;
use crate::services::process::get_current_process;
use crate::services::vfs::FileHandle;
use crate::ostd::mm::UserPtr;
use super::map_user_error;

pub fn sys_epoll_create1(_flags: i32) -> isize {
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();
    let epoll_instance = crate::services::vfs::epoll::EpollInstance::new();
    let handle = Arc::new(FileHandle::new(epoll_instance, O_RDWR));
    match proc.alloc_fd(handle) {
        Ok(fd) => fd as isize,
        Err(err) => -(err as isize),
    }
}

pub fn sys_epoll_ctl(epfd: i32, op: i32, fd: i32, event_ptr: *const EpollEvent) -> isize {
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let proc = proc_lock.lock();
    let ep_handle = match proc.get_fd(epfd) {
        Some(h) => h,
        None => return -(EBADF as isize),
    };

    let epoll = match ep_handle.inode.as_epoll() {
        Some(ep) => ep,
        None => return -(EINVAL as isize),
    };

    let event = if !event_ptr.is_null() && op != EPOLL_CTL_DEL {
        let up = match UserPtr::<EpollEvent>::from_raw(event_ptr as usize) {
            Ok(p) => p,
            Err(e) => return -(map_user_error(e) as isize),
        };
        match up.read() {
            Ok(ev) => ev,
            Err(e) => return -(map_user_error(e) as isize),
        }
    } else {
        EpollEvent::default()
    };

    match epoll.ctl(op, fd, event) {
        Ok(()) => 0,
        Err(err) => -(err as isize),
    }
}

pub fn sys_epoll_wait(epfd: i32, events_ptr: *mut EpollEvent, maxevents: i32, _timeout: i32) -> isize {
    if maxevents <= 0 || events_ptr.is_null() {
        return -(EINVAL as isize);
    }
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let proc = proc_lock.lock();
    let ep_handle = match proc.get_fd(epfd) {
        Some(h) => h,
        None => return -(EBADF as isize),
    };

    let epoll = match ep_handle.inode.as_epoll() {
        Some(ep) => ep,
        None => return -(EINVAL as isize),
    };

    let mut kbuf = alloc::vec![EpollEvent::default(); maxevents as usize];
    drop(proc);

    match epoll.wait(&mut kbuf, maxevents as usize) {
        Ok(count) => {
            let size = core::mem::size_of::<EpollEvent>();
            for i in 0..count {
                let addr = (events_ptr as usize).saturating_add(i.saturating_mul(size));
                let out = match UserPtr::<EpollEvent>::from_raw(addr) {
                    Ok(p) => p,
                    Err(e) => return -(map_user_error(e) as isize),
                };
                if let Err(e) = out.write(kbuf[i]) {
                    return -(map_user_error(e) as isize);
                }
            }
            count as isize
        }
        Err(err) => -(err as isize),
    }
}
