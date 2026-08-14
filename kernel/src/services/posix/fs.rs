//! POSIX Filesystem & I/O System Calls.
//!
//! ADR-0001 R2: no raw user-pointer derefs here. Raw pointers arrive from
//! the dispatcher and are converted via `super::{copy_user_path, map_user_error}`
//! and `ostd::mm::{UserPtr, UserSlice}`.

use super::{copy_user_path, map_user_error};
use crate::ostd::mm::{USER_STR_MAX, UserPtr, UserSlice};
use crate::services::audit::log_audit_event;
use crate::services::process::get_current_process;
use crate::services::vfs::pipe::PipeBuffer;
use crate::services::vfs::*;
use alloc::sync::Arc;
use posix_abi::*;

/// Upper bound on a single read/write bounce buffer, mirroring Linux's
/// MAX_RW_COUNT. Prevents a user-controlled `count` from triggering a huge
/// kernel allocation.
const MAX_RW_COUNT: usize = 1 << 20;

pub fn sys_read(fd: i32, buf: *mut u8, count: usize) -> isize {
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let (handle, calling_pid) = {
        let proc = proc_lock.lock();
        match proc.get_fd(fd) {
            Some(h) => (h, proc.pid),
            None => return -(EBADF as isize),
        }
    };

    let count = count.min(MAX_RW_COUNT);
    let uslice = match UserSlice::from_raw(buf as usize, count) {
        Ok(s) => s,
        Err(e) => return -(map_user_error(e) as isize),
    };
    if let Err(e) = uslice.validate(true) {
        return -(map_user_error(e) as isize);
    }

    let mut kbuf = alloc::vec![0u8; count];
    match handle.read(&mut kbuf, calling_pid) {
        Ok(n) => {
            let out = match UserSlice::from_raw(buf as usize, n) {
                Ok(s) => s,
                Err(e) => return -(map_user_error(e) as isize),
            };
            match out.copy_to_user(&kbuf[..n]) {
                Ok(_) => n as isize,
                Err(e) => -(map_user_error(e) as isize),
            }
        }
        Err(err) => -(err as isize),
    }
}

pub fn sys_write(fd: i32, buf: *const u8, count: usize) -> isize {
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let (handle, calling_pid) = {
        let proc = proc_lock.lock();
        match proc.get_fd(fd) {
            Some(h) => (h, proc.pid),
            None => return -(EBADF as isize),
        }
    };

    let count = count.min(MAX_RW_COUNT);
    let uslice = match UserSlice::from_raw(buf as usize, count) {
        Ok(s) => s,
        Err(e) => return -(map_user_error(e) as isize),
    };
    if let Err(e) = uslice.validate(false) {
        return -(map_user_error(e) as isize);
    }

    let mut kbuf = alloc::vec![0u8; count];
    if let Err(e) = uslice.copy_from_user(&mut kbuf) {
        return -(map_user_error(e) as isize);
    }
    match handle.write(&kbuf, calling_pid) {
        Ok(n) => n as isize,
        Err(err) => -(err as isize),
    }
}

