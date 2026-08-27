//! Memory Management Core (mm-core) - Host-Testable Page Allocation and Mapping State Machine.
//!
//! Provides architecture-neutral, pure algorithmic logic for virtual address allocation,
//! user boundary validation, and per-page mapping loops with atomic rollback on failure.

#![no_std]
#![warn(missing_docs)]
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use posix_abi::*;

/// Architectural page size in bytes (4 KiB).
pub const PAGE_SIZE: usize = 4096;

/// Interface for physical memory frame allocation and deallocation.
pub trait FrameAllocator {
    /// Type representing an allocated physical frame handle or physical base address.
    type Frame: Copy;

    /// Allocates a single 4 KiB physical frame.
    ///
    /// Returns `Some(frame)` on success, or `None` if physical memory is exhausted.
    fn alloc_frame(&mut self) -> Option<Self::Frame>;

    /// Frees a previously allocated physical frame back to the allocator.
    fn free_frame(&mut self, frame: Self::Frame);
}

/// Interface for managing virtual address space page mappings.
pub trait PageMapper<F> {
    /// Maps a 4 KiB virtual page at `vaddr` to the physical frame `frame` with protection attributes `prot`.
    ///
    /// Returns `Ok(())` on success, or `Err(reason)` if intermediate page table allocation or mapping fails.
    fn map_page(&mut self, vaddr: usize, frame: F, prot: u32) -> Result<(), &'static str>;

    /// Unmaps `count` consecutive 4 KiB pages starting at `start_vaddr` and frees their physical frames.
    fn unmap_range(&mut self, start_vaddr: usize, count: usize);
}

