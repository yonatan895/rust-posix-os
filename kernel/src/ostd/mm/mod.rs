//! Memory Management Subsystem in OSTD.

pub mod pmm;
pub mod vmm;
pub mod heap;

pub use pmm::{alloc_frame, free_frame, get_pmm_stats, PAGE_SIZE};
pub use vmm::{phys_to_virt, virt_to_phys, VmSpace, PAGE_PRESENT, PAGE_WRITABLE, PAGE_USER, PAGE_NX};
pub use heap::{HEAP_ALLOCATOR, get_heap_stats};

use crate::ostd::limine::LimineMemmapResponse;

pub unsafe fn mm_init(
    memmap_response: *mut LimineMemmapResponse,
    hhdm_offset: usize,
) {
    pmm::pmm_init(memmap_response, hhdm_offset);
    vmm::vmm_init(hhdm_offset);

    // Allocate initial kernel heap pages
    let heap_pages = heap::HEAP_SIZE / PAGE_SIZE;
    let heap_phys_start = alloc_frame().expect("Failed to allocate heap start frame");
    for _ in 1..heap_pages {
        alloc_frame().expect("Failed to allocate heap frame");
    }
    let heap_virt_start = phys_to_virt(heap_phys_start);
    HEAP_ALLOCATOR.init(heap_virt_start, heap::HEAP_SIZE);
}
