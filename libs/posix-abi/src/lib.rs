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
