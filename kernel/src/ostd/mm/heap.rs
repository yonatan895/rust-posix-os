//! Kernel Global Heap Allocator.

use crate::ostd::sync::SpinLock;
use core::alloc::{GlobalAlloc, Layout};

/// Fixed total size of the kernel global heap (16 MiB).
pub const HEAP_SIZE: usize = 16 * 1024 * 1024; // 16 MiB

/// Free-list intrusive header node embedded at the start of unallocated heap blocks.
struct ListNode {
    /// Total usable byte size of this free memory block.
    size: usize,
    /// Reference to the next free memory block in the intrusive linked list.
    next: Option<&'static mut ListNode>,
}

impl ListNode {
    /// Creates a new intrusive list node header for a block of `size` bytes.
    const fn new(size: usize) -> Self {
        Self { size, next: None }
    }
}

/// Intrusive first-fit linked-list allocator for the kernel global heap.
pub struct LinkedListAllocator {
    /// Sentinel head node of the free block list.
    head: ListNode,
}

impl LinkedListAllocator {
    /// Creates an empty, uninitialized [`LinkedListAllocator`].
    pub const fn new() -> Self {
        Self {
            head: ListNode::new(0),
        }
    }

    /// Initializes the allocator with a backing memory buffer.
    ///
    /// # Safety
    ///
    /// The memory region `[heap_start, heap_start + heap_size)` must be valid, unused,
    /// and exclusively owned by the heap allocator.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        // SAFETY: Caller guarantees that the memory region `[heap_start, heap_start + heap_size)` is valid,
        // unused, and exclusively owned by the heap allocator.
        unsafe {
            self.add_free_region(heap_start, heap_size);
        }
    }

    /// Inserts a raw memory region into the intrusive free list after aligning.
    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        let align = core::mem::align_of::<ListNode>();
        let aligned_addr = (addr + align - 1) & !(align - 1);
        let padding = aligned_addr - addr;
        if size <= padding + core::mem::size_of::<ListNode>() {
            return;
        }
        let usable_size = size - padding;

        let node_ptr = aligned_addr as *mut ListNode;
        // SAFETY: `aligned_addr` is properly aligned to `align_of::<ListNode>()` and `usable_size` is at least
        // `size_of::<ListNode>()`. The memory range is within caller-provided valid heap memory. Writing to
        // `node_ptr` and creating `&mut *node_ptr` is sound because no aliasing references exist.
        unsafe {
            core::ptr::write_unaligned(node_ptr, ListNode::new(usable_size));
            let node = &mut *node_ptr;
            node.next = self.head.next.take();
            self.head.next = Some(node);
        }
    }

    /// Searches the free list for a block satisfying `size` and `align` constraints.
    fn find_region(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        let mut current = &mut self.head;
        while let Some(ref mut region) = current.next {
            let node_addr = (&**region as *const ListNode) as usize;
            let alloc_start = (node_addr + align - 1) & !(align - 1);
            let alloc_end = alloc_start.checked_add(size)?;
            let region_end = node_addr.checked_add(region.size)?;

            if alloc_end <= region_end {
                let next = region.next.take();
                let ret = current.next.take().unwrap();
                current.next = next;
                return Some((ret, alloc_start));
            }
            current = current.next.as_mut().unwrap();
        }
        None
    }
}

impl Default for LinkedListAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// Spinlock-synchronized wrapper around [`LinkedListAllocator`] implementing [`GlobalAlloc`].
pub struct LockedHeap(SpinLock<LinkedListAllocator>);

impl LockedHeap {
    /// Creates a new uninitialized [`LockedHeap`].
    pub const fn new() -> Self {
        Self(SpinLock::new(LinkedListAllocator::new()))
    }

    /// Initializes the locked global heap allocator.
    ///
    /// # Safety
    ///
    /// The memory region `[heap_start, heap_start + heap_size)` must be valid, unused,
    /// and exclusively owned by the heap allocator.
    pub unsafe fn init(&self, heap_start: usize, heap_size: usize) {
        // SAFETY: Caller guarantees that `[heap_start, heap_start + heap_size)` is a valid,
        // unused memory region exclusively owned by the heap allocator.
        unsafe {
            self.0.lock().init(heap_start, heap_size);
        }
    }
}

impl Default for LockedHeap {
    fn default() -> Self {
        Self::new()
    }
}

use core::sync::atomic::{AtomicUsize, Ordering};

/// Total active allocated bytes in the kernel global heap.
pub static HEAP_USED_BYTES: AtomicUsize = AtomicUsize::new(0);

// SAFETY: `LockedHeap` synchronizes all internal mutations with a `SpinLock`, ensuring thread-safe
// and interrupt-safe allocation and deallocation across all CPU execution contexts.
unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.0.lock();
        let align = layout.align().max(core::mem::align_of::<ListNode>());
        let size = (layout.size() + align - 1) & !(align - 1);
        let min_size = size.max(core::mem::size_of::<ListNode>());

        if let Some((region, alloc_start)) = allocator.find_region(min_size, align) {
            let alloc_end = alloc_start + min_size;
            let node_addr = (region as *const ListNode) as usize;
            let region_end = node_addr + region.size;
            let excess_size = region_end.saturating_sub(alloc_end);

            if excess_size >= core::mem::size_of::<ListNode>() {
                // SAFETY: `[alloc_end, alloc_end + excess_size)` is within the valid memory block
                // that was previously free and has now been split; it is not returned to the caller.
                unsafe {
                    allocator.add_free_region(alloc_end, excess_size);
                }
            }
            HEAP_USED_BYTES.fetch_add(min_size, Ordering::Relaxed);
            alloc_start as *mut u8
        } else {
            core::ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.0.lock();
        let align = layout.align().max(core::mem::align_of::<ListNode>());
        let size = (layout.size() + align - 1) & !(align - 1);
        let min_size = size.max(core::mem::size_of::<ListNode>());
        // SAFETY: Caller guarantees `ptr` was previously allocated by `alloc` with `layout`,
        // is non-null, and will not be accessed again after deallocation.
        unsafe {
            allocator.add_free_region(ptr as usize, min_size);
        }
        HEAP_USED_BYTES.fetch_sub(min_size, Ordering::Relaxed);
    }
}

/// Returns a snapshot `(total_capacity_bytes, used_bytes)` of kernel heap usage.
pub fn get_heap_stats() -> (usize, usize) {
    (HEAP_SIZE, HEAP_USED_BYTES.load(Ordering::Relaxed))
}

/// Static kernel global allocator instance.
#[global_allocator]
pub static HEAP_ALLOCATOR: LockedHeap = LockedHeap::new();

/// Out-of-memory handler invoked when global heap allocation fails.
#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    log::error!("Kernel Heap Allocation Failure: {:?}", layout);
    loop {
        // SAFETY: Halts CPU execution indefinitely upon unrecoverable kernel heap exhaustion.
        unsafe { core::arch::asm!("hlt") };
    }
}
