//! POSIX.1-2024 System Call Dispatcher - De-privileged Safe Service.

use alloc::sync::Arc;
use posix_abi::*;
use crate::ostd::arch::syscall::SyscallRegisters;
use crate::ostd::mm::PAGE_SIZE;
use crate::services::process::*;
use crate::services::vfs::*;
use crate::services::vfs::pipe::PipeBuffer;
use crate::services::ipc::SIGNALS;
use crate::services::audit::{log_audit_event, create_audit_snapshot};

#[no_mangle]
pub extern "C" fn rust_syscall_dispatcher(regs: *mut SyscallRegisters) -> usize {
    let r = unsafe { &mut *regs };
    let syscall_nr = r.rax;
    let a1 = r.rdi;
    let a2 = r.rsi;
    let a3 = r.rdx;
    let a4 = r.r10;

    let ret: isize = match syscall_nr {
        SYS_READ => sys_read(a1 as i32, a2 as *mut u8, a3),
        SYS_WRITE => sys_write(a1 as i32, a2 as *const u8, a3),
        SYS_OPEN => sys_open(a1 as *const u8, a2 as i32, a3 as u32),
        SYS_CLOSE => sys_close(a1 as i32),
        SYS_STAT => sys_stat(a1 as *const u8, a2 as *mut Stat),
        SYS_FSTAT => sys_fstat(a1 as i32, a2 as *mut Stat),
        SYS_LSEEK => sys_lseek(a1 as i32, a2 as i64, a3 as i32),
        SYS_MMAP => sys_mmap(a1, a2, a3 as i32, a4 as i32),
        SYS_MUNMAP => sys_munmap(a1, a2),
        SYS_PIPE => sys_pipe(a1 as *mut [i32; 2]),
        SYS_DUP => sys_dup(a1 as i32),
        SYS_DUP2 => sys_dup2(a1 as i32, a2 as i32),
        SYS_FORK => sys_fork(),
        SYS_EXECVE => {
            let res = sys_execve(a1 as *const u8);
            if res == 0 {
                if let Some(p) = get_current_process() {
                    let proc = p.lock();
                    r.rcx = proc.entry_point;
                    r.rsp = proc.user_stack_top;
                    if let Some(ref vm) = proc.vm_space {
                        unsafe { core::arch::asm!("mov cr3, {}", in(reg) vm.pml4_phys) };
                    }
                }
            }
            res
        }
        SYS_WAIT4 => sys_wait4(a1 as i32, a2 as *mut i32, a3 as i32),
        SYS_GETPID => sys_getpid(),
        SYS_GETPPID => sys_getppid(),
        SYS_EXIT => sys_exit(a1 as i32),
        SYS_KILL => sys_kill(a1 as i32, a2 as i32),
        SYS_UNAME => sys_uname(a1 as *mut Utsname),
        SYS_SYSINFO => sys_sysinfo(a1 as *mut Sysinfo),
        SYS_GETCWD => sys_getcwd(a1 as *mut u8, a2),
        SYS_CHDIR => sys_chdir(a1 as *const u8),
        SYS_RENAME => sys_rename(a1 as *const u8, a2 as *const u8),
        SYS_MKDIR => sys_mkdir(a1 as *const u8, a2 as u32),
        SYS_RMDIR => sys_unlink(a1 as *const u8),
        SYS_UNLINK => sys_unlink(a1 as *const u8),
        SYS_GETDENTS64 => sys_getdents64(a1 as i32, a2 as *mut u8, a3),
        SYS_EPOLL_CREATE | SYS_EPOLL_CREATE1 => sys_epoll_create1(a1 as i32),
        SYS_EPOLL_CTL => sys_epoll_ctl(a1 as i32, a2 as i32, a3 as i32, a4 as *const EpollEvent),
        SYS_EPOLL_WAIT => sys_epoll_wait(a1 as i32, a2 as *mut EpollEvent, a3 as i32, a4 as i32),
        SYS_AUDIT_LOG => sys_audit_log(a1 as u32, a2 as *const u8, a3 as *const u8),
        SYS_AUDIT_SNAPSHOT => sys_audit_snapshot(a1 as *const u8, a2 as u32),
        _ => -(ENOSYS as isize),
    };

    r.rax = ret as usize;
    ret as usize
}

