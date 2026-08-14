//! Round-Robin Task Scheduler - De-privileged Safe Service.

use crate::ostd::sync::SpinLock;
use crate::services::process::{Process, ProcessState};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::sync::atomic::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeReason {
    Woken,
    // TODO(signals): WakeReason::Interrupted for EINTR delivery
    Interrupted,
}

pub struct Scheduler {
    pub ready_queue: VecDeque<Arc<SpinLock<Process>>>,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }

    pub fn add_task(&mut self, proc: Arc<SpinLock<Process>>) {
        self.ready_queue.push_back(proc);
    }

    pub fn pick_next(&mut self) -> Option<Arc<SpinLock<Process>>> {
        self.ready_queue.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.ready_queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ready_queue.len()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

pub static SCHEDULER: SpinLock<Scheduler> = SpinLock::new(Scheduler::new());
pub static CURRENT_PROCESS: SpinLock<Option<Arc<SpinLock<Process>>>> = SpinLock::new(None);
pub static IDLE_TASK: SpinLock<Option<Arc<SpinLock<Process>>>> = SpinLock::new(None);

/// Sets the initial running process on the CPU.
pub fn set_current_process(proc: Arc<SpinLock<Process>>) {
    let mut curr_guard = CURRENT_PROCESS.lock();
    *curr_guard = Some(proc);
}

/// Registers the PID 0 idle task descriptor.
pub fn set_idle_task(idle: Arc<SpinLock<Process>>) {
    *IDLE_TASK.lock() = Some(idle);
}

/// Marks the current running task as `Blocked` (unless it is already in a terminal state like `Zombie`).
///
/// Used in mark-then-recheck sequences to prevent lost wakeup race conditions.
pub fn mark_current_blocked() {
    let curr_guard = CURRENT_PROCESS.lock();
    if let Some(ref proc_arc) = *curr_guard {
        let mut proc = proc_arc.lock();
        if proc.state == ProcessState::Running {
            proc.state = ProcessState::Blocked;
        }
    }
}

/// Restores the current task's state to `Running` if a condition check succeeded after marking blocked.
pub fn mark_current_running() {
    let curr_guard = CURRENT_PROCESS.lock();
    if let Some(ref proc_arc) = *curr_guard {
        let mut proc = proc_arc.lock();
        if proc.state == ProcessState::Blocked {
            proc.state = ProcessState::Running;
        }
    }
}

/// Unblocks a list of task PIDs, transitioning their state from `Blocked` to `Ready`
/// and enqueuing them into the scheduler's ready queue.
///
/// Follows ADR-0002 lock ordering: acquires `PROCESS_TABLE` then `SCHEDULER`.
pub fn wake_tasks(pids: &[i32]) {
    if pids.is_empty() {
        return;
    }
    let table = crate::services::process::PROCESS_TABLE.lock();
    let mut sched = SCHEDULER.lock();
    for &pid in pids {
        if let Some(proc_arc) = table.get(&pid) {
            let mut proc = proc_arc.lock();
            if proc.state == ProcessState::Blocked {
                proc.state = ProcessState::Ready;
                sched.add_task(proc_arc.clone());
            }
        }
    }
}

/// Switches CPU execution away from the current task to the next ready task.
///
/// Assumes `mark_current_blocked()` has already been called if the task is blocking.
/// Operates strictly within Scheduler tier locks (ADR-0002).
pub fn switch_out_current() -> WakeReason {
    let mut sched = SCHEDULER.lock();
    let mut curr_guard = CURRENT_PROCESS.lock();

    let prev_proc_arc = match curr_guard.take() {
        Some(p) => p,
        None => return WakeReason::Woken,
    };

    // Pick next ready task, or fall back to PID 0 (Idle task) without querying PROCESS_TABLE
    let next_proc_arc = match sched.ready_queue.pop_front() {
        Some(p) => p,
        None => match IDLE_TASK.lock().as_ref() {
            Some(idle) => idle.clone(),
            None => {
                let mut prev_proc = prev_proc_arc.lock();
                if prev_proc.state == ProcessState::Blocked {
                    prev_proc.state = ProcessState::Running;
                }
                drop(prev_proc);
                *curr_guard = Some(prev_proc_arc);
                return WakeReason::Woken;
            }
        },
    };

    let mut prev_saved_rsp = 0usize;
    let next_saved_rsp = {
        let mut next_proc = next_proc_arc.lock();
        next_proc.state = ProcessState::Running;
        crate::ostd::task::switch_active_kernel_stack(next_proc.kernel_stack_top());
        if let Some(ref vm) = next_proc.vm_space {
            vm.activate();
        }
        crate::services::process::CURRENT_PID.store(next_proc.pid, Ordering::SeqCst);
        next_proc.saved_kernel_rsp
    };

    *curr_guard = Some(next_proc_arc);

    drop(curr_guard);
    drop(sched);

    // Architectural task switch via unified TrapFrame / iretq
    crate::ostd::task::switch_tasks(&mut prev_saved_rsp, next_saved_rsp);

    // Write back saved stack pointer on the outgoing task PCB
    prev_proc_arc.lock().saved_kernel_rsp = prev_saved_rsp;

    WakeReason::Woken
}

/// Voluntarily blocks the current process and switches CPU context to the next ready task.
pub fn block_current() -> WakeReason {
    mark_current_blocked();
    switch_out_current()
}

/// Voluntarily yields the CPU quantum to the next ready task.
pub fn schedule_yield() {
    let mut sched = SCHEDULER.lock();
    let mut curr_guard = CURRENT_PROCESS.lock();

    let prev_proc_arc = match curr_guard.take() {
        Some(p) => p,
        None => return,
    };

    if sched.ready_queue.is_empty() {
        *curr_guard = Some(prev_proc_arc);
        return;
    }

    let next_proc_arc = match sched.ready_queue.pop_front() {
        Some(p) => p,
        None => {
            *curr_guard = Some(prev_proc_arc);
            return;
        }
    };

    {
        let mut prev_proc = prev_proc_arc.lock();
        if prev_proc.state == ProcessState::Running {
            prev_proc.state = ProcessState::Ready;
        }
        sched.ready_queue.push_back(prev_proc_arc.clone());
    }

    let mut prev_saved_rsp = 0usize;
    let next_saved_rsp = {
        let mut next_proc = next_proc_arc.lock();
        next_proc.state = ProcessState::Running;
        crate::ostd::task::switch_active_kernel_stack(next_proc.kernel_stack_top());
        if let Some(ref vm) = next_proc.vm_space {
            vm.activate();
        }
        crate::services::process::CURRENT_PID.store(next_proc.pid, Ordering::SeqCst);
        next_proc.saved_kernel_rsp
    };

    *curr_guard = Some(next_proc_arc);

    drop(curr_guard);
    drop(sched);

    crate::ostd::task::switch_tasks(&mut prev_saved_rsp, next_saved_rsp);
    prev_proc_arc.lock().saved_kernel_rsp = prev_saved_rsp;
}

/// Invoked from the timer interrupt service routine to perform round-robin preemptive scheduling.
///
/// Returns the kernel stack pointer of the task selected to run next.
pub fn timer_tick_schedule(current_rsp: usize) -> usize {
    let mut sched = SCHEDULER.lock();
    let mut curr_guard = CURRENT_PROCESS.lock();
    let prev_proc_opt = curr_guard.take();

    if sched.ready_queue.is_empty() {
        *curr_guard = prev_proc_opt;
        return current_rsp;
    }

    let next_proc_arc = match sched.ready_queue.pop_front() {
        Some(p) => p,
        None => {
            *curr_guard = prev_proc_opt;
            return current_rsp;
        }
    };

    if let Some(ref prev_proc_arc) = prev_proc_opt {
        let mut prev_proc = prev_proc_arc.lock();
        prev_proc.saved_kernel_rsp = current_rsp;
        if prev_proc.state == ProcessState::Running {
            prev_proc.state = ProcessState::Ready;
        }
        drop(prev_proc);
        sched.ready_queue.push_back(prev_proc_arc.clone());
    }

    let next_rsp = {
        let mut next_proc = next_proc_arc.lock();
        next_proc.state = ProcessState::Running;
        crate::ostd::task::switch_active_kernel_stack(next_proc.kernel_stack_top());
        if let Some(ref vm) = next_proc.vm_space {
            vm.activate();
        }
        crate::services::process::CURRENT_PID.store(next_proc.pid, Ordering::SeqCst);
        next_proc.saved_kernel_rsp
    };

    *curr_guard = Some(next_proc_arc);
    next_rsp
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_scheduler_fifo_order() {
        let mut sched = Scheduler::new();
        let p1 = Arc::new(SpinLock::new(Process::new(1, 0, "/".to_string())));
        let p2 = Arc::new(SpinLock::new(Process::new(2, 0, "/".to_string())));

        sched.add_task(p1.clone());
        sched.add_task(p2.clone());

        assert_eq!(sched.len(), 2);
        assert_eq!(sched.pick_next().unwrap().lock().pid, 1);
        assert_eq!(sched.pick_next().unwrap().lock().pid, 2);
        assert!(sched.pick_next().is_none());
    }

    #[test]
    fn test_scheduler_round_robin_rotation() {
        let mut sched = Scheduler::new();
        let p1 = Arc::new(SpinLock::new(Process::new(1, 0, "/".to_string())));
        let p2 = Arc::new(SpinLock::new(Process::new(2, 0, "/".to_string())));

        sched.add_task(p1.clone());
        sched.add_task(p2.clone());

        let next = sched.pick_next().unwrap();
        assert_eq!(next.lock().pid, 1);
        sched.add_task(next);

        let next2 = sched.pick_next().unwrap();
        assert_eq!(next2.lock().pid, 2);
        sched.add_task(next2);

        let next3 = sched.pick_next().unwrap();
        assert_eq!(next3.lock().pid, 1);
    }

    #[test]
    fn test_scheduler_empty_queue() {
        let mut sched = Scheduler::new();
        assert!(sched.is_empty());
        assert_eq!(sched.len(), 0);
        assert!(sched.pick_next().is_none());
    }
}
