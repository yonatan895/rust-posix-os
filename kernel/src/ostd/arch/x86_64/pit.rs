//! 8254 Programmable Interval Timer (PIT) Driver for x86_64.

use super::{io_wait, outb};

pub const PIT_FREQUENCY_HZ: u32 = 100;
pub const PIT_BASE_FREQUENCY_HZ: u32 = 1_193_182;
pub const PIT_DIVISOR: u16 = (PIT_BASE_FREQUENCY_HZ / PIT_FREQUENCY_HZ) as u16; // 11931 = 0x2E9B

/// Initializes the 8254 Programmable Interval Timer (PIT) Channel 0 for a target frequency in Hz.
///
/// # Safety
///
/// Directly writes configuration commands and reload counts to PIT I/O ports (`0x43`, `0x40`).
pub unsafe fn pit_init_hz(hz: u32) {
    let divisor = ((PIT_BASE_FREQUENCY_HZ / hz.max(1)).clamp(1, 65535)) as u16;
    // SAFETY: Programming PIT Channel 0 mode 3 square wave generator.
    unsafe {
        // Mode/Command register (0x43): Channel 0, Access lo/hi byte, Mode 3 (Square Wave), Binary 16-bit
        outb(0x43, 0x36);
        io_wait();

        // Channel 0 Data port (0x40): Write low byte, then high byte
        outb(0x40, (divisor & 0xFF) as u8);
        io_wait();
        outb(0x40, ((divisor >> 8) & 0xFF) as u8);
        io_wait();
    }
}

/// Initializes the 8254 Programmable Interval Timer (PIT) Channel 0 for periodic 100 Hz interrupts.
///
/// # Safety
///
/// Directly writes configuration commands and reload counts to PIT I/O ports (`0x43`, `0x40`).
pub unsafe fn pit_init() {
    // SAFETY: Initializing PIT at default 100 Hz frequency.
    unsafe {
        pit_init_hz(PIT_FREQUENCY_HZ);
    }
}
