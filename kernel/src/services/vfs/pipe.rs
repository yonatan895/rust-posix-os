//! POSIX Anonymous Pipe Implementation - De-privileged Safe Service.

use super::wait_queue::WaitQueue;
use super::{FileType, Inode, InodePollFlags};
use crate::ostd::sync::SpinLock;
use alloc::sync::Arc;
use alloc::vec::Vec;
use posix_abi::*;

/// Capacity of an anonymous pipe circular memory buffer (4 KiB).
pub const PIPE_BUFFER_SIZE: usize = 4096;

/// Circular ring buffer backing a unidirectional POSIX pipe.
pub struct PipeBuffer {
    /// In-memory ring buffer storing pipe bytes.
    data: [u8; PIPE_BUFFER_SIZE],
    /// Ring buffer read cursor offset.
    read_pos: usize,
    /// Ring buffer write cursor offset.
    write_pos: usize,
    /// Number of unread bytes currently stored in the buffer.
    len: usize,
    /// Active open reader handle reference count.
    readers_open: usize,
    /// Active open writer handle reference count.
    writers_open: usize,
    /// Wait queue of blocked reading tasks.
    read_waiters: WaitQueue,
    /// Wait queue of blocked writing tasks.
    write_waiters: WaitQueue,
}

/// Inode representing the read end of an anonymous FIFO pipe.
pub struct PipeReadEnd {
    /// Reference to the shared ring buffer state.
    buf: Arc<SpinLock<PipeBuffer>>,
}

/// Inode representing the write end of an anonymous FIFO pipe.
pub struct PipeWriteEnd {
    /// Reference to the shared ring buffer state.
    buf: Arc<SpinLock<PipeBuffer>>,
}

impl PipeBuffer {
    /// Creates a new paired reader and writer inode connected to a shared pipe buffer.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> (Arc<PipeReadEnd>, Arc<PipeWriteEnd>) {
        let buffer = Arc::new(SpinLock::new(PipeBuffer {
            data: [0; PIPE_BUFFER_SIZE],
            read_pos: 0,
            write_pos: 0,
            len: 0,
            readers_open: 1,
            writers_open: 1,
            read_waiters: WaitQueue::new(),
            write_waiters: WaitQueue::new(),
        }));
        (
            Arc::new(PipeReadEnd {
                buf: buffer.clone(),
            }),
            Arc::new(PipeWriteEnd { buf: buffer }),
        )
    }
}

impl Inode for PipeReadEnd {
    fn file_type(&self) -> FileType {
        FileType::Fifo
    }

    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, i32> {
        self.read_with_flags(offset, buf, 0, 0)
    }

    fn read_with_flags(
        &self,
        _offset: usize,
        buf: &mut [u8],
        flags: i32,
        caller_pid: i32,
    ) -> Result<usize, i32> {
        loop {
            // Mark caller Blocked BEFORE acquiring pipe Inode lock (ADR-0001/0002 compliance)
            crate::services::scheduler::mark_current_blocked();

            let mut to_wake_writers = Vec::new();
            let (should_switch, bytes_read_opt) = {
                let mut pipe = self.buf.lock();
                if pipe.len == 0 {
                    if pipe.writers_open == 0 {
                        return Ok(0); // EOF
                    }
                    if flags & O_NONBLOCK != 0 {
                        return Err(EAGAIN);
                    }
                    // Register caller on read_waiters queue
                    pipe.read_waiters.add_waiter(caller_pid);

                    // Re-check condition under pipe lock to close lost-wakeup race
                    if pipe.len > 0 || pipe.writers_open == 0 {
                        pipe.read_waiters.remove_waiter(caller_pid);
                        (false, None)
                    } else {
                        (true, None)
                    }
                } else {
                    let was_full = pipe.len == PIPE_BUFFER_SIZE;
                    let to_read = buf.len().min(pipe.len);
                    for item in buf.iter_mut().take(to_read) {
                        *item = pipe.data[pipe.read_pos];
                        pipe.read_pos = (pipe.read_pos + 1) % PIPE_BUFFER_SIZE;
                    }
                    pipe.len -= to_read;

                    // Gate wakeup: only wake a writer if the pipe was full and gained space
                    if was_full
                        && pipe.len < PIPE_BUFFER_SIZE
                        && let Some(w) = pipe.write_waiters.drain_one()
                    {
                        to_wake_writers.push(w);
                    }

                    (false, Some(to_read))
                }
            }; // Pipe lock dropped before acquiring upper-tier locks (ADR-0002)

            if !to_wake_writers.is_empty() {
                crate::services::scheduler::wake_tasks(&to_wake_writers);
            }

            if let Some(n) = bytes_read_opt {
                crate::services::scheduler::mark_current_running();
                return Ok(n);
            }

            if should_switch {
                crate::services::scheduler::switch_out_current();
                crate::services::scheduler::mark_current_running();
                if crate::services::ipc::SIGNALS.has_unblocked_signals(caller_pid) {
                    self.buf.lock().read_waiters.remove_waiter(caller_pid);
                    return Err(EINTR);
                }
            } else {
                crate::services::scheduler::mark_current_running();
            }
        }
    }

