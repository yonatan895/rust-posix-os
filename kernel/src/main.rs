#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

pub mod ostd;
pub mod services;

use core::panic::PanicInfo;
use ostd::limine::*;
use ostd::*;

// ============================================================================
// Limine Boot Protocol Requests & Markers
// ============================================================================

#[used]
#[link_section = ".requests_start"]
static REQ_START: LimineRequestsStartMarker = LimineRequestsStartMarker {
    id: [0xf6b8f4b39de7d1ae, 0xfab91a6940fcb9cf, 0x785c6ed015d3e316, 0x181e920a7852b9d9],
};

#[used]
#[link_section = ".requests"]
static BASE_REVISION: LimineBaseRevision = LimineBaseRevision {
    id: [0xf9562b2d5c95a6c8, 0x6a7b384944536bdc],
    revision: 3,
};

#[used]
#[link_section = ".requests"]
static HHDM_REQUEST: LimineHhdmRequest = LimineHhdmRequest {
    id: LIMINE_HHDM_REQUEST,
    revision: 0,
    response: core::cell::UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[link_section = ".requests"]
static MEMMAP_REQUEST: LimineMemmapRequest = LimineMemmapRequest {
    id: LIMINE_MEMMAP_REQUEST,
    revision: 0,
    response: core::cell::UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[link_section = ".requests"]
static FRAMEBUFFER_REQUEST: LimineFramebufferRequest = LimineFramebufferRequest {
    id: LIMINE_FRAMEBUFFER_REQUEST,
    revision: 0,
    response: core::cell::UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[link_section = ".requests"]
static MODULE_REQUEST: LimineModuleRequest = LimineModuleRequest {
    id: LIMINE_MODULE_REQUEST,
    revision: 0,
    response: core::cell::UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[link_section = ".requests_end"]
static REQ_END: LimineRequestsEndMarker = LimineRequestsEndMarker {
    id: [0xadc0e0531bb10d03, 0x9572709f31764c62],
};

// ============================================================================
// Kernel Stack
// ============================================================================
#[repr(C, align(4096))]
struct KernelStack([u8; 64 * 1024]); // 64 KiB stack

static mut BOOT_STACK: KernelStack = KernelStack([0; 64 * 1024]);

// ============================================================================
// Kernel Entry Point
// ============================================================================
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    // 1. Initialize Serial Port & Logger (OSTD)
    serial_init();
    log::info!("=====================================================");
    log::info!("  Rust POSIX Operating System (Framekernel Model)   ");
    log::info!("  Target: x86_64 | Standard: POSIX.1-2024 (IEEE)     ");
    log::info!("=====================================================");

    // 2. Initialize GDT & TSS with 64 KiB stack top
    let stack_top = (&raw const BOOT_STACK as u64) + (64 * 1024);
    gdt_init(stack_top);
    log::info!("[OSTD] GDT and 64-bit TSS loaded successfully.");

    // 3. Initialize IDT & CPU Exceptions
    idt_init();
    log::info!("[OSTD] IDT and exception vectors configured.");

    // 4. Extract Limine Boot Information
    let hhdm_resp = *HHDM_REQUEST.response.get();
    let memmap_resp = *MEMMAP_REQUEST.response.get();
    let module_resp = *MODULE_REQUEST.response.get();

    let hhdm_offset = if !hhdm_resp.is_null() {
        (*hhdm_resp).offset as usize
    } else {
        0xFFFF_8000_0000_0000
    };

    // 5. Initialize Memory Management (PMM, VMM, Kernel Heap)
    mm_init(memmap_resp, hhdm_offset);
    log::info!("[OSTD] Memory management initialized (PMM, 4-Level Paging, 16MiB Kernel Heap).");

    // 6. Initialize Graphical Framebuffer if available
    let fb_resp = *FRAMEBUFFER_REQUEST.response.get();
    if !fb_resp.is_null() && (*fb_resp).framebuffer_count > 0 {
        let fb = **(*fb_resp).framebuffers;
        ostd::drivers::framebuffer::fb_init(fb);
        log::info!("[OSTD] Framebuffer initialized ({}x{} @ {}bpp).", fb.width, fb.height, fb.bpp);
    }

    // 7. Initialize Fast Syscall MSRs (LSTAR)
    syscall_init(stack_top);
    log::info!("[OSTD] Fast system call MSRs (LSTAR/STAR/FMASK) armed.");

    // 8. Initialize Interrupt Controller
    irq_init();
    log::info!("[OSTD] IRQ controllers prepared.");

    // 9. Initialize De-Privileged Safe Services (VFS, DevFS, Init Process, Initramfs)
    services::services_init(module_resp);

    // 10. Initialize Kernel Asynchronous Task Runtime
    ostd::task::executor::async_init();
    ostd::task::executor::spawn(async {
        log::info!("[ASYNC] Kernel Background Task Alpha started.");
        ostd::task::async_task::yield_now().await;
        log::info!("[ASYNC] Kernel Background Task Alpha resumed and completed.");
    });
    ostd::task::executor::spawn(services::monitor::system_resource_monitor_task());
    let executed_steps = ostd::task::executor::run_async_tasks();
    log::info!("[ASYNC] Kernel async executor initialized (executed {} task steps).", executed_steps);

    log::info!("=====================================================");
    log::info!("  Kernel Initialization Complete. Kernel running!   ");
    log::info!("=====================================================");

    // 11. Switch to User Mode (PID 1 Init Process)
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
            log::info!("[OSTD] Switching CPU to Ring 3 User Mode (entry: 0x{:x}, stack: 0x{:x})...", entry, stack);
            ostd::task::enter_user_mode(entry, stack, pml4);
        }
    }

    // Kernel idle loop
    loop {
        arch::hlt();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log::error!("KERNEL PANIC: {}", info);
    loop {
        unsafe { ostd::arch::hlt() };
    }
}
