//! RamFs - Safe In-Memory Filesystem.

use super::{FileType, Inode};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use posix_abi::*;
use crate::ostd::sync::SpinLock;

pub struct RamFsDir {
    pub entries: SpinLock<BTreeMap<String, Arc<dyn Inode>>>,
    pub subdirs: SpinLock<BTreeMap<String, Arc<RamFsDir>>>,
}

impl RamFsDir {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            entries: SpinLock::new(BTreeMap::new()),
            subdirs: SpinLock::new(BTreeMap::new()),
        })
    }

    pub fn add_child(&self, name: &str, inode: Arc<dyn Inode>) {
        self.entries.lock().insert(name.to_string(), inode);
    }

    pub fn get_or_create_subdir(&self, name: &str) -> Arc<RamFsDir> {
        let mut subdirs = self.subdirs.lock();
        if let Some(dir) = subdirs.get(name) {
            return dir.clone();
        }
        let new_dir = RamFsDir::new();
        subdirs.insert(name.to_string(), new_dir.clone());
        self.entries.lock().insert(name.to_string(), new_dir.clone());
        new_dir
    }
}

impl Inode for RamFsDir {
    fn file_type(&self) -> FileType {
        FileType::Directory
    }

    fn read(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize, i32> {
        Err(EISDIR)
    }

    fn write(&self, _offset: usize, _buf: &[u8]) -> Result<usize, i32> {
        Err(EISDIR)
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, i32> {
        self.entries.lock().get(name).cloned().ok_or(ENOENT)
    }

    fn readdir(&self) -> Result<Vec<Dirent64>, i32> {
        let entries = self.entries.lock();
        let mut list = Vec::new();
        for (name, child) in entries.iter() {
            let mut dirent = Dirent64::default();
            dirent.d_type = match child.file_type() {
                FileType::Directory => DT_DIR,
                FileType::CharacterDevice => DT_CHR,
                FileType::Fifo => DT_FIFO,
                _ => DT_REG,
            };
            let bytes = name.as_bytes();
            let len = bytes.len().min(dirent.d_name.len() - 1);
            dirent.d_name[..len].copy_from_slice(&bytes[..len]);
            dirent.d_name[len] = 0;
            list.push(dirent);
        }
        Ok(list)
    }

    fn stat(&self) -> Result<Stat, i32> {
        let mut s = Stat::default();
        s.st_mode = S_IFDIR | 0o755;
        Ok(s)
    }

    fn create_file(&self, name: &str) -> Result<Arc<dyn Inode>, i32> {
        let mut entries = self.entries.lock();
        if let Some(existing) = entries.get(name) {
            return Ok(existing.clone());
        }
        let file = RamFsFile::new(Vec::new());
        entries.insert(name.to_string(), file.clone());
        Ok(file)
    }

    fn create_dir(&self, name: &str) -> Result<Arc<dyn Inode>, i32> {
        let mut entries = self.entries.lock();
        if entries.contains_key(name) {
            return Err(EEXIST);
        }
        let new_dir = RamFsDir::new();
        self.subdirs.lock().insert(name.to_string(), new_dir.clone());
        entries.insert(name.to_string(), new_dir.clone());
        Ok(new_dir)
    }

    fn unlink(&self, name: &str) -> Result<(), i32> {
        let mut entries = self.entries.lock();
        if entries.remove(name).is_some() {
            self.subdirs.lock().remove(name);
            Ok(())
        } else {
            Err(ENOENT)
        }
    }
}

pub struct RamFsFile {
    pub data: SpinLock<Vec<u8>>,
}

impl RamFsFile {
    pub fn new(initial_data: Vec<u8>) -> Arc<Self> {
        Arc::new(Self {
            data: SpinLock::new(initial_data),
        })
    }
}

impl Inode for RamFsFile {
    fn file_type(&self) -> FileType {
        FileType::Regular
    }

    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, i32> {
        let data = self.data.lock();
        if offset >= data.len() {
            return Ok(0);
        }
        let available = data.len() - offset;
        let to_copy = buf.len().min(available);
        buf[..to_copy].copy_from_slice(&data[offset..offset + to_copy]);
        Ok(to_copy)
    }

    fn write(&self, offset: usize, buf: &[u8]) -> Result<usize, i32> {
        let mut data = self.data.lock();
        if offset + buf.len() > data.len() {
            data.resize(offset + buf.len(), 0);
        }
        data[offset..offset + buf.len()].copy_from_slice(buf);
        Ok(buf.len())
    }

    fn lookup(&self, _name: &str) -> Result<Arc<dyn Inode>, i32> {
        Err(ENOTDIR)
    }

    fn readdir(&self) -> Result<Vec<Dirent64>, i32> {
        Err(ENOTDIR)
    }

    fn stat(&self) -> Result<Stat, i32> {
        let mut s = Stat::default();
        s.st_mode = S_IFREG | 0o644;
        s.st_size = self.data.lock().len() as i64;
        Ok(s)
    }

    fn truncate(&self) -> Result<(), i32> {
        let mut data = self.data.lock();
        data.clear();
        Ok(())
    }
}
