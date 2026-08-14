//! Asynchronous Task Abstraction, Futures, and Zero-Allocation Wakers for OSTD.

use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU64, Ordering};
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(pub u64);

impl TaskId {
    pub fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        TaskId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

pub struct Task {
    pub id: TaskId,
    pub future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + Send + 'static) -> Self {
        Self {
            id: TaskId::new(),
            future: Box::pin(future),
        }
    }

    pub fn poll(&mut self, context: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(context)
    }
}

static WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    clone_raw,
    wake_raw,
    wake_by_ref_raw,
    drop_raw,
);

pub fn create_waker(task_id: TaskId) -> Waker {
    let raw = RawWaker::new(task_id.0 as usize as *const (), &WAKER_VTABLE);
    unsafe { Waker::from_raw(raw) }
}

unsafe fn clone_raw(ptr: *const ()) -> RawWaker {
    RawWaker::new(ptr, &WAKER_VTABLE)
}

unsafe fn wake_raw(ptr: *const ()) {
    let task_id = TaskId(ptr as usize as u64);
    super::executor::wake_task(task_id);
}

unsafe fn wake_by_ref_raw(ptr: *const ()) {
    let task_id = TaskId(ptr as usize as u64);
    super::executor::wake_task(task_id);
}

unsafe fn drop_raw(_ptr: *const ()) {
    // Zero-allocation waker: nothing to free
}

pub struct YieldFuture {
    yielded: bool,
}

impl Future for YieldFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pub fn yield_now() -> YieldFuture {
    YieldFuture { yielded: false }
}
