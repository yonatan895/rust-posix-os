#![no_std]

//! POSIX.1-2024 ABI Definitions for x86_64 Rust OS.
//!
//! This crate contains syscall numbers, standard structures, flags,
//! error numbers, and signal constants shared between the kernel,
//! the Rust libc implementation, and userland binaries.

// ============================================================================
// Standard File Descriptors
// ============================================================================
pub const STDIN_FILENO: i32 = 0;
pub const STDOUT_FILENO: i32 = 1;
pub const STDERR_FILENO: i32 = 2;

// ============================================================================
// System Call Numbers (x86_64 POSIX / Linux ABI Compatible)
// ============================================================================
pub const SYS_READ: usize = 0;
pub const SYS_WRITE: usize = 1;
pub const SYS_OPEN: usize = 2;
pub const SYS_CLOSE: usize = 3;
pub const SYS_STAT: usize = 4;
pub const SYS_FSTAT: usize = 5;
pub const SYS_LSEEK: usize = 8;
pub const SYS_MMAP: usize = 9;
pub const SYS_MPROTECT: usize = 10;
pub const SYS_MUNMAP: usize = 11;
pub const SYS_BRK: usize = 12;
pub const SYS_RT_SIGACTION: usize = 13;
pub const SYS_RT_SIGPROCMASK: usize = 14;
pub const SYS_RT_SIGRETURN: usize = 15;
pub const SYS_IOCTL: usize = 16;
pub const SYS_PIPE: usize = 22;
pub const SYS_DUP: usize = 32;
pub const SYS_DUP2: usize = 33;
pub const SYS_NANOSLEEP: usize = 35;
pub const SYS_GETPID: usize = 39;
pub const SYS_FORK: usize = 57;
pub const SYS_EXECVE: usize = 59;
pub const SYS_EXIT: usize = 60;
pub const SYS_WAIT4: usize = 61;
pub const SYS_KILL: usize = 62;
pub const SYS_UNAME: usize = 63;
pub const SYS_GETCWD: usize = 79;
pub const SYS_CHDIR: usize = 80;
pub const SYS_MKDIR: usize = 83;
pub const SYS_RMDIR: usize = 84;
pub const SYS_UNLINK: usize = 87;
pub const SYS_GETPPID: usize = 110;
pub const SYS_SYSINFO: usize = 99;
pub const SYS_EPOLL_CREATE: usize = 213;
pub const SYS_GETDENTS64: usize = 217;
pub const SYS_CLOCK_GETTIME: usize = 228;
pub const SYS_EPOLL_WAIT: usize = 232;
pub const SYS_EPOLL_CTL: usize = 233;
pub const SYS_EPOLL_CREATE1: usize = 291;
pub const SYS_PIPE2: usize = 293;
pub const SYS_AUDIT_LOG: usize = 301;
pub const SYS_AUDIT_SNAPSHOT: usize = 302;

// ============================================================================
// Epoll Operation Constants and Event Flags
// ============================================================================
pub const EPOLL_CTL_ADD: i32 = 1;
pub const EPOLL_CTL_DEL: i32 = 2;
pub const EPOLL_CTL_MOD: i32 = 3;

pub const EPOLLIN: u32 = 0x0001;
pub const EPOLLPRI: u32 = 0x0002;
pub const EPOLLOUT: u32 = 0x0004;
pub const EPOLLERR: u32 = 0x0008;
pub const EPOLLHUP: u32 = 0x0010;
pub const EPOLLRDHUP: u32 = 0x2000;
pub const EPOLLET: u32 = 1 << 31; // Edge Triggered

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EpollEvent {
    pub events: u32,
    pub data: u64,
}

