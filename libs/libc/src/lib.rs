#![no_std]
#![allow(suspicious_runtime_symbol_definitions)]

//! Lightweight POSIX C ABI Library in Rust.
//!
//! Exposes standard POSIX symbols with `#[no_mangle] pub extern "C"`.

pub mod syscall;
pub mod string;
pub mod unistd;
pub mod sys_mman;
pub mod stdlib;
pub mod stdio;
pub mod sys_stat;
pub mod sys_wait;
pub mod signal;
pub mod sys_epoll;

// Re-export common symbols
pub use string::*;
pub use unistd::*;
pub use sys_mman::*;
pub use stdlib::*;
pub use stdio::*;
pub use sys_stat::*;
pub use sys_wait::*;
pub use signal::*;
pub use sys_epoll::*;
