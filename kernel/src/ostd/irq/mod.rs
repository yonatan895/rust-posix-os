//! Interrupt Controller & Hardware Timers in OSTD.
//!
//! Exposes architecture-neutral IRQ primitives, delegating architecture-specific
//! controller (e.g. 8259 PIC) and timer (e.g. 8254 PIT) programming to `ostd::arch`.

#[cfg(target_arch = "x86_64")]
use crate::ostd::arch::x86_64::{io_wait, outb, pic, pit};

#[cfg(target_arch = "x86_64")]
pub use crate::ostd::arch::x86_64::pit::{PIT_BASE_FREQUENCY_HZ, PIT_DIVISOR, PIT_FREQUENCY_HZ};

/// Sends End of Interrupt (EOI) acknowledgment to the interrupt controller.
///
/// # Safety
///
/// Directly sends EOI command to the hardware interrupt controller.
#[inline]
pub unsafe fn send_eoi(irq: u8) {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Forwarding EOI to architecture-specific PIC handler.
    unsafe {
        pic::send_eoi(irq);
    }
}

/// Initializes hardware interrupt controllers and starts the periodic timer.
///
/// Remaps the 8259 PIC to vectors `0x20..0x2F`, configures the 8254 PIT for 100 Hz,
/// and unmasks IRQ0 (timer interrupt).
///
/// # Safety
///
/// Must be called during single-threaded kernel boot before interrupts are enabled.
pub unsafe fn irq_init() {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Remapping PIC, programming PIT timer, and unmasking timer IRQ0.
    unsafe {
        // Remap PIC: Master -> 0x20..0x27, Slave -> 0x28..0x2F
        pic::pic_remap(0x20, 0x28);

        // Program PIT Channel 0 at 100 Hz
        pit::pit_init();

        // Unmask IRQ0 (timer) on Master PIC (bit 0 = 0), mask all other IRQs (0xFE)
        outb(0x21, 0xFE);
        io_wait();
        // Mask all Slave IRQs (0xFF)
        outb(0xA1, 0xFF);
        io_wait();
    }
}
