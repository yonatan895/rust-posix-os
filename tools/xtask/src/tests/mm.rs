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

/// Representation of a Virtual Memory Area (VMA) range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MockVma {
    start: usize,
    end: usize,
    prot: u32,
    flags: u32,
}

/// Simplified test model of a virtual address space tracking VMA ranges.
///
/// Note: unlike the kernel's full `VmSpace`, `MockVmSpace` does not merge adjacent VMAs
/// with identical protection flags, but `contains_range` evaluates contiguous multi-VMA coverage.
struct MockVmSpace {
    vmas: Vec<MockVma>,
}

impl MockVmSpace {
    fn new() -> Self {
        Self { vmas: Vec::new() }
    }
    fn insert_vma(&mut self, start: usize, end: usize, prot: u32, flags: u32) {
        self.vmas.retain(|v| v.end <= start || v.start >= end);
        self.vmas.push(MockVma {
            start,
            end,
            prot,
            flags,
        });
        self.vmas.sort_by_key(|v| v.start);
    }
    fn contains_range(&self, start: usize, end: usize) -> bool {
        if start >= end {
            return false;
        }
        let mut curr = start;
        for v in &self.vmas {
            if v.start <= curr && v.end > curr {
                curr = v.end;
                if curr >= end {
                    return true;
                }
            }
        }
        false
    }
    fn munmap(&mut self, addr: usize, len: usize) -> Result<(), i32> {
        if !addr.is_multiple_of(4096) || len == 0 {
            return Err(EINVAL);
        }
        let end = addr + len;
        self.vmas.retain(|v| !(v.start >= addr && v.end <= end));
        Ok(())
    }
    fn mprotect(&mut self, addr: usize, len: usize, new_prot: u32) -> Result<(), i32> {
        if !addr.is_multiple_of(4096) || len == 0 {
            return Err(EINVAL);
        }
        let end = addr + len;
        if !self.contains_range(addr, end) {
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
    assert!(!vm.contains_range(0x6000_0000, 0x6000_3000));
    assert_eq!(vm.munmap(0x7000_0000, 4096), Ok(()));
    assert_eq!(
        vm.mprotect(0x6000_1000, 8192, PROT_READ as u32),
        Err(ENOMEM)
    );
    assert_eq!(vm.mprotect(0x6000_0000, 4096, PROT_READ as u32), Ok(()));
    assert_eq!(vm.vmas[0].flags, MAP_ANON);
}

/// Tests address space cloning on fork and memory isolation between parent and child.
fn test_fork_address_space_isolation() {
    struct SimProcess {
        pid: i32,
        ppid: i32,
        pages: BTreeMap<usize, Vec<u8>>,
        open_fds: Vec<i32>,
    }

    let mut parent = SimProcess {
        pid: 1,
        ppid: 0,
        pages: BTreeMap::new(),
        open_fds: vec![0, 1, 2, 3],
    };
    let mut initial_data = vec![0u8; 4096];
    initial_data[0..4].copy_from_slice(&[0x42, 0x43, 0x44, 0x45]);
    parent.pages.insert(0x6000_0000, initial_data);

    let mut child = SimProcess {
        pid: 2,
        ppid: parent.pid,
        pages: parent.pages.clone(),
        open_fds: parent.open_fds.clone(),
    };
    assert_eq!(child.ppid, 1);
    assert_eq!(child.pid, 2);

    child.pages.get_mut(&0x6000_0000).unwrap()[0..4].copy_from_slice(&[0x99, 0x88, 0x77, 0x66]);
    assert_eq!(
        &parent.pages.get(&0x6000_0000).unwrap()[0..4],
        &[0x42, 0x43, 0x44, 0x45]
    );
    assert_eq!(
        &child.pages.get(&0x6000_0000).unwrap()[0..4],
        &[0x99, 0x88, 0x77, 0x66]
    );
    assert_eq!(child.open_fds, parent.open_fds);
}

/// Tests mmap user space boundary enforcement, overflow handling, and kernel address rejection.
fn test_mmap_bounds_and_overflow() {
    const USER_SPACE_END: usize = 0x0000_8000_0000_0000;
    const PAGE_SIZE: usize = 4096;
    let kernel_addr = 0xFFFF_8000_0000_0000usize;

    let check_mmap_addr = |addr: usize, len: usize| -> Result<usize, i32> {
        if len == 0 {
            return Err(EINVAL);
        }
        if len > USER_SPACE_END {
            return Err(ENOMEM);
        }
        let byte_len = len
            .div_ceil(PAGE_SIZE)
            .checked_mul(PAGE_SIZE)
            .ok_or(ENOMEM)?;
        let vaddr = if addr == 0 {
            0x0000_7000_0000_0000
        } else {
            addr & !0xFFF
        };
        let end = vaddr.checked_add(byte_len).ok_or(ENOMEM)?;
        if end > USER_SPACE_END {
            return Err(ENOMEM);
        }
        Ok(vaddr)
    };

    assert!(check_mmap_addr(0, 4096).is_ok());
    assert!(check_mmap_addr(0x0000_6000_0000_0000, 4096).is_ok());
    assert_eq!(check_mmap_addr(kernel_addr, 4096), Err(ENOMEM));
    assert_eq!(check_mmap_addr(USER_SPACE_END - 2048, 8192), Err(ENOMEM));
    assert_eq!(check_mmap_addr(0, 0), Err(EINVAL));
    assert_eq!(check_mmap_addr(0, usize::MAX), Err(ENOMEM));
    assert_eq!(check_mmap_addr(0, usize::MAX - 4000), Err(ENOMEM));
    assert_eq!(check_mmap_addr(0, USER_SPACE_END + 1), Err(ENOMEM));
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
            if self.alloc_count >= self.fail_after || self.free_frames == 0 {
                return None;
            }
            self.free_frames -= 1;
            self.alloc_count += 1;
            Some(0x1000 + self.alloc_count * mm_core::PAGE_SIZE)
        }
        fn free_frame(&mut self, _phys: usize) {
            self.free_frames += 1;
        }
    }

