//! 8254 Programmable Interval Timer (PIT) Driver for x86_64.

use super::{io_wait, outb};
pub use posix_abi::{
    PIT_BASE_FREQUENCY_HZ, PIT_DIVISOR, PIT_FREQUENCY_HZ, PIT_MAX_FREQUENCY_HZ,
    PIT_MIN_FREQUENCY_HZ, pit_calc_divisor, pit_effective_freq,
};

/// Initializes the 8254 Programmable Interval Timer (PIT) Channel 0 for a target frequency in Hz.
///
/// # Panics
///
/// Panics if `hz` is not in the supported range `19..=1_193_182`.
///
/// # Safety
///
/// Directly writes configuration commands and reload counts to PIT I/O ports (`0x43`, `0x40`).
pub unsafe fn pit_init_hz(hz: u32) {
    let divisor = match pit_calc_divisor(hz) {
        Some(d) => d,
        None => panic!(
            "Unsupported PIT frequency: {} Hz (supported: {}..={})",
            hz, PIT_MIN_FREQUENCY_HZ, PIT_MAX_FREQUENCY_HZ
        ),
    };

    // SAFETY: Programming 8254 PIT Channel 0 mode register (0x43) to Mode 3 (Square Wave, low/high byte access) and writing 16-bit reload divisor to Channel 0 data port (0x40).
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
    // SAFETY: Delegating to pit_init_hz with the default POSIX frequency PIT_FREQUENCY_HZ (100 Hz).
    unsafe {
        pit_init_hz(PIT_FREQUENCY_HZ);
    }
}