// ============================================================================
// POSIX Error Numbers (errno)
// ============================================================================
pub const EPERM: i32 = 1;      // Operation not permitted
pub const ENOENT: i32 = 2;     // No such file or directory
pub const ESRCH: i32 = 3;      // No such process
pub const EINTR: i32 = 4;      // Interrupted system call
pub const EIO: i32 = 5;        // I/O error
pub const ENXIO: i32 = 6;      // No such device or address
pub const E2BIG: i32 = 7;      // Argument list too long
pub const ENOEXEC: i32 = 8;    // Exec format error
pub const EBADF: i32 = 9;      // Bad file number
pub const ECHILD: i32 = 10;    // No child processes
pub const EAGAIN: i32 = 11;    // Try again
pub const ENOMEM: i32 = 12;    // Out of memory
pub const EACCES: i32 = 13;    // Permission denied
pub const EFAULT: i32 = 14;    // Bad address
pub const EBUSY: i32 = 16;     // Device or resource busy
pub const EEXIST: i32 = 17;    // File exists
pub const EXDEV: i32 = 18;     // Cross-device link
pub const ENODEV: i32 = 19;    // No such device
pub const ENOTDIR: i32 = 20;   // Not a directory
pub const EISDIR: i32 = 21;    // Is a directory
pub const EINVAL: i32 = 22;    // Invalid argument
pub const ENFILE: i32 = 23;    // File table overflow
pub const EMFILE: i32 = 24;    // Too many open files
pub const ENOTTY: i32 = 25;    // Not a typewriter
pub const EFBIG: i32 = 27;     // File too large
pub const ENOSPC: i32 = 28;    // No space left on device
pub const ESPIPE: i32 = 29;    // Illegal seek
pub const EROFS: i32 = 30;     // Read-only file system
pub const EMLINK: i32 = 31;    // Too many links
pub const EPIPE: i32 = 32;     // Broken pipe
pub const ERANGE: i32 = 34;    // Math result not representable
pub const ENOSYS: i32 = 38;    // Invalid system call number

// ============================================================================
// File Open Flags (O_*)
// ============================================================================
pub const O_RDONLY: i32 = 0x0000;
pub const O_WRONLY: i32 = 0x0001;
pub const O_RDWR: i32   = 0x0002;
pub const O_CREAT: i32  = 0x0040;
pub const O_EXCL: i32   = 0x0080;
pub const O_TRUNC: i32  = 0x0200;
pub const O_APPEND: i32 = 0x0400;
pub const O_NONBLOCK: i32 = 0x0800;
pub const O_DIRECTORY: i32 = 0x10000;
pub const O_CLOEXEC: i32 = 0x80000;

// ============================================================================
// File Seek Modes
// ============================================================================
pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;

// ============================================================================
// Memory Management Flags (mmap / mprotect)
// ============================================================================
pub const PROT_NONE: i32  = 0x00;
pub const PROT_READ: i32  = 0x01;
pub const PROT_WRITE: i32 = 0x02;
pub const PROT_EXEC: i32  = 0x04;

pub const MAP_SHARED: i32    = 0x01;
pub const MAP_PRIVATE: i32   = 0x02;
pub const MAP_FIXED: i32     = 0x10;
pub const MAP_ANONYMOUS: i32 = 0x20;

// ============================================================================
// POSIX Signals
// ============================================================================
pub const SIGHUP: i32    = 1;
pub const SIGINT: i32    = 2;
pub const SIGQUIT: i32   = 3;
pub const SIGILL: i32    = 4;
pub const SIGTRAP: i32   = 5;
pub const SIGABRT: i32   = 6;
pub const SIGBUS: i32    = 7;
pub const SIGFPE: i32    = 8;
pub const SIGKILL: i32   = 9;
pub const SIGUSR1: i32   = 10;
pub const SIGSEGV: i32   = 11;
pub const SIGUSR2: i32   = 12;
pub const SIGPIPE: i32   = 13;
pub const SIGALRM: i32   = 14;
pub const SIGTERM: i32   = 15;
pub const SIGCHLD: i32   = 17;
pub const SIGCONT: i32   = 18;
pub const SIGSTOP: i32   = 19;
pub const SIGTSTP: i32   = 20;
pub const SIGWINCH: i32  = 28;

// ============================================================================
// POSIX File Mode Types (S_IF*)
// ============================================================================
pub const S_IFMT: u32   = 0o170000;
pub const S_IFSOCK: u32 = 0o140000;
pub const S_IFLNK: u32  = 0o120000;
pub const S_IFREG: u32  = 0o100000;
pub const S_IFBLK: u32  = 0o060000;
pub const S_IFDIR: u32  = 0o040000;
pub const S_IFCHR: u32  = 0o020000;
pub const S_IFIFO: u32  = 0o010000;

// ============================================================================
// Core POSIX Structures
// ============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timeval {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub __pad0: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atim: Timespec,
    pub st_mtim: Timespec,
    pub st_ctim: Timespec,
    pub __reserved: [i64; 3],
}