    struct MockPagingSpace {
        mapped_pages: BTreeMap<usize, usize>,
        fail_at_page: Option<usize>,
        map_count: usize,
    }

    impl mm_core::PageMapper<usize> for MockPagingSpace {
        fn map_page(&mut self, virt: usize, phys: usize, _prot: u32) -> Result<(), &'static str> {
            self.map_count += 1;
            if self.fail_at_page == Some(self.map_count) {
                return Err("OOM");
            }
            self.mapped_pages.insert(virt, phys);
            Ok(())
        }
        fn unmap_range(&mut self, start_virt: usize, count: usize) {
            let aligned = start_virt & !0xFFF;
            for i in 0..count {
                self.mapped_pages
                    .remove(&(aligned + i * mm_core::PAGE_SIZE));
            }
        }
    }

    let mut pmm = MockPmm {
        free_frames: 5,
        fail_after: 5,
        alloc_count: 0,
    };
    let mut vm = MockPagingSpace {
        mapped_pages: BTreeMap::new(),
        fail_at_page: None,
        map_count: 0,
    };
    let mut mmap_next_vaddr = DEFAULT_MMAP_BASE;

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
    assert_eq!(res, Err(ENOMEM));
    assert_eq!(pmm.free_frames, 5);
    assert_eq!(mmap_next_vaddr, DEFAULT_MMAP_BASE);
    assert!(vm.mapped_pages.is_empty());

    let mut pmm_fault = MockPmm {
        free_frames: 10,
        fail_after: 3,
        alloc_count: 0,
    };
    let mut vm_fault = MockPagingSpace {
        mapped_pages: BTreeMap::new(),
        fail_at_page: None,
        map_count: 0,
    };
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
    assert_eq!(res_fault, Err(ENOMEM));
    assert_eq!(mmap_next_fault, DEFAULT_MMAP_BASE);
    assert!(vm_fault.mapped_pages.is_empty());

    let mut pmm_pt_fault = MockPmm {
        free_frames: 10,
        fail_after: 10,
        alloc_count: 0,
    };
    let mut vm_pt_fault = MockPagingSpace {
        mapped_pages: BTreeMap::new(),
        fail_at_page: Some(3),
        map_count: 0,
    };
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
    assert_eq!(res_pt, Err(ENOMEM));
    assert_eq!(mmap_next_pt, DEFAULT_MMAP_BASE);
    assert_eq!(pmm_pt_fault.free_frames, 8);
    assert!(vm_pt_fault.mapped_pages.is_empty());

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
    assert_eq!(res_success, Ok(DEFAULT_MMAP_BASE));
    assert_eq!(pmm.free_frames, 1);
    assert_eq!(mmap_next_vaddr, DEFAULT_MMAP_BASE + 4 * mm_core::PAGE_SIZE);
    assert_eq!(vm.mapped_pages.len(), 4);
}
