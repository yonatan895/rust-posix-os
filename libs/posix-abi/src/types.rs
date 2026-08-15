//! POSIX.1-2024 Core Structures and Data Types.

/// Event structure used by `epoll_ctl` and `epoll_wait`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EpollEvent {
    /// Epoll event mask bitflags (e.g. `EPOLLIN`, `EPOLLOUT`, `EPOLLET`).
    pub events: u32,
    /// User data variable (file descriptor, pointer, or opaque integer).
    pub data: u64,
}

/// Nanosecond-precision time specification (`timespec`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timespec {
    /// Seconds component.
    pub tv_sec: i64,
    /// Nanoseconds component in range `0..=999_999_999`.
    pub tv_nsec: i64,
}

/// Microsecond-precision time specification (`timeval`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timeval {
    /// Seconds component.
    pub tv_sec: i64,
    /// Microseconds component in range `0..=999_999`.
    pub tv_usec: i64,
}

/// File metadata and status information structure (`stat`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Stat {
    /// ID of device containing file.
    pub st_dev: u64,
    /// Inode / file serial number.
    pub st_ino: u64,
    /// Number of hard links.
    pub st_nlink: u64,
    /// File mode (type and permissions).
    pub st_mode: u32,
    /// User ID of the file owner.
    pub st_uid: u32,
    /// Group ID of the file owner.
    pub st_gid: u32,
    /// Padding for 64-bit alignment in x86_64 ABI.
    pub __pad0: u32,
    /// Device ID (if character or block special file).
    pub st_rdev: u64,
    /// Total file size in bytes.
    pub st_size: i64,
    /// Preferred block size for efficient filesystem I/O operations.
    pub st_blksize: i64,
    /// Number of 512-byte blocks allocated.
    pub st_blocks: i64,
    /// Last access timestamp.
    pub st_atim: Timespec,
    /// Last modification timestamp.
    pub st_mtim: Timespec,
    /// Last status change timestamp.
    pub st_ctim: Timespec,
    /// Reserved space for future POSIX/ABI expansion.
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

/// System and operating system identification structure (`utsname`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Utsname {
    /// Operating system implementation name (e.g. `"rust-posix-os"`).
    pub sysname: [u8; 65],
    /// Network node hostname within communications network.
    pub nodename: [u8; 65],
    /// Operating system release identifier.
    pub release: [u8; 65],
    /// Operating system build version or date.
    pub version: [u8; 65],
    /// Hardware architecture identifier (e.g. `"x86_64"`).
    pub machine: [u8; 65],
    /// NIS / YP domain name.
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

/// Global system statistics and resource information (`sysinfo`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Sysinfo {
    /// Seconds elapsed since system boot.
    pub uptime: i64,
    /// 1, 5, and 15 minute load averages scaled by $2^{16}$.
    pub loads: [u64; 3],
    /// Total usable main memory size.
    pub totalram: u64,
    /// Available free memory size.
    pub freeram: u64,
    /// Amount of shared memory.
    pub sharedram: u64,
    /// Memory consumed by kernel buffers.
    pub bufferram: u64,
    /// Total swap space size.
    pub totalswap: u64,
    /// Available swap space.
    pub freeswap: u64,
    /// Current count of active processes.
    pub procs: u16,
    /// Explicit padding for alignment.
    pub pad: u16,
    /// Total high memory size (0 on 64-bit architectures).
    pub totalhigh: u64,
    /// Available high memory size (0 on 64-bit architectures).
    pub freehigh: u64,
    /// Memory unit size multiplier in bytes.
    pub mem_unit: u32,
    /// Reserved padding to conform to standard sysinfo size.
    pub _f: [u8; 8],
}

/// Directory entry header and variable-length record (`dirent64`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Dirent64 {
    /// 64-bit inode number.
    pub d_ino: u64,
    /// 64-bit offset to the next directory entry.
    pub d_off: i64,
    /// Length of this record in bytes.
    pub d_reclen: u16,
    /// File type indicator (e.g. `DT_REG`, `DT_DIR`).
    pub d_type: u8,
    /// Null-terminated directory entry name bytes.
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

/// Terminal attributes and line discipline configuration (`termios`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Termios {
    /// Terminal input mode flags.
    pub c_iflag: u32,
    /// Terminal output mode flags.
    pub c_oflag: u32,
    /// Terminal control mode flags.
    pub c_cflag: u32,
    /// Terminal local mode flags (e.g. `ICANON`, `ECHO`, `ISIG`).
    pub c_lflag: u32,
    /// Line discipline identifier.
    pub c_line: u8,
    /// Array of control characters indexed by control character constants.
    pub c_cc: [u8; 32],
    /// Input baud rate.
    pub c_ispeed: u32,
    /// Output baud rate.
    pub c_ospeed: u32,
}

/// Signal set representation bitmask (64-bit mask for signals 1 through 64).
pub type SigSet = u64;

/// Signal action configuration structure (`sigaction`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SigAction {
    /// Pointer to signal handler function, or `SIG_DFL` / `SIG_IGN`.
    pub sa_handler: usize,
    /// Signal configuration flags (e.g. `SA_RESTORER`, `SA_NODEFER`, `SA_RESTART`).
    pub sa_flags: u64,
    /// User-space signal restorer trampoline address.
    pub sa_restorer: usize,
    /// Mask of signals to block during signal handler execution.
    pub sa_mask: SigSet,
}

/// Stack frame layout pushed to user stack prior to signal delivery.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SignalFrame {
    /// Address of signal restorer function (`__restore_rt`).
    pub restorer: u64,
    /// Signal number being delivered.
    pub signum: u64,
    /// Signal mask active before signal delivery.
    pub old_mask: SigSet,
    /// Saved user register R15.
    pub r15: u64,
    /// Saved user register R14.
    pub r14: u64,
    /// Saved user register R13.
    pub r13: u64,
    /// Saved user register R12.
    pub r12: u64,
    /// Saved user base pointer (RBP).
    pub rbp: u64,
    /// Saved user register RBX.
    pub rbx: u64,
    /// Saved user register R9.
    pub r9: u64,
    /// Saved user register R8.
    pub r8: u64,
    /// Saved user register R10.
    pub r10: u64,
    /// Saved user register RDX.
    pub rdx: u64,
    /// Saved user register RSI.
    pub rsi: u64,
    /// Saved user register RDI.
    pub rdi: u64,
    /// Saved user register RAX.
    pub rax: u64,
    /// Saved user instruction pointer (RIP) for `sysretq`.
    pub rcx: u64,
    /// Saved user flags (RFLAGS) for `sysretq`.
    pub r11: u64,
    /// Saved user stack pointer (RSP).
    pub rsp: u64,
}

/// Header metadata for an audit snapshot capture.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AuditSnapshotHeader {
    /// Unique snapshot sequence identifier.
    pub id: u64,
    /// Kernel timer tick timestamp at snapshot capture time.
    pub timestamp_ticks: u64,
    /// Sequence counter in the system audit journal.
    pub journal_seq: u64,
    /// Total system memory in kilobytes at snapshot time.
    pub total_memory_kb: u64,
    /// Used system memory in kilobytes at snapshot time.
    pub used_memory_kb: u64,
    /// Kernel heap memory consumed in kilobytes at snapshot time.
    pub heap_used_kb: u64,
    /// Total active process count at snapshot time.
    pub process_count: u32,
    /// Null-terminated human-readable snapshot label or description.
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
