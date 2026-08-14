//! Preemptive / MLFQ Scheduler - De-privileged Safe Service.

use alloc::collections::VecDeque;
use crate::ostd::sync::SpinLock;

pub struct Scheduler {
    ready_queue: VecDeque<i32>,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }

    pub fn add_task(&mut self, pid: i32) {
        self.ready_queue.push_back(pid);
    }

    pub fn pick_next(&mut self) -> Option<i32> {
        self.ready_queue.pop_front()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

pub static SCHEDULER: SpinLock<Scheduler> = SpinLock::new(Scheduler::new());
