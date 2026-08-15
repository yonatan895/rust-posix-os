//! Bare-metal kernel entry point, bootstrap sequence, and panic handler.

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![feature(sync_unsafe_cell)]

extern crate alloc;

pub mod ostd;
#[deny(unsafe_code)]
pub mod services;

use core::cell::SyncUnsafeCell;
use core::panic::PanicInfo;
use ostd::*;

/// Page-aligned kernel execution stack.
#[repr(C, align(4096))]
struct KernelStack(
    /// Backing memory array for the stack (64 KiB).
    [u8; 64 * 1024],
);

/// Static allocation for the initial bootstrap CPU kernel stack.
static BOOT_STACK: SyncUnsafeCell<KernelStack> = SyncUnsafeCell::new(KernelStack([0; 64 * 1024]));

/// Kernel entry point called by the Limine bootloader.
///
/// # Safety
///
/// Must be invoked by a compliant 64-bit bootloader with paging and stack initialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    // SAFETY: Initialize early COM1 serial driver for panic and logging.
    unsafe {
        serial_init();
    }
    log::info!("=====================================================");
    log::info!("  Rust POSIX Operating System (Framekernel Model)    ");
    log::info!("  Target: x86_64 | Standard: POSIX.1-2024 (IEEE)     ");
    log::info!("  Implemented: VFS, fork, scheduler, signals, IPC    ");
    log::info!("  Next: userland networking / SMP                    ");
    log::info!("=====================================================");

    let stack_top = (BOOT_STACK.get() as u64) + (64 * 1024);
    // SAFETY: Initialize GDT and 64-bit TSS using dedicated static boot stack address.
    unsafe {
        gdt_init(stack_top);
    }
    log::info!("[OSTD] GDT and 64-bit TSS loaded successfully.");

    // SAFETY: Initialize IDT exception vectors and timer IRQ handler.
    unsafe {
        idt_init();
    }
    log::info!("[OSTD] IDT and exception vectors configured.");

    // SAFETY: Initialize physical memory management, kernel heap, and virtual address paging.
    unsafe {
        mm_init();
    }
    log::info!("[OSTD] Memory management initialized (PMM, 4-Level Paging, 16MiB Kernel Heap).");

    limine::init_framebuffer();

    // SAFETY: Arm x86_64 fast system call MSRs (LSTAR/STAR/FMASK).
    unsafe {
        syscall_init(stack_top);
    }
    log::info!("[OSTD] Fast system call MSRs (LSTAR/STAR/FMASK) armed.");

    // SAFETY: Remap PIC and configure PIT periodic timer.
    unsafe {
        irq_init();
    }
    log::info!("[OSTD] IRQ controllers prepared.");

    services::services_init(ostd::mm::boot_modules());

    log::info!("=====================================================");
    log::info!("  Kernel Initialization Complete. Kernel running!    ");
    log::info!("  Status: VFS, fork, scheduler, signals active       ");
    log::info!("  Next: userland networking / SMP                    ");
    log::info!("=====================================================");

    if let Some(init_proc_lock) = services::process::get_current_process() {
        let (entry, stack, pml4) = {
            let proc = init_proc_lock.lock();
            if let Some(ref vm) = proc.vm_space {
                (
                    proc.entry_point,
                    proc.user_stack_top,
                    vm.address_space.as_phys(),
                )
            } else {
                (0, 0, 0)
            }
        };
        if entry != 0 {
            log::info!(
                "[OSTD] Switching CPU to Ring 3 User Mode (entry: 0x{:x}, stack: 0x{:x})...",
                entry,
                stack
            );
            // SAFETY: Transitioning CPU to Ring 3 execution for PID 1 (init daemon).
            unsafe {
                ostd::task::enter_user_mode(entry, stack, pml4);
            }
        }
    }

    loop {
        // SAFETY: Halting CPU when idle in kernel main loop.
        unsafe {
            arch::hlt();
        }
    }
}

/// Global kernel panic handler.
///
/// Logs diagnostic information to the early serial console and permanently halts the CPU.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log::error!("KERNEL PANIC: {}", info);
    loop {
        // SAFETY: Halting CPU on fatal panic.
        unsafe { ostd::arch::hlt() };
    }
}