    fn write(&self, _offset: usize, _buf: &[u8]) -> Result<usize, i32> {
        Err(EBADF)
    }
    fn lookup(&self, _name: &str) -> Result<Arc<dyn Inode>, i32> {
        Err(ENOTDIR)
    }
    fn readdir(&self) -> Result<Vec<Dirent64>, i32> {
        Err(ENOTDIR)
    }

    fn stat(&self) -> Result<Stat, i32> {
        Ok(Stat {
            st_mode: S_IFIFO | 0o600,
            ..Default::default()
        })
    }

    fn poll(&self) -> InodePollFlags {
        let pipe = self.buf.lock();
        InodePollFlags {
            readable: pipe.len > 0,
            writable: false,
            hangup: pipe.writers_open == 0,
            error: false,
        }
    }
}

impl Inode for PipeWriteEnd {
    fn file_type(&self) -> FileType {
        FileType::Fifo
    }

    fn read(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize, i32> {
        Err(EBADF)
    }

    fn write(&self, offset: usize, buf: &[u8]) -> Result<usize, i32> {
        self.write_with_flags(offset, buf, 0, 0)
    }

    fn write_with_flags(
        &self,
        _offset: usize,
        buf: &[u8],
        flags: i32,
        caller_pid: i32,
    ) -> Result<usize, i32> {
        loop {
            // Mark caller Blocked BEFORE acquiring pipe Inode lock (ADR-0001/0002 compliance)
            crate::services::scheduler::mark_current_blocked();

            let mut to_wake_readers = Vec::new();
            let (should_switch, bytes_written_opt) = {
                let mut pipe = self.buf.lock();
                if pipe.readers_open == 0 {
                    // Last reader closed: return -EPIPE (and trigger SIGPIPE once signal dispatch is wired)
                    return Err(EPIPE);
                }
                let space = PIPE_BUFFER_SIZE - pipe.len;
                if space == 0 {
                    if flags & O_NONBLOCK != 0 {
                        return Err(EAGAIN);
                    }
                    // Register caller on write_waiters queue
                    pipe.write_waiters.add_waiter(caller_pid);

                    // Re-check condition under pipe lock to close lost-wakeup race
                    if pipe.len < PIPE_BUFFER_SIZE || pipe.readers_open == 0 {
                        pipe.write_waiters.remove_waiter(caller_pid);
                        (false, None)
                    } else {
                        (true, None)
                    }
                } else {
                    let was_empty = pipe.len == 0;
                    let to_write = buf.len().min(space);
                    for &byte in buf.iter().take(to_write) {
                        let pos = pipe.write_pos;
                        pipe.data[pos] = byte;
                        pipe.write_pos = (pos + 1) % PIPE_BUFFER_SIZE;
                    }
                    pipe.len += to_write;

                    // Gate wakeup: only wake readers if the pipe was empty and gained data
                    if was_empty && pipe.len > 0 {
                        to_wake_readers = pipe.read_waiters.drain_all();
                    }

                    (false, Some(to_write))
                }
            }; // Pipe lock dropped before acquiring upper-tier locks (ADR-0002)

            if !to_wake_readers.is_empty() {
                crate::services::scheduler::wake_tasks(&to_wake_readers);
            }

            if let Some(n) = bytes_written_opt {
                crate::services::scheduler::mark_current_running();
                return Ok(n);
            }

            if should_switch {
                crate::services::scheduler::switch_out_current();
                crate::services::scheduler::mark_current_running();
                if crate::services::ipc::SIGNALS.has_unblocked_signals(caller_pid) {
                    self.buf.lock().write_waiters.remove_waiter(caller_pid);
                    return Err(EINTR);
                }
            } else {
                crate::services::scheduler::mark_current_running();
            }
        }
    }

    fn lookup(&self, _name: &str) -> Result<Arc<dyn Inode>, i32> {
        Err(ENOTDIR)
    }
    fn readdir(&self) -> Result<Vec<Dirent64>, i32> {
        Err(ENOTDIR)
    }

    fn stat(&self) -> Result<Stat, i32> {
        Ok(Stat {
            st_mode: S_IFIFO | 0o600,
            ..Default::default()
        })
    }

    fn poll(&self) -> InodePollFlags {
        let pipe = self.buf.lock();
        InodePollFlags {
            readable: false,
            writable: pipe.len < PIPE_BUFFER_SIZE,
            hangup: false,
            error: pipe.readers_open == 0,
        }
    }
}

impl Drop for PipeReadEnd {
    fn drop(&mut self) {
        let to_wake = {
            let mut pipe = self.buf.lock();
            pipe.readers_open = pipe.readers_open.saturating_sub(1);
            if pipe.readers_open == 0 {
                pipe.write_waiters.drain_all()
            } else {
                Vec::new()
            }
        }; // Pipe lock dropped before waking tasks (ADR-0002)
        if !to_wake.is_empty() {
            crate::services::scheduler::wake_tasks(&to_wake);
        }
    }
}

impl Drop for PipeWriteEnd {
    fn drop(&mut self) {
        let to_wake = {
            let mut pipe = self.buf.lock();
            pipe.writers_open = pipe.writers_open.saturating_sub(1);
            if pipe.writers_open == 0 {
                pipe.read_waiters.drain_all()
            } else {
                Vec::new()
            }
        }; // Pipe lock dropped before waking tasks (ADR-0002)
        if !to_wake.is_empty() {
            crate::services::scheduler::wake_tasks(&to_wake);
        }
    }
}
