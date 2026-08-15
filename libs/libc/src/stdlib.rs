//! Standard General Utilities Library (stdlib.h).

use crate::string::memcpy;
use crate::sys_mman::*;
use crate::syscall::*;
use posix_abi::*;

/// Magic signature for large mmap-backed allocations (`"POSIXMEM"`).
const LARGE_MAGIC: usize = 0x504F5349584D454D;
/// Magic signature for arena chunk headers (`"POSIXARN"`).
const ARENA_MAGIC: usize = 0x504F53495841524E;
/// Magic signature for freed nodes to guard against double-free (`"POSIXFRE"`).
const FREE_MAGIC: usize = 0x504F534958465245;

/// Size of slab arena chunks in bytes (64 KiB).
const ARENA_SIZE: usize = 64 * 1024;
/// Number of segregated size classes for small allocations.
const NUM_CLASSES: usize = 8;
/// Bin size classes for small allocations in bytes.
const SIZE_CLASSES: [usize; NUM_CLASSES] = [16, 32, 64, 128, 256, 512, 1024, 2048];
/// Threshold in bytes below or equal to which the slab/bin allocator is used.
const SMALL_THRESHOLD: usize = 2048;
/// Maximum number of tracked arena chunks in fixed table.
const MAX_ARENAS: usize = 512;

/// Header preceding large mmap memory allocations.
#[repr(C)]
struct BlockHeader {
    /// Total allocation size in bytes including header.
    size: usize,
    /// Verification magic (`LARGE_MAGIC`).
    magic: usize,
}

/// Intrusive free list node stored in freed small object memory.
#[repr(C)]
struct FreeNode {
    /// Pointer to next free node in this size class list.
    next: *mut FreeNode,
    /// Verification magic (`FREE_MAGIC`).
    magic: usize,
}

/// Header preceding a 64 KiB slab arena chunk.
#[repr(C)]
struct ArenaChunk {
    /// Verification magic (`ARENA_MAGIC`).
    magic: usize,
    /// Size class index serviced by this chunk.
    class_idx: usize,
    /// Current bump allocation offset within chunk.
    bump_offset: usize,
    /// Pointer to next arena chunk in chain.
    next_arena: *mut ArenaChunk,
}

/// Metadata record tracking virtual address boundaries of an arena.
#[derive(Clone, Copy)]
struct ArenaRecord {
    /// Start virtual address of arena chunk.
    start: usize,
    /// End virtual address of arena chunk.
    end: usize,
    /// Size class index serviced by this arena chunk.
    class_idx: usize,
}

/// Global process-local memory allocator state.
struct AllocatorState {
    /// Segregated intrusive free list heads per size class.
    free_lists: [*mut FreeNode; NUM_CLASSES],
    /// Active bump arena chunk pointers per size class.
    current_arenas: [*mut ArenaChunk; NUM_CLASSES],
    /// Fixed-size table of all registered arena chunks.
    arena_records: [ArenaRecord; MAX_ARENAS],
    /// Total count of registered arena chunks.
    arena_count: usize,
    /// Counter of active/total mmap calls.
    mmap_count: usize,
}

// NOTE: Allocator state is process-local and not thread-safe under shared-memory multi-threading.
static mut STATE: AllocatorState = AllocatorState {
    free_lists: [core::ptr::null_mut(); NUM_CLASSES],
    current_arenas: [core::ptr::null_mut(); NUM_CLASSES],
    arena_records: [ArenaRecord {
        start: 0,
        end: 0,
        class_idx: 0,
    }; MAX_ARENAS],
    arena_count: 0,
    mmap_count: 0,
};

/// Returns a raw mutable pointer to the process-local allocator state.
#[inline(always)]
unsafe fn get_state() -> *mut AllocatorState {
    core::ptr::addr_of_mut!(STATE)
}

