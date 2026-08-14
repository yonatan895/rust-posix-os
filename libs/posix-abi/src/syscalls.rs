//! POSIX.1-2024 System Call Numbers for x86_64 ABI.

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
pub const SYS_RENAME: usize = 82;
pub const SYS_MKDIR: usize = 83;
pub const SYS_RMDIR: usize = 84;
pub const SYS_UNLINK: usize = 87;
pub const SYS_UMASK: usize = 95;
pub const SYS_SYSINFO: usize = 99;
pub const SYS_GETUID: usize = 102;
pub const SYS_GETGID: usize = 104;
pub const SYS_SETUID: usize = 105;
pub const SYS_SETGID: usize = 106;
pub const SYS_GETEUID: usize = 107;
pub const SYS_GETEGID: usize = 108;
pub const SYS_GETPPID: usize = 110;
pub const SYS_EPOLL_CREATE: usize = 213;
pub const SYS_GETDENTS64: usize = 217;
pub const SYS_CLOCK_GETTIME: usize = 228;
pub const SYS_EPOLL_WAIT: usize = 232;
pub const SYS_EPOLL_CTL: usize = 233;
pub const SYS_EPOLL_CREATE1: usize = 291;
pub const SYS_PIPE2: usize = 293;
pub const SYS_AUDIT_LOG: usize = 301;
pub const SYS_AUDIT_SNAPSHOT: usize = 302;
