//! POSIX Virtual Memory Management System Calls.

use posix_abi::*;
use crate::ostd::mm::{alloc_frame, zero_phys_frame, PAGE_SIZE};
use crate::services::process::get_current_process;

pub fn sys_mmap(addr: usize, length: usize, prot: i32, _flags: i32) -> isize {
    if length == 0 {
        return -(EINVAL as isize);
    }
    let pages = (length + PAGE_SIZE - 1) / PAGE_SIZE;

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
        addr
    };

    if let Some(ref mut vm) = proc.vm_space {
        for i in 0..pages {
            let page_vaddr = vaddr + i * PAGE_SIZE;
            if let Some(frame) = alloc_frame() {
                let mut pte_flags = crate::ostd::mm::PAGE_PRESENT | crate::ostd::mm::PAGE_USER;
                if prot & PROT_WRITE != 0 {
                    pte_flags |= crate::ostd::mm::PAGE_WRITABLE;
                }
                let _ = vm.map_page(page_vaddr, frame, pte_flags);
                zero_phys_frame(frame);
            } else {
                return -(ENOMEM as isize);
            }
        }
    }

    vaddr as isize
}

pub fn sys_munmap(addr: usize, length: usize) -> isize {
    if addr % PAGE_SIZE != 0 || length == 0 {
        return -(EINVAL as isize);
    }
    let pages = (length + PAGE_SIZE - 1) / PAGE_SIZE;

    let proc_lock = match get_current_process() {
        Some(p) => p,
        None => return -(ESRCH as isize),
    };
    let mut proc = proc_lock.lock();

    if let Some(ref mut vm) = proc.vm_space {
        for i in 0..pages {
            let page_vaddr = addr + i * PAGE_SIZE;
            vm.unmap_page(page_vaddr);
        }
    }

    0
}