/// Allocates `size` bytes of uninitialized memory.
///
/// Returns a pointer to the allocated memory, or null on failure.
///
/// # Safety
///
/// Returns raw memory pointer that must be freed via [`free`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut u8 {
    if size == 0 {
        return core::ptr::null_mut();
    }

    if size > SMALL_THRESHOLD {
        // Large allocation path: dedicated page-aligned mmap with BlockHeader
        let total_size = size + core::mem::size_of::<BlockHeader>();
        let aligned_size = (total_size + 4095) & !4095;

        // SAFETY: mmap is called with standard anonymous private mapping flags.
        let ptr = unsafe {
            mmap(
                core::ptr::null_mut(),
                aligned_size,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            )
        };

        if ptr.is_null() || (ptr as usize) >= (-(4095i64) as usize) {
            return core::ptr::null_mut();
        }

        // SAFETY: ptr is a valid newly mapped anonymous memory region of aligned_size bytes.
        unsafe {
            let state = get_state();
            (*state).mmap_count += 1;
            let header = ptr as *mut BlockHeader;
            (*header).size = aligned_size;
            (*header).magic = LARGE_MAGIC;
            ptr.add(core::mem::size_of::<BlockHeader>())
        }
    } else {
        // Small allocation path: slab/bin allocation from 64 KiB arena chunks
        let mut class_idx = 0;
        while class_idx < NUM_CLASSES && SIZE_CLASSES[class_idx] < size {
            class_idx += 1;
        }
        let b_size = SIZE_CLASSES[class_idx];

        // SAFETY: Operations on process-local allocator state and arena pointers.
        unsafe {
            let state = get_state();

            // 1. Pop from free list if available
            let node = (*state).free_lists[class_idx];
            if !node.is_null() {
                (*state).free_lists[class_idx] = (*node).next;
                (*node).magic = 0; // Clear free magic upon reallocation
                return node as *mut u8;
            }

            // 2. Bump-allocate from current arena if capacity remains
            let current = (*state).current_arenas[class_idx];
            if !current.is_null() && (*current).bump_offset + b_size <= ARENA_SIZE {
                let block = (current as *mut u8).add((*current).bump_offset);
                (*current).bump_offset += b_size;
                return block;
            }

            // 3. Allocate new 64 KiB arena chunk
            let count = (*state).arena_count;
            if count >= MAX_ARENAS {
                // Fail-closed: cannot record more arenas in fixed table
                return core::ptr::null_mut();
            }

            let arena_ptr = mmap(
                core::ptr::null_mut(),
                ARENA_SIZE,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            );

            if arena_ptr.is_null() || (arena_ptr as usize) >= (-(4095i64) as usize) {
                return core::ptr::null_mut();
            }

            (*state).mmap_count += 1;

            let arena = arena_ptr as *mut ArenaChunk;
            let hdr_size = (core::mem::size_of::<ArenaChunk>() + 15) & !15; // 16-byte aligned
            (*arena).magic = ARENA_MAGIC;
            (*arena).class_idx = class_idx;
            (*arena).bump_offset = hdr_size + b_size;
            (*arena).next_arena = (*state).current_arenas[class_idx];
            (*state).current_arenas[class_idx] = arena;

            (*state).arena_records[count] = ArenaRecord {
                start: arena_ptr as usize,
                end: (arena_ptr as usize) + ARENA_SIZE,
                class_idx,
            };
            (*state).arena_count = count + 1;

            arena_ptr.add(hdr_size)
        }
    }
}

/// Frees a memory block previously allocated by [`malloc`], [`calloc`], or [`realloc`].
///
/// If `ptr` is null, no operation is performed.
///
/// # Safety
///
/// `ptr` must either be null or point to memory allocated by the libc allocator that has not yet been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }

    let p = ptr as usize;

    // SAFETY: Validates arena boundaries before accessing free lists or BlockHeader.
    unsafe {
        let state = get_state();
        // 1. Check if pointer belongs to any small-object arena
        let count = (*state).arena_count;
        for i in 0..count {
            let rec = (*state).arena_records[i];
            if p >= rec.start && p < rec.end {
                let class_idx = rec.class_idx;
                let node = ptr as *mut FreeNode;
                // Double-free guard: if already marked freed, skip duplicate insertion
                if (*node).magic == FREE_MAGIC {
                    return;
                }
                (*node).magic = FREE_MAGIC;
                (*node).next = (*state).free_lists[class_idx];
                (*state).free_lists[class_idx] = node;
                return;
            }
        }

        // 2. Large allocation path
        let header_ptr = ptr.sub(core::mem::size_of::<BlockHeader>()) as *mut BlockHeader;
        if (*header_ptr).magic == LARGE_MAGIC {
            let size = (*header_ptr).size;
            (*header_ptr).magic = 0; // Guard against double-free
            munmap(header_ptr as *mut u8, size);
        }
    }
}

