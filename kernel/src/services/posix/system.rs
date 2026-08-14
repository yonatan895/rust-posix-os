//! POSIX System Information and Working Directory System Calls.
//!
//! ADR-0001 R2: no raw user-pointer derefs here. Raw pointers arrive from
//! the dispatcher and are converted via `super::{copy_user_path, map_user_error}`
//! and `ostd::mm::{UserPtr, UserSlice}`.

use posix_abi::*;
use crate::services::process::get_current_process;
use crate::services::vfs::*;
use crate::services::audit::log_audit_event;
use crate::ostd::mm::{UserPtr, UserSlice, USER_STR_MAX};
use super::{copy_user_path, map_user_error};

pub fn sys_uname(buf: *mut Utsname) -> isize {
    let out = match UserPtr::<Utsname>::from_raw(buf as usize) {
        Ok(p) => p,
        Err(e) => return -(map_user_error(e) as isize),
    };
    let mut uts = Utsname::default();
    let sysname = b"RustPOSIX\0";
    let release = b"1.0.0-framekernel\0";
    let version = b"POSIX.1-2024\0";
    let machine = b"x86_64\0";

    uts.sysname[..sysname.len()].copy_from_slice(sysname);
    uts.release[..release.len()].copy_from_slice(release);
    uts.version[..version.len()].copy_from_slice(version);
    uts.machine[..machine.len()].copy_from_slice(machine);

    match out.write(uts) {
        Ok(()) => 0,
        Err(e) => -(map_user_error(e) as isize),
    }
}

pub fn sys_sysinfo(info: *mut Sysinfo) -> isize {
    let out = match UserPtr::<Sysinfo>::from_raw(info as usize) {
        Ok(p) => p,
        Err(e) => return -(map_user_error(e) as isize),
    };
    crate::services::monitor::update_system_metrics();
    let mon = crate::services::monitor::SYSTEM_MONITOR.lock();

    let si = Sysinfo {
        uptime: mon.sample_tick as i64,
        totalram: mon.total_memory_bytes as u64,
        freeram: mon.free_memory_bytes as u64,
        bufferram: mon.total_heap_bytes as u64,
        sharedram: mon.used_heap_bytes as u64,
        procs: mon.total_processes as u16,
        mem_unit: 1,
        ..Default::default()
    };

    match out.write(si) {
        Ok(()) => 0,
        Err(e) => -(map_user_error(e) as isize),
    }
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
    let mut kbuf = [0u8; USER_STR_MAX];
    if cwd_bytes.len() + 1 > kbuf.len() {
        return -(ERANGE as isize);
    }
    kbuf[..cwd_bytes.len()].copy_from_slice(cwd_bytes);
    kbuf[cwd_bytes.len()] = 0;

    let out = match UserSlice::from_raw(buf as usize, cwd_bytes.len() + 1) {
        Ok(s) => s,
        Err(e) => return -(map_user_error(e) as isize),
    };
    match out.copy_to_user(&kbuf[..cwd_bytes.len() + 1]) {
        Ok(_) => buf as isize,
        Err(e) => -(map_user_error(e) as isize),
    }
}

pub fn sys_chdir(path_ptr: *const u8) -> isize {
    let mut kpath = [0u8; USER_STR_MAX];
    let path = match copy_user_path(path_ptr, &mut kpath) {
        Ok(p) => p,
        Err(e) => return -(e as isize),
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
