//! RamFs - Safe In-Memory Filesystem.

use super::{FileType, Inode};
use crate::ostd::sync::SpinLock;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use posix_abi::*;

/// In-memory directory inode storing entries and subdirectories in memory maps.
pub struct RamFsDir {
    /// Mapping of entry names to child Inode trait objects.
    pub entries: SpinLock<BTreeMap<String, Arc<dyn Inode>>>,
    /// Mapping of entry names to strongly-typed child directory inodes.
    pub subdirs: SpinLock<BTreeMap<String, Arc<RamFsDir>>>,
    /// Atomic count of directory entries for lock-free emptiness checks (ADR-0002 L6).
    pub entry_count: AtomicUsize,
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
            entry_count: AtomicUsize::new(0),
            mode: SpinLock::new(mode),
            uid: SpinLock::new(uid),
            gid: SpinLock::new(gid),
        })
    }

    /// Adds a child inode under `name` to this directory.
    pub fn add_child(&self, name: &str, inode: Arc<dyn Inode>) {
        let mut entries = self.entries.lock();
        if entries.insert(name.to_string(), inode).is_none() {
            self.entry_count.fetch_add(1, Ordering::Release);
        }
    }

    /// Retrieves an existing subdirectory or creates and inserts a new one if absent.
    pub fn get_or_create_subdir(&self, name: &str) -> Arc<RamFsDir> {
        let mut entries = self.entries.lock();
        let mut subdirs = self.subdirs.lock();
        if let Some(dir) = subdirs.get(name) {
            return dir.clone();
        }
        let new_dir = RamFsDir::new();
        subdirs.insert(name.to_string(), new_dir.clone());
        if entries.insert(name.to_string(), new_dir.clone()).is_none() {
            self.entry_count.fetch_add(1, Ordering::Release);
        }
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
        self.entry_count.fetch_add(1, Ordering::Release);
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
        self.entry_count.fetch_add(1, Ordering::Release);
        Ok(new_dir)
    }

    fn unlink(&self, name: &str) -> Result<(), i32> {
        let mut entries = self.entries.lock();
        if entries.remove(name).is_some() {
            self.subdirs.lock().remove(name);
            self.entry_count.fetch_sub(1, Ordering::Release);
            Ok(())
        } else {
            Err(ENOENT)
        }
    }

    fn link_entry(&self, name: &str, inode: Arc<dyn Inode>) -> Result<(), i32> {
        let mut entries = self.entries.lock();
        let was_absent = entries.insert(name.to_string(), inode).is_none();
        if was_absent {
            self.entry_count.fetch_add(1, Ordering::Release);
        } else {
            self.subdirs.lock().remove(name);
        }
        Ok(())
    }

    fn as_ramfs_dir(&self) -> Option<&RamFsDir> {
        Some(self)
    }

    fn rename(
        &self,
        old_name: &str,
        new_parent: &Arc<dyn Inode>,
        new_name: &str,
    ) -> Result<(), i32> {
        let new_ramfs_dir = new_parent.as_ramfs_dir().ok_or(EXDEV)?;

        if core::ptr::eq(self, new_ramfs_dir) {
            // Same directory rename: hold a single lock on entries and subdirs throughout
            let mut entries = self.entries.lock();
            let mut subdirs = self.subdirs.lock();

            let source = entries.get(old_name).cloned().ok_or(ENOENT)?;
            if old_name == new_name {
                return Ok(());
            }

            if let Some(target) = entries.get(new_name) {
                if source.file_type() == FileType::Directory
                    && target.file_type() != FileType::Directory
                {
                    return Err(ENOTDIR);
                }
                if source.file_type() != FileType::Directory
                    && target.file_type() == FileType::Directory
                {
                    return Err(EISDIR);
                }
                if source.file_type() == FileType::Directory
                    && target.file_type() == FileType::Directory
                {
                    if let Some(target_dir) = target.as_ramfs_dir() {
                        // Check for alias (self) or non-empty directory via atomic count lock-free
                        if core::ptr::eq(self, target_dir)
                            || target_dir.entry_count.load(Ordering::Acquire) > 0
                        {
                            return Err(ENOTEMPTY);
                        }
                    } else if !target.readdir()?.is_empty() {
                        return Err(ENOTEMPTY);
                    }
                }
            }

            let inode = entries.remove(old_name).unwrap();
            let subdir = subdirs.remove(old_name);

            let replaced = entries.remove(new_name).is_some();
            subdirs.remove(new_name);

            entries.insert(new_name.to_string(), inode);
            if let Some(sd) = subdir {
                subdirs.insert(new_name.to_string(), sd);
            }
            if replaced {
                self.entry_count.fetch_sub(1, Ordering::Release);
            }
            Ok(())
        } else {
            // Cross directory rename: acquire locks in pointer address order to eliminate AB-BA deadlock (ADR-0002 L4)
            let self_addr = self as *const _ as usize;
            let new_addr = new_ramfs_dir as *const _ as usize;

            let (mut old_entries, mut new_entries) = if self_addr < new_addr {
                let g1 = self.entries.lock();
                let g2 = new_ramfs_dir.entries.lock();
                (g1, g2)
            } else {
                let g2 = new_ramfs_dir.entries.lock();
                let g1 = self.entries.lock();
                (g1, g2)
            };

            let (mut old_subdirs, mut new_subdirs) = if self_addr < new_addr {
                let g1 = self.subdirs.lock();
                let g2 = new_ramfs_dir.subdirs.lock();
                (g1, g2)
            } else {
                let g2 = new_ramfs_dir.subdirs.lock();
                let g1 = self.subdirs.lock();
                (g1, g2)
            };

            let source = old_entries.get(old_name).cloned().ok_or(ENOENT)?;

            if let Some(target) = new_entries.get(new_name) {
                if source.file_type() == FileType::Directory
                    && target.file_type() != FileType::Directory
                {
                    return Err(ENOTDIR);
                }
                if source.file_type() != FileType::Directory
                    && target.file_type() == FileType::Directory
                {
                    return Err(EISDIR);
                }
                if source.file_type() == FileType::Directory
                    && target.file_type() == FileType::Directory
                {
                    if let Some(target_dir) = target.as_ramfs_dir() {
                        // Check for aliases (self or new_parent) or non-empty directory via atomic count lock-free
                        if core::ptr::eq(self, target_dir)
                            || core::ptr::eq(new_ramfs_dir, target_dir)
                            || target_dir.entry_count.load(Ordering::Acquire) > 0
                        {
                            return Err(ENOTEMPTY);
                        }
                    } else if !target.readdir()?.is_empty() {
                        return Err(ENOTEMPTY);
                    }
                }
            }

            let inode = old_entries.remove(old_name).unwrap();
            let subdir = old_subdirs.remove(old_name);
            self.entry_count.fetch_sub(1, Ordering::Release);

            let replaced = new_entries.remove(new_name).is_some();
            new_subdirs.remove(new_name);

            new_entries.insert(new_name.to_string(), inode);
            if let Some(sd) = subdir {
                new_subdirs.insert(new_name.to_string(), sd);
            }
            if !replaced {
                new_ramfs_dir.entry_count.fetch_add(1, Ordering::Release);
            }
            Ok(())
        }
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
