//! Asynchronous Task Executor and Global Async Runtime.

use alloc::collections::{BTreeMap, VecDeque};
use core::future::Future;
use core::task::{Context, Poll};
use crate::ostd::sync::SpinLock;
use super::async_task::{create_waker, Task, TaskId};

pub static WAKE_QUEUE: SpinLock<VecDeque<TaskId>> = SpinLock::new(VecDeque::new());

pub fn wake_task(task_id: TaskId) {
    WAKE_QUEUE.lock().push_back(task_id);
}

pub struct Executor {
    tasks: BTreeMap<TaskId, Task>,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor {

    pub fn spawn(&mut self, future: impl Future<Output = ()> + Send + 'static) -> TaskId {
        let task = Task::new(future);
        let task_id = task.id;
        self.tasks.insert(task_id, task);
        wake_task(task_id);
        task_id
    }

    pub fn run_ready_tasks(&mut self) -> usize {
        let mut executed = 0;

        loop {
            let task_id = match WAKE_QUEUE.lock().pop_front() {
                Some(id) => id,
                None => break,
            };

            let mut task = match self.tasks.remove(&task_id) {
                Some(t) => t,
                None => continue,
            };

            let waker = create_waker(task_id);
            let mut context = Context::from_waker(&waker);

            match task.poll(&mut context) {
                Poll::Ready(()) => {
                    executed += 1;
                }
                Poll::Pending => {
                    self.tasks.insert(task_id, task);
                    executed += 1;
                }
            }
        }

        executed
    }

    pub fn active_task_count(&self) -> usize {
        self.tasks.len()
    }
}

pub static GLOBAL_EXECUTOR: SpinLock<Option<Executor>> = SpinLock::new(None);

pub fn async_init() {
    *GLOBAL_EXECUTOR.lock() = Some(Executor::new());
}

pub fn spawn(future: impl Future<Output = ()> + Send + 'static) -> Option<TaskId> {
    GLOBAL_EXECUTOR.lock().as_mut().map(|exec| exec.spawn(future))
}

pub fn run_async_tasks() -> usize {
    GLOBAL_EXECUTOR.lock().as_mut().map(|exec| exec.run_ready_tasks()).unwrap_or(0)
}
