//! Interrupt Controller & Hardware Timers in OSTD.

use crate::ostd::arch::{io_wait, outb};

pub const PIT_FREQUENCY_HZ: u32 = 100;
pub const PIT_BASE_FREQUENCY_HZ: u32 = 1_193_182;
pub const PIT_DIVISOR: u16 = (PIT_BASE_FREQUENCY_HZ / PIT_FREQUENCY_HZ) as u16; // 11931 = 0x2E9B

/// Remaps the legacy dual 8259 Programmable Interrupt Controllers (PIC).
///
/// Master PIC IRQs (0..7) are remapped to vectors `offset1..offset1+7`.
/// Slave PIC IRQs (8..15) are remapped to vectors `offset2..offset2+7`.
///
/// # Safety
///
/// Directly programs legacy 8259 PIC hardware ports (`0x20`, `0x21`, `0xA0`, `0xA1`).
pub unsafe fn pic_remap(offset1: u8, offset2: u8) {
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
    unsafe {
        outb(0x21, 0xFF);
        io_wait();
        outb(0xA1, 0xFF);
        io_wait();
    }
}

/// Sends End of Interrupt (EOI) acknowledgment to the 8259 PIC.
///
/// # Safety
///
/// Directly sends EOI command byte `0x20` to PIC command registers.
pub unsafe fn send_eoi(irq: u8) {
    unsafe {
        if irq >= 8 {
            outb(0xA0, 0x20);
        }
        outb(0x20, 0x20);
    }
}

/// Initializes the 8254 Programmable Interval Timer (PIT) Channel 0 for periodic 100 Hz interrupts.
///
/// # Safety
///
/// Directly writes configuration commands and reload counts to PIT I/O ports (`0x43`, `0x40`).
pub unsafe fn pit_init() {
    unsafe {
        // Mode/Command register (0x43): Channel 0, Access lo/hi byte, Mode 3 (Square Wave), Binary 16-bit
        outb(0x43, 0x36);
        io_wait();

        // Channel 0 Data port (0x40): Write low byte, then high byte
        let divisor = PIT_DIVISOR;
        outb(0x40, (divisor & 0xFF) as u8);
        io_wait();
        outb(0x40, ((divisor >> 8) & 0xFF) as u8);
        io_wait();
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
    unsafe {
        // Remap PIC: Master -> 0x20..0x27, Slave -> 0x28..0x2F
        pic_remap(0x20, 0x28);

        // Program PIT Channel 0 at 100 Hz
        pit_init();

        // Unmask IRQ0 (timer) on Master PIC (bit 0 = 0), mask all other IRQs (0xFE)
        outb(0x21, 0xFE);
        io_wait();
        // Mask all Slave IRQs (0xFF)
        outb(0xA1, 0xFF);
        io_wait();
    }
}
