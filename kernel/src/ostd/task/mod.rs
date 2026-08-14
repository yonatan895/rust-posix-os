//! Task and Low-Level Context Switch Abstraction in OSTD.

pub mod async_task;
pub mod executor;

pub use async_task::yield_now;

use core::arch::naked_asm;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuContext {
    pub r15: usize,
    pub r14: usize,
    pub r13: usize,
    pub r12: usize,
    pub rbp: usize,
    pub rbx: usize,
    pub rip: usize,
    pub rsp: usize,
    pub rflags: usize,
}

/// Performs an architectural CPU register context switch between two threads.
///
/// # Safety
///
/// `prev_ctx` and `next_ctx` must be valid, aligned pointers to live `CpuContext` instances.
#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(_prev_ctx: *mut CpuContext, _next_ctx: *const CpuContext) {
    naked_asm!(
        // Save current callee-saved registers into prev_ctx (rdi)
        "mov [rdi + 0x00], r15",
        "mov [rdi + 0x08], r14",
        "mov [rdi + 0x10], r13",
        "mov [rdi + 0x18], r12",
        "mov [rdi + 0x20], rbp",
        "mov [rdi + 0x28], rbx",
        "lea rax, [2f + rip]",
        "mov [rdi + 0x30], rax", // rip
        "mov [rdi + 0x38], rsp", // rsp
        "pushfq",
        "pop rax",
        "mov [rdi + 0x40], rax", // rflags
        // Restore registers from next_ctx (rsi)
        "mov r15, [rsi + 0x00]",
        "mov r14, [rsi + 0x08]",
        "mov r13, [rsi + 0x10]",
        "mov r12, [rsi + 0x18]",
        "mov rbp, [rsi + 0x20]",
        "mov rbx, [rsi + 0x28]",
        "mov rsp, [rsi + 0x38]",
        "push [rsi + 0x40]",
        "popfq",
        "jmp [rsi + 0x30]", // Jump to next_ctx.rip
        "2:",
        "ret"
    );
}

/// Transitions the CPU to Ring 3 (user mode) and begins executing userland code.
///
/// # Safety
///
/// `entry_point` must be a valid executable user address, `user_stack_top` must be a valid user stack address,
/// and `pml4_phys` must be a valid page table physical address.
#[unsafe(naked)]
pub unsafe extern "C" fn enter_user_mode(
    _entry_point: usize,
    _user_stack_top: usize,
    _pml4_phys: usize,
) -> ! {
    naked_asm!(
        // rdi: entry_point
        // rsi: user_stack_top
        // rdx: pml4_phys

        // Switch to process address space
        "mov cr3, rdx",
        // Push iretq frame: [SS, RSP, RFLAGS, CS, RIP]
        "push 0x1b",  // User Data Segment (SS = 0x18 | 3)
        "push rsi",   // User Stack Pointer (RSP)
        "push 0x202", // RFLAGS (IF = 1)
        "push 0x23",  // User Code Segment (CS = 0x20 | 3)
        "push rdi",   // User Instruction Pointer (RIP)
        // Clear general registers
        "xor rax, rax",
        "xor rbx, rbx",
        "xor rcx, rcx",
        "xor rdx, rdx",
        "xor rdi, rdi",
        "xor rsi, rsi",
        "xor rbp, rbp",
        "xor r8, r8",
        "xor r9, r9",
        "xor r10, r10",
        "xor r11, r11",
        "xor r12, r12",
        "xor r13, r13",
        "xor r14, r14",
        "xor r15, r15",
        "iretq",
    );
}
