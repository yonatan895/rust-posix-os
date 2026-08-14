//! Synchronization primitives for the kernel framework (OSTD).
//!
//! SpinLock masks CPU interrupts on acquire and restores the previous RFLAGS
//! state on drop, preventing deadlocks when acquiring locks held across ISR contexts.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};
use crate::ostd::arch::{cli, read_rflags, restore_rflags};

pub struct SpinLock<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
    rflags: u64,
}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        // SAFETY: Read RFLAGS and disable interrupts before acquiring the spinlock
        // to prevent deadlock if an interrupt handler attempts to take the same lock.
        let rflags = unsafe { read_rflags() };
        unsafe { cli() };

        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while self.lock.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
        SpinLockGuard { lock: self, rflags }
    }

    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        // SAFETY: Read RFLAGS and disable interrupts before attempting lock acquisition.
        let rflags = unsafe { read_rflags() };
        unsafe { cli() };

        if self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinLockGuard { lock: self, rflags })
        } else {
            // SAFETY: Restore previous interrupt state if lock acquisition failed.
            unsafe { restore_rflags(rflags) };
            None
        }
    }
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        // SAFETY: SpinLockGuard guarantees exclusive access while held.
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: SpinLockGuard guarantees exclusive access while held.
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.lock.store(false, Ordering::Release);
        // SAFETY: Restore CPU interrupt enable flag from when the lock was acquired.
        unsafe { restore_rflags(self.rflags) };
    }
}
