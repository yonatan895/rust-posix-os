//! Synchronization Primitives for the Kernel Framework (OSTD).

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        while self.lock.compare_exchange_weak(
            false,
            true,
            Ordering::Acquire,
            Ordering::Relaxed,
        ).is_err() {
            while self.lock.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
        SpinLockGuard { lock: self }
    }

    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        if self.lock.compare_exchange(
            false,
            true,
            Ordering::Acquire,
            Ordering::Relaxed,
        ).is_ok() {
            Some(SpinLockGuard { lock: self })
        } else {
            None
        }
    }
}

impl<'a, T> Deref for SpinLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for SpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for SpinLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.lock.store(false, Ordering::Release);
    }
}

pub struct Once<T> {
    state: AtomicBool,
    data: UnsafeCell<Option<T>>,
}

unsafe impl<T: Send + Sync> Sync for Once<T> {}

impl<T> Once<T> {
    pub const fn new() -> Self {
        Self {
            state: AtomicBool::new(false),
            data: UnsafeCell::new(None),
        }
    }

    pub fn call_once<F: FnOnce() -> T>(&self, f: F) {
        if !self.state.load(Ordering::Acquire) {
            let val = f();
            unsafe {
                *self.data.get() = Some(val);
            }
            self.state.store(true, Ordering::Release);
        }
    }

    pub fn get(&self) -> Option<&T> {
        if self.state.load(Ordering::Acquire) {
            unsafe { (*self.data.get()).as_ref() }
        } else {
            None
        }
    }
}
