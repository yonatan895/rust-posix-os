//! Hardware Drivers for OSTD.

pub mod serial;
pub mod framebuffer;

pub use serial::{serial_init, SERIAL1};
pub use framebuffer::{fb_init, FB_CONSOLE};
