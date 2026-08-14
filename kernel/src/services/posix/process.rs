//! POSIX Process Lifecycle & Signal System Calls.

use super::{copy_user_path, map_user_error};
use crate::ostd::arch::syscall::SyscallRegisters;
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
        let mut exit_status = 0;
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
                            exit_status = if let Some(sig) = proc.killed_by_sig {
                                sig & 0x7f
                            } else {
                                (proc.exit_code & 0xff) << 8
                            };
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
                            exit_status = if let Some(sig) = proc.killed_by_sig {
                                sig & 0x7f
                            } else {
                                (proc.exit_code & 0xff) << 8
                            };
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
                        exit_status = if let Some(sig) = proc.killed_by_sig {
                            sig & 0x7f
                        } else {
                            (proc.exit_code & 0xff) << 8
                        };
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
            SIGNALS.cleanup_process(target);
            if !status_ptr.is_null() {
                let out = match UserPtr::<i32>::from_raw(status_ptr as usize) {
                    Ok(p) => p,
                    Err(e) => return -(map_user_error(e) as isize),
                };
                if let Err(e) = out.write(exit_status) {
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
        proc.killed_by_sig = None;
        proc.ppid
    } else {
        0
    };

    if ppid > 0 {
        crate::services::scheduler::wake_tasks(&[ppid]);
    }

    // Exited process never returns to userland and never re-enters the ready queue
    loop {
        crate::services::scheduler::switch_out_current();
    }
}

pub fn sys_exit_signal(sig: i32) -> ! {
    let ppid = if let Some(proc_lock) = get_current_process() {
        let mut proc = proc_lock.lock();
        proc.state = ProcessState::Zombie;
        proc.exit_code = sig & 0x7f;
        proc.killed_by_sig = Some(sig);
        proc.ppid
    } else {
        0
    };

    if ppid > 0 {
        crate::services::scheduler::wake_tasks(&[ppid]);
    }

    loop {
        crate::services::scheduler::switch_out_current();
    }
}

pub fn sys_kill(pid: i32, sig: i32) -> isize {
    if !(SIG_MIN..=SIG_MAX).contains(&sig) {
        return -(EINVAL as isize);
    }
    match SIGNALS.send_signal(pid, sig) {
        Ok(()) => 0,
        Err(err) => -(err as isize),
    }
}

pub fn sys_rt_sigaction(
    sig: i32,
    act_ptr: *const SigAction,
    oldact_ptr: *mut SigAction,
    sigsetsize: usize,
) -> isize {
    if sigsetsize != core::mem::size_of::<SigSet>() {
        return -(EINVAL as isize);
    }
    if !(SIG_MIN..=SIG_MAX).contains(&sig) {
        return -(EINVAL as isize);
    }
    if sig == SIGKILL || sig == SIGSTOP {
        return -(EINVAL as isize);
    }

    let pid = match get_current_process() {
        Some(proc) => proc.lock().pid,
        None => return -(ESRCH as isize),
    };

    if !oldact_ptr.is_null() {
        let old_act = SIGNALS.get_action(pid, sig);
        let out = match UserPtr::<SigAction>::from_raw(oldact_ptr as usize) {
            Ok(p) => p,
            Err(e) => return -(map_user_error(e) as isize),
        };
        if let Err(e) = out.write(old_act) {
            return -(map_user_error(e) as isize);
        }
    }

    if !act_ptr.is_null() {
        let in_ptr = match UserPtr::<SigAction>::from_raw(act_ptr as usize) {
            Ok(p) => p,
            Err(e) => return -(map_user_error(e) as isize),
        };
        let new_act = match in_ptr.read() {
            Ok(a) => a,
            Err(e) => return -(map_user_error(e) as isize),
        };
        if let Err(e) = SIGNALS.set_action(pid, sig, new_act) {
            return -(e as isize);
        }
    }

    0
}

pub fn sys_rt_sigprocmask(
    how: i32,
    set_ptr: *const SigSet,
    oldset_ptr: *mut SigSet,
    sigsetsize: usize,
) -> isize {
    if sigsetsize != core::mem::size_of::<SigSet>() {
        return -(EINVAL as isize);
    }

    let pid = match get_current_process() {
        Some(proc) => proc.lock().pid,
        None => return -(ESRCH as isize),
    };

    if !oldset_ptr.is_null() {
        let old_mask = SIGNALS.get_procmask(pid);
        let out = match UserPtr::<SigSet>::from_raw(oldset_ptr as usize) {
            Ok(p) => p,
            Err(e) => return -(map_user_error(e) as isize),
        };
        if let Err(e) = out.write(old_mask) {
            return -(map_user_error(e) as isize);
        }
    }

    if !set_ptr.is_null() {
        let in_ptr = match UserPtr::<SigSet>::from_raw(set_ptr as usize) {
            Ok(p) => p,
            Err(e) => return -(map_user_error(e) as isize),
        };
        let new_set = match in_ptr.read() {
            Ok(s) => s,
            Err(e) => return -(map_user_error(e) as isize),
        };
        if let Err(e) = SIGNALS.set_procmask(pid, how, new_set) {
            return -(e as isize);
        }
    }

    0
}

