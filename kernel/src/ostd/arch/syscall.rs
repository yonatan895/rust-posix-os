//! Fast x86_64 System Call Subsystem (MSR LSTAR).

use core::arch::naked_asm;
use super::{rdmsr, wrmsr};
use super::gdt::KERNEL_CODE_SEL;

const IA32_EFER: u32 = 0xC0000080;
const IA32_STAR: u32 = 0xC0000081;
const IA32_LSTAR: u32 = 0xC0000082;
const IA32_FMASK: u32 = 0xC0000084;
const IA32_KERNEL_GS_BASE: u32 = 0xC0000102;

#[repr(C, align(16))]
pub struct PerCpuData {
    pub kernel_rsp: u64,
    pub user_rsp: u64,
}

static mut BSP_PER_CPU: PerCpuData = PerCpuData {
    kernel_rsp: 0,
    user_rsp: 0,
};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SyscallRegisters {
    pub r15: usize,
    pub r14: usize,
    pub r13: usize,
    pub r12: usize,
    pub rbp: usize,
    pub rbx: usize,
    pub r9: usize,
    pub r8: usize,
    pub r10: usize,
    pub rdx: usize,
    pub rsi: usize,
    pub rdi: usize,
    pub rax: usize, // Syscall number on entry, return value on exit
    pub rcx: usize, // Saved RIP by syscall instruction
    pub r11: usize, // Saved RFLAGS by syscall instruction
    pub rsp: usize, // User RSP
}

/// Naked entry point for the x86_64 fast `syscall` instruction.
///
/// # Safety
///
/// Must only be jumped to directly by CPU hardware during the `syscall` instruction.
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() {
    naked_asm!(
        // Swap to kernel GS base
        "swapgs",
        "mov gs:[8], rsp",       // Save user RSP in PerCpuData.user_rsp
        "mov rsp, gs:[0]",       // Load kernel RSP from PerCpuData.kernel_rsp

        // Push registers to build SyscallRegisters structure
        "push gs:[8]",           // User RSP
        "push r11",              // User RFLAGS
        "push rcx",              // User RIP
        "push rax",              // Syscall number
        "push rdi",              // Arg 1
        "push rsi",              // Arg 2
        "push rdx",              // Arg 3
        "push r10",              // Arg 4 (POSIX x86_64 ABI uses r10 for syscall arg 4)
        "push r8",               // Arg 5
        "push r9",               // Arg 6
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // Pass pointer to SyscallRegisters as first argument (rdi)
        "mov rdi, rsp",
        "call rust_syscall_dispatcher",

        // Restore registers
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        "pop r9",
        "pop r8",
        "pop r10",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rax",               // Return value from syscall dispatcher
        "pop rcx",               // Restore RIP
        "pop r11",              // Restore RFLAGS
        "pop rsp",               // Restore user RSP
        "swapgs",
        "sysretq"
    );
}

/// Programs CPU Model-Specific Registers (STAR, LSTAR, FMASK, EFER) for fast system call dispatch.
///
/// # Safety
///
/// Must be invoked during boot before entering user mode with a valid kernel stack address.
pub unsafe fn syscall_init(kernel_stack_top: u64) {
    // 0. Initialize PerCpuData and Kernel GS Base
    BSP_PER_CPU.kernel_rsp = kernel_stack_top;
    wrmsr(IA32_KERNEL_GS_BASE, &raw const BSP_PER_CPU as u64);

    // 1. Enable System Call Extensions (SCE) in EFER
    let efer = rdmsr(IA32_EFER);
    wrmsr(IA32_EFER, efer | 1);

    // 2. Configure STAR MSR:
    // Bits 47:32 = Kernel CS (0x08)
    // Bits 63:48 = Base for User SS/CS (0x10 -> SS = 0x18 | 3 = 0x1B, CS = 0x20 | 3 = 0x23)
    let star = ((KERNEL_CODE_SEL as u64) << 32) | (0x10u64 << 48);
    wrmsr(IA32_STAR, star);

    // 3. Configure LSTAR MSR with address of syscall_entry
    wrmsr(IA32_LSTAR, syscall_entry as *const () as usize as u64);

    // 4. Configure FMASK MSR to clear IF (interrupts), DF, TF upon syscall entry
    let fmask = 0x200 | 0x100 | 0x400; // IF | TF | DF
    wrmsr(IA32_FMASK, fmask);
}

/// Syscall dispatcher bridge called from `syscall_entry` assembly.
///
/// # Safety
///
/// `regs` must either be null or point to a live `SyscallRegisters` frame on the current stack.
#[no_mangle]
pub unsafe extern "C" fn rust_syscall_dispatcher(regs: *mut SyscallRegisters) -> usize {
    crate::ostd::mm::with_syscall_regs(regs, crate::services::posix::dispatch_syscall)
}
