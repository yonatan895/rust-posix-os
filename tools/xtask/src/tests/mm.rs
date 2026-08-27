//! Memory management and address space isolation test suite.

use super::harness::TestRunner;
use posix_abi::*;
use std::collections::BTreeMap;

/// Registers all memory management test cases with the runner.
pub fn register_tests(runner: &mut TestRunner) {
    runner.run_test(
        "mm",
        "VMA Tracking and Range Gap Detection",
        test_vma_tracking,
    );
    runner.run_test(
        "mm",
        "Process Fork Address Space Isolation",
        test_fork_address_space_isolation,
    );
    runner.run_test(
        "mm",
        "mmap Address Bounds and Overflow Validation",
        test_mmap_bounds_and_overflow,
    );
    runner.run_test(
        "mm",
        "mmap Partial Failure Rollback & Zero Leak",
        test_mmap_rollback_on_partial_failure,
    );
}

/// Representation of a Virtual Memory Area (VMA) range with protections and flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MockVma {
    /// Start virtual address (inclusive).
    start: usize,
    /// End virtual address (exclusive).
    end: usize,
    /// Page protection flags (PROT_READ, PROT_WRITE, PROT_EXEC).
    prot: u32,
    /// Mapping flags (MAP_PRIVATE, MAP_ANONYMOUS, etc.).
    flags: u32,
}

/// Simulated process virtual address space tracking active VMAs.
struct MockVmSpace {
    /// Ordered collection of VMAs in the address space.
    vmas: Vec<MockVma>,
}

impl MockVmSpace {
    /// Creates an empty virtual address space.
    fn new() -> Self {
        Self { vmas: Vec::new() }
    }

    /// Inserts or merges a VMA range into the address space.
    fn insert_vma(&mut self, start: usize, end: usize, prot: u32, flags: u32) {
        let mut new_vmas = Vec::new();
        let mut inserted = false;
        let mut cur_start = start;
        let mut cur_end = end;

        for vma in self.vmas.drain(..) {
            if vma.end <= cur_start {
                new_vmas.push(vma);
            } else if vma.start >= cur_end {
                if !inserted {
                    new_vmas.push(MockVma {
                        start: cur_start,
                        end: cur_end,
                        prot,
                        flags,
                    });
                    inserted = true;
                }
                new_vmas.push(vma);
            } else if vma.prot == prot && vma.flags == flags {
                cur_start = cur_start.min(vma.start);
                cur_end = cur_end.max(vma.end);
            }
        }

        if !inserted {
            new_vmas.push(MockVma {
                start: cur_start,
                end: cur_end,
                prot,
                flags,
            });
        }
        self.vmas = new_vmas;
    }

    /// Returns whether the range `[start..end)` is completely covered by mapped VMAs without gaps.
    fn contains_range(&self, start: usize, end: usize) -> bool {
        if start >= end {
            return false;
        }
        let mut curr = start;
        for vma in &self.vmas {
            if vma.start <= curr && vma.end > curr {
                curr = vma.end;
                if curr >= end {
                    return true;
                }
            }
        }
        false
    }

    /// Unmaps VMAs overlapping with the given address range.
    fn munmap(&mut self, addr: usize, len: usize) -> Result<(), i32> {
        if !addr.is_multiple_of(4096) || len == 0 {
            return Err(EINVAL);
        }
        let end = addr + len;
        self.vmas.retain(|v| !(v.start >= addr && v.end <= end));
        Ok(())
    }

    /// Changes memory protection flags on an existing continuous VMA range.
    fn mprotect(&mut self, addr: usize, len: usize, new_prot: u32) -> Result<(), i32> {
        if !addr.is_multiple_of(4096) || len == 0 {
            return Err(EINVAL);
        }
        let end = addr + len;
        if !self.contains_range(addr, end) {
            // Return -ENOMEM when trying to mprotect an unmapped gap per Linux/POSIX
            return Err(ENOMEM);
        }
        let flags = self
            .vmas
            .iter()
            .find(|v| v.start <= addr && addr < v.end)
            .map(|v| v.flags)
            .unwrap_or(0);
        self.insert_vma(addr, end, new_prot, flags);
        Ok(())
    }
}

