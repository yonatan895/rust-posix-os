//! POSIX Virtual Memory Management System Calls.

use crate::ostd::mm::{PAGE_SIZE, alloc_frame, zero_phys_frame};
use crate::services::process::get_current_process;
use posix_abi::*;

pub fn sys_mmap(addr: usize, length: usize, prot: i32, flags: i32) -> isize {
    if length == 0 {
        return -(EINVAL as isize);
    }
    let pages = length.div_ceil(PAGE_SIZE);

    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();

    let vaddr = if addr == 0 {
        let base = proc.mmap_next_vaddr;
        proc.mmap_next_vaddr += pages * PAGE_SIZE;
        base
    } else {
        addr & !0xFFF
    };

    if let Some(ref mut vm) = proc.vm_space {
        for i in 0..pages {
            let page_vaddr = vaddr + i * PAGE_SIZE;
            if let Some(frame) = alloc_frame() {
                let mut pte_flags = crate::ostd::mm::PAGE_PRESENT | crate::ostd::mm::PAGE_USER;
                if prot & PROT_WRITE != 0 {
                    pte_flags |= crate::ostd::mm::PAGE_WRITABLE;
                }
                if prot & PROT_EXEC == 0 {
                    pte_flags |= crate::ostd::mm::PAGE_NX;
                }
                let _ = vm.map_page(page_vaddr, frame, pte_flags);
                zero_phys_frame(frame);
            } else {
                return -(ENOMEM as isize);
            }
        }
        vm.insert_vma(vaddr, vaddr + pages * PAGE_SIZE, prot as u32, flags as u32);
    }

    vaddr as isize
}

pub fn sys_munmap(addr: usize, length: usize) -> isize {
    if !addr.is_multiple_of(PAGE_SIZE) || length == 0 {
        return -(EINVAL as isize);
    }
    let pages = length.div_ceil(PAGE_SIZE);
    let end_addr = addr + pages * PAGE_SIZE;

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

pub fn sys_mprotect(addr: usize, length: usize, prot: i32) -> isize {
    if !addr.is_multiple_of(PAGE_SIZE) || length == 0 {
        return -(EINVAL as isize);
    }
    let pages = length.div_ceil(PAGE_SIZE);
    let end_addr = addr + pages * PAGE_SIZE;

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
