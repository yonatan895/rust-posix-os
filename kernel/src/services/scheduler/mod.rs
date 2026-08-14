//! Round-Robin Task Scheduler - De-privileged Safe Service.

use crate::ostd::sync::SpinLock;
use crate::services::process::{Process, ProcessState};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::sync::atomic::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeReason {
    Woken,
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

/// Sets the initial running process on the CPU.
pub fn set_current_process(proc: Arc<SpinLock<Process>>) {
    let mut curr_guard = CURRENT_PROCESS.lock();
    *curr_guard = Some(proc);
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

/// Voluntarily blocks the current process, transitioning its state to `Blocked`
/// and switching CPU context to the next ready task (or PID 0 Idle task).
///
/// # Concurrency & Lock Ordering (ADR-0002)
///
/// Callers MUST release all lower-tier locks (e.g. Inode, VFS, RamFs) BEFORE calling `block_current()`.
/// `block_current()` only acquires `SCHEDULER` and individual process locks in strict D1 order.
pub fn block_current() -> WakeReason {
    let mut sched = SCHEDULER.lock();
    let mut curr_guard = CURRENT_PROCESS.lock();

    let prev_proc_arc = match curr_guard.take() {
        Some(p) => p,
        None => return WakeReason::Woken,
    };

    // Transition current task to Blocked state (do not re-add to ready_queue)
    {
        let mut prev_proc = prev_proc_arc.lock();
        prev_proc.state = ProcessState::Blocked;
    }

    // Pick next ready task, or fall back to PID 0 (Idle task)
    let next_proc_arc = match sched.ready_queue.pop_front() {
        Some(p) => p,
        None => match crate::services::process::PROCESS_TABLE.lock().get(&0) {
            Some(idle) => idle.clone(),
            None => {
                let mut prev_proc = prev_proc_arc.lock();
                prev_proc.state = ProcessState::Running;
                drop(prev_proc);
                *curr_guard = Some(prev_proc_arc);
                return WakeReason::Woken;
            }
        },
    };

    let (mut prev_ctx, next_ctx) = {
        let prev_proc = prev_proc_arc.lock();
        let mut next_proc = next_proc_arc.lock();

        next_proc.state = ProcessState::Running;
        crate::ostd::task::switch_active_kernel_stack(next_proc.kernel_stack_top());
        if let Some(ref vm) = next_proc.vm_space {
            vm.activate();
        }
        crate::services::process::CURRENT_PID.store(next_proc.pid, Ordering::SeqCst);

        (prev_proc.cpu_context, next_proc.cpu_context)
    };

    *curr_guard = Some(next_proc_arc.clone());

    drop(curr_guard);
    drop(sched);

    // Architectural CPU register context switch
    crate::ostd::task::switch_cpu_context(&mut prev_ctx, &next_ctx);

    prev_proc_arc.lock().cpu_context = prev_ctx;

    WakeReason::Woken
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
        prev_proc.state = ProcessState::Ready;
        sched.ready_queue.push_back(prev_proc_arc.clone());
    }

    let (mut prev_ctx, next_ctx) = {
        let prev_proc = prev_proc_arc.lock();
        let mut next_proc = next_proc_arc.lock();

        next_proc.state = ProcessState::Running;
        crate::ostd::task::switch_active_kernel_stack(next_proc.kernel_stack_top());
        if let Some(ref vm) = next_proc.vm_space {
            vm.activate();
        }
        crate::services::process::CURRENT_PID.store(next_proc.pid, Ordering::SeqCst);

        (prev_proc.cpu_context, next_proc.cpu_context)
    };

    *curr_guard = Some(next_proc_arc);

    drop(curr_guard);
    drop(sched);

    crate::ostd::task::switch_cpu_context(&mut prev_ctx, &next_ctx);
    prev_proc_arc.lock().cpu_context = prev_ctx;
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
        prev_proc.state = ProcessState::Ready;
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
