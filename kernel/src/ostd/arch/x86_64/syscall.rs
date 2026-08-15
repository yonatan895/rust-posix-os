//! Fast x86_64 System Call Subsystem (MSR LSTAR).

use super::gdt::KERNEL_CODE_SEL;
use super::{rdmsr, wrmsr};
use core::arch::naked_asm;

/// Extended Feature Enable Register (EFER) MSR address.
const IA32_EFER: u32 = 0xC0000080;
/// System Call Target Address (STAR) MSR address.
const IA32_STAR: u32 = 0xC0000081;
/// Long Mode System Call Target Address (LSTAR) MSR address.
const IA32_LSTAR: u32 = 0xC0000082;
/// System Call Flag Mask (FMASK) MSR address.
const IA32_FMASK: u32 = 0xC0000084;
/// Kernel GS Base MSR address used by `swapgs`.
const IA32_KERNEL_GS_BASE: u32 = 0xC0000102;

use core::cell::SyncUnsafeCell;

/// Per-CPU scratch data referenced via the GS base register during fast `syscall` entry.
#[repr(C, align(16))]
pub struct PerCpuData {
    /// Active kernel stack pointer loaded upon entering `syscall_entry`.
    pub kernel_rsp: u64,
    /// Temporary save slot for the user stack pointer upon `syscall_entry`.
    pub user_rsp: u64,
}

/// Global static per-CPU control block for the Bootstrap Processor (BSP).
static BSP_PER_CPU: SyncUnsafeCell<PerCpuData> = SyncUnsafeCell::new(PerCpuData {
    kernel_rsp: 0,
    user_rsp: 0,
});

/// Updates the kernel stack pointer used by the fast syscall entry.
///
/// # Safety
///
/// `stack_top` must be a valid, mapped kernel stack memory address.
pub unsafe fn set_syscall_kernel_stack(stack_top: u64) {
    // SAFETY: Updating per-CPU kernel stack pointer used on fast syscall entry.
    unsafe {
        (*BSP_PER_CPU.get()).kernel_rsp = stack_top;
    }
}

/// Register frame constructed on the kernel stack by the fast `syscall` assembly stub.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SyscallRegisters {
    /// General-purpose register R15.
    pub r15: usize,
    /// General-purpose register R14.
    pub r14: usize,
    /// General-purpose register R13.
    pub r13: usize,
    /// General-purpose register R12.
    pub r12: usize,
    /// Base pointer register RBP.
    pub rbp: usize,
    /// Base register RBX.
    pub rbx: usize,
    /// 6th syscall argument (System V ABI).
    pub r9: usize,
    /// 5th syscall argument (System V ABI).
    pub r8: usize,
    /// 4th syscall argument (POSIX x86_64 ABI uses R10 instead of RCX).
    pub r10: usize,
    /// 3rd syscall argument (System V ABI).
    pub rdx: usize,
    /// 2nd syscall argument (System V ABI).
    pub rsi: usize,
    /// 1st syscall argument (System V ABI).
    pub rdi: usize,
    /// Syscall number on entry, return value on exit (RAX).
    pub rax: usize,
    /// Saved user RIP captured by the hardware `syscall` instruction (RCX).
    pub rcx: usize,
    /// Saved user RFLAGS captured by the hardware `syscall` instruction (R11).
    pub r11: usize,
    /// Saved user stack pointer (RSP).
    pub rsp: usize,
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
        "mov gs:[8], rsp", // Save user RSP in PerCpuData.user_rsp
        "mov rsp, gs:[0]", // Load kernel RSP from PerCpuData.kernel_rsp
        // Push registers to build SyscallRegisters structure
        "push gs:[8]", // User RSP
        "push r11",    // User RFLAGS
        "push rcx",    // User RIP
        "push rax",    // Syscall number
        "push rdi",    // Arg 1
        "push rsi",    // Arg 2
        "push rdx",    // Arg 3
        "push r10",    // Arg 4 (POSIX x86_64 ABI uses r10 for syscall arg 4)
        "push r8",     // Arg 5
        "push r9",     // Arg 6
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
        "pop rax", // Return value from syscall dispatcher
        "pop rcx", // Restore RIP
        "pop r11", // Restore RFLAGS
        "pop rsp", // Restore user RSP
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
    let per_cpu_ptr = BSP_PER_CPU.get();

    // SAFETY: Initializing PerCpuData with kernel stack during single-threaded boot.
    unsafe {
        (*per_cpu_ptr).kernel_rsp = kernel_stack_top;
    }

    // SAFETY: Programming IA32_KERNEL_GS_BASE with pointer to PerCpuData structure.
    unsafe {
        wrmsr(IA32_KERNEL_GS_BASE, per_cpu_ptr as u64);
    }

    // SAFETY: Enabling System Call Extensions (SCE) in EFER MSR.
    unsafe {
        let efer = rdmsr(IA32_EFER);
        wrmsr(IA32_EFER, efer | 1);
    }

    // SAFETY: Configuring STAR MSR with kernel and user code/data segment selectors.
    unsafe {
        let star = ((KERNEL_CODE_SEL as u64) << 32) | (0x10u64 << 48);
        wrmsr(IA32_STAR, star);
    }

    // SAFETY: Configuring LSTAR MSR with address of syscall_entry.
    unsafe {
        wrmsr(IA32_LSTAR, syscall_entry as *const () as usize as u64);
    }

    // SAFETY: Configuring FMASK MSR to clear IF, TF, and DF upon syscall entry.
    unsafe {
        let fmask = 0x200 | 0x100 | 0x400; // IF | TF | DF
        wrmsr(IA32_FMASK, fmask);
    }
}

/// Syscall dispatcher bridge called from `syscall_entry` assembly.
///
/// # Safety
///
/// `regs` must either be null or point to a live `SyscallRegisters` frame on the current stack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_syscall_dispatcher(regs: *mut SyscallRegisters) -> usize {
    // SAFETY: Passing raw register pointer to safe dispatcher callback via with_syscall_regs.
    unsafe { crate::ostd::mm::with_syscall_regs(regs, crate::services::posix::dispatch_syscall) }
}
