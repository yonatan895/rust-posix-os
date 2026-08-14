//! Memory Management Subsystem in OSTD.

pub mod pmm;
pub mod vmm;
pub mod heap;
pub mod user;
pub mod pod;
pub mod boot;

pub use pmm::{alloc_contiguous_frames, alloc_frame, free_frame, get_pmm_stats, PAGE_SIZE};
pub use vmm::{phys_to_virt, virt_to_phys, zero_phys_frame, VmSpace, PAGE_PRESENT, PAGE_WRITABLE, PAGE_USER, PAGE_NX};
pub use heap::{HEAP_ALLOCATOR, get_heap_stats};
pub use user::{copy_cstr_from_user, UserAccessError, UserPtr, UserSlice, USER_SPACE_END, USER_STR_MAX};
pub use pod::read_pod;
pub use boot::{boot_module_blobs, with_syscall_regs, BootBlob};

use crate::ostd::limine::LimineMemmapResponse;

pub unsafe fn mm_init(
    memmap_response: *mut LimineMemmapResponse,
    hhdm_offset: usize,
) {
    pmm::pmm_init(memmap_response, hhdm_offset);
    vmm::vmm_init(hhdm_offset);

    // Allocate contiguous physical frames for the kernel heap
    let heap_pages = heap::HEAP_SIZE / PAGE_SIZE;
    let heap_phys_start = alloc_contiguous_frames(heap_pages)
        .expect("Failed to allocate contiguous frames for kernel heap");
    let heap_virt_start = phys_to_virt(heap_phys_start);
    HEAP_ALLOCATOR.init(heap_virt_start, heap::HEAP_SIZE);
}