pub fn sys_open(path_ptr: *const u8, flags: i32, mode: u32) -> isize {
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let (pid, uid, gid, umask) = {
        let proc = proc_lock.lock();
        (proc.pid, proc.uid, proc.gid, proc.umask)
    };

    let mut kpath = [0u8; USER_STR_MAX];
    let path = match copy_user_path(path_ptr, &mut kpath) {
        Ok(p) => p,
        Err(e) => return -(e as isize),
    };

    let mut is_created = false;
    let inode = match resolve_path(path) {
        Ok(i) => {
            if (flags & O_CREAT != 0) && (flags & O_EXCL != 0) {
                return -(EEXIST as isize);
            }
            i
        }
        Err(ENOENT) if (flags & O_CREAT != 0) => {
            let (parent, basename) = match resolve_parent_and_basename(path) {
                Ok(res) => res,
                Err(err) => return -(err as isize),
            };
            let creation_mode = ((mode as u16) & 0o777) & !(umask as u16);
            match parent.create_file(&basename, creation_mode, uid, gid) {
                Ok(new_inode) => {
                    is_created = true;
                    new_inode
                }
                Err(err) => return -(err as isize),
            }
        }
        Err(err) => return -(err as isize),
    };

    // Permission enforcement for existing files (root uid == 0 bypasses)
    if !is_created
        && uid != 0
        && let Ok(st) = inode.stat()
    {
        let imode = st.st_mode;
        let req_write = (flags & O_WRONLY != 0) || (flags & O_RDWR != 0);
        let req_read = flags & O_WRONLY == 0;

        let (can_read, can_write) = if uid == st.st_uid {
            (imode & S_IRUSR != 0, imode & S_IWUSR != 0)
        } else if gid == st.st_gid {
            (imode & S_IRGRP != 0, imode & S_IWGRP != 0)
        } else {
            (imode & S_IROTH != 0, imode & S_IWOTH != 0)
        };

        if (req_read && !can_read) || (req_write && !can_write) {
            return -(EACCES as isize);
        }
    }

    if !is_created && (flags & O_TRUNC != 0) && ((flags & O_WRONLY != 0) || (flags & O_RDWR != 0)) {
        let _ = inode.truncate();
    }

    let handle = Arc::new(FileHandle::new(inode, flags));
    let mut proc = proc_lock.lock();
    match proc.alloc_fd(handle) {
        Ok(fd) => {
            if is_created {
                log_audit_event(
                    pid,
                    uid,
                    AUDIT_TYPE_FILE_CREATE,
                    0,
                    path,
                    "File created via open(O_CREAT)",
                );
            }
            fd as isize
        }
        Err(err) => -(err as isize),
    }
}

pub fn sys_close(fd: i32) -> isize {
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();
    match proc.close_fd(fd) {
        Ok(()) => 0,
        Err(err) => -(err as isize),
    }
}

pub fn sys_stat(path_ptr: *const u8, statbuf: *mut Stat) -> isize {
    let mut kpath = [0u8; USER_STR_MAX];
    let path = match copy_user_path(path_ptr, &mut kpath) {
        Ok(p) => p,
        Err(e) => return -(e as isize),
    };

    let inode = match resolve_path(path) {
        Ok(i) => i,
        Err(err) => return -(err as isize),
    };

    match inode.stat() {
        Ok(st) => {
            let out = match UserPtr::<Stat>::from_raw(statbuf as usize) {
                Ok(p) => p,
                Err(e) => return -(map_user_error(e) as isize),
            };
            match out.write(st) {
                Ok(()) => 0,
                Err(e) => -(map_user_error(e) as isize),
            }
        }
        Err(err) => -(err as isize),
    }
}

pub fn sys_fstat(fd: i32, statbuf: *mut Stat) -> isize {
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let handle = {
        let proc = proc_lock.lock();
        match proc.get_fd(fd) {
            Some(h) => h,
            None => return -(EBADF as isize),
        }
    };

    match handle.inode.stat() {
        Ok(st) => {
            let out = match UserPtr::<Stat>::from_raw(statbuf as usize) {
                Ok(p) => p,
                Err(e) => return -(map_user_error(e) as isize),
            };
            match out.write(st) {
                Ok(()) => 0,
                Err(e) => -(map_user_error(e) as isize),
            }
        }
        Err(err) => -(err as isize),
    }
}

pub fn sys_lseek(fd: i32, offset: i64, whence: i32) -> isize {
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let handle = {
        let proc = proc_lock.lock();
        match proc.get_fd(fd) {
            Some(h) => h,
            None => return -(EBADF as isize),
        }
    };

    match handle.lseek(offset, whence) {
        Ok(new_off) => new_off as isize,
        Err(err) => -(err as isize),
    }
}

