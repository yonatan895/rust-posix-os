#![no_std]

//! POSIX.1-2024 ABI Definitions for x86_64 Rust OS.
//!
//! This crate contains syscall numbers, standard structures, flags,
//! error numbers, and signal constants shared between the kernel,
//! the Rust libc implementation, and userland binaries.

pub mod errno;
pub mod flags;
pub mod syscalls;
pub mod types;

pub use errno::*;
pub use flags::*;
pub use syscalls::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_posix_constants() {
        assert_eq!(SYS_READ, 0);
        assert_eq!(SYS_WRITE, 1);
        assert_eq!(SYS_OPEN, 2);
        assert_eq!(SYS_CLOSE, 3);
        assert_eq!(SYS_STAT, 4);
        assert_eq!(SYS_EXIT, 60);
    }

    #[test]
    fn test_errno_values() {
        assert_eq!(EPERM, 1);
        assert_eq!(ENOENT, 2);
        assert_eq!(ESRCH, 3);
        assert_eq!(EINTR, 4);
        assert_eq!(EIO, 5);
        assert_eq!(ENOSYS, 38);
    }

    #[test]
    fn test_open_flags() {
        assert_eq!(O_RDONLY, 0);
        assert_eq!(O_WRONLY, 1);
        assert_eq!(O_RDWR, 2);
        assert_eq!(O_CREAT, 0o100);
    }

    #[test]
    fn test_wait_flags() {
        assert_eq!(WNOHANG, 1);
        assert_eq!(WUNTRACED, 2);
        assert_eq!(WCONTINUED, 8);
    }

    #[test]
    fn test_signal_constants() {
        assert_eq!(SIG_MIN, 1);
        assert_eq!(SIG_MAX, 31);
        assert_eq!(SIGHUP, 1);
        assert_eq!(SIGINT, 2);
        assert_eq!(SIGKILL, 9);
        assert_eq!(SIGUSR1, 10);
        assert_eq!(SIGTERM, 15);
        assert_eq!(SIGSTOP, 19);
        assert_eq!(SIGSYS, 31);

        assert_eq!(SIG_DFL, 0);
        assert_eq!(SIG_IGN, 1);

        assert_eq!(SIG_BLOCK, 0);
        assert_eq!(SIG_UNBLOCK, 1);
        assert_eq!(SIG_SETMASK, 2);

        assert_eq!(SA_RESTORER, 0x04000000);
        assert_eq!(SA_NODEFER, 0x40000000);
    }
}
