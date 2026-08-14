//! POSIX System Information and Working Directory System Calls.

use posix_abi::*;
use crate::services::process::get_current_process;
use crate::services::vfs::*;
use crate::services::audit::log_audit_event;

pub fn sys_uname(buf: *mut Utsname) -> isize {
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

pub fn sys_sysinfo(info: *mut Sysinfo) -> isize {
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

pub fn sys_getcwd(buf: *mut u8, size: usize) -> isize {
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

pub fn sys_chdir(path_ptr: *const u8) -> isize {
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
