//! TTY Subsystem & Line Discipline - De-privileged Safe Service.

use crate::ostd::sync::SpinLock;
use crate::services::vfs::{FileType, Inode};
use alloc::sync::Arc;
use alloc::vec::Vec;
use posix_abi::*;

pub struct LineDiscipline {
    pub input_buffer: Vec<u8>,
    pub canonical_queue: Vec<u8>,
    pub termios: Termios,
}

impl LineDiscipline {
    pub fn new() -> Self {
        let termios = Termios {
            c_lflag: ECHO | ICANON | ISIG,
            ..Default::default()
        };
        Self {
            input_buffer: Vec::with_capacity(256),
            canonical_queue: Vec::with_capacity(1024),
            termios,
        }
    }

    pub fn push_char(&mut self, c: u8) -> Option<u8> {
        let is_canon = (self.termios.c_lflag & ICANON) != 0;
        let is_echo = (self.termios.c_lflag & ECHO) != 0;

        if is_canon {
            if c == 0x08 || c == 0x7F {
                // Backspace / Delete
                if self.input_buffer.pop().is_some() && is_echo {
                    Some(0x08)
                } else {
                    None
                }
            } else if c == b'\r' || c == b'\n' {
                self.input_buffer.push(b'\n');
                self.canonical_queue.extend_from_slice(&self.input_buffer);
                self.input_buffer.clear();
                if is_echo {
                    Some(b'\n')
                } else {
                    None
                }
            } else {
                self.input_buffer.push(c);
                if is_echo {
                    Some(c)
                } else {
                    None
                }
            }
        } else {
            self.canonical_queue.push(c);
            if is_echo {
                Some(c)
            } else {
                None
            }
        }
    }
}

impl Default for LineDiscipline {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TtyDevice {
    pub ldisc: SpinLock<LineDiscipline>,
}

impl TtyDevice {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            ldisc: SpinLock::new(LineDiscipline::new()),
        })
    }
}

impl Inode for TtyDevice {
    fn file_type(&self) -> FileType {
        FileType::CharacterDevice
    }

    fn read(&self, _offset: usize, buf: &mut [u8]) -> Result<usize, i32> {
        let mut ldisc = self.ldisc.lock();
        if ldisc.canonical_queue.is_empty() {
            return Err(EAGAIN);
        }
        let to_read = buf.len().min(ldisc.canonical_queue.len());
        for item in buf.iter_mut().take(to_read) {
            *item = ldisc.canonical_queue.remove(0);
        }
        Ok(to_read)
    }

    fn write(&self, _offset: usize, buf: &[u8]) -> Result<usize, i32> {
        let serial = crate::ostd::drivers::serial::SERIAL1.lock();
        for &b in buf {
            serial.write_byte(b);
        }
        Ok(buf.len())
    }

    fn lookup(&self, _name: &str) -> Result<Arc<dyn Inode>, i32> {
        Err(ENOTDIR)
    }
    fn readdir(&self) -> Result<Vec<Dirent64>, i32> {
        Err(ENOTDIR)
    }

    fn stat(&self) -> Result<Stat, i32> {
        Ok(Stat {
            st_mode: S_IFCHR | 0o620,
            ..Default::default()
        })
    }
}
