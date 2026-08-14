//! Virtual Memory Manager (VMM) - 4-Level x86_64 Paging.

use super::pmm::{PAGE_SIZE, alloc_frame, free_frame};
use crate::ostd::sync::SpinLock;
use alloc::vec::Vec;

pub const PAGE_PRESENT: u64 = 1 << 0;
pub const PAGE_WRITABLE: u64 = 1 << 1;
pub const PAGE_USER: u64 = 1 << 2;
pub const PAGE_NX: u64 = 1 << 63;

pub static HHDM_OFFSET: SpinLock<usize> = SpinLock::new(0);

#[repr(C, align(4096))]
pub struct PageTable {
    pub entries: [u64; 512],
}

impl PageTable {
    pub const fn empty() -> Self {
        Self { entries: [0; 512] }
    }
}

pub fn phys_to_virt(phys: usize) -> usize {
    phys + *HHDM_OFFSET.lock()
}

pub fn virt_to_phys(virt: usize) -> usize {
    virt.saturating_sub(*HHDM_OFFSET.lock())
}

/// Zero one 4 KiB physical frame via the HHDM.
pub fn zero_phys_frame(phys: usize) {
    let virt = phys_to_virt(phys) as *mut u8;
    // SAFETY: HHDM covers all physical RAM. Frame base is 4KiB page-aligned.
    unsafe { core::ptr::write_bytes(virt, 0, PAGE_SIZE) };
}

