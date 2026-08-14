#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

pub mod ostd;
#[deny(unsafe_code)]
pub mod services;

use core::panic::PanicInfo;
use ostd::*;

#[repr(C, align(4096))]
struct KernelStack([u8; 64 * 1024]);

static mut BOOT_STACK: KernelStack = KernelStack([0; 64 * 1024]);

/// Kernel entry point called by the Limine bootloader.
///
/// # Safety
///
/// Must be invoked by a compliant 64-bit bootloader with paging and stack initialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    unsafe {
        serial_init();
    }
    log::info!("=====================================================");
    log::info!("  Rust POSIX Operating System (Framekernel Model)   ");
    log::info!("  Target: x86_64 | Standard: POSIX.1-2024 (IEEE)     ");
    log::info!("=====================================================");

    let stack_top = (&raw const BOOT_STACK as u64) + (64 * 1024);
    unsafe {
        gdt_init(stack_top);
    }
    log::info!("[OSTD] GDT and 64-bit TSS loaded successfully.");

    unsafe {
        idt_init();
    }
    log::info!("[OSTD] IDT and exception vectors configured.");

    unsafe {
        mm_init();
    }
    log::info!("[OSTD] Memory management initialized (PMM, 4-Level Paging, 16MiB Kernel Heap).");

    limine::init_framebuffer();

    unsafe {
        syscall_init(stack_top);
    }
    log::info!("[OSTD] Fast system call MSRs (LSTAR/STAR/FMASK) armed.");

    unsafe {
        irq_init();
    }
    log::info!("[OSTD] IRQ controllers prepared.");

    services::services_init(ostd::mm::boot_modules());

    ostd::task::executor::async_init();
    ostd::task::executor::spawn(services::monitor::system_resource_monitor_task());
    let executed_steps = ostd::task::executor::run_async_tasks();
    log::info!(
        "[ASYNC] Kernel async executor initialized (executed {} task steps).",
        executed_steps
    );

    log::info!("=====================================================");
    log::info!("  Kernel Initialization Complete. Kernel running!   ");
    log::info!("=====================================================");

    if let Some(init_proc_lock) = services::process::get_current_process() {
        let (entry, stack, pml4) = {
            let proc = init_proc_lock.lock();
            if let Some(ref vm) = proc.vm_space {
                (proc.entry_point, proc.user_stack_top, vm.pml4_phys)
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
            unsafe {
                ostd::task::enter_user_mode(entry, stack, pml4);
            }
        }
    }

    loop {
        unsafe {
            arch::hlt();
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log::error!("KERNEL PANIC: {}", info);
    loop {
        unsafe { ostd::arch::hlt() };
    }
}
