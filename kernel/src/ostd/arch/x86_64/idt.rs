//! Interrupt Descriptor Table (IDT) and Exception Handling for x86_64.

use super::gdt::KERNEL_CODE_SEL;
use super::read_cr2;
use core::arch::{asm, naked_asm};
use core::mem::size_of;

/// x86_64 16-byte Interrupt Descriptor Table (IDT) gate entry.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IdtEntry {
    /// Lower 16 bits of the target ISR address (bits 0..15).
    pub offset_low: u16,
    /// Code segment selector in the GDT.
    pub selector: u16,
    /// Interrupt Stack Table (IST) offset (bits 0..2); remaining bits reserved.
    pub ist: u8,
    /// Type and attribute flags (Present bit, DPL, Gate Type).
    pub type_attr: u8,
    /// Middle 16 bits of the target ISR address (bits 16..31).
    pub offset_mid: u16,
    /// Upper 32 bits of the target ISR address (bits 32..63).
    pub offset_high: u32,
    /// Reserved; must be zero.
    pub zero: u32,
}

impl IdtEntry {
    /// Creates an empty, absent IDT gate entry.
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

    /// Sets the gate handler virtual address, IST stack index, and Descriptor Privilege Level (DPL).
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

/// x86_64 Interrupt Descriptor Table Register (IDTR) descriptor format.
#[repr(C, packed)]
pub struct IdtDescriptor {
    /// Limit of the IDT in bytes minus 1.
    pub limit: u16,
    /// Linear base address of the IDT.
    pub base: u64,
}

/// 256-entry x86_64 Interrupt Descriptor Table.
#[repr(C, align(16))]
pub struct Idt {
    /// Vector entries 0..255 for CPU exceptions and external IRQs.
    pub entries: [IdtEntry; 256],
}

use core::cell::SyncUnsafeCell;

/// Global static Interrupt Descriptor Table for the BSP.
static GLOBAL_IDT: SyncUnsafeCell<Idt> = SyncUnsafeCell::new(Idt {
    entries: [IdtEntry::missing(); 256],
});

/// Hardware exception stack frame pushed by the x86_64 CPU on interrupt/exception entry.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptFrame {
    /// Saved instruction pointer (RIP) pointing to resume location.
    pub rip: u64,
    /// Saved code segment selector (CS).
    pub cs: u64,
    /// Saved CPU condition flags register (RFLAGS).
    pub rflags: u64,
    /// Saved stack pointer (RSP).
    pub rsp: u64,
    /// Saved stack segment selector (SS).
    pub ss: u64,
}

/// Complete architectural register context frame saved across interrupts and task switches.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapFrame {
    /// General-purpose register R15.
    pub r15: u64,
    /// General-purpose register R14.
    pub r14: u64,
    /// General-purpose register R13.
    pub r13: u64,
    /// General-purpose register R12.
    pub r12: u64,
    /// General-purpose register R11.
    pub r11: u64,
    /// General-purpose register R10.
    pub r10: u64,
    /// General-purpose register R9.
    pub r9: u64,
    /// General-purpose register R8.
    pub r8: u64,
    /// Base pointer register RBP.
    pub rbp: u64,
    /// Destination index register RDI (System V ABI arg 1).
    pub rdi: u64,
    /// Source index register RSI (System V ABI arg 2).
    pub rsi: u64,
    /// Data register RDX (System V ABI arg 3).
    pub rdx: u64,
    /// Count register RCX (System V ABI arg 4).
    pub rcx: u64,
    /// Base register RBX.
    pub rbx: u64,
    /// Accumulator register RAX (return value register).
    pub rax: u64,
    /// Saved instruction pointer (RIP) from CPU hardware frame.
    pub rip: u64,
    /// Saved code segment selector (CS) from CPU hardware frame.
    pub cs: u64,
    /// Saved RFLAGS register from CPU hardware frame.
    pub rflags: u64,
    /// Saved stack pointer (RSP) from CPU hardware frame.
    pub rsp: u64,
    /// Saved stack segment selector (SS) from CPU hardware frame.
    pub ss: u64,
}

