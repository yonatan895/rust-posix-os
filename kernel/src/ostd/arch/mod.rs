//! x86_64 Architecture Hardware Support.

pub mod gdt;
pub mod idt;
pub mod syscall;

use core::arch::asm;

#[inline(always)]
pub unsafe fn outb(port: u16, val: u8) {
    asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
}

#[inline(always)]
pub unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    val
}

#[inline(always)]
pub unsafe fn outw(port: u16, val: u16) {
    asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack, preserves_flags));
}

#[inline(always)]
pub unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    asm!("in ax, dx", in("dx") port, out("ax") val, options(nomem, nostack, preserves_flags));
    val
}

#[inline(always)]
pub unsafe fn outl(port: u16, val: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack, preserves_flags));
}

#[inline(always)]
pub unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    asm!("in eax, dx", in("dx") port, out("eax") val, options(nomem, nostack, preserves_flags));
    val
}

#[inline(always)]
pub unsafe fn io_wait() {
    outb(0x80, 0);
}

#[inline(always)]
pub unsafe fn wrmsr(msr: u32, val: u64) {
    let low = val as u32;
    let high = (val >> 32) as u32;
    asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high, options(nostack, preserves_flags));
}

#[inline(always)]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high, options(nostack, preserves_flags));
    ((high as u64) << 32) | (low as u64)
}

#[inline(always)]
pub unsafe fn hlt() {
    asm!("hlt", options(nomem, nostack));
}

#[inline(always)]
pub unsafe fn cli() {
    asm!("cli", options(nomem, nostack));
}

#[inline(always)]
pub unsafe fn sti() {
    asm!("sti", options(nomem, nostack));
}

#[inline(always)]
pub unsafe fn read_cr2() -> u64 {
    let val: u64;
    asm!("mov {}, cr2", out(reg) val, options(nomem, nostack));
    val
}

#[inline(always)]
pub unsafe fn read_cr3() -> u64 {
    let val: u64;
    asm!("mov {}, cr3", out(reg) val, options(nomem, nostack));
    val
}

#[inline(always)]
pub unsafe fn write_cr3(val: u64) {
    asm!("mov cr3, {}", in(reg) val, options(nostack));
}
