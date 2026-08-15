//! Inter-Process Communication (IPC) and Pipe Synchronization Test Suite.

use super::harness::TestRunner;
use std::collections::{BTreeMap, VecDeque};

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

struct SpecPipe {
    buf: Vec<u8>,
    cap: usize,
    readers_open: usize,
    writers_open: usize,
    read_waiters: Vec<i32>,
    write_waiters: Vec<i32>,
}

impl SpecPipe {
    fn new(cap: usize) -> Self {
        Self {
            buf: Vec::new(),
            cap,
            readers_open: 1,
            writers_open: 1,
            read_waiters: Vec::new(),
            write_waiters: Vec::new(),
        }
    }

    fn read(&mut self, out: &mut [u8], nonblock: bool, caller: i32) -> Result<usize, &'static str> {
        if self.buf.is_empty() {
            if self.writers_open == 0 {
                return Ok(0); // EOF
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
            self.write_waiters.remove(0); // Wake one writer
        }
        Ok(n)
    }

    fn write(&mut self, data: &[u8], nonblock: bool, caller: i32) -> Result<usize, &'static str> {
        if self.readers_open == 0 {
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
        if was_empty && !self.buf.is_empty() {
            self.read_waiters.clear();
        }
        Ok(to_write)
    }

    fn close_writer(&mut self) {
        self.writers_open = self.writers_open.saturating_sub(1);
        if self.writers_open == 0 {
            self.read_waiters.clear(); // Wakes readers for EOF
        }
    }

    fn close_reader(&mut self) {
        self.readers_open = self.readers_open.saturating_sub(1);
        if self.readers_open == 0 {
            self.write_waiters.clear(); // Wakes writers for EPIPE
        }
    }
}

fn test_pipe_blocking_and_eof_semantics() {
    let mut pipe = SpecPipe::new(4);
    let mut buf = [0u8; 8];

    // 1. Empty read with nonblock -> EAGAIN
    assert_eq!(pipe.read(&mut buf, true, 1), Err("EAGAIN"));

    // 2. Empty read without nonblock -> BLOCKED (registers on read_waiters)
    assert_eq!(pipe.read(&mut buf, false, 1), Err("BLOCKED"));
    assert_eq!(pipe.read_waiters, vec![1]);

    // 3. Write data -> gated wakeup clears read_waiters and fills buffer
    assert_eq!(pipe.write(b"abcd", false, 2), Ok(4));
    assert!(
        pipe.read_waiters.is_empty(),
        "Writers failed to wake read_waiters"
    );

    // 4. Full write with nonblock -> EAGAIN
    assert_eq!(pipe.write(b"e", true, 2), Err("EAGAIN"));

    // 5. Full write without nonblock -> BLOCKED (registers on write_waiters)
    assert_eq!(pipe.write(b"e", false, 2), Err("BLOCKED"));
    assert_eq!(pipe.write_waiters, vec![2]);

    // 6. Read data -> gets 4 bytes, gated wakeup drains one write waiter
    assert_eq!(pipe.read(&mut buf, false, 1), Ok(4));
    assert_eq!(&buf[..4], b"abcd");
    assert!(
        pipe.write_waiters.is_empty(),
        "Reader failed to unblock writer"
    );

    // 7. Close writer -> reader gets EOF 0
    pipe.close_writer();
    assert_eq!(pipe.read(&mut buf, false, 1), Ok(0));

    // 8. Close reader on new pipe -> writer gets EPIPE
    let mut pipe2 = SpecPipe::new(4);
    pipe2.close_reader();
    assert_eq!(pipe2.write(b"a", false, 2), Err("EPIPE"));
}

