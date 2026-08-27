//! POSIX.1-2024 System Call Numbers for x86_64 ABI.

// ============================================================================
// Standard File Descriptors
// ============================================================================

/// Standard input file descriptor (stdin = 0).
pub const STDIN_FILENO: i32 = 0;
/// Standard output file descriptor (stdout = 1).
pub const STDOUT_FILENO: i32 = 1;
/// Standard error file descriptor (stderr = 2).
pub const STDERR_FILENO: i32 = 2;

// ============================================================================
// System Call Numbers (x86_64 POSIX / Linux ABI Compatible)
// ============================================================================

/// `read(fd, buf, count)`: Read from a file descriptor (syscall 0).
pub const SYS_READ: usize = 0;
/// `write(fd, buf, count)`: Write to a file descriptor (syscall 1).
pub const SYS_WRITE: usize = 1;
/// `open(pathname, flags, mode)`: Open or create a file (syscall 2).
pub const SYS_OPEN: usize = 2;
/// `close(fd)`: Close an open file descriptor (syscall 3).
pub const SYS_CLOSE: usize = 3;
/// `stat(pathname, statbuf)`: Get file status by pathname (syscall 4).
pub const SYS_STAT: usize = 4;
/// `fstat(fd, statbuf)`: Get file status by open file descriptor (syscall 5).
pub const SYS_FSTAT: usize = 5;
/// `lseek(fd, offset, whence)`: Reposition read/write file offset (syscall 8).
pub const SYS_LSEEK: usize = 8;
/// `mmap(addr, length, prot, flags, fd, offset)`: Map files or anonymous memory (syscall 9).
pub const SYS_MMAP: usize = 9;
/// `mprotect(addr, len, prot)`: Set protection on a region of memory (syscall 10).
pub const SYS_MPROTECT: usize = 10;
/// `munmap(addr, length)`: Unmap a memory mapped region (syscall 11).
pub const SYS_MUNMAP: usize = 11;
/// `brk(addr)`: Change data segment size for heap allocation (syscall 12).
pub const SYS_BRK: usize = 12;
/// `rt_sigaction(signum, act, oldact, sigsetsize)`: Examine and change a signal action (syscall 13).
pub const SYS_RT_SIGACTION: usize = 13;
/// `rt_sigprocmask(how, set, oldset, sigsetsize)`: Examine and change blocked signals (syscall 14).
pub const SYS_RT_SIGPROCMASK: usize = 14;
/// `rt_sigreturn()`: Return from signal handler and cleanup stack frame (syscall 15).
pub const SYS_RT_SIGRETURN: usize = 15;
/// `ioctl(fd, request, argp)`: Control device parameters (syscall 16).
pub const SYS_IOCTL: usize = 16;
/// `pipe(pipefd)`: Create an interprocess communication pipe (syscall 22).
pub const SYS_PIPE: usize = 22;
/// `dup(oldfd)`: Duplicate an open file descriptor (syscall 32).
pub const SYS_DUP: usize = 32;
/// `dup2(oldfd, newfd)`: Duplicate an open file descriptor to a specific descriptor (syscall 33).
pub const SYS_DUP2: usize = 33;
/// `nanosleep(req, rem)`: High-resolution sleep (syscall 35).
pub const SYS_NANOSLEEP: usize = 35;
/// `getpid()`: Get process identification number (syscall 39).
pub const SYS_GETPID: usize = 39;
/// `fork()`: Create a child process by cloning the current process (syscall 57).
pub const SYS_FORK: usize = 57;
/// `execve(pathname, argv, envp)`: Execute a program (syscall 59).
pub const SYS_EXECVE: usize = 59;
/// `exit(status)`: Terminate calling process (syscall 60).
pub const SYS_EXIT: usize = 60;
/// `wait4(pid, wstatus, options, rusage)`: Wait for process state changes (syscall 61).
pub const SYS_WAIT4: usize = 61;
/// `kill(pid, sig)`: Send a signal to a process (syscall 62).
pub const SYS_KILL: usize = 62;
/// `uname(buf)`: Get name and information about current operating system (syscall 63).
pub const SYS_UNAME: usize = 63;
/// `getcwd(buf, size)`: Get current working directory pathname (syscall 79).
pub const SYS_GETCWD: usize = 79;
/// `chdir(path)`: Change current working directory (syscall 80).
pub const SYS_CHDIR: usize = 80;
/// `rename(oldpath, newpath)`: Rename or move a file/directory (syscall 82).
pub const SYS_RENAME: usize = 82;
/// `mkdir(pathname, mode)`: Create a new directory (syscall 83).
pub const SYS_MKDIR: usize = 83;
/// `rmdir(pathname)`: Delete an empty directory (syscall 84).
pub const SYS_RMDIR: usize = 84;
/// `unlink(pathname)`: Delete a directory entry / file link (syscall 87).
pub const SYS_UNLINK: usize = 87;
/// `umask(mask)`: Set file mode creation mask (syscall 95).
pub const SYS_UMASK: usize = 95;
/// `sysinfo(info)`: Return global system statistics (syscall 99).
pub const SYS_SYSINFO: usize = 99;
/// `getuid()`: Get real user identity of calling process (syscall 102).
pub const SYS_GETUID: usize = 102;
/// `getgid()`: Get real group identity of calling process (syscall 104).
pub const SYS_GETGID: usize = 104;
/// `setuid(uid)`: Set user identity of calling process (syscall 105).
pub const SYS_SETUID: usize = 105;
/// `setgid(gid)`: Set group identity of calling process (syscall 106).
pub const SYS_SETGID: usize = 106;
/// `geteuid()`: Get effective user identity of calling process (syscall 107).
pub const SYS_GETEUID: usize = 107;
/// `getegid()`: Get effective group identity of calling process (syscall 108).
pub const SYS_GETEGID: usize = 108;
/// `getppid()`: Get parent process identification number (syscall 110).
pub const SYS_GETPPID: usize = 110;
/// `seteuid(euid)`: Set effective user identity of calling process (syscall 115).
pub const SYS_SETEUID: usize = 115;
/// `setegid(egid)`: Set effective group identity of calling process (syscall 116).
pub const SYS_SETEGID: usize = 116;
/// `setresuid(ruid, euid, suid)`: Set real, effective, and saved user IDs (syscall 117).
pub const SYS_SETRESUID: usize = 117;
/// `getresuid(ruid, euid, suid)`: Get real, effective, and saved user IDs (syscall 118).
pub const SYS_GETRESUID: usize = 118;
/// `setresgid(rgid, egid, sgid)`: Set real, effective, and saved group IDs (syscall 119).
pub const SYS_SETRESGID: usize = 119;
/// `getresgid(rgid, egid, sgid)`: Get real, effective, and saved group IDs (syscall 120).
pub const SYS_GETRESGID: usize = 120;
/// `epoll_create(size)`: Open an epoll file descriptor (syscall 213).
pub const SYS_EPOLL_CREATE: usize = 213;
/// `getdents64(fd, dirp, count)`: Read directory entries into buffer (syscall 217).
pub const SYS_GETDENTS64: usize = 217;
/// `clock_gettime(clk_id, tp)`: Retrieve timestamp from specified system clock (syscall 228).
pub const SYS_CLOCK_GETTIME: usize = 228;
/// `epoll_wait(epfd, events, maxevents, timeout)`: Wait for I/O events on an epoll instance (syscall 232).
pub const SYS_EPOLL_WAIT: usize = 232;
/// `epoll_ctl(epfd, op, fd, event)`: Control interface for an epoll file descriptor (syscall 233).
pub const SYS_EPOLL_CTL: usize = 233;
/// `epoll_create1(flags)`: Open an epoll file descriptor with flags (syscall 291).
pub const SYS_EPOLL_CREATE1: usize = 291;
/// `pipe2(pipefd, flags)`: Create pipe with flags like `O_CLOEXEC` (syscall 293).
pub const SYS_PIPE2: usize = 293;
/// `audit_log(event_type, target, details)`: Record event in kernel audit log (syscall 301).
pub const SYS_AUDIT_LOG: usize = 301;
/// `audit_snapshot(label, flags)`: Capture point-in-time system state snapshot (syscall 302).
pub const SYS_AUDIT_SNAPSHOT: usize = 302;
