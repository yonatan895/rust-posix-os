//! Interrupt Controller & Timers in OSTD.

use crate::ostd::arch::{outb, io_wait};

pub unsafe fn pic_disable() {
    // Mask all interrupts on Master and Slave PIC
    outb(0x21, 0xFF);
    io_wait();
    outb(0xA1, 0xFF);
    io_wait();
}

pub unsafe fn irq_init() {
    pic_disable();
}
