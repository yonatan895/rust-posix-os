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
    /// Queue of waiting process IDs in arrival order.
    waiters: VecDeque<i32>,
}

impl WaitQueue {
    /// Creates a new empty wait queue.
    pub const fn new() -> Self {
        Self {
            waiters: VecDeque::new(),
        }
    }

    /// Enqueues a process PID if not already present in the wait queue.
    pub fn add_waiter(&mut self, pid: i32) {
        if !self.waiters.contains(&pid) {
            self.waiters.push_back(pid);
        }
    }

    /// Removes a process PID from the wait queue.
    pub fn remove_waiter(&mut self, pid: i32) {
        self.waiters.retain(|&p| p != pid);
    }

    /// Dequeues and returns the first waiting process PID.
    pub fn drain_one(&mut self) -> Option<i32> {
        self.waiters.pop_front()
    }

    /// Dequeues and returns all waiting process PIDs.
    pub fn drain_all(&mut self) -> Vec<i32> {
        self.waiters.drain(..).collect()
    }

    /// Returns `true` if no tasks are currently waiting in the queue.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.waiters.is_empty()
    }

    /// Returns the number of waiting processes in the queue.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.waiters.len()
    }
}
