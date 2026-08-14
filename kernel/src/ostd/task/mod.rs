//! Task and Low-Level Context Switch Abstraction in OSTD.

pub mod async_task;
pub mod executor;

pub use async_task::yield_now;

use crate::ostd::arch::gdt::{KERNEL_CODE_SEL, KERNEL_DATA_SEL, USER_CODE_SEL, USER_DATA_SEL};
use crate::ostd::arch::idt::TrapFrame;
use core::arch::naked_asm;

pub const KERNEL_STACK_SIZE: usize = 16 * 1024; // 16 KiB

/// Safely switches the CPU's active kernel stack in the TSS and syscall MSR.
pub fn switch_active_kernel_stack(stack_top: u64) {
    unsafe {
        crate::ostd::arch::gdt::set_kernel_stack(stack_top);
    }
}

/// Safely performs a task switch from `prev_saved_rsp` to `next_saved_rsp` under interrupt masking.
pub fn switch_tasks(prev_saved_rsp: &core::sync::atomic::AtomicUsize, next_saved_rsp: usize) {
    unsafe {
        crate::ostd::arch::cli();
        voluntary_task_switch(prev_saved_rsp.as_ptr(), next_saved_rsp);
        crate::ostd::arch::sti();
    }
}

/// Kernel idle loop running with interrupts enabled.
pub extern "C" fn kernel_idle_loop() -> ! {
    loop {
        unsafe {
            crate::ostd::arch::sti();
            crate::ostd::arch::hlt();
        }
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

/// Initializes a kernel stack slice with an initial `TrapFrame` for Ring 0 kernel task entry.
pub fn init_kernel_task_stack(stack: &mut [u8], entry_point: usize) -> usize {
    let stack_top = stack.as_ptr() as usize + stack.len();
    assert!(stack.len() >= core::mem::size_of::<TrapFrame>());
    let offset = stack.len() - core::mem::size_of::<TrapFrame>();
    let frame_ptr = (stack.as_mut_ptr() as usize + offset) as *mut TrapFrame;
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
        "lea rax, [1f + rip]",
        "push rax", // RIP (resume point at label 1)
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
        "1:",
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
