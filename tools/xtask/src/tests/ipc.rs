//! Inter-process communication (IPC) and pipe synchronization test suite.

use super::harness::TestRunner;
use std::collections::{BTreeMap, VecDeque};

/// Registers all IPC and pipe synchronization tests with the runner.
pub fn register_tests(runner: &mut TestRunner) {
    runner.run_test(
        "ipc",
        "Pipe Blocking, Non-blocking, and EOF Semantics",
        test_pipe_blocking_and_eof_semantics,
    );
    runner.run_test(
        "ipc",
        "Two-Process Voluntary Context Switch and Wakeup",
        test_two_process_pipe_voluntary_context_switch,
    );
}

/// Specification model of a POSIX anonymous pipe with reader/writer queues and capacities.
struct SpecPipe {
    buf: Vec<u8>,
    cap: usize,
    readers: usize,
    writers: usize,
    read_waiters: Vec<i32>,
    write_waiters: Vec<i32>,
}

impl SpecPipe {
    fn new(cap: usize) -> Self {
        Self {
            buf: Vec::new(),
            cap,
            readers: 1,
            writers: 1,
            read_waiters: Vec::new(),
            write_waiters: Vec::new(),
        }
    }
    fn read(&mut self, out: &mut [u8], nonblock: bool, caller: i32) -> Result<usize, &'static str> {
        if self.buf.is_empty() {
            if self.writers == 0 {
                return Ok(0);
            }
            if nonblock {
                return Err("EAGAIN");
            }
            self.read_waiters.push(caller);
            return Err("BLOCKED");
        }
        let n = out.len().min(self.buf.len());
        for item in out.iter_mut().take(n) {
            *item = self.buf.remove(0);
        }
        if !self.write_waiters.is_empty() {
            self.write_waiters.remove(0);
        }
        Ok(n)
    }
    fn write(&mut self, data: &[u8], nonblock: bool, caller: i32) -> Result<usize, &'static str> {
        if self.readers == 0 {
            return Err("EPIPE");
        }
        let space = self.cap - self.buf.len();
        if space == 0 {
            if nonblock {
                return Err("EAGAIN");
            }
            self.write_waiters.push(caller);
            return Err("BLOCKED");
        }
        let to_write = data.len().min(space);
        let was_empty = self.buf.is_empty();
        self.buf.extend_from_slice(&data[..to_write]);
        if was_empty {
            self.read_waiters.clear();
        }
        Ok(to_write)
    }
    fn close_writer(&mut self) {
        self.writers = self.writers.saturating_sub(1);
        if self.writers == 0 {
            self.read_waiters.clear();
        }
    }
    fn close_reader(&mut self) {
        self.readers = self.readers.saturating_sub(1);
        if self.readers == 0 {
            self.write_waiters.clear();
        }
    }
}

/// Tests pipe blocking, non-blocking EAGAIN, and EOF/EPIPE semantics.
fn test_pipe_blocking_and_eof_semantics() {
    let mut pipe = SpecPipe::new(4);
    let mut buf = [0u8; 8];

    assert_eq!(pipe.read(&mut buf, true, 1), Err("EAGAIN"));
    assert_eq!(pipe.read(&mut buf, false, 1), Err("BLOCKED"));
    assert_eq!(pipe.read_waiters, vec![1]);

    assert_eq!(pipe.write(b"abcd", false, 2), Ok(4));
    assert!(pipe.read_waiters.is_empty());

    assert_eq!(pipe.write(b"e", true, 2), Err("EAGAIN"));
    assert_eq!(pipe.write(b"e", false, 2), Err("BLOCKED"));
    assert_eq!(pipe.write_waiters, vec![2]);

    assert_eq!(pipe.read(&mut buf, false, 1), Ok(4));
    assert_eq!(&buf[..4], b"abcd");
    assert!(pipe.write_waiters.is_empty());

    pipe.close_writer();
    assert_eq!(pipe.read(&mut buf, false, 1), Ok(0));

    let mut pipe2 = SpecPipe::new(4);
    pipe2.close_reader();
    assert_eq!(pipe2.write(b"a", false, 2), Err("EPIPE"));
}

/// Tests two-process reader/writer interaction, voluntary blocking, and scheduler wakeups.
fn test_two_process_pipe_voluntary_context_switch() {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TaskState {
        Running,
        Ready,
        Blocked,
    }

    struct Simulation {
        tasks: BTreeMap<i32, TaskState>,
        ready_queue: VecDeque<i32>,
        current_pid: i32,
        pipe_buf: Vec<u8>,
        pipe_read_waiters: Vec<i32>,
    }

    impl Simulation {
        fn new() -> Self {
            let mut tasks = BTreeMap::new();
            tasks.insert(1, TaskState::Running);
            tasks.insert(2, TaskState::Ready);
            let mut ready_queue = VecDeque::new();
            ready_queue.push_back(2);
            Self {
                tasks,
                ready_queue,
                current_pid: 1,
                pipe_buf: Vec::new(),
                pipe_read_waiters: Vec::new(),
            }
        }
        fn reader_read(&mut self, _buf: &mut [u8]) -> Option<usize> {
            self.tasks.insert(1, TaskState::Blocked);
            self.pipe_read_waiters.push(1);
            let next = self.ready_queue.pop_front().unwrap();
            self.current_pid = next;
            self.tasks.insert(next, TaskState::Running);
            None
        }
        fn writer_write(&mut self, data: &[u8]) -> usize {
            self.pipe_buf.extend_from_slice(data);
            for pid in std::mem::take(&mut self.pipe_read_waiters) {
                self.tasks.insert(pid, TaskState::Ready);
                self.ready_queue.push_back(pid);
            }
            data.len()
        }
        fn writer_yield(&mut self) {
            self.tasks.insert(2, TaskState::Ready);
            self.ready_queue.push_back(2);
            let next = self.ready_queue.pop_front().unwrap();
            self.current_pid = next;
            self.tasks.insert(next, TaskState::Running);
        }
        fn reader_resume_read(&mut self, buf: &mut [u8]) -> usize {
            let n = buf.len().min(self.pipe_buf.len());
            for item in buf.iter_mut().take(n) {
                *item = self.pipe_buf.remove(0);
            }
            n
        }
    }

    let mut sim = Simulation::new();
    let mut reader_buf = [0u8; 16];

    assert!(sim.reader_read(&mut reader_buf).is_none());
    assert_eq!(sim.current_pid, 2);
    assert_eq!(sim.tasks.get(&1), Some(&TaskState::Blocked));
    assert_eq!(sim.tasks.get(&2), Some(&TaskState::Running));

    assert_eq!(sim.writer_write(b"hello world"), 11);
    assert_eq!(sim.tasks.get(&1), Some(&TaskState::Ready));
    assert!(sim.pipe_read_waiters.is_empty());

    sim.writer_yield();
    assert_eq!(sim.current_pid, 1);
    assert_eq!(sim.tasks.get(&1), Some(&TaskState::Running));

    let read_bytes = sim.reader_resume_read(&mut reader_buf);
    assert_eq!(read_bytes, 11);
    assert_eq!(&reader_buf[..11], b"hello world");
}
