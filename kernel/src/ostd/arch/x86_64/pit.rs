//! 8254 Programmable Interval Timer (PIT) Driver for x86_64.

use super::{io_wait, outb};

pub const PIT_FREQUENCY_HZ: u32 = 100;
pub const PIT_BASE_FREQUENCY_HZ: u32 = 1_193_182;
pub const PIT_MIN_FREQUENCY_HZ: u32 = 19; // 1_193_182 / 65535 ≈ 18.2 Hz
pub const PIT_MAX_FREQUENCY_HZ: u32 = PIT_BASE_FREQUENCY_HZ; // 1_193_182 / 1 = 1_193_182 Hz
pub const PIT_DIVISOR: u16 = (PIT_BASE_FREQUENCY_HZ / PIT_FREQUENCY_HZ) as u16; // 11931 = 0x2E9B

/// Computes the 16-bit reload divisor for a requested timer frequency in Hz.
///
/// Returns `Some(divisor)` if `hz` falls within the supported range `19..=1_193_182`,
/// or `None` if the requested frequency cannot be achieved by the 16-bit counter.
pub const fn pit_calc_divisor(hz: u32) -> Option<u16> {
    if hz < PIT_MIN_FREQUENCY_HZ || hz > PIT_MAX_FREQUENCY_HZ {
        return None;
    }
    let divisor = PIT_BASE_FREQUENCY_HZ / hz;
    if divisor == 0 || divisor > 65535 {
        None
    } else {
        Some(divisor as u16)
    }
}

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
