//! Virtual File System (VFS) Layer - De-privileged Safe Service.

pub mod devfs;
pub mod epoll;
pub mod pipe;
pub mod procfs;
pub mod ramfs;
pub mod tar;
pub mod wait_queue;

use crate::ostd::sync::SpinLock;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use posix_abi::*;

/// Categorical file type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// Standard regular file.
    Regular,
    /// Directory container node.
    Directory,
    /// Unbuffered character device.
    CharacterDevice,
    /// Block-addressable storage device.
    BlockDevice,
    /// Named pipe (FIFO).
    Fifo,
    /// UNIX domain or network communication socket.
    Socket,
    /// Symbolic path link.
    Symlink,
    /// Anonymous or kernel-internal pseudo inode.
    Anonymous,
}

/// Readiness flags reported when polling an inode for asynchronous I/O events.
#[derive(Debug, Clone, Copy, Default)]
pub struct InodePollFlags {
    /// Ready for reading without blocking.
    pub readable: bool,
    /// Ready for writing without blocking.
    pub writable: bool,
    /// Peer hung up or closed the channel.
    pub hangup: bool,
    /// Error condition occurred.
    pub error: bool,
}

/// Constructs a 64-bit directory entry struct with the specified name and type flag.
pub fn create_dirent(name: &str, d_type: u8) -> Dirent64 {
    let mut dirent = Dirent64 {
        d_type,
        ..Default::default()
    };
    let bytes = name.as_bytes();
    let len = bytes.len().min(dirent.d_name.len() - 1);
    dirent.d_name[..len].copy_from_slice(&bytes[..len]);
    dirent.d_name[len] = 0;
    dirent
}

/// Core abstraction for all file system nodes and pseudo-devices in the VFS.
pub trait Inode: Send + Sync {
    /// Returns the file type of this inode.
    fn file_type(&self) -> FileType;
    /// Reads up to `buf.len()` bytes starting at `offset`.
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, i32>;
    /// Writes up to `buf.len()` bytes starting at `offset`.
    fn write(&self, offset: usize, buf: &[u8]) -> Result<usize, i32>;
    /// Looks up a child node by filename within a directory inode.
    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, i32>;
    /// Enumerates directory contents into a vector of directory entries.
    fn readdir(&self) -> Result<Vec<Dirent64>, i32>;
    /// Retrieves file metadata and status attributes.
    fn stat(&self) -> Result<Stat, i32>;

    /// Reads with additional open flags (e.g. `O_NONBLOCK`) and calling process identification.
    fn read_with_flags(
        &self,
        offset: usize,
        buf: &mut [u8],
        _flags: i32,
        _caller_pid: i32,
    ) -> Result<usize, i32> {
        self.read(offset, buf)
    }

    /// Writes with additional open flags (e.g. `O_NONBLOCK`) and calling process identification.
    fn write_with_flags(
        &self,
        offset: usize,
        buf: &[u8],
        _flags: i32,
        _caller_pid: i32,
    ) -> Result<usize, i32> {
        self.write(offset, buf)
    }

    /// Creates a new regular file within a directory inode.
    fn create_file(
        &self,
        _name: &str,
        _mode: u16,
        _uid: u32,
        _gid: u32,
    ) -> Result<Arc<dyn Inode>, i32> {
        Err(ENOTDIR)
    }

    /// Creates a new child directory within a directory inode.
    fn create_dir(
        &self,
        _name: &str,
        _mode: u16,
        _uid: u32,
        _gid: u32,
    ) -> Result<Arc<dyn Inode>, i32> {
        Err(ENOTDIR)
    }

    /// Removes a named entry from a directory inode.
    fn unlink(&self, _name: &str) -> Result<(), i32> {
        Err(ENOTDIR)
    }

    /// Truncates the file content to 0 bytes.
    fn truncate(&self) -> Result<(), i32> {
        Ok(())
    }

    /// Links an existing inode into this directory under `name`.
    fn link_entry(&self, _name: &str, _inode: Arc<dyn Inode>) -> Result<(), i32> {
        Err(ENOTDIR)
    }

    /// Renames a child entry `old_name` within this directory into `new_name` in `new_parent`.
    ///
    /// Defaults to returning `-EROFS` (read-only filesystem) for filesystems or pseudo-nodes
    /// that do not support directory mutation (e.g. devfs, procfs).
    fn rename(
        &self,
        _old_name: &str,
        _new_parent: &Arc<dyn Inode>,
        _new_name: &str,
    ) -> Result<(), i32> {
        Err(EROFS)
    }

    /// Polls the inode for readiness flags without blocking.
    fn poll(&self) -> InodePollFlags {
        InodePollFlags {
            readable: true,
            writable: true,
            hangup: false,
            error: false,
        }
    }

    /// Downcasts the inode to an epoll monitor instance if applicable.
    fn as_epoll(&self) -> Option<&crate::services::vfs::epoll::EpollInstance> {
        None
    }

