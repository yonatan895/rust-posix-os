//! POSIX.1-2024 Core Structures and Data Types.

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EpollEvent {
    pub events: u32,
    pub data: u64,
}

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

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
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

pub type SigSet = u64;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SigAction {
    pub sa_handler: usize,
    pub sa_flags: u64,
    pub sa_restorer: usize,
    pub sa_mask: SigSet,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SignalFrame {
    pub restorer: u64,
    pub signum: u64,
    pub old_mask: SigSet,
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r9: u64,
    pub r8: u64,
    pub r10: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rax: u64,
    pub rcx: u64, // Saved RIP for sysretq
    pub r11: u64, // Saved RFLAGS for sysretq
    pub rsp: u64, // Saved user RSP
}

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
