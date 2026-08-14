//! POSIX Process Lifecycle & Signal System Calls.

use alloc::sync::Arc;
use posix_abi::*;
use crate::services::process::*;
use crate::services::ipc::SIGNALS;

pub fn sys_fork() -> isize {
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

pub fn sys_execve(path_ptr: *const u8) -> isize {
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
                unsafe { *status_ptr = (exit_code & 0xff) << 8; }
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
