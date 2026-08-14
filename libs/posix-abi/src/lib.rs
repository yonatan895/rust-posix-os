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
}
