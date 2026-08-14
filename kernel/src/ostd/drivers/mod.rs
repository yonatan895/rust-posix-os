//! Hardware Drivers for OSTD.

pub mod framebuffer;
pub mod serial;

pub use framebuffer::{fb_init, FB_CONSOLE};
pub use serial::{serial_init, SERIAL1};
