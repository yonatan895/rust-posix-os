//! 8259 Programmable Interrupt Controller (PIC) Driver for x86_64.

use super::{inb, io_wait, outb};

/// Remaps the legacy dual 8259 Programmable Interrupt Controllers (PIC).
///
/// Master PIC IRQs (0..7) are remapped to vectors `offset1..offset1+7`.
/// Slave PIC IRQs (8..15) are remapped to vectors `offset2..offset2+7`.
///
/// # Safety
///
/// Directly programs legacy 8259 PIC hardware ports (`0x20`, `0x21`, `0xA0`, `0xA1`).
pub unsafe fn pic_remap(offset1: u8, offset2: u8) {
    // SAFETY: Programming 8259 PIC initialization command words (ICW1..ICW4).
    unsafe {
        // ICW1: Start initialization sequence in cascade mode
        outb(0x20, 0x11);
        io_wait();
        outb(0xA0, 0x11);
        io_wait();

        // ICW2: Vector offset mapping
        outb(0x21, offset1);
        io_wait();
        outb(0xA1, offset2);
        io_wait();

        // ICW3: Cascade configuration (Master has Slave on IRQ2; Slave cascade identity is 2)
        outb(0x21, 0x04);
        io_wait();
        outb(0xA1, 0x02);
        io_wait();

        // ICW4: Set 8086/88 mode
        outb(0x21, 0x01);
        io_wait();
        outb(0xA1, 0x01);
        io_wait();
    }
}

/// Disables all IRQ lines on both Master and Slave PICs.
///
/// # Safety
///
/// Directly manipulates legacy 8259 PIC hardware registers.
pub unsafe fn pic_disable() {
    // SAFETY: Masking all IRQ lines on Master and Slave PICs via I/O ports.
    unsafe {
        outb(0x21, 0xFF);
        io_wait();
        outb(0xA1, 0xFF);
        io_wait();
    }
}

/// Masks the specified IRQ line (0..15) on the 8259 PIC.
///
/// # Safety
///
/// Directly reads and writes PIC interrupt mask registers.
pub unsafe fn mask(irq: u8) {
    // SAFETY: Reading and modifying PIC mask register.
    unsafe {
        let port = if irq < 8 { 0x21 } else { 0xA1 };
        let bit = if irq < 8 { irq } else { irq - 8 };
        let val = inb(port) | (1 << bit);
        outb(port, val);
        io_wait();
    }
}

/// Unmasks the specified IRQ line (0..15) on the 8259 PIC.
///
/// # Safety
///
/// Directly reads and writes PIC interrupt mask registers.
pub unsafe fn unmask(irq: u8) {
    // SAFETY: Reading and modifying PIC mask register.
    unsafe {
        let port = if irq < 8 { 0x21 } else { 0xA1 };
        let bit = if irq < 8 { irq } else { irq - 8 };
        let val = inb(port) & !(1 << bit);
        outb(port, val);
        io_wait();
    }
}

/// Sends End of Interrupt (EOI) acknowledgment to the 8259 PIC.
///
/// # Safety
///
/// Directly sends EOI command byte `0x20` to PIC command registers.
pub unsafe fn send_eoi(irq: u8) {
    // SAFETY: Sending EOI acknowledgment byte 0x20 to PIC command port.
    unsafe {
        if irq >= 8 {
            outb(0xA0, 0x20);
        }
        outb(0x20, 0x20);
    }
}
