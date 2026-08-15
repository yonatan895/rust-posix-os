//! x86_64 Low-Level Task Management and Context Switching.
//!
//! Implements hardware context frame setup, voluntary task switches via naked assembly,
//! Ring 3 privilege transitions, and CPU idle execution.

use super::gdt::{
    KERNEL_CODE_SEL, KERNEL_DATA_SEL, USER_CODE_SEL, USER_DATA_SEL, set_kernel_stack,
};
use super::idt::TrapFrame;
use super::syscall::SyscallRegisters;
use core::arch::naked_asm;

/// Safely updates the CPU's active kernel stack in the TSS and syscall MSR.
#[inline(always)]
pub fn switch_active_kernel_stack(stack_top: u64) {
    // SAFETY: Delegating to architecture GDT/TSS and MSR update function.
    unsafe {
        set_kernel_stack(stack_top);
    }
}

/// Initializes a kernel stack slice with an initial `TrapFrame` for Ring 3 userland entry.
pub fn init_user_kernel_stack(
    stack: &mut [u8],
    entry_point: usize,
    user_stack_top: usize,
) -> usize {
    assert!(stack.len() >= core::mem::size_of::<TrapFrame>());
    let offset = stack.len() - core::mem::size_of::<TrapFrame>();
    let frame_ptr = (stack.as_mut_ptr() as usize + offset) as *mut TrapFrame;
    // SAFETY: `frame_ptr` points inside the allocated kernel stack slice.
    unsafe {
        *frame_ptr = TrapFrame {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 0,
            r10: 0,
            r9: 0,
            r8: 0,
            rbp: 0,
            rdi: 0,
            rsi: 0,
            rdx: 0,
            rcx: 0,
            rbx: 0,
            rax: 0,
            rip: entry_point as u64,
            cs: USER_CODE_SEL as u64,
            rflags: 0x202, // IF=1
            rsp: user_stack_top as u64,
            ss: USER_DATA_SEL as u64,
        };
    }
    frame_ptr as usize
}

/// Initializes a child process kernel stack with a synthetic `TrapFrame` cloned from the parent `SyscallRegisters`.
///
/// Under the x86_64 fast syscall contract, `syscall` saves user RIP into `RCX` and user RFLAGS into `R11`.
/// Thus `TrapFrame.rip` receives `parent_regs.rcx` and `TrapFrame.rflags` receives `parent_regs.r11`,
/// while the GPR save slots `TrapFrame.rcx` and `TrapFrame.r11` are set to 0.
/// Returns the initial `saved_kernel_rsp` pointing to the TrapFrame with `rax = 0` (child return value).
pub fn init_fork_child_stack(stack: &mut [u8], parent_regs: &SyscallRegisters) -> usize {
    assert!(stack.len() >= core::mem::size_of::<TrapFrame>());
    let offset = stack.len() - core::mem::size_of::<TrapFrame>();
    let frame_ptr = (stack.as_mut_ptr() as usize + offset) as *mut TrapFrame;
    // SAFETY: `frame_ptr` points inside the valid `stack` slice.
    unsafe {
        *frame_ptr = TrapFrame {
            r15: parent_regs.r15 as u64,
            r14: parent_regs.r14 as u64,
            r13: parent_regs.r13 as u64,
            r12: parent_regs.r12 as u64,
            r11: 0, // Saved GPR slot (clobbered by syscall)
            r10: parent_regs.r10 as u64,
            r9: parent_regs.r9 as u64,
            r8: parent_regs.r8 as u64,
            rbp: parent_regs.rbp as u64,
            rdi: parent_regs.rdi as u64,
            rsi: parent_regs.rsi as u64,
            rdx: parent_regs.rdx as u64,
            rcx: 0, // Saved GPR slot (clobbered by syscall)
            rbx: parent_regs.rbx as u64,
            rax: 0,                      // Child return value from sys_fork = 0
            rip: parent_regs.rcx as u64, // Return RIP in userland right after syscall
            cs: USER_CODE_SEL as u64,
            rflags: parent_regs.r11 as u64, // User RFLAGS
            rsp: parent_regs.rsp as u64,    // User RSP
            ss: USER_DATA_SEL as u64,
        };
    }
    frame_ptr as usize
}

/// Initializes a kernel stack slice with an initial `TrapFrame` for Ring 0 kernel task entry.
pub fn init_kernel_task_stack(stack: &mut [u8], entry_point: usize) -> usize {
    let stack_top = stack.as_ptr() as usize + stack.len();
    assert!(stack.len() >= core::mem::size_of::<TrapFrame>());
    let offset = stack.len() - core::mem::size_of::<TrapFrame>();
    let frame_ptr = (stack.as_mut_ptr() as usize + offset) as *mut TrapFrame;
    // SAFETY: `frame_ptr` points inside the allocated kernel stack slice.
    unsafe {
        *frame_ptr = TrapFrame {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 0,
            r10: 0,
            r9: 0,
            r8: 0,
            rbp: 0,
            rdi: 0,
            rsi: 0,
            rdx: 0,
            rcx: 0,
            rbx: 0,
            rax: 0,
            rip: entry_point as u64,
            cs: KERNEL_CODE_SEL as u64,
            rflags: 0x202, // IF=1
            rsp: stack_top as u64,
            ss: KERNEL_DATA_SEL as u64,
        };
    }
    frame_ptr as usize
}

/// Performs a voluntary task switch by creating a kernel-mode `TrapFrame` on the outgoing stack
/// and restoring the incoming task's stack via standard `TrapFrame` / `iretq` execution.
///
/// # Safety
///
/// `prev_saved_rsp` must be a valid pointer to store the outgoing RSP into the PCB.
/// `next_saved_rsp` must point to a valid `TrapFrame` on the incoming task's kernel stack.
#[unsafe(naked)]
pub unsafe extern "C" fn voluntary_task_switch(
    _prev_saved_rsp: *mut usize,
    _next_saved_rsp: usize,
) {
    naked_asm!(
        // Push synthetic TrapFrame for returning to kernel mode (CS = 0x08, SS = 0x10)
        // Hardware frame: [SS, RSP, RFLAGS, CS, RIP]
        "push 0x10", // SS = KERNEL_DATA_SEL (0x10)
        "lea rax, [rsp + 8]",
        "push rax",  // RSP (stack before push)
        "pushfq",    // RFLAGS
        "push 0x08", // CS = KERNEL_CODE_SEL (0x08)
        "lea rax, [2f + rip]",
        "push rax", // RIP (resume point at label 2)
        // 15 GPRs: rax, rbx, rcx, rdx, rsi, rdi, rbp, r8..r15
        "push 0", // rax
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
        // Save current RSP directly into the outgoing PCB field (*prev_saved_rsp = rsp)
        "mov [rdi], rsp",
        // Load next_saved_rsp (rsi) into RSP
        "mov rsp, rsi",
        // Resume next task via unified TrapFrame pop + iretq
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
        "2:",
        "ret",
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

/// Halts the CPU waiting for the next hardware interrupt.
#[inline(always)]
pub fn idle() {
    // SAFETY: Enabling interrupts and halting CPU in Ring 0.
    unsafe {
        super::sti();
        super::hlt();
    }
}