impl Default for Stat {
    fn default() -> Self {
        Self {
            st_dev: 0,
            st_ino: 0,
            st_nlink: 1,
            st_mode: 0,
            st_uid: 0,
            st_gid: 0,
            __pad0: 0,
            st_rdev: 0,
            st_size: 0,
            st_blksize: 4096,
            st_blocks: 0,
            st_atim: Timespec::default(),
            st_mtim: Timespec::default(),
            st_ctim: Timespec::default(),
            __reserved: [0; 3],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Utsname {
    pub sysname: [u8; 65],
    pub nodename: [u8; 65],
    pub release: [u8; 65],
    pub version: [u8; 65],
    pub machine: [u8; 65],
    pub domainname: [u8; 65],
}

impl Default for Utsname {
    fn default() -> Self {
        Self {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
            domainname: [0; 65],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Sysinfo {
    pub uptime: i64,
    pub loads: [u64; 3],
    pub totalram: u64,
    pub freeram: u64,
    pub sharedram: u64,
    pub bufferram: u64,
    pub totalswap: u64,
    pub freeswap: u64,
    pub procs: u16,
    pub pad: u16,
    pub totalhigh: u64,
    pub freehigh: u64,
    pub mem_unit: u32,
    pub _f: [u8; 8],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Dirent64 {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 256],
}

impl Default for Dirent64 {
    fn default() -> Self {
        Self {
            d_ino: 0,
            d_off: 0,
            d_reclen: 0,
            d_type: 0,
            d_name: [0; 256],
        }
    }
}

pub const DT_UNKNOWN: u8 = 0;
pub const DT_FIFO: u8 = 1;
pub const DT_CHR: u8 = 2;
pub const DT_DIR: u8 = 4;
pub const DT_BLK: u8 = 6;
pub const DT_REG: u8 = 8;
pub const DT_LNK: u8 = 10;
pub const DT_SOCK: u8 = 12;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Termios {
    pub c_iflag: u32,
    pub c_oflag: u32,
    pub c_cflag: u32,
    pub c_lflag: u32,
    pub c_line: u8,
    pub c_cc: [u8; 32],
    pub c_ispeed: u32,
    pub c_ospeed: u32,
}

impl Default for Termios {
    fn default() -> Self {
        Self {
            c_iflag: 0,
            c_oflag: 0,
            c_cflag: 0,
            c_lflag: 0,
            c_line: 0,
            c_cc: [0; 32],
            c_ispeed: 0,
            c_ospeed: 0,
        }
    }
}

pub const ECHO: u32 = 0x0008;
pub const ICANON: u32 = 0x0002;
pub const ISIG: u32 = 0x0001;

pub type SigSet = u64;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SigAction {
    pub sa_handler: usize,
    pub sa_flags: u64,
    pub sa_restorer: usize,
    pub sa_mask: SigSet,
}

impl Default for SigAction {
    fn default() -> Self {
        Self {
            sa_handler: 0,
            sa_flags: 0,
            sa_restorer: 0,
            sa_mask: 0,
        }
    }
}

// ============================================================================
// Audit Journal and Snapshot Definitions
// ============================================================================
pub const AUDIT_TYPE_USER_ACTION: u32 = 1;
pub const AUDIT_TYPE_PROCESS_SPAWN: u32 = 2;
pub const AUDIT_TYPE_PROCESS_EXIT: u32 = 3;
pub const AUDIT_TYPE_FILE_CREATE: u32 = 4;
pub const AUDIT_TYPE_FILE_MODIFY: u32 = 5;
pub const AUDIT_TYPE_FILE_UNLINK: u32 = 6;
pub const AUDIT_TYPE_DIR_CREATE: u32 = 7;
pub const AUDIT_TYPE_DIR_CHANGE: u32 = 8;
pub const AUDIT_TYPE_SNAPSHOT_CREATED: u32 = 9;
pub const AUDIT_TYPE_SECURITY_ALERT: u32 = 10;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AuditSnapshotHeader {
    pub id: u64,
    pub timestamp_ticks: u64,
    pub journal_seq: u64,
    pub total_memory_kb: u64,
    pub used_memory_kb: u64,
    pub heap_used_kb: u64,
    pub process_count: u32,
    pub label: [u8; 64],
}

impl Default for AuditSnapshotHeader {
    fn default() -> Self {
        Self {
            id: 0,
            timestamp_ticks: 0,
            journal_seq: 0,
            total_memory_kb: 0,
            used_memory_kb: 0,
            heap_used_kb: 0,
            process_count: 0,
            label: [0; 64],
        }
    }
}
