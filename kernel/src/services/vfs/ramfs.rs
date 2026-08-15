//! RamFs - Safe In-Memory Filesystem.

use super::{FileType, Inode};
use crate::ostd::sync::SpinLock;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use posix_abi::*;

/// In-memory directory inode storing entries and subdirectories in memory maps.
pub struct RamFsDir {
    /// Mapping of entry names to child Inode trait objects.
    pub entries: SpinLock<BTreeMap<String, Arc<dyn Inode>>>,
    /// Mapping of entry names to strongly-typed child directory inodes.
    pub subdirs: SpinLock<BTreeMap<String, Arc<RamFsDir>>>,
    /// POSIX file permission mode bits.
    pub mode: SpinLock<u16>,
    /// Owner user ID.
    pub uid: SpinLock<u32>,
    /// Owner group ID.
    pub gid: SpinLock<u32>,
}

impl RamFsDir {
    /// Creates a new directory inode with default `0755` permissions owned by root.
    pub fn new() -> Arc<Self> {
        Self::new_with_creds(0o755, 0, 0)
    }

    /// Creates a new directory inode with explicit permission mode and ownership credentials.
    pub fn new_with_creds(mode: u16, uid: u32, gid: u32) -> Arc<Self> {
        Arc::new(Self {
            entries: SpinLock::new(BTreeMap::new()),
            subdirs: SpinLock::new(BTreeMap::new()),
            mode: SpinLock::new(mode),
            uid: SpinLock::new(uid),
            gid: SpinLock::new(gid),
        })
    }

    /// Adds a child inode under `name` to this directory.
    pub fn add_child(&self, name: &str, inode: Arc<dyn Inode>) {
        self.entries.lock().insert(name.to_string(), inode);
    }

    /// Retrieves an existing subdirectory or creates and inserts a new one if absent.
    pub fn get_or_create_subdir(&self, name: &str) -> Arc<RamFsDir> {
        let mut subdirs = self.subdirs.lock();
        if let Some(dir) = subdirs.get(name) {
            return dir.clone();
        }
        let new_dir = RamFsDir::new();
        subdirs.insert(name.to_string(), new_dir.clone());
        self.entries
            .lock()
            .insert(name.to_string(), new_dir.clone());
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
            let d_type = match child.file_type() {
                FileType::Directory => DT_DIR,
                FileType::CharacterDevice => DT_CHR,
                FileType::Fifo => DT_FIFO,
                _ => DT_REG,
            };
            list.push(super::create_dirent(name, d_type));
        }
        Ok(list)
    }

    fn stat(&self) -> Result<Stat, i32> {
        Ok(Stat {
            st_mode: S_IFDIR | (*self.mode.lock() as u32),
            st_uid: *self.uid.lock(),
            st_gid: *self.gid.lock(),
            ..Default::default()
        })
    }

    fn create_file(
        &self,
        name: &str,
        mode: u16,
        uid: u32,
        gid: u32,
    ) -> Result<Arc<dyn Inode>, i32> {
        let mut entries = self.entries.lock();
        if let Some(existing) = entries.get(name) {
            return Ok(existing.clone());
        }
        let file = RamFsFile::new_with_creds(Vec::new(), mode, uid, gid);
        entries.insert(name.to_string(), file.clone());
        Ok(file)
    }

    fn create_dir(&self, name: &str, mode: u16, uid: u32, gid: u32) -> Result<Arc<dyn Inode>, i32> {
        let mut entries = self.entries.lock();
        if entries.contains_key(name) {
            return Err(EEXIST);
        }
        let new_dir = RamFsDir::new_with_creds(mode, uid, gid);
        self.subdirs
            .lock()
            .insert(name.to_string(), new_dir.clone());
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

    fn link_entry(&self, name: &str, inode: Arc<dyn Inode>) -> Result<(), i32> {
        let mut entries = self.entries.lock();
        if entries.contains_key(name) {
            entries.remove(name);
            self.subdirs.lock().remove(name);
        }
        entries.insert(name.to_string(), inode);
        Ok(())
    }
}

/// In-memory regular file inode backed by a dynamic byte vector.
pub struct RamFsFile {
    /// File payload buffer guarded by a spinlock.
    pub data: SpinLock<Vec<u8>>,
    /// POSIX file permission mode bits.
    pub mode: SpinLock<u16>,
    /// Owner user ID.
    pub uid: SpinLock<u32>,
    /// Owner group ID.
    pub gid: SpinLock<u32>,
}

impl RamFsFile {
    /// Creates a new regular file with initial data, mode `0644`, and root ownership.
    pub fn new(initial_data: Vec<u8>) -> Arc<Self> {
        Self::new_with_creds(initial_data, 0o644, 0, 0)
    }

    /// Creates a new regular file with initial data, explicit mode, and ownership credentials.
    pub fn new_with_creds(initial_data: Vec<u8>, mode: u16, uid: u32, gid: u32) -> Arc<Self> {
        Arc::new(Self {
            data: SpinLock::new(initial_data),
            mode: SpinLock::new(mode),
            uid: SpinLock::new(uid),
            gid: SpinLock::new(gid),
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
        Ok(Stat {
            st_mode: S_IFREG | (*self.mode.lock() as u32),
            st_uid: *self.uid.lock(),
            st_gid: *self.gid.lock(),
            st_size: self.data.lock().len() as i64,
            ..Default::default()
        })
    }

    fn truncate(&self) -> Result<(), i32> {
        let mut data = self.data.lock();
        data.clear();
        Ok(())
    }
}
