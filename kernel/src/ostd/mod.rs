//! Privileged OS Framework (OSTD) - Trusted Computing Base (TCB).
//!
//! This is the only module in the kernel allowed to contain `unsafe` code.
//! All hardware-oriented, architecture-specific, and low-level memory operations
//! are encapsulated here behind safe abstractions.

pub mod arch;
pub mod mm;
pub mod task;
pub mod sync;
pub mod irq;
pub mod drivers;
pub mod limine;

pub use arch::gdt::gdt_init;
pub use arch::idt::idt_init;
pub use arch::syscall::syscall_init;
pub use mm::mm_init;
pub use drivers::serial::serial_init;
pub use irq::irq_init;