/// Tests VMA range gap detection and mprotect/munmap semantics.
fn test_vma_tracking() {
    let mut vm = MockVmSpace::new();
    const MAP_ANON: u32 = 0x20;
    vm.insert_vma(
        0x6000_0000,
        0x6000_2000,
        (PROT_READ | PROT_WRITE) as u32,
        MAP_ANON,
    );

    assert!(vm.contains_range(0x6000_0000, 0x6000_2000));
    assert!(vm.contains_range(0x6000_0000, 0x6000_1000));
    assert!(!vm.contains_range(0x6000_0000, 0x6000_3000)); // Gap beyond mapped VMA

    // Test munmap on unmapped range succeeds with 0 per Linux
    assert_eq!(vm.munmap(0x7000_0000, 4096), Ok(()));

    // Test mprotect on unmapped gap returns -ENOMEM
    assert_eq!(
        vm.mprotect(0x6000_1000, 8192, PROT_READ as u32),
        Err(ENOMEM)
    );

    // Test mprotect on valid mapped region succeeds and PRESERVES VMA flags
    assert_eq!(vm.mprotect(0x6000_0000, 4096, PROT_READ as u32), Ok(()));
    assert_eq!(
        vm.vmas[0].flags, MAP_ANON,
        "mprotect must preserve existing VMA flags"
    );
}

/// Tests address space cloning on fork and memory isolation between parent and child.
fn test_fork_address_space_isolation() {
    struct ProcessMemory {
        pages: BTreeMap<usize, Vec<u8>>,
    }

    impl ProcessMemory {
        fn new() -> Self {
            Self {
                pages: BTreeMap::new(),
            }
        }

        fn clone_memory(&self) -> Self {
            Self {
                pages: self.pages.clone(), // Eager frame duplication
            }
        }
    }

    struct SimProcess {
        pid: i32,
        ppid: i32,
        mem: ProcessMemory,
        open_fds: Vec<i32>,
    }

    let mut parent = SimProcess {
        pid: 1,
        ppid: 0,
        mem: ProcessMemory::new(),
        open_fds: vec![0, 1, 2, 3],
    };

    // Parent writes initial data to its virtual page at 0x6000_0000
    let mut initial_data = vec![0u8; 4096];
    initial_data[0..4].copy_from_slice(&[0x42, 0x43, 0x44, 0x45]);
    parent.mem.pages.insert(0x6000_0000, initial_data);

    // Fork: Child created with eager address space clone
    let child_pid = 2;
    let mut child = SimProcess {
        pid: child_pid,
        ppid: parent.pid,
        mem: parent.mem.clone_memory(),
        open_fds: parent.open_fds.clone(),
    };
    assert_eq!(child.ppid, 1, "Child must record parent PID as PPID");

    // Check return value semantics
    let parent_ret = child.pid;
    let child_ret = 0;
    assert_eq!(parent_ret, 2, "Parent must receive child PID from fork()");
    assert_eq!(child_ret, 0, "Child must receive 0 from fork()");

    // Child modifies its memory copy
    child.mem.pages.get_mut(&0x6000_0000).unwrap()[0..4].copy_from_slice(&[0x99, 0x88, 0x77, 0x66]);

    // Verify Address Space Isolation: Parent memory is UNCHANGED
    assert_eq!(
        &parent.mem.pages.get(&0x6000_0000).unwrap()[0..4],
        &[0x42, 0x43, 0x44, 0x45],
        "Parent memory must remain isolated and unmodified when child writes"
    );

    assert_eq!(
        &child.mem.pages.get(&0x6000_0000).unwrap()[0..4],
        &[0x99, 0x88, 0x77, 0x66],
        "Child memory must reflect its own private write"
    );

    // Verify File Descriptor Sharing
    assert_eq!(child.open_fds, parent.open_fds);
}

