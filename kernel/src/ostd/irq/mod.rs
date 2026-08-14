//! Interrupt Controller & Timers in OSTD.

use crate::ostd::arch::{outb, io_wait};

/// Disables the legacy 8259 PIC by masking all IRQ lines.
///
/// # Safety
///
/// Directly manipulates legacy 8259 PIC hardware registers.
pub unsafe fn pic_disable() {
    // Mask all interrupts on Master and Slave PIC
    outb(0x21, 0xFF);
    io_wait();
    outb(0xA1, 0xFF);
    io_wait();
}

/// Initializes the hardware interrupt subsystem.
///
/// # Safety
///
/// Disables legacy interrupt controllers; must be called during early boot.
pub unsafe fn irq_init() {
    pic_disable();
}
