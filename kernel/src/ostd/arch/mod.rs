//! x86_64 Architecture Hardware Support.

pub mod gdt;
pub mod idt;
pub mod syscall;

use core::arch::asm;

/// Writes an 8-bit value to the specified I/O port.
///
/// # Safety
///
/// Writing to an I/O port can directly affect hardware state and may cause system instability.
#[inline(always)]
pub unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

/// Reads an 8-bit value from the specified I/O port.
///
/// # Safety
///
/// Reading from an I/O port can have hardware side effects.
#[inline(always)]
pub unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    }
    val
}

/// Writes a 16-bit value to the specified I/O port.
///
/// # Safety
///
/// Writing to an I/O port can directly affect hardware state and may cause system instability.
#[inline(always)]
pub unsafe fn outw(port: u16, val: u16) {
    unsafe {
        asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack, preserves_flags));
    }
}

/// Reads a 16-bit value from the specified I/O port.
///
/// # Safety
///
/// Reading from an I/O port can have hardware side effects.
#[inline(always)]
pub unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    unsafe {
        asm!("in ax, dx", in("dx") port, out("ax") val, options(nomem, nostack, preserves_flags));
    }
    val
}

/// Writes a 32-bit value to the specified I/O port.
///
/// # Safety
///
/// Writing to an I/O port can directly affect hardware state and may cause system instability.
#[inline(always)]
pub unsafe fn outl(port: u16, val: u32) {
    unsafe {
        asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack, preserves_flags));
    }
}

/// Reads a 32-bit value from the specified I/O port.
///
/// # Safety
///
/// Reading from an I/O port can have hardware side effects.
#[inline(always)]
pub unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    unsafe {
        asm!("in eax, dx", in("dx") port, out("eax") val, options(nomem, nostack, preserves_flags));
    }
    val
}

/// Waits for an I/O operation to complete by writing to port 0x80.
///
/// # Safety
///
/// Modifies port 0x80 diagnostics port state.
#[inline(always)]
pub unsafe fn io_wait() {
    unsafe {
        outb(0x80, 0);
    }
}

/// Writes a 64-bit value to a Model-Specific Register (MSR).
///
/// # Safety
///
/// Writing invalid values or writing to unsupported MSRs causes CPU general protection faults.
#[inline(always)]
pub unsafe fn wrmsr(msr: u32, val: u64) {
    let low = val as u32;
    let high = (val >> 32) as u32;
    unsafe {
        asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high, options(nostack, preserves_flags));
    }
}

/// Reads a 64-bit value from a Model-Specific Register (MSR).
///
/// # Safety
///
/// Reading from unsupported MSRs causes CPU general protection faults.
#[inline(always)]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high, options(nostack, preserves_flags));
    }
    ((high as u64) << 32) | (low as u64)
}

/// Halts the CPU until the next external interrupt.
///
/// # Safety
///
/// Execution stops until an interrupt occurs.
#[inline(always)]
pub unsafe fn hlt() {
    unsafe {
        asm!("hlt", options(nomem, nostack));
    }
}

/// Clears the interrupt flag (disables interrupts).
///
/// # Safety
///
/// Disabling interrupts affects scheduling and device responsiveness.
#[inline(always)]
pub unsafe fn cli() {
    unsafe {
        asm!("cli", options(nomem, nostack));
    }
}

/// Sets the interrupt flag (enables interrupts).
///
/// # Safety
///
/// Enabling interrupts allows CPU interrupt handlers to execute.
#[inline(always)]
pub unsafe fn sti() {
    unsafe {
        asm!("sti", options(nomem, nostack));
    }
}

/// Read the current CPU RFLAGS register.
///
/// # Safety
///
/// Reads architectural CPU flags register directly.
#[inline(always)]
pub unsafe fn read_rflags() -> u64 {
    let val: u64;
    unsafe {
        asm!("pushfq", "pop {}", out(reg) val, options(nomem, preserves_flags));
    }
    val
}

/// Restore the CPU RFLAGS register (including interrupt enable state).
///
/// # Safety
///
/// Restoring invalid CPU flags or changing interrupt state can disrupt kernel execution.
#[inline(always)]
pub unsafe fn restore_rflags(val: u64) {
    unsafe {
        asm!("push {}", "popfq", in(reg) val, options(nomem));
    }
}

/// Reads the CR2 register containing the linear address that caused a page fault.
///
/// # Safety
///
/// Reads architectural CPU control register directly.
#[inline(always)]
pub unsafe fn read_cr2() -> u64 {
    let val: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) val, options(nomem, nostack));
    }
    val
}

/// Reads the CR3 register containing the physical address of the active PML4 table.
///
/// # Safety
///
/// Reads architectural CPU control register directly.
#[inline(always)]
pub unsafe fn read_cr3() -> u64 {
    let val: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) val, options(nomem, nostack));
    }
    val
}

/// Writes a new physical base address to the CR3 register, switching the page table.
///
/// # Safety
///
/// Writing an invalid PML4 physical address causes immediate page faults and triple faults.
#[inline(always)]
pub unsafe fn write_cr3(val: u64) {
    unsafe {
        asm!("mov cr3, {}", in(reg) val, options(nostack));
    }
}
