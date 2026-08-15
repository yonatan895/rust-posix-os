//! Interrupt Descriptor Table (IDT) and Exception Handling.

use super::gdt::KERNEL_CODE_SEL;
use super::read_cr2;
use core::arch::{asm, naked_asm};
use core::mem::size_of;

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

use core::cell::SyncUnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};

static GLOBAL_IDT: SyncUnsafeCell<Idt> = SyncUnsafeCell::new(Idt {
    entries: [IdtEntry::missing(); 256],
});

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapFrame {
    // General Purpose Registers pushed by ISR stub
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    // Hardware InterruptFrame pushed by CPU
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

// Handlers in Rust

/// Rust handler for Page Fault (#PF) exceptions.
///
/// # Safety
///
/// `frame` must point to a valid hardware exception stack frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_page_fault_handler(frame: *const InterruptFrame, error_code: u64) {
    // SAFETY: Reading CR2 register to obtain the faulting virtual address.
    let fault_addr = unsafe { read_cr2() };
    // SAFETY: Dereferencing valid hardware exception frame pointer passed by CPU.
    let rip = unsafe { (*frame).rip };
    log::error!(
        "PAGE FAULT (#PF) at 0x{:016x}, Error Code: 0x{:x}, RIP: 0x{:016x}",
        fault_addr,
        error_code,
        rip
    );
    loop {
        // SAFETY: Halting CPU on unrecoverable page fault.
        unsafe { asm!("hlt") };
    }
}

/// Rust handler for General Protection Fault (#GP) exceptions.
///
/// # Safety
///
/// `frame` must point to a valid hardware exception stack frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_general_protection_fault(
    frame: *const InterruptFrame,
    error_code: u64,
) {
    // SAFETY: Dereferencing valid hardware exception frame pointer passed by CPU.
    let rip = unsafe { (*frame).rip };
    log::error!(
        "GENERAL PROTECTION FAULT (#GP), Error Code: 0x{:x}, RIP: 0x{:016x}",
        error_code,
        rip
    );
    loop {
        // SAFETY: Halting CPU on unrecoverable general protection fault.
        unsafe { asm!("hlt") };
    }
}

pub static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

/// Rust timer tick handler called by `timer_interrupt_stub`.
///
/// Increments system ticks, acknowledges the interrupt controller (EOI),
/// and delegates to the scheduler for preemptive multitasking.
///
/// Returns the kernel stack pointer for the process that should resume execution.
#[unsafe(no_mangle)]
pub extern "C" fn rust_timer_tick_handler(current_rsp: usize) -> usize {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: Sending End of Interrupt for IRQ0 to PIC.
    unsafe {
        crate::ostd::irq::send_eoi(0);
    }
    let mut next_rsp = crate::services::scheduler::timer_tick_schedule(current_rsp);
    let target_pid =
        crate::services::process::CURRENT_PID.load(core::sync::atomic::Ordering::SeqCst);
    if target_pid > 0 && crate::services::ipc::SIGNALS.has_unblocked_signals(target_pid) {
        // SAFETY: next_rsp points to a valid TrapFrame on the kernel stack.
        let frame = unsafe { &mut *(next_rsp as *mut TrapFrame) };
        let terminated = crate::services::posix::check_and_deliver_signals_irq(frame, target_pid);
        if terminated {
            next_rsp = crate::services::scheduler::timer_tick_schedule(next_rsp);
        }
    }
    next_rsp
}

/// Naked assembly ISR entry stub for the timer interrupt (vector 0x20).
///
/// # Safety
///
/// Must only be jumped to directly by the CPU hardware during interrupt vector 0x20 handling.
#[unsafe(naked)]
pub unsafe extern "C" fn timer_interrupt_stub() {
    naked_asm!(
        // Push GPRs in reverse order of TrapFrame
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        // rdi = current stack pointer (pointing to TrapFrame)
        "mov rdi, rsp",
        "call rust_timer_tick_handler",
        // Switch to returned stack pointer (handles preemptive process switch)
        "mov rsp, rax",
        // Pop GPRs
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "iretq",
    );
}

/// Naked assembly entry stub for the General Protection Fault (#GP, vector 0x0D).
///
/// # Safety
///
/// Must only be jumped to directly by CPU hardware during exception vector 0x0D handling.
#[unsafe(naked)]
pub unsafe extern "C" fn gp_fault_stub() {
    naked_asm!(
        "mov rsi, [rsp]",     // error code
        "lea rdi, [rsp + 8]", // InterruptFrame pointer
        "call rust_general_protection_fault",
        "iretq",
    );
}

/// Naked assembly entry stub for the Page Fault (#PF, vector 0x0E).
///
/// # Safety
///
/// Must only be jumped to directly by CPU hardware during exception vector 0x0E handling.
#[unsafe(naked)]
pub unsafe extern "C" fn page_fault_stub() {
    naked_asm!(
        "mov rsi, [rsp]",     // error code
        "lea rdi, [rsp + 8]", // InterruptFrame pointer
        "call rust_page_fault_handler",
        "iretq",
    );
}

/// Loads the Interrupt Descriptor Table (IDT) register into the CPU and arms exception & timer vectors.
///
/// # Safety
///
/// Must be invoked during single-threaded boot initialization.
pub unsafe fn idt_init() {
    let idt_ptr = GLOBAL_IDT.get();

    // SAFETY: Arming exception and timer interrupt handlers in IDT during single-threaded boot.
    unsafe {
        (*idt_ptr).entries[0x0D].set_handler(gp_fault_stub as *const () as usize, 0, 0); // #GP
        (*idt_ptr).entries[0x0E].set_handler(page_fault_stub as *const () as usize, 0, 0); // #PF
        (*idt_ptr).entries[0x20].set_handler(timer_interrupt_stub as *const () as usize, 0, 0); // PIT Timer IRQ0
    }

    let descriptor = IdtDescriptor {
        limit: (size_of::<Idt>() - 1) as u16,
        base: idt_ptr as u64,
    };

    // SAFETY: Loading IDT descriptor into CPU via lidt instruction.
    unsafe {
        asm!("lidt [{}]", in(reg) &descriptor, options(nostack));
    }
}
