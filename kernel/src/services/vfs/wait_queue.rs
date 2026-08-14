//! Wait Queue Primitive for Process Blocking and Wakeup - De-privileged Safe Service.

#![deny(unsafe_code)]

use alloc::collections::VecDeque;
use alloc::vec::Vec;

/// A lightweight, lock-free wait queue tracking process IDs waiting on a resource.
///
/// # Concurrency & Lock Ordering (ADR-0002)
///
/// `WaitQueue` holds only data (PIDs) and does NOT acquire any locks itself.
/// Callers collect woken PIDs under their local lock (e.g. Inode SpinLock),
/// drop that lock, and pass the PIDs to the scheduler-tier wake function
/// (`crate::services::scheduler::wake_tasks`), preserving the
/// `PROCESS_TABLE -> Scheduler -> VFS -> Inode` acquisition hierarchy.
///
/// # IRQ Safety (ADR-0002 L5)
///
/// Waking tasks acquires `PROCESS_TABLE` at the scheduler tier and must never
/// be called from IRQ context.
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

    pub fn remove_waiter(&mut self, pid: i32) {
        self.waiters.retain(|&p| p != pid);
    }

    pub fn drain_one(&mut self) -> Option<i32> {
        self.waiters.pop_front()
    }

    pub fn drain_all(&mut self) -> Vec<i32> {
        self.waiters.drain(..).collect()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.waiters.is_empty()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.waiters.len()
    }
}
