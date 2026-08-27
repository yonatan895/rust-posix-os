//! POSIX Virtual Memory Management System Calls.

use crate::ostd::mm::{PAGE_SIZE, alloc_frame, free_frame, zero_phys_frame};
use crate::services::process::get_current_process;
use posix_abi::*;

/// Maps pages of anonymous virtual memory into the calling process address space.
pub fn sys_mmap(addr: usize, length: usize, prot: i32, flags: i32) -> isize {
    if length == 0 {
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

    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();

    let rollback_vaddr = proc.mmap_next_vaddr;
    let is_anonymous_bump = addr == 0;

    let vaddr = if is_anonymous_bump {
        let base = proc.mmap_next_vaddr;
        proc.mmap_next_vaddr = match proc.mmap_next_vaddr.checked_add(byte_len) {
            Some(next) if next <= crate::ostd::mm::USER_SPACE_END => next,
            _ => return -(ENOMEM as isize),
        };
        base
    } else {
        addr & !0xFFF
    };

    let end_vaddr = match vaddr.checked_add(byte_len) {
        Some(end) if end <= crate::ostd::mm::USER_SPACE_END => end,
        _ => {
            if is_anonymous_bump {
                proc.mmap_next_vaddr = rollback_vaddr;
            }
            return -(ENOMEM as isize);
        }
    };

    if let Some(ref mut vm) = proc.vm_space {
        let pte_flags = crate::ostd::mm::PageFlags::from_prot(prot as u32);
        for (pages_mapped, i) in (0..pages).enumerate() {
            let page_vaddr = match vaddr.checked_add(i * PAGE_SIZE) {
                Some(va) => va,
                None => {
                    vm.unmap_range(vaddr, pages_mapped);
                    if is_anonymous_bump {
                        proc.mmap_next_vaddr = rollback_vaddr;
                    }
                    return -(ENOMEM as isize);
                }
            };
            let frame = match alloc_frame() {
                Some(f) => f,
                None => {
                    vm.unmap_range(vaddr, pages_mapped);
                    if is_anonymous_bump {
                        proc.mmap_next_vaddr = rollback_vaddr;
                    }
                    return -(ENOMEM as isize);
                }
            };
            if vm.map_page(page_vaddr, frame, pte_flags).is_err() {
                free_frame(frame);
                vm.unmap_range(vaddr, pages_mapped);
                if is_anonymous_bump {
                    proc.mmap_next_vaddr = rollback_vaddr;
                }
                return -(ENOMEM as isize);
            }
            zero_phys_frame(frame);
        }
        vm.insert_vma(vaddr, end_vaddr, prot as u32, flags as u32);
    }

    vaddr as isize
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