fn sys_read(fd: i32, buf: *mut u8, count: usize) -> isize {
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

fn sys_write(fd: i32, buf: *const u8, count: usize) -> isize {
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

fn sys_open(path_ptr: *const u8, flags: i32, _mode: u32) -> isize {
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

fn sys_close(fd: i32) -> isize {
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

fn sys_stat(path_ptr: *const u8, statbuf: *mut Stat) -> isize {
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

fn sys_fstat(fd: i32, statbuf: *mut Stat) -> isize {
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

fn sys_lseek(fd: i32, offset: i64, whence: i32) -> isize {
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

fn sys_mmap(addr: usize, length: usize, prot: i32, _flags: i32) -> isize {
    if length == 0 {
        return -(EINVAL as isize);
    }
    let pages = (length + PAGE_SIZE - 1) / PAGE_SIZE;

    static MMAP_ALLOC_BASE: crate::ostd::sync::SpinLock<usize> = crate::ostd::sync::SpinLock::new(0x60000000);

    let vaddr = if addr == 0 {
        let mut base_guard = MMAP_ALLOC_BASE.lock();
        let base = *base_guard;
        *base_guard += pages * PAGE_SIZE;
        base
    } else {
        addr
    };

    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();

    if let Some(ref mut vm) = proc.vm_space {
        for i in 0..pages {
            let page_vaddr = vaddr + i * PAGE_SIZE;
            if let Some(frame) = crate::ostd::mm::alloc_frame() {
                let mut pte_flags = crate::ostd::mm::PAGE_PRESENT | crate::ostd::mm::PAGE_USER;
                if prot & PROT_WRITE != 0 {
                    pte_flags |= crate::ostd::mm::PAGE_WRITABLE;
                }
                unsafe { let _ = vm.map_page(page_vaddr, frame, pte_flags); };

                let virt = crate::ostd::mm::phys_to_virt(frame);
                unsafe { core::ptr::write_bytes(virt as *mut u8, 0, PAGE_SIZE) };
            } else {
                return -(ENOMEM as isize);
            }
        }
    }

    vaddr as isize
}

fn sys_munmap(addr: usize, length: usize) -> isize {
    if addr % PAGE_SIZE != 0 || length == 0 {
        return -(EINVAL as isize);
    }
    let pages = (length + PAGE_SIZE - 1) / PAGE_SIZE;

    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();

    if let Some(ref mut vm) = proc.vm_space {
        for i in 0..pages {
            let page_vaddr = addr + i * PAGE_SIZE;
            unsafe { vm.unmap_page(page_vaddr) };
        }
    }

    0
}

fn sys_pipe(pipefd_ptr: *mut [i32; 2]) -> isize {
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

fn sys_dup(oldfd: i32) -> isize {
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

fn sys_dup2(oldfd: i32, newfd: i32) -> isize {
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

fn sys_fork() -> isize {
    let parent_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let parent = parent_lock.lock();
    let new_pid = crate::services::process::alloc_pid();
    let mut child = Process::new(new_pid, parent.pid, parent.cwd.clone());

    for handle in parent.fds.iter() {
        child.fds.push(handle.clone());
    }

    child.vm_space = crate::ostd::mm::VmSpace::new();
    child.entry_point = parent.entry_point;
    child.user_stack_top = parent.user_stack_top;

    let child_lock = Arc::new(crate::ostd::sync::SpinLock::new(child));
    PROCESS_TABLE.lock().insert(new_pid, child_lock);
    crate::services::scheduler::SCHEDULER.lock().add_task(new_pid);

    new_pid as isize
}

fn sys_execve(path_ptr: *const u8) -> isize {
    let path = unsafe {
        let mut len = 0;
        while *path_ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(path_ptr, len))
    };

    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();

    match proc.exec(path) {
        Ok(()) => 0,
        Err(err) => -(err as isize),
    }
}

fn sys_wait4(pid: i32, status_ptr: *mut i32, _options: i32) -> isize {
    let mut table = PROCESS_TABLE.lock();
    let target_pid = if pid == -1 {
        let mut found = None;
        for (&p, proc) in table.iter() {
            if proc.lock().state == ProcessState::Zombie {
                found = Some(p);
                break;
            }
        }
        match found {
            Some(p) => p,
            None => return -(ECHILD as isize),
        }
    } else {
        pid
    };

    if let Some(proc_lock) = table.get(&target_pid) {
        let proc = proc_lock.lock();
        if proc.state == ProcessState::Zombie {
            let exit_code = proc.exit_code;
            drop(proc);
            table.remove(&target_pid);
            if !status_ptr.is_null() {
                unsafe { *status_ptr = (exit_code & 0xff) << 8; }
            }
            return target_pid as isize;
        }
    }

    -(ECHILD as isize)
}

fn sys_getpid() -> isize {
    if let Some(proc) = get_current_process() {
        proc.lock().pid as isize
    } else {
        1
    }
}

fn sys_getppid() -> isize {
    if let Some(proc) = get_current_process() {
        proc.lock().ppid as isize
    } else {
        0
    }
}

fn sys_exit(code: i32) -> isize {
    if let Some(proc_lock) = get_current_process() {
        let mut proc = proc_lock.lock();
        proc.state = ProcessState::Zombie;
        proc.exit_code = code;
    }
    0
}

fn sys_kill(pid: i32, sig: i32) -> isize {
    if sig < 1 || sig > 31 {
        return -(EINVAL as isize);
    }
    match SIGNALS.send_signal(pid, sig) {
        Ok(()) => 0,
        Err(err) => -(err as isize),
    }
}

fn sys_uname(buf: *mut Utsname) -> isize {
    if buf.is_null() {
        return -(EFAULT as isize);
    }
    let mut uts = Utsname::default();
    let sysname = b"RustPOSIX\0";
    let release = b"1.0.0-framekernel\0";
    let version = b"POSIX.1-2024\0";
    let machine = b"x86_64\0";

    uts.sysname[..sysname.len()].copy_from_slice(sysname);
    uts.release[..release.len()].copy_from_slice(release);
    uts.version[..version.len()].copy_from_slice(version);
    uts.machine[..machine.len()].copy_from_slice(machine);

    unsafe { *buf = uts; }
    0
}

fn sys_sysinfo(info: *mut Sysinfo) -> isize {
    if info.is_null() {
        return -(EFAULT as isize);
    }
    crate::services::monitor::update_system_metrics();
    let mon = crate::services::monitor::SYSTEM_MONITOR.lock();

    let mut si = Sysinfo::default();
    si.uptime = mon.sample_tick as i64;
    si.totalram = mon.total_memory_bytes as u64;
    si.freeram = mon.free_memory_bytes as u64;
    si.bufferram = mon.total_heap_bytes as u64;
    si.sharedram = mon.used_heap_bytes as u64;
    si.procs = mon.total_processes as u16;
    si.mem_unit = 1;

    unsafe { *info = si; }
    0
}

fn sys_getcwd(buf: *mut u8, size: usize) -> isize {
    if buf.is_null() || size == 0 {
        return -(EINVAL as isize);
    }
    let cwd = get_current_process_cwd();
    let cwd_bytes = cwd.as_bytes();
    if cwd_bytes.len() + 1 > size {
        return -(ERANGE as isize);
    }
    unsafe {
        core::ptr::copy_nonoverlapping(cwd_bytes.as_ptr(), buf, cwd_bytes.len());
        *buf.add(cwd_bytes.len()) = 0;
    }
    buf as isize
}

fn sys_chdir(path_ptr: *const u8) -> isize {
    let path = unsafe {
        let mut len = 0;
        while *path_ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(path_ptr, len))
    };

    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };

    let target_norm = {
        let proc = proc_lock.lock();
        normalize_path(&proc.cwd, path)
    };

    let inode = match resolve_path(&target_norm) {
        Ok(i) => i,
        Err(err) => return -(err as isize),
    };

    if inode.file_type() != FileType::Directory {
        return -(ENOTDIR as isize);
    }

    let mut proc = proc_lock.lock();
    proc.cwd = target_norm.clone();
    let pid = proc.pid;
    drop(proc);

    log_audit_event(pid, 0, AUDIT_TYPE_DIR_CHANGE, 0, &target_norm, "Working directory changed");
    0
}

fn sys_mkdir(path_ptr: *const u8, _mode: u32) -> isize {
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

fn sys_unlink(path_ptr: *const u8) -> isize {
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

fn sys_rename(oldpath_ptr: *const u8, newpath_ptr: *const u8) -> isize {
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

fn sys_getdents64(fd: i32, dirp: *mut u8, count: usize) -> isize {
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

fn sys_epoll_create1(_flags: i32) -> isize {
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

fn sys_epoll_ctl(epfd: i32, op: i32, fd: i32, event_ptr: *const EpollEvent) -> isize {
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
        unsafe { *event_ptr }
    } else {
        EpollEvent::default()
    };

    match epoll.ctl(op, fd, event) {
        Ok(()) => 0,
        Err(err) => -(err as isize),
    }
}

fn sys_epoll_wait(epfd: i32, events_ptr: *mut EpollEvent, maxevents: i32, _timeout: i32) -> isize {
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

    let slice = unsafe { core::slice::from_raw_parts_mut(events_ptr, maxevents as usize) };
    drop(proc);

    match epoll.wait(slice, maxevents as usize) {
        Ok(count) => count as isize,
        Err(err) => -(err as isize),
    }
}

fn sys_audit_log(event_type: u32, target_ptr: *const u8, details_ptr: *const u8) -> isize {
    let pid = match get_current_process() {
        Some(p) => p.lock().pid,
        None => 0,
    };
    let target = unsafe {
        if target_ptr.is_null() {
            ""
        } else {
            let mut len = 0;
            while *target_ptr.add(len) != 0 {
                len += 1;
            }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(target_ptr, len))
        }
    };
    let details = unsafe {
        if details_ptr.is_null() {
            ""
        } else {
            let mut len = 0;
            while *details_ptr.add(len) != 0 {
                len += 1;
            }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(details_ptr, len))
        }
    };
    let seq = log_audit_event(pid, 0, event_type, 0, target, details);
    seq as isize
}

fn sys_audit_snapshot(label_ptr: *const u8, _flags: u32) -> isize {
    let label = unsafe {
        if label_ptr.is_null() {
            "snapshot"
        } else {
            let mut len = 0;
            while *label_ptr.add(len) != 0 {
                len += 1;
            }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(label_ptr, len))
        }
    };
    let snap_id = create_audit_snapshot(label);
    snap_id as isize
}
