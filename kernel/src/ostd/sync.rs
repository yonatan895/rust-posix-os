//! Synchronization primitives for the kernel framework (OSTD).
//!
//! SpinLock masks CPU interrupts on acquire and restores the previous interrupt
//! state on drop, preventing deadlocks when acquiring locks held across ISR contexts.

use crate::ostd::irq::IrqGuard;
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// Mutual exclusion primitive that masks CPU interrupts while held to avoid ISR deadlocks.
pub struct SpinLock<T> {
    /// Atomic lock flag (true = locked, false = unlocked).
    lock: AtomicBool,
    /// Underlying data protected by the spinlock.
    data: UnsafeCell<T>,
}

// SAFETY: SpinLock provides mutual exclusion via atomic spinlock flag and masks CPU interrupts via IrqGuard, ensuring safe concurrent access across threads and interrupt handlers.
unsafe impl<T: Send> Sync for SpinLock<T> {}

// SAFETY: Transferring ownership of SpinLock<T> across thread boundaries is safe as long as the underlying T is Send.
unsafe impl<T: Send> Send for SpinLock<T> {}

/// RAII guard providing exclusive access to locked data and restoring CPU interrupt state on drop.
pub struct SpinLockGuard<'a, T> {
    /// Reference to the parent spinlock.
    lock: &'a SpinLock<T>,
    /// RAII interrupt guard managing CPU interrupt state.
    guard: IrqGuard,
}

impl<T> SpinLock<T> {
    /// Creates a new spinlock protecting the provided `data`.
    pub const fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    /// Disables CPU interrupts and acquires the spinlock, spinning until available.
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        // Disables interrupts and stores previous state token inside IrqGuard.
        let guard = IrqGuard::new();

        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while self.lock.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
        SpinLockGuard { lock: self, guard }
    }

    /// Attempts to acquire the lock without blocking, returning `None` if currently locked.
    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        // Disables interrupts and stores previous state token inside IrqGuard.
        let guard = IrqGuard::new();

        if self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinLockGuard { lock: self, guard })
        } else {
            // IrqGuard drops and restores interrupts automatically on failed lock acquisition.
            None
        }
    }
}

impl<T> SpinLockGuard<'_, T> {
    /// Unlocks the spinlock without restoring the CPU interrupt state (leaves interrupts masked).
    pub fn unlock_without_restoring_interrupts(mut self) {
        self.lock.lock.store(false, Ordering::Release);
        self.guard.disarm();
    }
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        // SAFETY: SpinLockGuard existence proves the spinlock is acquired with Acquire ordering, and interrupts are masked, ensuring exclusive access to UnsafeCell inner data.
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: SpinLockGuard existence proves the spinlock is acquired with Acquire ordering, and interrupts are masked, ensuring exclusive access to UnsafeCell inner data.
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.lock.store(false, Ordering::Release);
        // `self.guard` drops here and restores interrupts automatically.
    }
}
