//! Hardware Drivers for OSTD.

pub mod framebuffer;
pub mod serial;

pub use framebuffer::{FB_CONSOLE, fb_init};
pub use serial::{SERIAL1, serial_init};
