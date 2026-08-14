//! POSIX Process Lifecycle & Signal System Calls.

use super::{copy_user_path, map_user_error};
use crate::ostd::arch::gdt::USER_CODE_SEL;
use crate::ostd::arch::idt::TrapFrame;
use crate::ostd::arch::syscall::SyscallRegisters;
use crate::ostd::mm::{USER_STR_MAX, UserPtr};
use crate::services::ipc::SIGNALS;
use crate::services::process::*;
use core::sync::atomic::Ordering;
use posix_abi::*;

const RED_ZONE_SIZE: usize = 128;
const USER_RFLAGS_MASK: usize = 0xCD5; // User arithmetic/status flags + direction
const USER_RFLAGS_RESERVED: usize = 0x202; // Bit 1 is fixed 1, Bit 9 is IF (interrupt enable)

/// POSIX fork system call.
///
/// Duplicates the calling process and its virtual address space (eager cloning),
/// returning child PID to the parent and 0 to the child.
pub fn sys_fork(parent_regs: &SyscallRegisters) -> isize {
    let parent_arc = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };

    // Extract all needed parent state and duplicate address space before dropping parent lock.
    // Adheres strictly to ADR-0002 lock ordering: no process lock may be held while acquiring
    // PROCESS_TABLE, SCHEDULER, or IPC locks.
    let (parent_pid, parent_cwd, parent_vm, entry_point, user_stack_top, mmap_next_vaddr, fds) = {
        let parent = parent_arc.lock();
        let vm_clone = if let Some(ref parent_vm) = parent.vm_space {
            match parent_vm.clone_from() {
                Some(vm) => Some(vm),
                None => return -(ENOMEM as isize),
            }
        } else {
            None
        };
        (
            parent.pid,
            parent.cwd.clone(),
            vm_clone,
            parent.entry_point,
            parent.user_stack_top,
            parent.mmap_next_vaddr,
            parent.fds.clone(),
        )
    };

    let child_pid = alloc_pid();
    let mut child = Process::new(child_pid, parent_pid, parent_cwd);
    child.vm_space = parent_vm;
    child.entry_point = entry_point;
    child.user_stack_top = user_stack_top;
    child.mmap_next_vaddr = mmap_next_vaddr;
    child.has_started = true;
    child.fds = fds;

    // Clone signal mask and signal dispositions from parent (IPC tier)
    let parent_mask = SIGNALS.get_procmask(parent_pid);
    let _ = SIGNALS.set_procmask(child_pid, SIG_SETMASK, parent_mask);
    for sig in SIG_MIN..=SIG_MAX {
        let act = SIGNALS.get_action(parent_pid, sig);
        let _ = SIGNALS.set_action(child_pid, sig, act);
    }

    // Initialize child's kernel stack with TrapFrame where rax = 0
    let child_saved_rsp =
        crate::ostd::task::init_fork_child_stack(&mut child.kernel_stack, parent_regs);
    child
        .saved_kernel_rsp
        .store(child_saved_rsp, Ordering::Release);

    let child_arc = alloc::sync::Arc::new(crate::ostd::sync::SpinLock::new(child));

    // Register child in PROCESS_TABLE (PROCESS_TABLE tier)
    {
        let mut table = PROCESS_TABLE.lock();
        table.insert(child_pid, child_arc.clone());
    }

    // Add child to scheduler ready queue (SCHEDULER tier)
    crate::services::scheduler::SCHEDULER
        .lock()
        .add_task(child_arc);

    child_pid as isize
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

#[inline(always)]
fn encode_wait_status(exit_code: i32, killed_by_sig: Option<i32>) -> i32 {
    if let Some(sig) = killed_by_sig {
        sig & 0x7f
    } else {
        (exit_code & 0xff) << 8
    }
}

pub fn sys_wait4(pid: i32, status_ptr: *mut i32, options: i32) -> isize {
    let calling_pid = CURRENT_PID.load(Ordering::SeqCst);

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
                            exit_status = encode_wait_status(proc.exit_code, proc.killed_by_sig);
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
                            exit_status = encode_wait_status(proc.exit_code, proc.killed_by_sig);
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
                        exit_status = encode_wait_status(proc.exit_code, proc.killed_by_sig);
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
            crate::services::scheduler::mark_current_running();
            if SIGNALS.has_unblocked_signals(calling_pid) {
                return -(EINTR as isize);
            }
        }
    }
}