pub fn sys_rt_sigreturn(r: &mut SyscallRegisters) -> isize {
    let frame_ptr = match UserPtr::<SignalFrame>::from_raw(r.rsp) {
        Ok(p) => p,
        Err(e) => return -(map_user_error(e) as isize),
    };
    let frame = match frame_ptr.read() {
        Ok(f) => f,
        Err(e) => return -(map_user_error(e) as isize),
    };

    let pid = match get_current_process() {
        Some(proc) => proc.lock().pid,
        None => return -(ESRCH as isize),
    };

    // Restore previous blocked signals mask
    let _ = SIGNALS.set_procmask(pid, SIG_SETMASK, frame.old_mask);

    // Restore user CPU register state
    r.r15 = frame.r15 as usize;
    r.r14 = frame.r14 as usize;
    r.r13 = frame.r13 as usize;
    r.r12 = frame.r12 as usize;
    r.rbp = frame.rbp as usize;
    r.rbx = frame.rbx as usize;
    r.r9 = frame.r9 as usize;
    r.r8 = frame.r8 as usize;
    r.r10 = frame.r10 as usize;
    r.rdx = frame.rdx as usize;
    r.rsi = frame.rsi as usize;
    r.rdi = frame.rdi as usize;
    r.rax = frame.rax as usize;
    r.rcx = frame.rcx as usize; // Saved user RIP
    r.r11 = frame.r11 as usize; // Saved user RFLAGS
    r.rsp = frame.rsp as usize; // Saved user RSP

    r.rax as isize
}

/// Checks pending unblocked signals on the return-to-userland path and delivers them.
pub fn check_and_deliver_signals(r: &mut SyscallRegisters) {
    let pid = match get_current_process() {
        Some(proc) => proc.lock().pid,
        None => return,
    };

    let pending = SIGNALS.get_pending(pid);
    let blocked = SIGNALS.get_procmask(pid);
    let unblocked_pending = pending & !blocked;
    if unblocked_pending == 0 {
        return;
    }

    for sig in SIG_MIN..=SIG_MAX {
        if (unblocked_pending & (1 << (sig - 1))) != 0 {
            SIGNALS.clear_pending(pid, sig);
            let action = SIGNALS.get_action(pid, sig);

            if action.sa_handler == SIG_IGN
                || (action.sa_handler == SIG_DFL && is_default_ignore(sig))
            {
                continue;
            }

            if action.sa_handler == SIG_DFL {
                // Default action: Terminate
                sys_exit_signal(sig);
            }

            // Custom user handler
            deliver_signal_to_user(pid, sig, action, blocked, r);
            break;
        }
    }
}

fn is_default_ignore(sig: i32) -> bool {
    sig == SIGCHLD || sig == SIGURG || sig == SIGWINCH
}

fn deliver_signal_to_user(
    pid: i32,
    sig: i32,
    action: SigAction,
    blocked: SigSet,
    r: &mut SyscallRegisters,
) {
    let frame_size = core::mem::size_of::<SignalFrame>();
    // Align user RSP down by 16 bytes for System V AMD64 ABI stack discipline
    let new_rsp = (r.rsp.saturating_sub(frame_size)) & !0xF;

    let frame = SignalFrame {
        restorer: action.sa_restorer as u64,
        signum: sig as u64,
        old_mask: blocked,
        r15: r.r15 as u64,
        r14: r.r14 as u64,
        r13: r.r13 as u64,
        r12: r.r12 as u64,
        rbp: r.rbp as u64,
        rbx: r.rbx as u64,
        r9: r.r9 as u64,
        r8: r.r8 as u64,
        r10: r.r10 as u64,
        rdx: r.rdx as u64,
        rsi: r.rsi as u64,
        rdi: r.rdi as u64,
        rax: r.rax as u64,
        rcx: r.rcx as u64, // Saved user RIP
        r11: r.r11 as u64, // Saved user RFLAGS
        rsp: r.rsp as u64, // Saved original user RSP
    };

    let user_ptr = match UserPtr::<SignalFrame>::from_raw(new_rsp) {
        Ok(p) => p,
        Err(_) => sys_exit_signal(SIGSEGV),
    };
    if user_ptr.write(frame).is_err() {
        sys_exit_signal(SIGSEGV);
    }

    // Set up register context for signal handler execution
    r.rsp = new_rsp;
    r.rcx = action.sa_handler; // RIP for sysretq
    r.rdi = sig as usize; // Arg 1: signal number
    r.rsi = 0; // Arg 2: siginfo (null)
    r.rdx = new_rsp; // Arg 3: ucontext (points to SignalFrame)

    // Update process blocked signal mask
    let mut new_mask = blocked | action.sa_mask;
    if (action.sa_flags & SA_NODEFER) == 0 {
        new_mask |= 1 << (sig - 1);
    }
    let _ = SIGNALS.set_procmask(pid, SIG_SETMASK, new_mask);
}
