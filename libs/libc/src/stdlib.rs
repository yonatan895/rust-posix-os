//! Standard General Utilities Library (stdlib.h).

use crate::string::memcpy;
use crate::sys_mman::*;
use crate::syscall::*;
use posix_abi::*;

const LARGE_MAGIC: usize = 0x504F5349584D454D; // "POSIXMEM"
const ARENA_MAGIC: usize = 0x504F53495841524E; // "POSIXARN"

const ARENA_SIZE: usize = 64 * 1024; // 64 KiB chunks
const NUM_CLASSES: usize = 8;
const SIZE_CLASSES: [usize; NUM_CLASSES] = [16, 32, 64, 128, 256, 512, 1024, 2048];
const SMALL_THRESHOLD: usize = 2048;
const MAX_ARENAS: usize = 512;

#[repr(C)]
struct BlockHeader {
    size: usize,
    magic: usize,
}

#[repr(C)]
struct FreeNode {
    next: *mut FreeNode,
}

#[repr(C)]
struct ArenaChunk {
    magic: usize,
    class_idx: usize,
    bump_offset: usize,
    next_arena: *mut ArenaChunk,
}

#[derive(Clone, Copy)]
struct ArenaRecord {
    start: usize,
    end: usize,
    class_idx: usize,
}

struct AllocatorState {
    free_lists: [*mut FreeNode; NUM_CLASSES],
    current_arenas: [*mut ArenaChunk; NUM_CLASSES],
    arena_records: [ArenaRecord; MAX_ARENAS],
    arena_count: usize,
    mmap_count: usize,
}

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

#[inline(always)]
unsafe fn get_state() -> *mut AllocatorState {
    core::ptr::addr_of_mut!(STATE)
}

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
        if class_idx >= NUM_CLASSES {
            class_idx = NUM_CLASSES - 1;
        }
        let b_size = SIZE_CLASSES[class_idx];

        // SAFETY: Operations on process-local allocator state and arena pointers.
        unsafe {
            let state = get_state();

            // 1. Pop from free list if available
            let node = (*state).free_lists[class_idx];
            if !node.is_null() {
                (*state).free_lists[class_idx] = (*node).next;
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

            let count = (*state).arena_count;
            if count < MAX_ARENAS {
                (*state).arena_records[count] = ArenaRecord {
                    start: arena_ptr as usize,
                    end: (arena_ptr as usize) + ARENA_SIZE,
                    class_idx,
                };
                (*state).arena_count = count + 1;
            }

            arena_ptr.add(hdr_size)
        }
    }
}

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn abort() -> ! {
    // SAFETY: Invoking exit with status 134 (SIGABRT).
    unsafe { exit(134) }
}

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn abs(j: i32) -> i32 {
    if j < 0 { -j } else { j }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __libc_get_mmap_count() -> usize {
    // SAFETY: Reading process-local mmap counter from allocator state.
    unsafe { (*get_state()).mmap_count }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __libc_reset_mmap_count() {
    // SAFETY: Resetting process-local mmap counter in allocator state.
    unsafe { (*get_state()).mmap_count = 0 };
}
