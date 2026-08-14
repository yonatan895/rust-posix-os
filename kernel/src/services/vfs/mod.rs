//! Virtual File System (VFS) Layer - De-privileged Safe Service.

pub mod devfs;
pub mod epoll;
pub mod pipe;
pub mod procfs;
pub mod ramfs;
pub mod tar;

use crate::ostd::sync::SpinLock;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use posix_abi::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Regular,
    Directory,
    CharacterDevice,
    BlockDevice,
    Fifo,
    Socket,
    Symlink,
    Anonymous,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InodePollFlags {
    pub readable: bool,
    pub writable: bool,
    pub hangup: bool,
    pub error: bool,
}

pub trait Inode: Send + Sync {
    fn file_type(&self) -> FileType;
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, i32>;
    fn write(&self, offset: usize, buf: &[u8]) -> Result<usize, i32>;
    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, i32>;
    fn readdir(&self) -> Result<Vec<Dirent64>, i32>;
    fn stat(&self) -> Result<Stat, i32>;

    fn create_file(&self, _name: &str) -> Result<Arc<dyn Inode>, i32> {
        Err(ENOTDIR)
    }

    fn create_dir(&self, _name: &str) -> Result<Arc<dyn Inode>, i32> {
        Err(ENOTDIR)
    }

    fn unlink(&self, _name: &str) -> Result<(), i32> {
        Err(ENOTDIR)
    }

    fn truncate(&self) -> Result<(), i32> {
        Ok(())
    }

    fn link_entry(&self, _name: &str, _inode: Arc<dyn Inode>) -> Result<(), i32> {
        Err(ENOTDIR)
    }

    fn poll(&self) -> InodePollFlags {
        InodePollFlags {
            readable: true,
            writable: true,
            hangup: false,
            error: false,
        }
    }

    fn as_epoll(&self) -> Option<&crate::services::vfs::epoll::EpollInstance> {
        None
    }
}

pub struct FileHandle {
    pub inode: Arc<dyn Inode>,
    pub offset: SpinLock<usize>,
    pub flags: i32,
}

impl FileHandle {
    pub fn new(inode: Arc<dyn Inode>, flags: i32) -> Self {
        Self {
            inode,
            offset: SpinLock::new(0),
            flags,
        }
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize, i32> {
        let mut offset_guard = self.offset.lock();
        let bytes_read = self.inode.read(*offset_guard, buf)?;
        *offset_guard += bytes_read;
        Ok(bytes_read)
    }

    pub fn write(&self, buf: &[u8]) -> Result<usize, i32> {
        let mut offset_guard = self.offset.lock();
        if self.flags & O_APPEND != 0
            && let Ok(st) = self.inode.stat()
        {
            *offset_guard = st.st_size as usize;
        }
        let bytes_written = self.inode.write(*offset_guard, buf)?;
        *offset_guard += bytes_written;
        Ok(bytes_written)
    }

    pub fn lseek(&self, offset: i64, whence: i32) -> Result<i64, i32> {
        let mut offset_guard = self.offset.lock();
        let current = *offset_guard as i64;
        let new_offset = match whence {
            SEEK_SET => offset,
            SEEK_CUR => current + offset,
            SEEK_END => {
                let stat = self.inode.stat()?;
                stat.st_size + offset
            }
            _ => return Err(EINVAL),
        };

        if new_offset < 0 {
            return Err(EINVAL);
        }
        *offset_guard = new_offset as usize;
        Ok(new_offset)
    }
}

pub struct Vfs {
    pub root: Arc<dyn Inode>,
}

pub static ROOT_VFS: SpinLock<Option<Vfs>> = SpinLock::new(None);

pub fn vfs_init(root: Arc<dyn Inode>) {
    *ROOT_VFS.lock() = Some(Vfs { root });
}

pub fn get_current_process_cwd() -> String {
    if let Some(p) = crate::services::process::get_current_process() {
        p.lock().cwd.clone()
    } else {
        "/".to_string()
    }
}

pub fn normalize_path(cwd: &str, path: &str) -> String {
    let mut components: Vec<&str> = Vec::new();

    let full_path = if path.starts_with('/') {
        path.to_string()
    } else if cwd == "/" {
        alloc::format!("/{}", path)
    } else {
        alloc::format!("{}/{}", cwd, path)
    };

    for part in full_path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        } else if part == ".." {
            components.pop();
        } else {
            components.push(part);
        }
    }

    if components.is_empty() {
        "/".to_string()
    } else {
        let mut res = String::new();
        for c in components {
            res.push('/');
            res.push_str(c);
        }
        res
    }
}

pub fn resolve_path_with_cwd(cwd: &str, path: &str) -> Result<Arc<dyn Inode>, i32> {
    let norm = normalize_path(cwd, path);

    let vfs_guard = ROOT_VFS.lock();
    let vfs = vfs_guard.as_ref().ok_or(ENODEV)?;

    if norm == "/" {
        return Ok(vfs.root.clone());
    }

    let mut current = vfs.root.clone();
    for component in norm.split('/') {
        if component.is_empty() {
            continue;
        }
        current = current.lookup(component)?;
    }
    Ok(current)
}

pub fn resolve_parent_and_basename_with_cwd(
    cwd: &str,
    path: &str,
) -> Result<(Arc<dyn Inode>, String), i32> {
    let norm = normalize_path(cwd, path);

    if norm == "/" {
        return Err(EINVAL);
    }

    let mut parts: Vec<&str> = norm.split('/').filter(|s| !s.is_empty()).collect();
    let basename = parts.pop().ok_or(EINVAL)?.to_string();

    let vfs_guard = ROOT_VFS.lock();
    let vfs = vfs_guard.as_ref().ok_or(ENODEV)?;
    let mut current = vfs.root.clone();

    for component in parts {
        current = current.lookup(component)?;
    }

    Ok((current, basename))
}

pub fn resolve_path(path: &str) -> Result<Arc<dyn Inode>, i32> {
    let cwd = get_current_process_cwd();
    resolve_path_with_cwd(&cwd, path)
}

pub fn resolve_parent_and_basename(path: &str) -> Result<(Arc<dyn Inode>, String), i32> {
    let cwd = get_current_process_cwd();
    resolve_parent_and_basename_with_cwd(&cwd, path)
}
