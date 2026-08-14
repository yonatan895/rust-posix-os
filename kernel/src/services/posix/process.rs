//! POSIX Process Lifecycle & Signal System Calls.

use super::{copy_user_path, map_user_error};
use crate::ostd::mm::{USER_STR_MAX, UserPtr};
use crate::services::ipc::SIGNALS;
use crate::services::process::*;
use posix_abi::*;

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

pub fn sys_wait4(pid: i32, status_ptr: *mut i32, options: i32) -> isize {
    let calling_pid = match get_current_process() {
        Some(proc) => proc.lock().pid,
        None => return -(ESRCH as isize),
    };

    loop {
        let mut has_children = false;
        let mut reaped_pid = None;
        let mut exit_code = 0;
        let mut should_switch = false;

        {
            let mut table = PROCESS_TABLE.lock();

            if pid == -1 || pid == 0 || pid < -1 {
                // Wait for any child where ppid == calling_pid
                for (&p, proc_arc) in table.iter() {
                    let proc = proc_arc.lock();
                    if proc.ppid == calling_pid {
                        has_children = true;
                        if proc.state == ProcessState::Zombie {
                            reaped_pid = Some(p);
                            exit_code = proc.exit_code;
                            break;
                        }
                    }
                }
            } else {
                // Wait for specific child pid
                if let Some(proc_arc) = table.get(&pid) {
                    let proc = proc_arc.lock();
                    if proc.ppid == calling_pid {
                        has_children = true;
                        if proc.state == ProcessState::Zombie {
                            reaped_pid = Some(pid);
                            exit_code = proc.exit_code;
                        }
                    } else {
                        // Target is not a child of the calling process -> -ECHILD
                        return -(ECHILD as isize);
                    }
                } else {
                    // Target PID does not exist in table -> -ECHILD
                    return -(ECHILD as isize);
                }
            }

            if let Some(target) = reaped_pid {
                table.remove(&target);
            } else if has_children {
                if options & WNOHANG != 0 {
                    return 0;
                }
                // Mark current process as Blocked under table lock
                crate::services::scheduler::mark_current_blocked();

                // Re-check to close lost-wakeup race with sys_exit
                let mut zombie_found = false;
                for (&p, proc_arc) in table.iter() {
                    let proc = proc_arc.lock();
                    if proc.ppid == calling_pid && proc.state == ProcessState::Zombie {
                        zombie_found = true;
                        reaped_pid = Some(p);
                        exit_code = proc.exit_code;
                        break;
                    }
                }

                if zombie_found {
                    crate::services::scheduler::mark_current_running();
                    if let Some(target) = reaped_pid {
                        table.remove(&target);
                    }
                } else {
                    should_switch = true;
                }
            } else {
                return -(ECHILD as isize);
            }
        } // PROCESS_TABLE lock dropped before writing to user memory or switching

        if let Some(target) = reaped_pid {
            if !status_ptr.is_null() {
                let out = match UserPtr::<i32>::from_raw(status_ptr as usize) {
                    Ok(p) => p,
                    Err(e) => return -(map_user_error(e) as isize),
                };
                if let Err(e) = out.write((exit_code & 0xff) << 8) {
                    return -(map_user_error(e) as isize);
                }
            }
            return target as isize;
        }

        if should_switch {
            crate::services::scheduler::switch_out_current();
        }
    }
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
    let ppid = if let Some(proc_lock) = get_current_process() {
        let mut proc = proc_lock.lock();
        proc.state = ProcessState::Zombie;
        proc.exit_code = code;
        proc.ppid
    } else {
        0
    };

    if ppid > 0 {
        crate::services::scheduler::wake_tasks(&[ppid]);
    }

    crate::services::scheduler::switch_out_current();

    // Exited process never returns to userland
    loop {
        crate::services::scheduler::schedule_yield();
    }
}

pub fn sys_kill(pid: i32, sig: i32) -> isize {
    if !(1..=31).contains(&sig) {
        return -(EINVAL as isize);
    }
    match SIGNALS.send_signal(pid, sig) {
        Ok(()) => 0,
        Err(err) => -(err as isize),
    }
}