impl TrapFrame {
    /// Returns true if this TrapFrame was captured while executing in Ring 3 (User Mode).
    ///
    /// Checks that the Code Segment register has RPL = 3 (`cs & 3 == 3`) or matches `USER_CODE_SEL` (`0x20 | 3 = 0x23`).
    #[inline(always)]
    pub fn is_user_mode(&self) -> bool {
        (self.cs & 3) == 3 || (self.cs as u16) == super::gdt::USER_CODE_SEL
    }
}

// Handlers in Rust

/// Rust handler for Page Fault (#PF) exceptions.
///
/// # Safety
///
/// `frame` must point to a valid hardware exception stack frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_page_fault_handler(frame: *const InterruptFrame, error_code: u64) {
    // SAFETY: Reading architectural CR2 register to obtain the faulting linear virtual address.
    let fault_addr = unsafe { read_cr2() };
    // SAFETY: Dereferencing valid hardware exception stack frame pointer supplied by the CPU.
    let rip = unsafe { (*frame).rip };
    log::error!(
        "PAGE FAULT (#PF) at 0x{:016x}, Error Code: 0x{:x}, RIP: 0x{:016x}",
        fault_addr,
        error_code,
        rip
    );
    loop {
        // SAFETY: Halting CPU indefinitely on unrecoverable kernel page fault.
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
    // SAFETY: Dereferencing valid hardware exception stack frame pointer supplied by the CPU.
    let rip = unsafe { (*frame).rip };
    log::error!(
        "GENERAL PROTECTION FAULT (#GP), Error Code: 0x{:x}, RIP: 0x{:016x}",
        error_code,
        rip
    );
    loop {
        // SAFETY: Halting CPU indefinitely on unrecoverable general protection fault.
        unsafe { asm!("hlt") };
    }
}

/// Rust timer tick handler called by `timer_interrupt_stub`.
///
/// Increments system ticks, acknowledges the interrupt controller (EOI),
/// and delegates to the scheduler for preemptive multitasking.
///
/// Returns the kernel stack pointer for the process that should resume execution.
#[unsafe(no_mangle)]
pub extern "C" fn rust_timer_tick_handler(current_rsp: usize) -> usize {
    crate::ostd::irq::tick();
    crate::ostd::irq::ack_timer();
    let mut next_rsp = crate::services::scheduler::timer_tick_schedule(current_rsp);
    let target_pid =
        crate::services::process::CURRENT_PID.load(core::sync::atomic::Ordering::SeqCst);
    if target_pid > 0 && crate::services::ipc::SIGNALS.has_unblocked_signals(target_pid) {
        // SAFETY: next_rsp points to a valid TrapFrame allocated and preserved on the task kernel stack during the interrupt transition.
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

    // SAFETY: Initializing IDT entries for #GP (0x0D), #PF (0x0E), and PIT timer (0x20) in statically allocated GLOBAL_IDT during single-threaded boot initialization.
    unsafe {
        (*idt_ptr).entries[0x0D].set_handler(gp_fault_stub as *const () as usize, 0, 0); // #GP
        (*idt_ptr).entries[0x0E].set_handler(page_fault_stub as *const () as usize, 0, 0); // #PF
        (*idt_ptr).entries[0x20].set_handler(timer_interrupt_stub as *const () as usize, 0, 0); // PIT Timer IRQ0
    }

    let descriptor = IdtDescriptor {
        limit: (size_of::<Idt>() - 1) as u16,
        base: idt_ptr as u64,
    };

    // SAFETY: Loading IDT register with valid base and limit pointing to static GLOBAL_IDT via lidt instruction.
    unsafe {
        asm!("lidt [{}]", in(reg) &descriptor, options(nostack));
    }
}