fn test_two_process_pipe_voluntary_context_switch() {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TaskState {
        Running,
        Ready,
        Blocked,
    }

    #[allow(dead_code)]
    struct Task {
        pid: i32,
        state: TaskState,
    }

    struct Simulation {
        tasks: BTreeMap<i32, Task>,
        ready_queue: VecDeque<i32>,
        current_pid: i32,
        pipe_buf: Vec<u8>,
        pipe_read_waiters: Vec<i32>,
        writers_open: usize,
    }

    impl Simulation {
        fn new() -> Self {
            let mut tasks = BTreeMap::new();
            tasks.insert(
                1,
                Task {
                    pid: 1,
                    state: TaskState::Running,
                },
            );
            tasks.insert(
                2,
                Task {
                    pid: 2,
                    state: TaskState::Ready,
                },
            );

            let mut ready_queue = VecDeque::new();
            ready_queue.push_back(2);

            Self {
                tasks,
                ready_queue,
                current_pid: 1,
                pipe_buf: Vec::new(),
                pipe_read_waiters: Vec::new(),
                writers_open: 1,
            }
        }

        // Simulates Reader PID 1 calling read() on empty pipe
        fn reader_read(&mut self, buf: &mut [u8]) -> Option<usize> {
            assert_eq!(self.current_pid, 1);
            // 1. Mark current blocked
            self.tasks.get_mut(&1).unwrap().state = TaskState::Blocked;
            self.pipe_read_waiters.push(1);

            // 2. Re-check condition
            if self.pipe_buf.is_empty() && self.writers_open > 0 {
                // Switch out current task 1 to next ready task
                let next_pid = self
                    .ready_queue
                    .pop_front()
                    .expect("No ready task to switch to");
                self.current_pid = next_pid;
                self.tasks.get_mut(&next_pid).unwrap().state = TaskState::Running;
                None // Blocked, execution switched
            } else {
                self.tasks.get_mut(&1).unwrap().state = TaskState::Running;
                let n = buf.len().min(self.pipe_buf.len());
                for item in buf.iter_mut().take(n) {
                    *item = self.pipe_buf.remove(0);
                }
                Some(n)
            }
        }

        // Simulates Writer PID 2 calling write() to the pipe
        fn writer_write(&mut self, data: &[u8]) -> usize {
            assert_eq!(self.current_pid, 2);
            let was_empty = self.pipe_buf.is_empty();
            self.pipe_buf.extend_from_slice(data);

            if was_empty && !self.pipe_buf.is_empty() {
                // Wake read waiters
                let waiters: Vec<i32> = std::mem::take(&mut self.pipe_read_waiters);
                for pid in waiters {
                    let task = self.tasks.get_mut(&pid).unwrap();
                    if task.state == TaskState::Blocked {
                        task.state = TaskState::Ready;
                        self.ready_queue.push_back(pid);
                    }
                }
            }
            data.len()
        }

        // Simulates Writer PID 2 voluntarily yielding quantum
        fn writer_yield(&mut self) {
            assert_eq!(self.current_pid, 2);
            self.tasks.get_mut(&2).unwrap().state = TaskState::Ready;
            self.ready_queue.push_back(2);

            let next_pid = self
                .ready_queue
                .pop_front()
                .expect("No ready task to schedule");
            self.current_pid = next_pid;
            self.tasks.get_mut(&next_pid).unwrap().state = TaskState::Running;
        }

        // Simulates Reader PID 1 waking up and re-reading data
        fn reader_resume_read(&mut self, buf: &mut [u8]) -> usize {
            assert_eq!(self.current_pid, 1);
            assert_eq!(self.tasks.get(&1).unwrap().state, TaskState::Running);
            let n = buf.len().min(self.pipe_buf.len());
            for item in buf.iter_mut().take(n) {
                *item = self.pipe_buf.remove(0);
            }
            n
        }
    }

    let mut sim = Simulation::new();
    let mut reader_buf = [0u8; 16];

    // Step 1: Reader PID 1 reads from empty pipe -> Blocks and switches to Writer PID 2
    let res = sim.reader_read(&mut reader_buf);
    assert!(res.is_none(), "Reader should have blocked on empty pipe");
    assert_eq!(
        sim.current_pid, 2,
        "CPU should have context-switched to PID 2"
    );
    assert_eq!(
        sim.tasks.get(&1).unwrap().state,
        TaskState::Blocked,
        "Task 1 should be Blocked"
    );
    assert_eq!(
        sim.tasks.get(&2).unwrap().state,
        TaskState::Running,
        "Task 2 should be Running"
    );

    // Step 2: Writer PID 2 writes "hello world" -> unblocks Task 1 (transitions Blocked -> Ready)
    let written = sim.writer_write(b"hello world");
    assert_eq!(written, 11);
    assert_eq!(
        sim.tasks.get(&1).unwrap().state,
        TaskState::Ready,
        "Task 1 should have been woken to Ready"
    );
    assert!(
        sim.pipe_read_waiters.is_empty(),
        "Read waiters queue should be cleared"
    );

    // Step 3: Writer PID 2 yields -> Task 1 is scheduled and resumes
    sim.writer_yield();
    assert_eq!(sim.current_pid, 1, "Scheduler should pick Task 1");
    assert_eq!(
        sim.tasks.get(&1).unwrap().state,
        TaskState::Running,
        "Task 1 should be Running"
    );

    // Step 4: Task 1 reads data from pipe
    let read_bytes = sim.reader_resume_read(&mut reader_buf);
    assert_eq!(read_bytes, 11);
    assert_eq!(&reader_buf[..11], b"hello world");
}