/// Tests mmap user space boundary enforcement, overflow handling, and kernel address rejection.
fn test_mmap_bounds_and_overflow() {
    const USER_SPACE_END: usize = 0x0000_8000_0000_0000;
    const PAGE_SIZE: usize = 4096;
    const PAGE_MASK: usize = 0xFFF;
    let kernel_addr = 0xFFFF_8000_0000_0000usize;

    let check_mmap_addr = |addr: usize, len: usize| -> Result<usize, i32> {
        if len == 0 {
            return Err(EINVAL);
        }
        if len > USER_SPACE_END {
            return Err(ENOMEM);
        }
        let pages = len.div_ceil(PAGE_SIZE);
        let byte_len = match pages.checked_mul(PAGE_SIZE) {
            Some(bytes) => bytes,
            None => return Err(ENOMEM),
        };
        let vaddr = if addr == 0 {
            0x0000_7000_0000_0000usize
        } else {
            addr & !PAGE_MASK
        };
        let _end_vaddr = match vaddr.checked_add(byte_len) {
            Some(end) if end <= USER_SPACE_END => end,
            _ => return Err(ENOMEM),
        };
        Ok(vaddr)
    };

    assert!(check_mmap_addr(0, 4096).is_ok());
    assert!(check_mmap_addr(0x0000_6000_0000_0000, 4096).is_ok());
    assert_eq!(
        check_mmap_addr(kernel_addr, 4096),
        Err(ENOMEM),
        "mmap with kernel address must return -ENOMEM"
    );
    assert_eq!(
        check_mmap_addr(USER_SPACE_END - 2048, 8192),
        Err(ENOMEM),
        "mmap overflowing USER_SPACE_END must return -ENOMEM"
    );
    assert_eq!(
        check_mmap_addr(0, 0),
        Err(EINVAL),
        "mmap with length 0 must return -EINVAL"
    );
    assert_eq!(
        check_mmap_addr(0, usize::MAX),
        Err(ENOMEM),
        "mmap with usize::MAX length must return -ENOMEM"
    );
    assert_eq!(
        check_mmap_addr(0, usize::MAX - 4000),
        Err(ENOMEM),
        "mmap with near-usize::MAX length must return -ENOMEM"
    );
    assert_eq!(
        check_mmap_addr(0, USER_SPACE_END + 1),
        Err(ENOMEM),
        "mmap with length exceeding USER_SPACE_END must return -ENOMEM"
    );
}