pub fn sys_pipe(pipefd_ptr: *mut [i32; 2]) -> isize {
    // Validate BEFORE allocating fds: a bad user pointer must not leak them.
    let out = match UserPtr::<[i32; 2]>::from_raw(pipefd_ptr as usize) {
        Ok(p) => p,
        Err(e) => return -(map_user_error(e) as isize),
    };
    if let Err(e) = out.validate(true) {
        return -(map_user_error(e) as isize);
    }

    let (read_end, write_end) = PipeBuffer::new();
    let read_handle = Arc::new(FileHandle::new(read_end, O_RDONLY));
    let write_handle = Arc::new(FileHandle::new(write_end, O_WRONLY));

    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();

    let fd0 = match proc.alloc_fd(read_handle) {
        Ok(fd) => fd,
        Err(err) => return -(err as isize),
    };

    let fd1 = match proc.alloc_fd(write_handle) {
        Ok(fd) => fd,
        Err(err) => {
            let _ = proc.close_fd(fd0);
            return -(err as isize);
        }
    };

    if let Err(e) = out.write([fd0, fd1]) {
        let _ = proc.close_fd(fd0);
        let _ = proc.close_fd(fd1);
        return -(map_user_error(e) as isize);
    }

    0
}

pub fn sys_dup(oldfd: i32) -> isize {
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();
    let handle = match proc.get_fd(oldfd) {
        Some(h) => h,
        None => return -(EBADF as isize),
    };

    match proc.alloc_fd(handle) {
        Ok(new_fd) => new_fd as isize,
        Err(err) => -(err as isize),
    }
}

pub fn sys_dup2(oldfd: i32, newfd: i32) -> isize {
    if !(0..256).contains(&newfd) {
        return -(EBADF as isize);
    }
    if oldfd == newfd {
        return newfd as isize;
    }

    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();
    let handle = match proc.get_fd(oldfd) {
        Some(h) => h,
        None => return -(EBADF as isize),
    };

    let _ = proc.close_fd(newfd);
    while proc.fds.len() <= newfd as usize {
        proc.fds.push(None);
    }
    proc.fds[newfd as usize] = Some(handle);
    newfd as isize
}

pub fn sys_mkdir(path_ptr: *const u8, mode: u32) -> isize {
    let (pid, uid, gid, umask) = match get_current_process() {
        Some(p) => {
            let proc = p.lock();
            (proc.pid, proc.uid, proc.gid, proc.umask)
        }
        None => (0, 0, 0, 0o022),
    };
    let mut kpath = [0u8; USER_STR_MAX];
    let path = match copy_user_path(path_ptr, &mut kpath) {
        Ok(p) => p,
        Err(e) => return -(e as isize),
    };

    let (parent, basename) = match resolve_parent_and_basename(path) {
        Ok(res) => res,
        Err(err) => {
            log_audit_event(
                pid,
                uid,
                AUDIT_TYPE_DIR_CREATE,
                -err,
                path,
                "Directory create failed (parent unresolved)",
            );
            return -(err as isize);
        }
    };

    let effective_mode = ((mode as u16) & 0o777) & !(umask as u16);
    match parent.create_dir(&basename, effective_mode, uid, gid) {
        Ok(_) => {
            log_audit_event(
                pid,
                uid,
                AUDIT_TYPE_DIR_CREATE,
                0,
                path,
                "Directory created",
            );
            0
        }
        Err(err) => {
            log_audit_event(
                pid,
                uid,
                AUDIT_TYPE_DIR_CREATE,
                -err,
                path,
                "Directory create failed",
            );
            -(err as isize)
        }
    }
}

pub fn sys_unlink(path_ptr: *const u8) -> isize {
    let (pid, uid) = match get_current_process() {
        Some(p) => {
            let proc = p.lock();
            (proc.pid, proc.uid)
        }
        None => (0, 0),
    };
    let mut kpath = [0u8; USER_STR_MAX];
    let path = match copy_user_path(path_ptr, &mut kpath) {
        Ok(p) => p,
        Err(e) => return -(e as isize),
    };

    let (parent, basename) = match resolve_parent_and_basename(path) {
        Ok(res) => res,
        Err(err) => {
            log_audit_event(
                pid,
                uid,
                AUDIT_TYPE_FILE_UNLINK,
                -err,
                path,
                "Unlink failed (parent unresolved)",
            );
            return -(err as isize);
        }
    };

    match parent.unlink(&basename) {
        Ok(()) => {
            log_audit_event(
                pid,
                uid,
                AUDIT_TYPE_FILE_UNLINK,
                0,
                path,
                "File or directory unlinked",
            );
            0
        }
        Err(err) => {
            log_audit_event(
                pid,
                uid,
                AUDIT_TYPE_FILE_UNLINK,
                -err,
                path,
                "Unlink failed",
            );
            -(err as isize)
        }
    }
}

