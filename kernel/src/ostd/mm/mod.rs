//! Memory Management Subsystem in OSTD.

pub mod address_space;
pub mod boot;
pub mod flags;
pub mod heap;
pub mod pmm;
pub mod pod;
pub mod user;
pub mod vmm;

pub use address_space::AddressSpace;
pub use boot::{BootBlob, boot_modules, with_syscall_regs};
pub use flags::PageFlags;
pub use heap::{HEAP_ALLOCATOR, get_heap_stats};
pub use pmm::{PAGE_SIZE, alloc_contiguous_frames, alloc_frame, free_frame, get_pmm_stats};
pub use pod::read_pod;
pub use user::{
    USER_SPACE_END, USER_STR_MAX, UserAccessError, UserPtr, UserSlice, copy_cstr_from_user,
};
pub use vmm::{VmSpace, Vma, phys_to_virt, virt_to_phys, zero_phys_frame};

/// Initializes the kernel memory management subsystem (PMM, VMM, and global heap).
///
/// # Panics
///
/// Panics if physical frame allocation for the kernel heap fails during early boot.
///
/// # Safety
///
/// Must be invoked during early boot with valid bootloader memory responses.
pub unsafe fn mm_init() {
    let memmap_response = crate::ostd::limine::memmap_response();
    let hhdm_offset = crate::ostd::limine::hhdm_offset();

    unsafe {
        pmm::pmm_init(memmap_response, hhdm_offset);
        vmm::vmm_init(hhdm_offset);
    }

    // Allocate contiguous physical frames for the kernel heap
    let heap_pages = heap::HEAP_SIZE / PAGE_SIZE;
    let heap_phys_start = alloc_contiguous_frames(heap_pages)
        .expect("Failed to allocate contiguous frames for kernel heap");
    let heap_virt_start = phys_to_virt(heap_phys_start);
    unsafe {
        HEAP_ALLOCATOR.init(heap_virt_start, heap::HEAP_SIZE);
    }
}