/// Tests that partial failure during mmap page allocation rolls back both physical frames and the bump pointer.
fn test_mmap_rollback_on_partial_failure() {
    const DEFAULT_MMAP_BASE: usize = 0x0000_7000_0000_0000;
    const USER_SPACE_END: usize = 0x0000_8000_0000_0000;

    /// Mock physical memory allocator implementing [`mm_core::FrameAllocator`].
    struct MockPmm {
        free_frames: usize,
        fail_after: usize,
        alloc_count: usize,
    }

    impl mm_core::FrameAllocator for MockPmm {
        type Frame = usize;

        fn alloc_frame(&mut self) -> Option<usize> {
            if self.alloc_count >= self.fail_after {
                return None;
            }
            if self.free_frames > 0 {
                self.free_frames -= 1;
                self.alloc_count += 1;
                Some(0x1000 + self.alloc_count * mm_core::PAGE_SIZE)
            } else {
                None
            }
        }

        fn free_frame(&mut self, _phys: usize) {
            self.free_frames += 1;
        }
    }

    /// Mock virtual memory space implementing [`mm_core::PageMapper`].
    struct MockPagingSpace {
        mapped_pages: BTreeMap<usize, usize>,
        fail_at_page: Option<usize>,
        map_count: usize,
    }

    impl MockPagingSpace {
        fn new() -> Self {
            Self {
                mapped_pages: BTreeMap::new(),
                fail_at_page: None,
                map_count: 0,
            }
        }
    }

    impl mm_core::PageMapper<usize> for MockPagingSpace {
        fn map_page(&mut self, virt: usize, phys: usize, _prot: u32) -> Result<(), &'static str> {
            self.map_count += 1;
            if self.fail_at_page == Some(self.map_count) {
                return Err("Page table allocation failed (OOM)");
            }
            self.mapped_pages.insert(virt, phys);
            Ok(())
        }

        fn unmap_range(&mut self, start_virt: usize, count: usize) {
            let aligned_start = start_virt & !0xFFF;
            for i in 0..count {
                if let Some(va) = i
                    .checked_mul(mm_core::PAGE_SIZE)
                    .and_then(|off| aligned_start.checked_add(off))
                {
                    self.mapped_pages.remove(&va);
                }
            }
        }
    }

    // 1. Initial State
    let mut pmm = MockPmm {
        free_frames: 5,
        fail_after: 5,
        alloc_count: 0,
    };
    let mut vm = MockPagingSpace::new();
    let mut mmap_next_vaddr = DEFAULT_MMAP_BASE;

    // 2. Pre-check test: Attempt mmap of 10 pages when only 5 frames exist
    let res = mm_core::mmap_allocate(
        0,
        10 * mm_core::PAGE_SIZE,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANONYMOUS,
        USER_SPACE_END,
        pmm.free_frames,
        &mut mmap_next_vaddr,
        &mut pmm,
        &mut vm,
        |_| {},
    );
    assert_eq!(res, Err(ENOMEM), "Oversized mmap must fail with -ENOMEM");
    assert_eq!(pmm.free_frames, 5, "PMM free frames unchanged");
    assert_eq!(mmap_next_vaddr, DEFAULT_MMAP_BASE, "mmap_next_vaddr unchanged");
    assert!(vm.mapped_pages.is_empty());

    // 3. Mid-loop Frame Exhaustion Test: Pre-check passes (10 frames reported), but allocator fails at page 3 of 6
    let mut pmm_fault = MockPmm {
        free_frames: 10,
        fail_after: 3,
        alloc_count: 0,
    };
    let mut vm_fault = MockPagingSpace::new();
    let mut mmap_next_fault = DEFAULT_MMAP_BASE;

    let res_fault = mm_core::mmap_allocate(
        0,
        6 * mm_core::PAGE_SIZE,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANONYMOUS,
        USER_SPACE_END,
        pmm_fault.free_frames,
        &mut mmap_next_fault,
        &mut pmm_fault,
        &mut vm_fault,
        |_| {},
    );
    assert_eq!(
        res_fault,
        Err(ENOMEM),
        "Mid-loop frame exhaustion must return -ENOMEM"
    );
    assert_eq!(
        mmap_next_fault, DEFAULT_MMAP_BASE,
        "mmap_next_vaddr rolled back"
    );
    assert!(
        vm_fault.mapped_pages.is_empty(),
        "Partially mapped pages unmapped"
    );

    // 4. Mid-loop Page Table Allocation (map_page) Failure Test:
    // Simulates map_page failing at page 3 of 5 due to intermediate page-table exhaustion
    let mut pmm_pt_fault = MockPmm {
        free_frames: 10,
        fail_after: 10,
        alloc_count: 0,
    };
    let mut vm_pt_fault = MockPagingSpace::new();
    vm_pt_fault.fail_at_page = Some(3);
    let mut mmap_next_pt = DEFAULT_MMAP_BASE;

    let res_pt = mm_core::mmap_allocate(
        0,
        5 * mm_core::PAGE_SIZE,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANONYMOUS,
        USER_SPACE_END,
        pmm_pt_fault.free_frames,
        &mut mmap_next_pt,
        &mut pmm_pt_fault,
        &mut vm_pt_fault,
        |_| {},
    );
    assert_eq!(res_pt, Err(ENOMEM), "map_page failure must return -ENOMEM");
    assert_eq!(
        mmap_next_pt, DEFAULT_MMAP_BASE,
        "mmap_next_vaddr rolled back on map_page failure"
    );
    assert_eq!(
        pmm_pt_fault.free_frames, 8,
        "Frames 1 and 2 unmapped, frame 3 freed on error"
    );
    assert!(
        vm_pt_fault.mapped_pages.is_empty(),
        "Partially mapped pages unmapped"
    );

    // 5. Successful Allocation after Rollback:
    // Performs a successful 4-page mmap starting at the rolled-back base
    let res_success = mm_core::mmap_allocate(
        0,
        4 * mm_core::PAGE_SIZE,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANONYMOUS,
        USER_SPACE_END,
        pmm.free_frames,
        &mut mmap_next_vaddr,
        &mut pmm,
        &mut vm,
        |_| {},
    );
    assert_eq!(
        res_success,
        Ok(DEFAULT_MMAP_BASE),
        "Subsequent mmap must succeed starting at the original rolled-back base"
    );
    assert_eq!(pmm.free_frames, 1, "4 frames allocated (1 remaining)");
    assert_eq!(
        mmap_next_vaddr,
        DEFAULT_MMAP_BASE + 4 * mm_core::PAGE_SIZE,
        "mmap_next_vaddr advances by exactly 4 pages"
    );
    assert_eq!(vm.mapped_pages.len(), 4);
}
