//! POSIX Anonymous Pipe Implementation - De-privileged Safe Service.

use super::wait_queue::WaitQueue;
use super::{FileType, Inode, InodePollFlags};
use crate::ostd::sync::SpinLock;
use alloc::sync::Arc;
use alloc::vec::Vec;
use posix_abi::*;

pub const PIPE_BUFFER_SIZE: usize = 4096;

pub struct PipeBuffer {
    data: [u8; PIPE_BUFFER_SIZE],
    read_pos: usize,
    write_pos: usize,
    len: usize,
    readers_open: usize,
    writers_open: usize,
    pub read_waiters: WaitQueue,
    pub write_waiters: WaitQueue,
}

pub struct PipeReadEnd {
    buf: Arc<SpinLock<PipeBuffer>>,
}

pub struct PipeWriteEnd {
    buf: Arc<SpinLock<PipeBuffer>>,
}

impl PipeBuffer {
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
        self.read_with_flags(offset, buf, 0)
    }

    fn read_with_flags(&self, _offset: usize, buf: &mut [u8], flags: i32) -> Result<usize, i32> {
        let mut pipe = self.buf.lock();
        if pipe.len == 0 {
            if pipe.writers_open == 0 {
                return Ok(0); // EOF
            }
            if flags & O_NONBLOCK != 0 {
                return Err(EAGAIN);
            }
            // Blocking wait: register on read_waiters queue
            if let Some(proc) = crate::services::process::get_current_process() {
                let pid = proc.lock().pid;
                pipe.read_waiters.add_waiter(pid);
            }
            // Cooperative placeholder for blocking path until task schedule-out (Issue #26)
            return Err(EAGAIN);
        }

        let to_read = buf.len().min(pipe.len);
        for item in buf.iter_mut().take(to_read) {
            *item = pipe.data[pipe.read_pos];
            pipe.read_pos = (pipe.read_pos + 1) % PIPE_BUFFER_SIZE;
        }
        pipe.len -= to_read;

        // Wake write waiters after freeing space
        pipe.write_waiters.wake_all();

        Ok(to_read)
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
        self.write_with_flags(offset, buf, 0)
    }

    fn write_with_flags(&self, _offset: usize, buf: &[u8], flags: i32) -> Result<usize, i32> {
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
            // Blocking wait: register on write_waiters queue
            if let Some(proc) = crate::services::process::get_current_process() {
                let pid = proc.lock().pid;
                pipe.write_waiters.add_waiter(pid);
            }
            // Cooperative placeholder for blocking path until task schedule-out (Issue #26)
            return Err(EAGAIN);
        }

        let to_write = buf.len().min(space);
        for &byte in buf.iter().take(to_write) {
            let pos = pipe.write_pos;
            pipe.data[pos] = byte;
            pipe.write_pos = (pos + 1) % PIPE_BUFFER_SIZE;
        }
        pipe.len += to_write;

        // Wake read waiters after producing data
        pipe.read_waiters.wake_all();

        Ok(to_write)
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
        let mut pipe = self.buf.lock();
        pipe.readers_open = pipe.readers_open.saturating_sub(1);
        if pipe.readers_open == 0 {
            pipe.write_waiters.wake_all();
        }
    }
}

impl Drop for PipeWriteEnd {
    fn drop(&mut self) {
        let mut pipe = self.buf.lock();
        pipe.writers_open = pipe.writers_open.saturating_sub(1);
        if pipe.writers_open == 0 {
            pipe.read_waiters.wake_all();
        }
    }
}
