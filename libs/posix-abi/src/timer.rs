//! Hardware Timer and Clock Constants and Calculations.
//!
//! Provides the mathematical models, constants, and divisor calculation routines
//! for standard system interval timers (e.g. Intel 8254 PIT).

/// Base oscillator input frequency of the 8254 Programmable Interval Timer (1.193182 MHz).
pub const PIT_BASE_FREQUENCY_HZ: u32 = 1_193_182;

/// Default target timer interrupt frequency (100 Hz = 10 ms per tick).
pub const PIT_FREQUENCY_HZ: u32 = 100;

/// Policy minimum supported frequency in Hz.
///
/// Note: The 16-bit reload register with base 1.193182 MHz supports an absolute hardware
/// minimum of `1_193_182 / 65535 ≈ 18.207 Hz`. The kernel enforces 19 Hz as the integer lower bound
/// to ensure clean divisor truncation (`divisor = 62799`, effective ~19.000 Hz).
pub const PIT_MIN_FREQUENCY_HZ: u32 = 19;

/// Maximum supported timer interrupt frequency (1.193182 MHz, divisor = 1).
pub const PIT_MAX_FREQUENCY_HZ: u32 = PIT_BASE_FREQUENCY_HZ;

/// Default 100 Hz reload divisor (11,931 = 0x2E9B).
pub const PIT_DIVISOR: u16 = (PIT_BASE_FREQUENCY_HZ / PIT_FREQUENCY_HZ) as u16;

/// Computes the 16-bit reload divisor for a requested timer frequency in Hz.
///
/// Because integer arithmetic is used (`divisor = PIT_BASE_FREQUENCY_HZ / hz`),
/// the programmed frequency is an approximation. For example:
/// - 100 Hz requested $\rightarrow$ divisor 11,931 (effective: 100.0068 Hz).
/// - 19 Hz requested $\rightarrow$ divisor 62,799 (effective: 19.00001 Hz).
///
/// Returns `Some(divisor)` if `hz` falls within `19..=1_193_182`, or `None` if out of bounds.
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

/// Computes the effective timer frequency in Hz resulting from a programmed 16-bit divisor.
///
/// Useful for determining clock drift and precision for a given divisor.
pub const fn pit_effective_freq(divisor: u16) -> u32 {
    if divisor == 0 {
        0
    } else {
        PIT_BASE_FREQUENCY_HZ / (divisor as u32)
    }
}