/// Initializes the virtual memory manager with the bootloader HHDM virtual offset.
///
/// # Safety
///
/// `hhdm` must be the valid base virtual address of the physical memory direct map.
pub unsafe fn vmm_init(hhdm: usize) {
    *HHDM_OFFSET.lock() = hhdm;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vma {
    pub start: usize,
    pub end: usize,
    pub prot: u32,
    pub flags: u32,
}

pub struct VmSpace {
    pub pml4_phys: usize,
    pub vmas: Vec<Vma>,
}

impl VmSpace {
    pub fn new() -> Option<Self> {
        let pml4_phys = alloc_frame()?;
        let pml4_virt = phys_to_virt(pml4_phys) as *mut PageTable;
        // SAFETY: `pml4_virt` is a valid HHDM pointer to our newly allocated frame.
        unsafe {
            core::ptr::write_bytes(pml4_virt, 0, 1);
            // Copy higher-half kernel mappings from current active PML4
            let active_pml4 =
                phys_to_virt(crate::ostd::arch::read_cr3() as usize & !0xFFF) as *const PageTable;
            for i in 256..512 {
                (*pml4_virt).entries[i] = (*active_pml4).entries[i];
            }
        }
        Some(Self {
            pml4_phys,
            vmas: Vec::new(),
        })
    }

    /// Load this address space into CR3.
    pub fn activate(&self) {
        // SAFETY: `pml4_phys` is a 4 KiB-aligned page-table root we allocated
        // and initialized in `new`. Reloading CR3 is valid in ring 0.
        unsafe {
            core::arch::asm!(
                "mov cr3, {}",
                in(reg) self.pml4_phys,
                options(nostack, preserves_flags)
            );
        }
    }

    /// Inserts a VMA into the sorted VMA list, merging adjacent regions with identical prot/flags.
    pub fn insert_vma(&mut self, start: usize, end: usize, prot: u32, flags: u32) {
        if start >= end {
            return;
        }

        let mut new_vmas = Vec::new();
        let mut inserted = false;
        let mut cur_start = start;
        let mut cur_end = end;

        for vma in self.vmas.drain(..) {
            if vma.end <= cur_start {
                new_vmas.push(vma);
            } else if vma.start >= cur_end {
                if !inserted {
                    new_vmas.push(Vma {
                        start: cur_start,
                        end: cur_end,
                        prot,
                        flags,
                    });
                    inserted = true;
                }
                new_vmas.push(vma);
            } else {
                // Overlap: merge if identical protection/flags
                if vma.prot == prot && vma.flags == flags {
                    cur_start = cur_start.min(vma.start);
                    cur_end = cur_end.max(vma.end);
                } else {
                    // Split existing VMA around the new region
                    if vma.start < cur_start {
                        new_vmas.push(Vma {
                            start: vma.start,
                            end: cur_start,
                            prot: vma.prot,
                            flags: vma.flags,
                        });
                    }
                    if !inserted {
                        new_vmas.push(Vma {
                            start: cur_start,
                            end: cur_end,
                            prot,
                            flags,
                        });
                        inserted = true;
                    }
                    if vma.end > cur_end {
                        new_vmas.push(Vma {
                            start: cur_end,
                            end: vma.end,
                            prot: vma.prot,
                            flags: vma.flags,
                        });
                    }
                }
            }
        }

        if !inserted {
            new_vmas.push(Vma {
                start: cur_start,
                end: cur_end,
                prot,
                flags,
            });
        }

        // Merge contiguous adjacent VMAs with identical permissions
        let mut merged = Vec::new();
        for vma in new_vmas {
            if let Some(last) = merged.last_mut() {
                let last: &mut Vma = last;
                if last.end == vma.start && last.prot == vma.prot && last.flags == vma.flags {
                    last.end = vma.end;
                    continue;
                }
            }
            merged.push(vma);
        }

        self.vmas = merged;
    }

    /// Finds the VMA containing a given virtual address.
    pub fn find_vma(&self, addr: usize) -> Option<&Vma> {
        self.vmas.iter().find(|v| v.start <= addr && addr < v.end)
    }

    /// Checks if the entire range `[start, end)` is covered by one or more contiguous VMAs.
    pub fn contains_range(&self, start: usize, end: usize) -> bool {
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

    /// Removes all VMAs and unmaps all pages in `[start, end)`.
    pub fn remove_vma_range(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }

        let aligned_start = start & !0xFFF;
        let aligned_end = (end + PAGE_SIZE - 1) & !0xFFF;
        let mut page_vaddr = aligned_start;
        while page_vaddr < aligned_end {
            self.unmap_page(page_vaddr);
            page_vaddr += PAGE_SIZE;
        }

        let mut new_vmas = Vec::new();
        for vma in self.vmas.drain(..) {
            if vma.end <= start || vma.start >= end {
                new_vmas.push(vma);
            } else {
                if vma.start < start {
                    new_vmas.push(Vma {
                        start: vma.start,
                        end: start,
                        prot: vma.prot,
                        flags: vma.flags,
                    });
                }
                if vma.end > end {
                    new_vmas.push(Vma {
                        start: end,
                        end: vma.end,
                        prot: vma.prot,
                        flags: vma.flags,
                    });
                }
            }
        }
        self.vmas = new_vmas;
    }

    /// Modifies protection permissions for all pages and VMAs in `[start, end)`.
    pub fn mprotect_range(
        &mut self,
        start: usize,
        end: usize,
        new_prot: u32,
    ) -> Result<(), &'static str> {
        if start >= end || !self.contains_range(start, end) {
            return Err("Unmapped gap in range");
        }

        let aligned_start = start & !0xFFF;
        let aligned_end = (end + PAGE_SIZE - 1) & !0xFFF;
        let mut page_vaddr = aligned_start;

        let mut flags = PAGE_PRESENT | PAGE_USER;
        if new_prot & (posix_abi::PROT_WRITE as u32) != 0 {
            flags |= PAGE_WRITABLE;
        }
        if new_prot & (posix_abi::PROT_EXEC as u32) == 0 {
            flags |= PAGE_NX;
        }

        while page_vaddr < aligned_end {
            self.set_page_flags(page_vaddr, flags);
            page_vaddr += PAGE_SIZE;
        }

        self.insert_vma(start, end, new_prot, 0);

        Ok(())
    }

    /// Updates page table flags for a mapped virtual page.
    pub fn set_page_flags(&mut self, virt_addr: usize, new_flags: u64) {
        let pml4_idx = (virt_addr >> 39) & 0x1FF;
        let pdpt_idx = (virt_addr >> 30) & 0x1FF;
        let pd_idx = (virt_addr >> 21) & 0x1FF;
        let pt_idx = (virt_addr >> 12) & 0x1FF;

        // SAFETY: Table pointers are valid HHDM views of allocated page tables.
        unsafe {
            let pml4 = phys_to_virt(self.pml4_phys) as *mut PageTable;
            if (*pml4).entries[pml4_idx] & PAGE_PRESENT == 0 {
                return;
            }
            let pdpt = phys_to_virt(((*pml4).entries[pml4_idx] & 0x000F_FFFF_FFFF_F000) as usize)
                as *mut PageTable;

            if (*pdpt).entries[pdpt_idx] & PAGE_PRESENT == 0 {
                return;
            }
            let pd = phys_to_virt(((*pdpt).entries[pdpt_idx] & 0x000F_FFFF_FFFF_F000) as usize)
                as *mut PageTable;

            if (*pd).entries[pd_idx] & PAGE_PRESENT == 0 {
                return;
            }
            let pt = phys_to_virt(((*pd).entries[pd_idx] & 0x000F_FFFF_FFFF_F000) as usize)
                as *mut PageTable;

            let entry = (*pt).entries[pt_idx];
            if entry & PAGE_PRESENT != 0 {
                let phys_base = entry & 0x000F_FFFF_FFFF_F000;
                (*pt).entries[pt_idx] = phys_base | new_flags | PAGE_PRESENT;
                core::arch::asm!("invlpg [{}]", in(reg) virt_addr, options(nostack, preserves_flags));
            }
        }
    }

    /// Maps a 4 KiB virtual page to a physical frame in this address space.
    pub fn map_page(
        &mut self,
        virt_addr: usize,
        phys_addr: usize,
        flags: u64,
    ) -> Result<(), &'static str> {
        let pml4_idx = (virt_addr >> 39) & 0x1FF;
        let pdpt_idx = (virt_addr >> 30) & 0x1FF;
        let pd_idx = (virt_addr >> 21) & 0x1FF;
        let pt_idx = (virt_addr >> 12) & 0x1FF;

        // SAFETY: Table pointers are HHDM views of frames we allocated or copied from kernel PML4.
        unsafe {
            let pml4 = phys_to_virt(self.pml4_phys) as *mut PageTable;

            if (*pml4).entries[pml4_idx] & PAGE_PRESENT == 0 {
                let frame = alloc_frame().ok_or("Out of memory for PDPT")?;
                zero_phys_frame(frame);
                (*pml4).entries[pml4_idx] =
                    (frame as u64) | PAGE_PRESENT | PAGE_WRITABLE | (flags & PAGE_USER);
            }
            let pdpt_phys = ((*pml4).entries[pml4_idx] & 0x000F_FFFF_FFFF_F000) as usize;
            let pdpt = phys_to_virt(pdpt_phys) as *mut PageTable;

            if (*pdpt).entries[pdpt_idx] & PAGE_PRESENT == 0 {
                let frame = alloc_frame().ok_or("Out of memory for PD")?;
                zero_phys_frame(frame);
                (*pdpt).entries[pdpt_idx] =
                    (frame as u64) | PAGE_PRESENT | PAGE_WRITABLE | (flags & PAGE_USER);
            }
            let pd_phys = ((*pdpt).entries[pdpt_idx] & 0x000F_FFFF_FFFF_F000) as usize;
            let pd = phys_to_virt(pd_phys) as *mut PageTable;

            if (*pd).entries[pd_idx] & PAGE_PRESENT == 0 {
                let frame = alloc_frame().ok_or("Out of memory for PT")?;
                zero_phys_frame(frame);
                (*pd).entries[pd_idx] =
                    (frame as u64) | PAGE_PRESENT | PAGE_WRITABLE | (flags & PAGE_USER);
            }
            let pt_phys = ((*pd).entries[pd_idx] & 0x000F_FFFF_FFFF_F000) as usize;
            let pt = phys_to_virt(pt_phys) as *mut PageTable;

            (*pt).entries[pt_idx] = (phys_addr as u64) | flags | PAGE_PRESENT;
        }
        Ok(())
    }

    /// Unmaps a 4 KiB virtual page from this address space and frees its physical frame.
    pub fn unmap_page(&mut self, virt_addr: usize) {
        let pml4_idx = (virt_addr >> 39) & 0x1FF;
        let pdpt_idx = (virt_addr >> 30) & 0x1FF;
        let pd_idx = (virt_addr >> 21) & 0x1FF;
        let pt_idx = (virt_addr >> 12) & 0x1FF;

        // SAFETY: Table pointers are HHDM views of allocated frames.
        unsafe {
            let pml4 = phys_to_virt(self.pml4_phys) as *mut PageTable;
            if (*pml4).entries[pml4_idx] & PAGE_PRESENT == 0 {
                return;
            }
            let pdpt = phys_to_virt(((*pml4).entries[pml4_idx] & 0x000F_FFFF_FFFF_F000) as usize)
                as *mut PageTable;

            if (*pdpt).entries[pdpt_idx] & PAGE_PRESENT == 0 {
                return;
            }
            let pd = phys_to_virt(((*pdpt).entries[pdpt_idx] & 0x000F_FFFF_FFFF_F000) as usize)
                as *mut PageTable;

            if (*pd).entries[pd_idx] & PAGE_PRESENT == 0 {
                return;
            }
            let pt = phys_to_virt(((*pd).entries[pd_idx] & 0x000F_FFFF_FFFF_F000) as usize)
                as *mut PageTable;

            let entry = (*pt).entries[pt_idx];
            if entry & PAGE_PRESENT != 0 {
                let phys = (entry & 0x000F_FFFF_FFFF_F000) as usize;
                (*pt).entries[pt_idx] = 0;
                free_frame(phys);
                core::arch::asm!("invlpg [{}]", in(reg) virt_addr, options(nostack, preserves_flags));
            }
        }
    }

    /// Translates a virtual address to its mapped physical address.
    pub fn translate(&self, virt_addr: usize) -> Option<usize> {
        let pml4_idx = (virt_addr >> 39) & 0x1FF;
        let pdpt_idx = (virt_addr >> 30) & 0x1FF;
        let pd_idx = (virt_addr >> 21) & 0x1FF;
        let pt_idx = (virt_addr >> 12) & 0x1FF;
        let offset = virt_addr & 0xFFF;

        // SAFETY: Table pointers are HHDM views of allocated frames.
        unsafe {
            let pml4 = phys_to_virt(self.pml4_phys) as *const PageTable;
            if (*pml4).entries[pml4_idx] & PAGE_PRESENT == 0 {
                return None;
            }
            let pdpt = phys_to_virt(((*pml4).entries[pml4_idx] & 0x000F_FFFF_FFFF_F000) as usize)
                as *const PageTable;

            if (*pdpt).entries[pdpt_idx] & PAGE_PRESENT == 0 {
                return None;
            }
            let pd = phys_to_virt(((*pdpt).entries[pdpt_idx] & 0x000F_FFFF_FFFF_F000) as usize)
                as *const PageTable;

            if (*pd).entries[pd_idx] & PAGE_PRESENT == 0 {
                return None;
            }
            let pt = phys_to_virt(((*pd).entries[pd_idx] & 0x000F_FFFF_FFFF_F000) as usize)
                as *const PageTable;

            let entry = (*pt).entries[pt_idx];
            if entry & PAGE_PRESENT == 0 {
                return None;
            }
            let phys_base = (entry & 0x000F_FFFF_FFFF_F000) as usize;
            Some(phys_base + offset)
        }
    }

    /// Allocates physical frames, maps them into `[start_virt, start_virt + size)`, and registers a VMA.
    pub fn alloc_and_map_range(
        &mut self,
        start_virt: usize,
        size: usize,
        flags: u64,
    ) -> Result<(), &'static str> {
        let aligned_start = start_virt & !0xFFF;
        let aligned_end = (start_virt + size + PAGE_SIZE - 1) & !0xFFF;
        let page_count = (aligned_end - aligned_start) / PAGE_SIZE;

        for i in 0..page_count {
            let virt = aligned_start + i * PAGE_SIZE;
            let phys = alloc_frame().ok_or("Out of physical frames")?;
            zero_phys_frame(phys);
            self.map_page(virt, phys, flags)?;
        }

        let mut prot = posix_abi::PROT_READ as u32;
        if flags & PAGE_WRITABLE != 0 {
            prot |= posix_abi::PROT_WRITE as u32;
        }
        if flags & PAGE_NX == 0 {
            prot |= posix_abi::PROT_EXEC as u32;
        }
        self.insert_vma(aligned_start, aligned_end, prot, 0);

        Ok(())
    }

    /// Writes data from kernel memory into this virtual address space.
    pub fn write_bytes_to_space(
        &mut self,
        target_virt: usize,
        data: &[u8],
    ) -> Result<(), &'static str> {
        let mut written = 0;
        while written < data.len() {
            let curr_virt = target_virt + written;
            let offset_in_page = curr_virt & 0xFFF;
            let available_in_page = PAGE_SIZE - offset_in_page;
            let to_write = (data.len() - written).min(available_in_page);

            let phys = self
                .translate(curr_virt)
                .ok_or("Unmapped virtual address during write")?;
            let virt = phys_to_virt(phys) as *mut u8;

            // SAFETY: `virt` is a valid HHDM pointer to the mapped page frame.
            unsafe {
                core::ptr::copy_nonoverlapping(data[written..].as_ptr(), virt, to_write);
            }
            written += to_write;
        }
        Ok(())
    }

    /// Creates an independent clone of this address space by allocating a new PML4
    /// and eagerly duplicating all physical memory pages recorded in the VMA list.
    pub fn clone_from(&self) -> Option<Self> {
        let pml4_phys = alloc_frame()?;
        let pml4_virt = phys_to_virt(pml4_phys) as *mut PageTable;
        // SAFETY: `pml4_virt` is a newly allocated 4KiB page frame.
        unsafe {
            core::ptr::write_bytes(pml4_virt, 0, 1);
            // Copy higher-half kernel entries (256..512) from current active PML4
            let active_pml4 =
                phys_to_virt(crate::ostd::arch::read_cr3() as usize & !0xFFF) as *const PageTable;
            for i in 256..512 {
                (*pml4_virt).entries[i] = (*active_pml4).entries[i];
            }
        }

        let mut new_vm = Self {
            pml4_phys,
            vmas: self.vmas.clone(),
        };

        for vma in &self.vmas {
            let mut vaddr = vma.start & !0xFFF;
            let end_vaddr = (vma.end + PAGE_SIZE - 1) & !0xFFF;
            while vaddr < end_vaddr {
                if let Some(parent_phys) = self.translate(vaddr) {
                    let child_phys = alloc_frame()?;
                    let parent_src = phys_to_virt(parent_phys & !0xFFF) as *const u8;
                    let child_dst = phys_to_virt(child_phys) as *mut u8;
                    // SAFETY: Both frames are valid, non-overlapping allocated physical pages.
                    unsafe {
                        core::ptr::copy_nonoverlapping(parent_src, child_dst, PAGE_SIZE);
                    }

                    let mut flags = PAGE_PRESENT | PAGE_USER;
                    if vma.prot & (posix_abi::PROT_WRITE as u32) != 0 {
                        flags |= PAGE_WRITABLE;
                    }
                    if vma.prot & (posix_abi::PROT_EXEC as u32) == 0 {
                        flags |= PAGE_NX;
                    }

                    let _ = new_vm.map_page(vaddr, child_phys, flags);
                }
                vaddr += PAGE_SIZE;
            }
        }

        Some(new_vm)
    }
}

