//! POSIX Virtual Memory Management System Calls.

use crate::ostd::mm::{PAGE_SIZE, alloc_frame, free_frame, get_pmm_stats, zero_phys_frame};
use crate::services::process::get_current_process;
use posix_abi::*;

/// Adapter implementing [`mm_core::FrameAllocator`] over the kernel Physical Memory Manager.
struct KernelPmmAllocator;

impl mm_core::FrameAllocator for KernelPmmAllocator {
    type Frame = usize;

    #[inline(always)]
    fn alloc_frame(&mut self) -> Option<usize> {
        alloc_frame()
    }

    #[inline(always)]
    fn free_frame(&mut self, frame: usize) {
        free_frame(frame);
    }
}

/// Adapter implementing [`mm_core::PageMapper`] over the process [`crate::ostd::mm::VmSpace`].
struct VmSpaceMapper<'a>(&'a mut crate::ostd::mm::VmSpace);

impl mm_core::PageMapper<usize> for VmSpaceMapper<'_> {
    #[inline(always)]
    fn map_page(&mut self, vaddr: usize, frame: usize, prot: u32) -> Result<(), &'static str> {
        let pte_flags = crate::ostd::mm::PageFlags::from_prot(prot);
        self.0.map_page(vaddr, frame, pte_flags)
    }

    #[inline(always)]
    fn unmap_range(&mut self, start_vaddr: usize, count: usize) {
        self.0.unmap_range(start_vaddr, count);
    }
}

/// Maps pages of anonymous virtual memory into the calling process address space.
pub fn sys_mmap(addr: usize, length: usize, prot: i32, flags: i32) -> isize {
    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();

    let (_, free_frames) = get_pmm_stats();
    let mut pmm_alloc = KernelPmmAllocator;

    let proc_ref = &mut *proc;
    let vm = match proc_ref.vm_space {
        Some(ref mut vm) => vm,
        None => return -(ENOMEM as isize),
    };
    let mmap_next_vaddr = &mut proc_ref.mmap_next_vaddr;

    let mut mapper = VmSpaceMapper(vm);
    match mm_core::mmap_allocate(
        addr,
        length,
        prot,
        flags,
        crate::ostd::mm::USER_SPACE_END,
        free_frames,
        mmap_next_vaddr,
        &mut pmm_alloc,
        &mut mapper,
        zero_phys_frame,
    ) {
        Ok(vaddr) => {
            let pages = length.div_ceil(PAGE_SIZE);
            if let Some(end_vaddr) = pages
                .checked_mul(PAGE_SIZE)
                .and_then(|byte_len| vaddr.checked_add(byte_len))
            {
                mapper.0.insert_vma(vaddr, end_vaddr, prot as u32, flags as u32);
            }
            vaddr as isize
        }
        Err(err) => -(err as isize),
    }
}

/// Unmaps a range of virtual memory pages from the calling process address space.
pub fn sys_munmap(addr: usize, length: usize) -> isize {
    if !addr.is_multiple_of(PAGE_SIZE) || length == 0 {
        return -(EINVAL as isize);
    }
    if length > crate::ostd::mm::USER_SPACE_END {
        return -(EINVAL as isize);
    }
    let pages = length.div_ceil(PAGE_SIZE);
    let byte_len = match pages.checked_mul(PAGE_SIZE) {
        Some(len) => len,
        None => return -(EINVAL as isize),
    };
    let end_addr = match addr.checked_add(byte_len) {
        Some(end) if end <= crate::ostd::mm::USER_SPACE_END => end,
        _ => return -(EINVAL as isize),
    };

    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();

    if let Some(ref mut vm) = proc.vm_space {
        vm.remove_vma_range(addr, end_addr);
    }

    0
}

/// Modifies access protections on a range of mapped virtual memory pages.
pub fn sys_mprotect(addr: usize, length: usize, prot: i32) -> isize {
    if !addr.is_multiple_of(PAGE_SIZE) || length == 0 {
        return -(EINVAL as isize);
    }
    if length > crate::ostd::mm::USER_SPACE_END {
        return -(ENOMEM as isize);
    }
    let pages = length.div_ceil(PAGE_SIZE);
    let byte_len = match pages.checked_mul(PAGE_SIZE) {
        Some(len) => len,
        None => return -(ENOMEM as isize),
    };
    let end_addr = match addr.checked_add(byte_len) {
        Some(end) if end <= crate::ostd::mm::USER_SPACE_END => end,
        _ => return -(ENOMEM as isize),
    };

    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();

    if let Some(ref mut vm) = proc.vm_space {
        if !vm.contains_range(addr, end_addr) {
            // Linux/POSIX specification: return -ENOMEM if any part of the address range is not mapped
            return -(ENOMEM as isize);
        }
        if vm.mprotect_range(addr, end_addr, prot as u32).is_err() {
            return -(ENOMEM as isize);
        }
    }

    0
}
