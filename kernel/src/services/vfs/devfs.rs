//! DevFs - Special Device Nodes (/dev/null, /dev/zero, /dev/tty, /dev/console).

use super::{FileType, Inode};
use crate::ostd::drivers::serial::SERIAL1;
use alloc::sync::Arc;
use alloc::vec::Vec;
use posix_abi::*;

/// Special `/dev/null` character device discarding all writes and returning EOF on reads.
pub struct DevNull;

impl Inode for DevNull {
    fn file_type(&self) -> FileType {
        FileType::CharacterDevice
    }
    fn read(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize, i32> {
        Ok(0)
    }
    fn write(&self, _offset: usize, buf: &[u8]) -> Result<usize, i32> {
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
            st_mode: S_IFCHR | 0o666,
            ..Default::default()
        })
    }
}

/// Special `/dev/zero` character device returning zero bytes on read and discarding writes.
pub struct DevZero;

impl Inode for DevZero {
    fn file_type(&self) -> FileType {
        FileType::CharacterDevice
    }
    fn read(&self, _offset: usize, buf: &mut [u8]) -> Result<usize, i32> {
        buf.fill(0);
        Ok(buf.len())
    }
    fn write(&self, _offset: usize, buf: &[u8]) -> Result<usize, i32> {
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
            st_mode: S_IFCHR | 0o666,
            ..Default::default()
        })
    }
}

/// Special `/dev/console` and `/dev/tty` character device connected to the serial COM1 driver.
pub struct DevConsole;

impl Inode for DevConsole {
    fn file_type(&self) -> FileType {
        FileType::CharacterDevice
    }

    fn read(&self, _offset: usize, buf: &mut [u8]) -> Result<usize, i32> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            let count = {
                let mut serial = SERIAL1.lock();
                serial.read_bytes(buf)
            };
            if count > 0 {
                return Ok(count);
            }
            core::hint::spin_loop();
        }
    }

    fn write(&self, _offset: usize, buf: &[u8]) -> Result<usize, i32> {
        let serial = SERIAL1.lock();
        for &byte in buf {
            serial.write_byte(byte);
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
            st_mode: S_IFCHR | 0o666,
            ..Default::default()
        })
    }
}