pub fn sys_rename(oldpath_ptr: *const u8, newpath_ptr: *const u8) -> isize {
    let (pid, uid) = match get_current_process() {
        Some(p) => {
            let proc = p.lock();
            (proc.pid, proc.uid)
        }
        None => (0, 0),
    };
    let mut kold = [0u8; USER_STR_MAX];
    let oldpath = match copy_user_path(oldpath_ptr, &mut kold) {
        Ok(p) => p,
        Err(e) => return -(e as isize),
    };
    let mut knew = [0u8; USER_STR_MAX];
    let newpath = match copy_user_path(newpath_ptr, &mut knew) {
        Ok(p) => p,
        Err(e) => return -(e as isize),
    };

    let (old_parent, old_basename) = match resolve_parent_and_basename(oldpath) {
        Ok(res) => res,
        Err(err) => return -(err as isize),
    };
    let (new_parent, new_basename) = match resolve_parent_and_basename(newpath) {
        Ok(res) => res,
        Err(err) => return -(err as isize),
    };

    let source_inode = match old_parent.lookup(&old_basename) {
        Ok(i) => i,
        Err(err) => return -(err as isize),
    };

    if let Ok(target_inode) = new_parent.lookup(&new_basename) {
        if source_inode.file_type() == FileType::Directory
            && target_inode.file_type() != FileType::Directory
        {
            return -(ENOTDIR as isize);
        }
        if source_inode.file_type() != FileType::Directory
            && target_inode.file_type() == FileType::Directory
        {
            return -(EISDIR as isize);
        }
    }

    if let Err(err) = new_parent.link_entry(&new_basename, source_inode) {
        return -(err as isize);
    }
    if let Err(err) = old_parent.unlink(&old_basename) {
        return -(err as isize);
    }

    log_audit_event(
        pid,
        uid,
        AUDIT_TYPE_FILE_MODIFY,
        0,
        newpath,
        "File or directory renamed/moved",
    );
    0
}

pub fn sys_getdents64(fd: i32, dirp: *mut u8, count: usize) -> isize {
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let handle = {
        let proc = proc_lock.lock();
        match proc.get_fd(fd) {
            Some(h) => h,
            None => return -(EBADF as isize),
        }
    };

    let ubuf = match UserSlice::from_raw(dirp as usize, count) {
        Ok(s) => s,
        Err(e) => return -(map_user_error(e) as isize),
    };
    if let Err(e) = ubuf.validate(true) {
        return -(map_user_error(e) as isize);
    }

    let entries = match handle.inode.readdir() {
        Ok(e) => e,
        Err(err) => return -(err as isize),
    };

    let mut offset_guard = handle.offset.lock();
    let current_idx = *offset_guard;
    if current_idx >= entries.len() {
        return 0;
    }

    let dirent_size = core::mem::size_of::<Dirent64>();
    let mut written = 0;
    let mut entries_written = 0;

    for entry in entries.iter().skip(current_idx) {
        if written + dirent_size > count {
            break;
        }
        let target = match UserPtr::<Dirent64>::from_raw(dirp as usize + written) {
            Ok(p) => p,
            Err(e) => return -(map_user_error(e) as isize),
        };
        if let Err(e) = target.write(*entry) {
            return -(map_user_error(e) as isize);
        }
        written += dirent_size;
        entries_written += 1;
    }

    *offset_guard += entries_written;
    written as isize
}
