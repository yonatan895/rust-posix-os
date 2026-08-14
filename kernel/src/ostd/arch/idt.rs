//! Interrupt Descriptor Table (IDT) and Exception Handling.

use core::arch::asm;
use core::mem::size_of;
use super::gdt::KERNEL_CODE_SEL;
use super::read_cr2;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IdtEntry {
    pub offset_low: u16,
    pub selector: u16,
    pub ist: u8,
    pub type_attr: u8,
    pub offset_mid: u16,
    pub offset_high: u32,
    pub zero: u32,
}

impl IdtEntry {
    pub const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            zero: 0,
        }
    }

    pub fn set_handler(&mut self, handler: usize, ist: u8, dpl: u8) {
        self.offset_low = handler as u16;
        self.selector = KERNEL_CODE_SEL;
        self.ist = ist;
        self.type_attr = 0x8E | (dpl << 5); // Present, 64-bit Interrupt Gate
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.zero = 0;
    }
}

#[repr(C, packed)]
pub struct IdtDescriptor {
    pub limit: u16,
    pub base: u64,
}

#[repr(C, align(16))]
pub struct Idt {
    pub entries: [IdtEntry; 256],
}

static mut GLOBAL_IDT: Idt = Idt {
    entries: [IdtEntry::missing(); 256],
};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

// Handlers in Rust
#[no_mangle]
pub extern "C" fn rust_page_fault_handler(frame: *const InterruptFrame, error_code: u64) {
    let fault_addr = unsafe { read_cr2() };
    log::error!(
        "PAGE FAULT (#PF) at 0x{:016x}, Error Code: 0x{:x}, RIP: 0x{:016x}",
        fault_addr,
        error_code,
        unsafe { (*frame).rip }
    );
    loop {
        unsafe { asm!("hlt") };
    }
}

#[no_mangle]
pub extern "C" fn rust_general_protection_fault(frame: *const InterruptFrame, error_code: u64) {
    log::error!(
        "GENERAL PROTECTION FAULT (#GP), Error Code: 0x{:x}, RIP: 0x{:016x}",
        error_code,
        unsafe { (*frame).rip }
    );
    loop {
        unsafe { asm!("hlt") };
    }
}

pub static mut TIMER_TICKS: u64 = 0;

#[no_mangle]
pub extern "C" fn rust_timer_handler() {
    unsafe {
        TIMER_TICKS = TIMER_TICKS.wrapping_add(1);
    }
}

pub unsafe fn idt_init() {
    let descriptor = IdtDescriptor {
        limit: (size_of::<Idt>() - 1) as u16,
        base: &raw const GLOBAL_IDT as u64,
    };

    asm!("lidt [{}]", in(reg) &descriptor, options(nostack));
}