impl Drop for VmSpace {
    fn drop(&mut self) {
        // Free user frames across all VMAs
        let vmas = core::mem::take(&mut self.vmas);
        for vma in &vmas {
            let mut vaddr = vma.start & !0xFFF;
            let end_vaddr = (vma.end + PAGE_SIZE - 1) & !0xFFF;
            while vaddr < end_vaddr {
                self.unmap_page(vaddr);
                vaddr += PAGE_SIZE;
            }
        }
        // Free lower-half page table hierarchy
        // SAFETY: `pml4_phys` was allocated by alloc_frame and is valid HHDM memory.
        unsafe {
            let pml4 = phys_to_virt(self.pml4_phys) as *mut PageTable;
            for i in 0..256 {
                if (*pml4).entries[i] & PAGE_PRESENT != 0 {
                    let pdpt_phys = ((*pml4).entries[i] & 0x000F_FFFF_FFFF_F000) as usize;
                    let pdpt = phys_to_virt(pdpt_phys) as *mut PageTable;
                    for j in 0..512 {
                        if (*pdpt).entries[j] & PAGE_PRESENT != 0 {
                            let pd_phys = ((*pdpt).entries[j] & 0x000F_FFFF_FFFF_F000) as usize;
                            let pd = phys_to_virt(pd_phys) as *mut PageTable;
                            for k in 0..512 {
                                if (*pd).entries[k] & PAGE_PRESENT != 0 {
                                    let pt_phys =
                                        ((*pd).entries[k] & 0x000F_FFFF_FFFF_F000) as usize;
                                    free_frame(pt_phys);
                                }
                            }
                            free_frame(pd_phys);
                        }
                    }
                    free_frame(pdpt_phys);
                }
            }
            free_frame(self.pml4_phys);
        }
    }
}
