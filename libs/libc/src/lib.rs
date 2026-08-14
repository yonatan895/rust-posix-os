#![no_std]
#![allow(suspicious_runtime_symbol_definitions)]
#![allow(clippy::missing_safety_doc)]

//! Lightweight POSIX C ABI Library in Rust.
//!
//! Exposes standard POSIX symbols with `#[no_mangle] pub extern "C"`.

pub mod signal;
pub mod stdio;
pub mod stdlib;
pub mod string;
pub mod sys_epoll;
pub mod sys_mman;
pub mod sys_stat;
pub mod sys_wait;
pub mod syscall;
pub mod unistd;

// Re-export common symbols
pub use signal::*;
pub use stdio::*;
pub use stdlib::*;
pub use string::*;
pub use sys_epoll::*;
pub use sys_mman::*;
pub use sys_stat::*;
pub use sys_wait::*;
pub use unistd::*;