/// Computes mmap virtual address layout, validates boundaries, and performs per-page allocation
/// and mapping with atomic rollback on any failure.
///
/// If `addr == 0`, allocates from the bump pointer `mmap_next_vaddr` and advances it.
/// If `addr != 0`, maps at page-aligned `addr`.
///
/// On any failure during page allocation or mapping:
/// - Any physical frame allocated for the failing page is freed immediately.
/// - All pages mapped in earlier iterations (`0..pages_mapped`) are unmapped via [`PageMapper::unmap_range`].
/// - If `addr == 0`, `mmap_next_vaddr` is rolled back to its pre-call value.
/// - Returns `-ENOMEM` (or `-EINVAL` for invalid length).
#[allow(clippy::too_many_arguments)]
pub fn mmap_allocate<A, M, F>(
    addr: usize,
    length: usize,
    prot: i32,
    _flags: i32,
    user_space_end: usize,
    free_frames: usize,
    mmap_next_vaddr: &mut usize,
    allocator: &mut A,
    mapper: &mut M,
    zero_frame: impl Fn(F),
) -> Result<usize, i32>
where
    A: FrameAllocator<Frame = F>,
    M: PageMapper<F>,
    F: Copy,
{
    if length == 0 {
        return Err(EINVAL);
    }
    if length > user_space_end {
        return Err(ENOMEM);
    }
    let pages = length.div_ceil(PAGE_SIZE);
    let byte_len = match pages.checked_mul(PAGE_SIZE) {
        Some(len) => len,
        None => return Err(ENOMEM),
    };

    if pages > free_frames {
        return Err(ENOMEM);
    }

    let rollback_vaddr = *mmap_next_vaddr;
    let is_anonymous_bump = addr == 0;

    let vaddr = if is_anonymous_bump {
        let base = *mmap_next_vaddr;
        *mmap_next_vaddr = match mmap_next_vaddr.checked_add(byte_len) {
            Some(next) if next <= user_space_end => next,
            _ => return Err(ENOMEM),
        };
        base
    } else {
        addr & !0xFFF
    };

    let _end_vaddr = match vaddr.checked_add(byte_len) {
        Some(end) if end <= user_space_end => end,
        _ => {
            if is_anonymous_bump {
                *mmap_next_vaddr = rollback_vaddr;
            }
            return Err(ENOMEM);
        }
    };

    for (pages_mapped, i) in (0..pages).enumerate() {
        let offset = match i.checked_mul(PAGE_SIZE) {
            Some(off) => off,
            None => {
                mapper.unmap_range(vaddr, pages_mapped);
                if is_anonymous_bump {
                    *mmap_next_vaddr = rollback_vaddr;
                }
                return Err(ENOMEM);
            }
        };
        let page_vaddr = match vaddr.checked_add(offset) {
            Some(va) => va,
            None => {
                mapper.unmap_range(vaddr, pages_mapped);
                if is_anonymous_bump {
                    *mmap_next_vaddr = rollback_vaddr;
                }
                return Err(ENOMEM);
            }
        };

        let frame = match allocator.alloc_frame() {
            Some(f) => f,
            None => {
                mapper.unmap_range(vaddr, pages_mapped);
                if is_anonymous_bump {
                    *mmap_next_vaddr = rollback_vaddr;
                }
                return Err(ENOMEM);
            }
        };

        if mapper.map_page(page_vaddr, frame, prot as u32).is_err() {
            allocator.free_frame(frame);
            mapper.unmap_range(vaddr, pages_mapped);
            if is_anonymous_bump {
                *mmap_next_vaddr = rollback_vaddr;
            }
            return Err(ENOMEM);
        }
        zero_frame(frame);
    }

    Ok(vaddr)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::collections::BTreeMap;

    const DEFAULT_MMAP_BASE: usize = 0x0000_7000_0000_0000;
    const USER_SPACE_END: usize = 0x0000_8000_0000_0000;

    struct TestPmm {
        free_frames: usize,
        fail_after: usize,
        alloc_count: usize,
    }

    impl FrameAllocator for TestPmm {
        type Frame = usize;

        fn alloc_frame(&mut self) -> Option<usize> {
            if self.alloc_count >= self.fail_after {
                return None;
            }
            if self.free_frames > 0 {
                self.free_frames -= 1;
                self.alloc_count += 1;
                Some(0x10000 + self.alloc_count * PAGE_SIZE)
            } else {
                None
            }
        }

        fn free_frame(&mut self, _frame: usize) {
            self.free_frames += 1;
        }
    }

    struct TestMapper {
        mapped: BTreeMap<usize, usize>,
        fail_at_page: Option<usize>,
        map_count: usize,
    }

    impl TestMapper {
        fn new() -> Self {
            Self {
                mapped: BTreeMap::new(),
                fail_at_page: None,
                map_count: 0,
            }
        }
    }

    impl PageMapper<usize> for TestMapper {
        fn map_page(&mut self, vaddr: usize, frame: usize, _prot: u32) -> Result<(), &'static str> {
            self.map_count += 1;
            if self.fail_at_page == Some(self.map_count) {
                return Err("Page table allocation failed (OOM)");
            }
            self.mapped.insert(vaddr, frame);
            Ok(())
        }

        fn unmap_range(&mut self, start_vaddr: usize, count: usize) {
            let aligned_start = start_vaddr & !0xFFF;
            for i in 0..count {
                if let Some(va) = i
                    .checked_mul(PAGE_SIZE)
                    .and_then(|off| aligned_start.checked_add(off))
                {
                    self.mapped.remove(&va);
                }
            }
        }
    }

    #[test]
    fn test_mmap_allocate_success() {
        let mut pmm = TestPmm {
            free_frames: 10,
            fail_after: 10,
            alloc_count: 0,
        };
        let mut mapper = TestMapper::new();
        let mut mmap_next = DEFAULT_MMAP_BASE;

        let res = mmap_allocate(
            0,
            4 * PAGE_SIZE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            USER_SPACE_END,
            pmm.free_frames,
            &mut mmap_next,
            &mut pmm,
            &mut mapper,
            |_| {},
        );

        assert_eq!(res, Ok(DEFAULT_MMAP_BASE));
        assert_eq!(mmap_next, DEFAULT_MMAP_BASE + 4 * PAGE_SIZE);
        assert_eq!(pmm.free_frames, 6);
        assert_eq!(mapper.mapped.len(), 4);
    }

    #[test]
    fn test_mmap_allocate_precheck_exhaustion() {
        let mut pmm = TestPmm {
            free_frames: 5,
            fail_after: 5,
            alloc_count: 0,
        };
        let mut mapper = TestMapper::new();
        let mut mmap_next = DEFAULT_MMAP_BASE;

        let res = mmap_allocate(
            0,
            10 * PAGE_SIZE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            USER_SPACE_END,
            pmm.free_frames,
            &mut mmap_next,
            &mut pmm,
            &mut mapper,
            |_| {},
        );

        assert_eq!(res, Err(ENOMEM));
        assert_eq!(mmap_next, DEFAULT_MMAP_BASE);
        assert_eq!(pmm.free_frames, 5);
        assert!(mapper.mapped.is_empty());
    }

    #[test]
    fn test_mmap_allocate_mid_loop_frame_exhaustion_rollback() {
        // Pre-check says 10 free frames, but allocator artificially fails at page 3
        let mut pmm = TestPmm {
            free_frames: 10,
            fail_after: 3,
            alloc_count: 0,
        };
        let mut mapper = TestMapper::new();
        let mut mmap_next = DEFAULT_MMAP_BASE;

        let res = mmap_allocate(
            0,
            6 * PAGE_SIZE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            USER_SPACE_END,
            pmm.free_frames,
            &mut mmap_next,
            &mut pmm,
            &mut mapper,
            |_| {},
        );

        assert_eq!(res, Err(ENOMEM));
        assert_eq!(mmap_next, DEFAULT_MMAP_BASE, "mmap_next_vaddr must be rolled back");
        assert!(mapper.mapped.is_empty(), "All partially mapped pages must be unmapped");
    }

    #[test]
    fn test_mmap_allocate_map_page_failure_rollback() {
        // Simulates page table allocation failure during map_page on page 3 of 5
        let mut pmm = TestPmm {
            free_frames: 10,
            fail_after: 10,
            alloc_count: 0,
        };
        let mut mapper = TestMapper::new();
        mapper.fail_at_page = Some(3);
        let mut mmap_next = DEFAULT_MMAP_BASE;

        let res = mmap_allocate(
            0,
            5 * PAGE_SIZE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            USER_SPACE_END,
            pmm.free_frames,
            &mut mmap_next,
            &mut pmm,
            &mut mapper,
            |_| {},
        );

        assert_eq!(res, Err(ENOMEM));
        assert_eq!(mmap_next, DEFAULT_MMAP_BASE, "mmap_next_vaddr must be rolled back on map_page failure");
        assert_eq!(pmm.free_frames, 8, "Frames 1 and 2 unmapped, frame 3 freed on map_page error");
        assert!(mapper.mapped.is_empty(), "All partially mapped pages must be unmapped");
    }

    #[test]
    fn test_mmap_allocate_bounds_and_overflow() {
        let mut pmm = TestPmm {
            free_frames: 100,
            fail_after: 100,
            alloc_count: 0,
        };
        let mut mapper = TestMapper::new();
        let mut mmap_next = DEFAULT_MMAP_BASE;

        assert_eq!(
            mmap_allocate(
                0,
                0,
                PROT_READ,
                MAP_PRIVATE,
                USER_SPACE_END,
                pmm.free_frames,
                &mut mmap_next,
                &mut pmm,
                &mut mapper,
                |_| {},
            ),
            Err(EINVAL)
        );

        assert_eq!(
            mmap_allocate(
                0,
                usize::MAX,
                PROT_READ,
                MAP_PRIVATE,
                USER_SPACE_END,
                pmm.free_frames,
                &mut mmap_next,
                &mut pmm,
                &mut mapper,
                |_| {},
            ),
            Err(ENOMEM)
        );

        assert_eq!(
            mmap_allocate(
                USER_SPACE_END - 2048,
                8192,
                PROT_READ,
                MAP_PRIVATE,
                USER_SPACE_END,
                pmm.free_frames,
                &mut mmap_next,
                &mut pmm,
                &mut mapper,
                |_| {},
            ),
            Err(ENOMEM)
        );
    }
}
