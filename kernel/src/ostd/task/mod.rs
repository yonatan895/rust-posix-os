//! Task and Low-Level Context Switch Abstraction in OSTD.
//!
//! Provides architecture-neutral task management primitives adhering to the
//! unified `TrapFrame`-at-`saved_kernel_rsp` invariant (ADR-0003).

pub use crate::ostd::arch::SyscallRegisters;
pub use crate::ostd::arch::TrapFrame;

/// Default kernel stack size allocated per task (16 KiB).
pub const KERNEL_STACK_SIZE: usize = 16 * 1024; // 16 KiB

/// Safely updates the CPU's active kernel stack in the architectural TSS/per-CPU control.
pub fn switch_active_kernel_stack(stack_top: u64) {
    #[cfg(target_arch = "x86_64")]
    crate::ostd::arch::x86_64::task::switch_active_kernel_stack(stack_top);
    #[cfg(not(target_arch = "x86_64"))]
    unimplemented!("switch_active_kernel_stack not implemented for this architecture");
}

/// Safely performs a task switch from `prev_saved_rsp_ptr` to `next_saved_rsp` under interrupt masking.
///
/// In OSTD task model, caller passes raw pointer to outgoing PCB AtomicUsize field.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn switch_tasks(prev_saved_rsp_ptr: *mut usize, next_saved_rsp: usize) {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Disabling interrupts around voluntary context switch.
    unsafe {
        crate::ostd::arch::cli();
        crate::ostd::arch::x86_64::task::voluntary_task_switch(prev_saved_rsp_ptr, next_saved_rsp);
        crate::ostd::arch::sti();
    }
    #[cfg(not(target_arch = "x86_64"))]
    unimplemented!("switch_tasks not implemented for this architecture");
}

/// Kernel idle loop running with interrupts enabled.
pub extern "C" fn kernel_idle_loop() -> ! {
    loop {
        #[cfg(target_arch = "x86_64")]
        crate::ostd::arch::x86_64::task::idle();
        #[cfg(not(target_arch = "x86_64"))]
        unimplemented!("kernel_idle_loop not implemented for this architecture");
    }
}

/// Initializes a kernel stack slice with an initial `TrapFrame` for Ring 3 userland entry.
///
/// # Panics
///
/// Panics if `stack.len()` is smaller than `size_of::<TrapFrame>()`.
pub fn init_user_kernel_stack(
    stack: &mut [u8],
    entry_point: usize,
    user_stack_top: usize,
) -> usize {
    #[cfg(target_arch = "x86_64")]
    return crate::ostd::arch::x86_64::task::init_user_kernel_stack(
        stack,
        entry_point,
        user_stack_top,
    );
    #[cfg(not(target_arch = "x86_64"))]
    unimplemented!("init_user_kernel_stack not implemented for this architecture")
}

/// Initializes a child process kernel stack with a synthetic `TrapFrame` cloned from the parent `SyscallRegisters`.
///
/// Returns the initial `saved_kernel_rsp` pointing to the TrapFrame with return value 0.
///
/// # Panics
///
/// Panics if `stack.len()` is smaller than `size_of::<TrapFrame>()`.
pub fn init_fork_child_stack(stack: &mut [u8], parent_regs: &SyscallRegisters) -> usize {
    #[cfg(target_arch = "x86_64")]
    return crate::ostd::arch::x86_64::task::init_fork_child_stack(stack, parent_regs);
    #[cfg(not(target_arch = "x86_64"))]
    unimplemented!("init_fork_child_stack not implemented for this architecture")
}

/// Initializes a kernel stack slice with an initial `TrapFrame` for Ring 0 kernel task entry.
///
/// # Panics
///
/// Panics if `stack.len()` is smaller than `size_of::<TrapFrame>()`.
pub fn init_kernel_task_stack(stack: &mut [u8], entry_point: usize) -> usize {
    #[cfg(target_arch = "x86_64")]
    return crate::ostd::arch::x86_64::task::init_kernel_task_stack(stack, entry_point);
    #[cfg(not(target_arch = "x86_64"))]
    unimplemented!("init_kernel_task_stack not implemented for this architecture")
}

/// Transitions the CPU to Ring 3 (user mode) and begins executing userland code.
///
/// # Safety
///
/// `entry_point` must be a valid executable user address, `user_stack_top` must be a valid user stack address,
/// and `root_table` must be a valid page table physical address.
pub unsafe fn enter_user_mode(entry_point: usize, user_stack_top: usize, root_table: usize) -> ! {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Transitioning CPU to Ring 3 user mode execution.
    unsafe {
        crate::ostd::arch::x86_64::task::enter_user_mode(entry_point, user_stack_top, root_table)
    }
    #[cfg(not(target_arch = "x86_64"))]
    unimplemented!("enter_user_mode not implemented for this architecture")
}