pub fn sys_getpid() -> isize {
    CURRENT_PID.load(Ordering::SeqCst) as isize
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
    if pid <= 0 || !(SIG_MIN..=SIG_MAX).contains(&sig) {
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

    let pid = CURRENT_PID.load(Ordering::SeqCst);

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

    let pid = CURRENT_PID.load(Ordering::SeqCst);

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
    // When the user signal handler finishes with `ret`, it pops the 8-byte restorer address
    // from [new_rsp], jumping into `__restore_rt` with `rsp = new_rsp + 8`.
    // The syscall instruction does not touch `rsp`. Therefore, the SignalFrame base is at `r.rsp - 8`.
    let frame_addr = r.rsp.saturating_sub(core::mem::size_of::<u64>());
    let frame_ptr = match UserPtr::<SignalFrame>::from_raw(frame_addr) {
        Ok(p) => p,
        Err(e) => return -(map_user_error(e) as isize),
    };
    let frame = match frame_ptr.read() {
        Ok(f) => f,
        Err(e) => return -(map_user_error(e) as isize),
    };

    let pid = CURRENT_PID.load(Ordering::SeqCst);

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

    // Mask user-controlled RFLAGS to prevent forged IOPL/NT
    r.r11 = (frame.r11 as usize & USER_RFLAGS_MASK) | USER_RFLAGS_RESERVED;

    r.rsp = frame.rsp as usize; // Saved user RSP

    r.rax as isize
}

/// Checks pending unblocked signals on the return-to-userland path and delivers them.
pub fn check_and_deliver_signals(r: &mut SyscallRegisters) {
    let pid = CURRENT_PID.load(Ordering::SeqCst);

    let pending = SIGNALS.get_pending(pid);
    let blocked = SIGNALS.get_procmask(pid);
    let unblocked_pending = pending & !blocked;
    if unblocked_pending == 0 {
        return;
    }

    for sig in SIG_MIN..=SIG_MAX {
        if (unblocked_pending & (1 << (sig - 1))) != 0 {
            let action = SIGNALS.get_action(pid, sig);

            if action.sa_handler == SIG_IGN
                || (action.sa_handler == SIG_DFL && is_default_ignore(sig))
            {
                SIGNALS.clear_pending(pid, sig);
                continue;
            }

            if action.sa_handler == SIG_DFL && is_default_stop(sig) {
                // POSIX stop-class signals under SIG_DFL pause the process.
                // Leave pending bit set and skip delivery until job control is implemented.
                continue;
            }

            SIGNALS.clear_pending(pid, sig);

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

pub fn is_default_ignore(sig: i32) -> bool {
    sig == SIGCHLD || sig == SIGURG || sig == SIGWINCH
}

pub fn is_default_stop(sig: i32) -> bool {
    sig == SIGSTOP || sig == SIGTSTP || sig == SIGTTIN || sig == SIGTTOU
}

fn deliver_signal_to_user(
    pid: i32,
    sig: i32,
    action: SigAction,
    blocked: SigSet,
    r: &mut SyscallRegisters,
) {
    let frame_size = core::mem::size_of::<SignalFrame>();
    // SysV AMD64 ABI: allocate below the 128-byte red zone, 16-byte aligned
    let new_rsp = (r.rsp.saturating_sub(RED_ZONE_SIZE + frame_size)) & !0xF;

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
    update_signal_mask_and_disposition(pid, sig, &action, blocked);
}

fn update_signal_mask_and_disposition(pid: i32, sig: i32, action: &SigAction, blocked: SigSet) {
    let mut new_mask = blocked | action.sa_mask;
    if (action.sa_flags & SA_NODEFER) == 0 {
        new_mask |= 1 << (sig - 1);
    }
    let _ = SIGNALS.set_procmask(pid, SIG_SETMASK, new_mask);

    if (action.sa_flags & SA_RESETHAND) != 0 {
        let _ = SIGNALS.set_action(pid, sig, SigAction::default());
    }
}

fn terminate_cpu_bound_task(pid: i32, sig: i32) {
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
    SIGNALS.cleanup_process(pid);
}

/// Checks pending signals when returning from an interrupt to ring 3 (user mode).
///
/// Modifies the hardware TrapFrame on the kernel stack so `iretq` lands in the
/// user signal handler, or terminates CPU-bound tasks on `SIGKILL`/`SIGTERM`.
pub fn check_and_deliver_signals_irq(frame: &mut TrapFrame, pid: i32) -> bool {
    // Only deliver signals when returning to ring 3 (user mode)
    if (frame.cs as u16) != USER_CODE_SEL {
        return false;
    }

    let pending = SIGNALS.get_pending(pid);
    let blocked = SIGNALS.get_procmask(pid);
    let unblocked_pending = pending & !blocked;
    if unblocked_pending == 0 {
        return false;
    }

    for sig in SIG_MIN..=SIG_MAX {
        if (unblocked_pending & (1 << (sig - 1))) != 0 {
            let action = SIGNALS.get_action(pid, sig);

            if action.sa_handler == SIG_IGN
                || (action.sa_handler == SIG_DFL && is_default_ignore(sig))
            {
                SIGNALS.clear_pending(pid, sig);
                continue;
            }

            if action.sa_handler == SIG_DFL && is_default_stop(sig) {
                continue;
            }

            SIGNALS.clear_pending(pid, sig);

            if action.sa_handler == SIG_DFL {
                // Default action: Terminate CPU-bound task
                terminate_cpu_bound_task(pid, sig);
                return true;
            }

            // Custom user handler: construct SignalFrame on user stack
            let frame_size = core::mem::size_of::<SignalFrame>();
            let new_rsp = (frame
                .rsp
                .saturating_sub(RED_ZONE_SIZE as u64 + frame_size as u64))
                & !0xF;

            let sig_frame = SignalFrame {
                restorer: action.sa_restorer as u64,
                signum: sig as u64,
                old_mask: blocked,
                r15: frame.r15,
                r14: frame.r14,
                r13: frame.r13,
                r12: frame.r12,
                rbp: frame.rbp,
                rbx: frame.rbx,
                r9: frame.r9,
                r8: frame.r8,
                r10: frame.r10,
                rdx: frame.rdx,
                rsi: frame.rsi,
                rdi: frame.rdi,
                rax: frame.rax,
                rcx: frame.rip,
                r11: frame.rflags,
                rsp: frame.rsp,
            };

            let frame_write_ok =
                if let Ok(user_ptr) = UserPtr::<SignalFrame>::from_raw(new_rsp as usize) {
                    user_ptr.write(sig_frame).is_ok()
                } else {
                    false
                };

            if frame_write_ok {
                frame.rsp = new_rsp;
                frame.rip = action.sa_handler as u64;
                frame.rdi = sig as u64;
                frame.rsi = 0;
                frame.rdx = new_rsp;

                update_signal_mask_and_disposition(pid, sig, &action, blocked);
                return false;
            } else {
                // Frame write failed: terminate with SIGSEGV matching syscall path
                terminate_cpu_bound_task(pid, SIGSEGV);
                return true;
            }
        }
    }

    false
}
