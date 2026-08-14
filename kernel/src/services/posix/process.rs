//! POSIX Process Lifecycle & Signal System Calls.

use posix_abi::*;
use crate::services::process::*;
use crate::services::ipc::SIGNALS;
use crate::ostd::mm::{UserPtr, USER_STR_MAX};
use super::{copy_user_path, map_user_error};

/// POSIX fork system call.
///
/// Returns -ENOSYS until true copy-on-write / deep address-space cloning is
/// implemented in OSTD (ADR-0001 / AGENTS.md), avoiding silent memory faults.
pub fn sys_fork() -> isize {
    -(ENOSYS as isize)
}

pub fn sys_execve(
    path_ptr: *const u8,
    argv_ptr: *const *const u8,
    envp_ptr: *const *const u8,
) -> isize {
    let mut kpath = [0u8; USER_STR_MAX];
    let path = match copy_user_path(path_ptr, &mut kpath) {
        Ok(p) => p,
        Err(e) => return -(e as isize),
    };

    let argv_vec = match super::copy_user_str_array(argv_ptr, 256) {
        Ok(v) => v,
        Err(e) => return -(e as isize),
    };
    let envp_vec = match super::copy_user_str_array(envp_ptr, 256) {
        Ok(v) => v,
        Err(e) => return -(e as isize),
    };

    let argv_refs: alloc::vec::Vec<&str> = argv_vec.iter().map(|s| s.as_str()).collect();
    let envp_refs: alloc::vec::Vec<&str> = envp_vec.iter().map(|s| s.as_str()).collect();

    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();

    match proc.exec(path, &argv_refs, &envp_refs) {
        Ok(()) => 0,
        Err(err) => -(err as isize),
    }
}

pub fn sys_wait4(pid: i32, status_ptr: *mut i32, _options: i32) -> isize {
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
                let out = match UserPtr::<i32>::from_raw(status_ptr as usize) {
                    Ok(p) => p,
                    Err(e) => return -(map_user_error(e) as isize),
                };
                if let Err(e) = out.write((exit_code & 0xff) << 8) {
                    return -(map_user_error(e) as isize);
                }
            }
            return target_pid as isize;
        }
    }

    -(ECHILD as isize)
}

pub fn sys_getpid() -> isize {
    if let Some(proc) = get_current_process() {
        proc.lock().pid as isize
    } else {
        1
    }
}

pub fn sys_getppid() -> isize {
    if let Some(proc) = get_current_process() {
        proc.lock().ppid as isize
    } else {
        0
    }
}

pub fn sys_exit(code: i32) -> isize {
    if let Some(proc_lock) = get_current_process() {
        let mut proc = proc_lock.lock();
        proc.state = ProcessState::Zombie;
        proc.exit_code = code;
    }
    0
}

pub fn sys_kill(pid: i32, sig: i32) -> isize {
    if sig < 1 || sig > 31 {
        return -(EINVAL as isize);
    }
    match SIGNALS.send_signal(pid, sig) {
        Ok(()) => 0,
        Err(err) => -(err as isize),
    }
}
