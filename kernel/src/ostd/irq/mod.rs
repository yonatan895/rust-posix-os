//! Interrupt Controller & Hardware Timers in OSTD.
//!
//! ## Portable API Boundary
//! This module defines the architecture-neutral interrupt and timer interface for OSTD.
//! - On `x86_64`, `mask`, `unmask`, and `send_eoi` currently delegate to the dual 8259 PIC driver,
//!   and `ack_timer()` acknowledges PIC IRQ0 (8254 PIT line).
//! - Architecture-specific controller details remain strictly isolated within `ostd::arch::x86_64`.
//! - Future architecture ports (e.g. GIC on `aarch64`, PLIC/SBI on `riscv64`) will implement this
//!   same portable interface without inheriting legacy x86 PIC/IRQ0 semantics.

use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_arch = "x86_64")]
use crate::ostd::arch::x86_64::{io_wait, outb, pic, pit};

#[cfg(target_arch = "x86_64")]
pub use crate::ostd::arch::x86_64::pit::{
    PIT_BASE_FREQUENCY_HZ, PIT_DIVISOR, PIT_FREQUENCY_HZ, PIT_MAX_FREQUENCY_HZ,
    PIT_MIN_FREQUENCY_HZ, pit_calc_divisor, pit_effective_freq,
};

pub(crate) static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

/// Opaque, one-shot CPU interrupt state token used for safe, portable `irq_save` and `irq_restore`.
///
/// The internal representation is private and non-copyable to prevent manual synthesis or replay
/// of CPU interrupt flags.
#[derive(Debug, PartialEq, Eq)]
pub struct IrqFlags(usize);

/// RAII guard that disables CPU interrupts upon creation and restores previous interrupt state on drop.
pub struct IrqGuard {
    flags: Option<IrqFlags>,
}

impl IrqGuard {
    /// Creates a new `IrqGuard`, saving previous interrupt state and disabling CPU interrupts.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            flags: Some(irq_save()),
        }
    }

    /// Disarms the guard without restoring CPU interrupts on drop.
    #[inline(always)]
    pub fn disarm(&mut self) {
        self.flags = None;
    }
}

impl Default for IrqGuard {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for IrqGuard {
    #[inline(always)]
    fn drop(&mut self) {
        if let Some(flags) = self.flags.take() {
            irq_restore(flags);
        }
    }
}

/// Executes a closure with CPU interrupts disabled, restoring the previous interrupt state on completion.
#[inline(always)]
pub fn without_interrupts<R, F: FnOnce() -> R>(f: F) -> R {
    let _guard = IrqGuard::new();
    f()
}

/// Disables CPU interrupts and returns an opaque, one-shot interrupt state token.
#[inline(always)]
pub fn irq_save() -> IrqFlags {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Reading architectural RFLAGS and clearing IF flag.
    unsafe {
        let flags = crate::ostd::arch::x86_64::read_rflags() as usize;
        crate::ostd::arch::x86_64::cli();
        IrqFlags(flags)
    }
    #[cfg(not(target_arch = "x86_64"))]
    unimplemented!("irq_save not implemented for this architecture")
}

/// Restores the CPU interrupt state from a previously captured one-shot `IrqFlags` token.
///
/// Consumes `flags` by value to ensure tokens cannot be replayed.
#[inline(always)]
pub fn irq_restore(flags: IrqFlags) {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Restoring architectural RFLAGS register from token.
    unsafe {
        crate::ostd::arch::x86_64::restore_rflags(flags.0 as u64);
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = flags;
        unimplemented!("irq_restore not implemented for this architecture");
    }
}

/// Enables CPU interrupts.
#[inline(always)]
pub fn enable() {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Enabling interrupts via sti instruction.
    unsafe {
        crate::ostd::arch::x86_64::sti();
    }
    #[cfg(not(target_arch = "x86_64"))]
    unimplemented!("irq::enable not implemented for this architecture");
}

/// Disables CPU interrupts.
#[inline(always)]
pub fn disable() {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Disabling interrupts via cli instruction.
    unsafe {
        crate::ostd::arch::x86_64::cli();
    }
    #[cfg(not(target_arch = "x86_64"))]
    unimplemented!("irq::disable not implemented for this architecture");
}

/// Checks if CPU interrupts are currently enabled.
#[inline(always)]
pub fn is_enabled() -> bool {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Checking IF flag in RFLAGS register.
    unsafe {
        (crate::ostd::arch::x86_64::read_rflags() & 0x200) != 0
    }
    #[cfg(not(target_arch = "x86_64"))]
    unimplemented!("irq::is_enabled not implemented for this architecture")
}

/// Masks a hardware IRQ line.
#[inline(always)]
pub fn mask(irq: u8) {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Delegating IRQ masking to PIC driver.
    unsafe {
        pic::mask(irq);
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = irq;
        unimplemented!("irq::mask not implemented for this architecture");
    }
}

/// Unmasks a hardware IRQ line.
#[inline(always)]
pub fn unmask(irq: u8) {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Delegating IRQ unmasking to PIC driver.
    unsafe {
        pic::unmask(irq);
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = irq;
        unimplemented!("irq::unmask not implemented for this architecture");
    }
}

/// Sends End of Interrupt (EOI) acknowledgment to the interrupt controller.
///
/// # Safety
///
/// Directly sends EOI command to the hardware interrupt controller.
#[inline(always)]
pub unsafe fn send_eoi(irq: u8) {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Forwarding EOI to architecture-specific PIC handler.
    unsafe {
        pic::send_eoi(irq);
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = irq;
        unimplemented!("send_eoi not implemented for this architecture");
    }
}

/// Configures and starts the hardware periodic timer at a specified frequency in Hz.
pub fn init_timer(hz: u32) {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Programming PIT timer at requested frequency.
    unsafe {
        pit::pit_init_hz(hz);
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = hz;
        unimplemented!("init_timer not implemented for this architecture");
    }
}

/// Acknowledges the periodic timer interrupt.
///
/// Note: On x86_64, this sends EOI to PIC IRQ0 (8254 PIT), an architecture-specific
/// detail encapsulated behind this portable API.
#[inline(always)]
pub fn ack_timer() {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Acknowledging timer IRQ0 on PIC.
    unsafe {
        send_eoi(0);
    }
    #[cfg(not(target_arch = "x86_64"))]
    unimplemented!("ack_timer not implemented for this architecture");
}

/// Returns the current total system timer ticks elapsed since boot.
#[inline(always)]
pub fn ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Relaxed)
}

/// Increments and returns the new total system timer ticks elapsed since boot.
#[inline(always)]
pub fn tick() -> u64 {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed) + 1
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

        // Program PIT Channel 0 at 100 Hz via portable timer abstraction
        init_timer(PIT_FREQUENCY_HZ);

        // Unmask IRQ0 (timer) on Master PIC (bit 0 = 0), mask all other IRQs (0xFE)
        outb(0x21, 0xFE);
        io_wait();
        // Mask all Slave IRQs (0xFF)
        outb(0xA1, 0xFF);
        io_wait();
    }
    #[cfg(not(target_arch = "x86_64"))]
    unimplemented!("irq_init not implemented for this architecture");
}