/// Allocates zero-initialized memory for an array of `nmemb` elements of `size` bytes each.
///
/// Returns a pointer to the zeroed memory, or null on overflow or failure.
///
/// # Safety
///
/// Returns raw heap memory that must be deallocated using [`free`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn calloc(nmemb: usize, size: usize) -> *mut u8 {
    let total = nmemb.saturating_mul(size);
    // SAFETY: malloc and memset handle null check and allocation boundaries safely.
    let ptr = unsafe { malloc(total) };
    if !ptr.is_null() {
        // SAFETY: ptr is valid for writes of total bytes.
        unsafe {
            crate::string::memset(ptr, 0, total);
        }
    }
    ptr
}

/// Reallocates a memory block `ptr` to a new size `size` bytes.
///
/// Preserves existing content up to the minimum of old and new size.
///
/// # Safety
///
/// `ptr` must either be null or point to a valid active allocation from the libc allocator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(ptr: *mut u8, size: usize) -> *mut u8 {
    if ptr.is_null() {
        // SAFETY: Delegating to malloc with requested size.
        return unsafe { malloc(size) };
    }
    if size == 0 {
        // SAFETY: Freeing valid non-null pointer and returning null per POSIX.
        unsafe {
            free(ptr);
        }
        return core::ptr::null_mut();
    }

    let p = ptr as usize;

    // SAFETY: Inspects arena records and large block headers to compute capacity safely.
    unsafe {
        let state = get_state();
        let mut old_capacity = 0;
        let mut is_small = false;

        let count = (*state).arena_count;
        for i in 0..count {
            let rec = (*state).arena_records[i];
            if p >= rec.start && p < rec.end {
                old_capacity = SIZE_CLASSES[rec.class_idx];
                is_small = true;
                break;
            }
        }

        if !is_small {
            let header_ptr = ptr.sub(core::mem::size_of::<BlockHeader>()) as *mut BlockHeader;
            if (*header_ptr).magic != LARGE_MAGIC {
                return core::ptr::null_mut();
            }
            old_capacity = (*header_ptr).size - core::mem::size_of::<BlockHeader>();
        }

        // In-place reuse if existing block capacity already satisfies requested size
        if old_capacity >= size {
            return ptr;
        }

        let new_ptr = malloc(size);
        if !new_ptr.is_null() {
            memcpy(new_ptr, ptr, old_capacity);
            free(ptr);
        }
        new_ptr
    }
}

/// Terminates calling process immediately with status code `status`.
///
/// # Safety
///
/// Issues the `SYS_EXIT` syscall and never returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn exit(status: i32) -> ! {
    // SAFETY: Performing direct exit system call.
    unsafe {
        syscall1(SYS_EXIT, status as usize);
        loop {
            core::arch::asm!("hlt");
        }
    }
}

/// Abnormally terminates process execution by exiting with status 134 (`SIGABRT`).
///
/// # Safety
///
/// Calls [`exit`] and never returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abort() -> ! {
    // SAFETY: Invoking exit with status 134 (SIGABRT).
    unsafe { exit(134) }
}

/// Converts a string representing a signed integer to an `i32` value.
///
/// # Safety
///
/// `s` must be null or point to a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atoi(s: *const u8) -> i32 {
    if s.is_null() {
        return 0;
    }
    let mut i = 0;
    let mut sign = 1;
    // SAFETY: Reading bytes sequentially until null terminator or non-digit.
    unsafe {
        while *s.add(i) == b' ' || *s.add(i) == b'\t' || *s.add(i) == b'\n' {
            i += 1;
        }
        if *s.add(i) == b'-' {
            sign = -1;
            i += 1;
        } else if *s.add(i) == b'+' {
            i += 1;
        }
        let mut res = 0;
        while *s.add(i) >= b'0' && *s.add(i) <= b'9' {
            res = res * 10 + (*s.add(i) - b'0') as i32;
            i += 1;
        }
        sign * res
    }
}

/// Computes the absolute value of an integer `j`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abs(j: i32) -> i32 {
    if j < 0 { -j } else { j }
}

/// Retrieves the count of mmap allocations performed by the allocator.
///
/// # Safety
///
/// Reads process-local allocator state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __libc_get_mmap_count() -> usize {
    // SAFETY: Reading process-local mmap counter from allocator state.
    unsafe { (*get_state()).mmap_count }
}

/// Resets the count of mmap allocations tracked by the allocator.
///
/// # Safety
///
/// Modifies process-local allocator state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __libc_reset_mmap_count() {
    // SAFETY: Resetting process-local mmap counter in allocator state.
    unsafe { (*get_state()).mmap_count = 0 };
}
