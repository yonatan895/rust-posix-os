//! Architecture Abstraction Layer (OSTD).
//!
//! Provides architecture-specific hardware support and uniform re-exports
//! under the target architecture.

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "x86_64")]
pub use self::x86_64::*;
