//! Wait Queue Primitive for Process Blocking and Wakeup - De-privileged Safe Service.

use crate::services::process::{PROCESS_TABLE, ProcessState};
use crate::services::scheduler::SCHEDULER;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

#[derive(Debug, Default)]
pub struct WaitQueue {
    waiters: VecDeque<i32>,
}

impl WaitQueue {
    pub const fn new() -> Self {
        Self {
            waiters: VecDeque::new(),
        }
    }

    pub fn add_waiter(&mut self, pid: i32) {
        if !self.waiters.contains(&pid) {
            self.waiters.push_back(pid);
        }
    }

    pub fn wake_one(&mut self) -> Option<i32> {
        while let Some(pid) = self.waiters.pop_front() {
            if wake_task(pid) {
                return Some(pid);
            }
        }
        None
    }

    pub fn wake_all(&mut self) -> Vec<i32> {
        let mut woken = Vec::new();
        while let Some(pid) = self.waiters.pop_front() {
            if wake_task(pid) {
                woken.push(pid);
            }
        }
        woken
    }

    pub fn is_empty(&self) -> bool {
        self.waiters.is_empty()
    }

    pub fn len(&self) -> usize {
        self.waiters.len()
    }
}

/// Unblocks a sleeping task by setting its state to `Ready` and enqueuing it in `SCHEDULER`.
pub fn wake_task(pid: i32) -> bool {
    let table = PROCESS_TABLE.lock();
    if let Some(proc_arc) = table.get(&pid) {
        let mut proc = proc_arc.lock();
        if proc.state == ProcessState::Blocked {
            proc.state = ProcessState::Ready;
            SCHEDULER.lock().add_task(proc_arc.clone());
            return true;
        }
    }
    false
}