    /// Downcasts the inode to a RAM filesystem directory if applicable.
    ///
    /// This RTTI-style downcast is a pragmatic mechanism for the single-mount
    /// ramfs writable filesystem and will be superseded when generic multi-filesystem
    /// writable abstraction (#73) is introduced.
    fn as_ramfs_dir(&self) -> Option<&crate::services::vfs::ramfs::RamFsDir> {
        None
    }
}

/// Open file description tracking an inode, seek offset, and access mode flags.
pub struct FileHandle {
    /// Underlying VFS inode.
    pub inode: Arc<dyn Inode>,
    /// Current read/write file offset guarded by a spinlock.
    pub offset: SpinLock<usize>,
    /// Open status and access mode flags (e.g. `O_RDWR`, `O_NONBLOCK`).
    pub flags: i32,
}

impl FileHandle {
    /// Creates a new file handle pointing to `inode` with the given access flags.
    pub fn new(inode: Arc<dyn Inode>, flags: i32) -> Self {
        Self {
            inode,
            offset: SpinLock::new(0),
            flags,
        }
    }

    /// Reads data from the file handle advancing its internal offset.
    pub fn read(&self, buf: &mut [u8], caller_pid: i32) -> Result<usize, i32> {
        let mut offset_guard = self.offset.lock();
        let bytes_read = self
            .inode
            .read_with_flags(*offset_guard, buf, self.flags, caller_pid)?;
        *offset_guard += bytes_read;
        Ok(bytes_read)
    }

    /// Writes data to the file handle advancing its internal offset.
    pub fn write(&self, buf: &[u8], caller_pid: i32) -> Result<usize, i32> {
        let mut offset_guard = self.offset.lock();
        if self.flags & O_APPEND != 0
            && let Ok(st) = self.inode.stat()
        {
            *offset_guard = st.st_size as usize;
        }
        let bytes_written =
            self.inode
                .write_with_flags(*offset_guard, buf, self.flags, caller_pid)?;
        *offset_guard += bytes_written;
        Ok(bytes_written)
    }

    /// Adjusts the file handle's read/write offset according to `whence`.
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

/// Process credentials snapshot passed into VFS operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VfsCreds {
    /// Real user identifier.
    pub uid: u32,
    /// Real group identifier.
    pub gid: u32,
    /// Effective user identifier.
    pub euid: u32,
    /// Effective group identifier.
    pub egid: u32,
    /// File mode creation mask.
    pub umask: u32,
}

impl VfsCreds {
    /// Creates a new credentials snapshot.
    pub const fn new(uid: u32, gid: u32, euid: u32, egid: u32, umask: u32) -> Self {
        Self {
            uid,
            gid,
            euid,
            egid,
            umask,
        }
    }

    /// Computes the effective file creation mode by masking requested `mode` with `!umask`.
    #[inline(always)]
    pub fn creation_mode(&self, mode: u32) -> u16 {
        ((mode as u16) & 0o777) & !(self.umask as u16)
    }

    /// Returns `true` if the caller has root/superuser privileges (`euid == 0`).
    #[inline(always)]
    pub fn is_root(&self) -> bool {
        self.euid == 0
    }
}

/// Checks access permission on `inode` according to open `flags` and caller `creds`.
pub fn check_inode_permission(inode: &dyn Inode, creds: &VfsCreds, flags: i32) -> Result<(), i32> {
    // Root (euid == 0) bypasses standard read/write permission checks
    if creds.is_root() {
        return Ok(());
    }

    let st = match inode.stat() {
        Ok(s) => s,
        Err(_) => return Ok(()), // Pseudodevices/anonymous nodes without stat support bypass permission checks
    };

    let imode = st.st_mode;
    let req_write = (flags & O_WRONLY != 0) || (flags & O_RDWR != 0);
    let req_read = flags & O_WRONLY == 0;

    let (can_read, can_write) = if creds.euid == st.st_uid {
        (imode & S_IRUSR != 0, imode & S_IWUSR != 0)
    } else if creds.egid == st.st_gid {
        (imode & S_IRGRP != 0, imode & S_IWGRP != 0)
    } else {
        (imode & S_IROTH != 0, imode & S_IWOTH != 0)
    };

    if (req_read && !can_read) || (req_write && !can_write) {
        Err(EACCES)
    } else {
        Ok(())
    }
}

/// Global Virtual File System representation holding the root directory node.
pub struct Vfs {
    /// Root directory inode (`/`).
    pub root: Arc<dyn Inode>,
}

/// Global active VFS instance wrapped in a spinlock.
pub static ROOT_VFS: SpinLock<Option<Vfs>> = SpinLock::new(None);

/// Initializes the global VFS root instance.
pub fn vfs_init(root: Arc<dyn Inode>) {
    *ROOT_VFS.lock() = Some(Vfs { root });
}

/// Normalizes a relative or absolute path against a base working directory, resolving `.` and `..`.
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

/// Resolves a path relative to `cwd` to its target VFS `Inode`.
pub fn resolve_path(cwd: &str, path: &str) -> Result<Arc<dyn Inode>, i32> {
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

/// Resolves a path relative to `cwd` into its parent directory `Inode` and final basename string.
pub fn resolve_parent_and_basename(cwd: &str, path: &str) -> Result<(Arc<dyn Inode>, String), i32> {
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
