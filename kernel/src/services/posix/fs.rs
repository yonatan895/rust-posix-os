//! POSIX Filesystem & I/O System Calls.

use alloc::sync::Arc;
use posix_abi::*;
use crate::services::process::get_current_process;
use crate::services::vfs::*;
use crate::services::vfs::pipe::PipeBuffer;
use crate::services::audit::log_audit_event;

pub fn sys_read(fd: i32, buf: *mut u8, count: usize) -> isize {
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

    let slice = unsafe { core::slice::from_raw_parts_mut(buf, count) };
    match handle.read(slice) {
        Ok(n) => n as isize,
        Err(err) => -(err as isize),
    }
}

pub fn sys_write(fd: i32, buf: *const u8, count: usize) -> isize {
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

    let slice = unsafe { core::slice::from_raw_parts(buf, count) };
    match handle.write(slice) {
        Ok(n) => n as isize,
        Err(err) => -(err as isize),
    }
}

pub fn sys_open(path_ptr: *const u8, flags: i32, _mode: u32) -> isize {
    let path = unsafe {
        let mut len = 0;
        while *path_ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(path_ptr, len))
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
            match parent.create_file(&basename) {
                Ok(new_inode) => {
                    is_created = true;
                    new_inode
                }
                Err(err) => return -(err as isize),
            }
        }
        Err(err) => return -(err as isize),
    };

    if !is_created && (flags & O_TRUNC != 0) && ((flags & O_WRONLY != 0) || (flags & O_RDWR != 0)) {
        let _ = inode.truncate();
    }

    let handle = Arc::new(FileHandle::new(inode, flags));
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();
    let pid = proc.pid;
    match proc.alloc_fd(handle) {
        Ok(fd) => {
            if is_created {
                log_audit_event(pid, 0, AUDIT_TYPE_FILE_CREATE, 0, path, "File created via open(O_CREAT)");
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
    let path = unsafe {
        let mut len = 0;
        while *path_ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(path_ptr, len))
    };

    let inode = match resolve_path(path) {
        Ok(i) => i,
        Err(err) => return -(err as isize),
    };

    match inode.stat() {
        Ok(st) => {
            unsafe { *statbuf = st; }
            0
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
            unsafe { *statbuf = st; }
            0
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
    if pipefd_ptr.is_null() {
        return -(EFAULT as isize);
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

    unsafe {
        (*pipefd_ptr)[0] = fd0;
        (*pipefd_ptr)[1] = fd1;
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
    if newfd < 0 || newfd >= 256 {
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

pub fn sys_mkdir(path_ptr: *const u8, _mode: u32) -> isize {
    let pid = match get_current_process() {
        Some(p) => p.lock().pid,
        None => 0,
    };
    let path = unsafe {
        let mut len = 0;
        while *path_ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(path_ptr, len))
    };

    let (parent, basename) = match resolve_parent_and_basename(path) {
        Ok(res) => res,
        Err(err) => {
            log_audit_event(pid, 0, AUDIT_TYPE_DIR_CREATE, -(err as i32), path, "Directory create failed (parent unresolved)");
            return -(err as isize);
        }
    };

    match parent.create_dir(&basename) {
        Ok(_) => {
            log_audit_event(pid, 0, AUDIT_TYPE_DIR_CREATE, 0, path, "Directory created");
            0
        }
        Err(err) => {
            log_audit_event(pid, 0, AUDIT_TYPE_DIR_CREATE, -(err as i32), path, "Directory create failed");
            -(err as isize)
        }
    }
}

pub fn sys_unlink(path_ptr: *const u8) -> isize {
    let pid = match get_current_process() {
        Some(p) => p.lock().pid,
        None => 0,
    };
    let path = unsafe {
        let mut len = 0;
        while *path_ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(path_ptr, len))
    };

    let (parent, basename) = match resolve_parent_and_basename(path) {
        Ok(res) => res,
        Err(err) => {
            log_audit_event(pid, 0, AUDIT_TYPE_FILE_UNLINK, -(err as i32), path, "Unlink failed (parent unresolved)");
            return -(err as isize);
        }
    };

    match parent.unlink(&basename) {
        Ok(()) => {
            log_audit_event(pid, 0, AUDIT_TYPE_FILE_UNLINK, 0, path, "File or directory unlinked");
            0
        }
        Err(err) => {
            log_audit_event(pid, 0, AUDIT_TYPE_FILE_UNLINK, -(err as i32), path, "Unlink failed");
            -(err as isize)
        }
    }
}

pub fn sys_rename(oldpath_ptr: *const u8, newpath_ptr: *const u8) -> isize {
    let pid = match get_current_process() {
        Some(p) => p.lock().pid,
        None => 0,
    };
    if oldpath_ptr.is_null() || newpath_ptr.is_null() {
        return -(EFAULT as isize);
    }
    let oldpath = unsafe {
        let mut len = 0;
        while *oldpath_ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(oldpath_ptr, len))
    };
    let newpath = unsafe {
        let mut len = 0;
        while *newpath_ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(newpath_ptr, len))
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
        if source_inode.file_type() == FileType::Directory && target_inode.file_type() != FileType::Directory {
            return -(ENOTDIR as isize);
        }
        if source_inode.file_type() != FileType::Directory && target_inode.file_type() == FileType::Directory {
            return -(EISDIR as isize);
        }
    }

    if let Err(err) = new_parent.link_entry(&new_basename, source_inode) {
        return -(err as isize);
    }
    if let Err(err) = old_parent.unlink(&old_basename) {
        return -(err as isize);
    }

    log_audit_event(pid, 0, AUDIT_TYPE_FILE_MODIFY, 0, newpath, "File or directory renamed/moved");
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

    for i in current_idx..entries.len() {
        if written + dirent_size > count {
            break;
        }
        unsafe {
            let target = (dirp as usize + written) as *mut Dirent64;
            *target = entries[i];
        }
        written += dirent_size;
        entries_written += 1;
    }

    *offset_guard += entries_written;
    written as isize
}
