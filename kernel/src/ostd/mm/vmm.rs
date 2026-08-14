//! Virtual Memory Manager (VMM) - 4-Level x86_64 Paging.

use super::pmm::{alloc_frame, free_frame, PAGE_SIZE};
use crate::ostd::sync::SpinLock;

pub const PAGE_PRESENT: u64  = 1 << 0;
pub const PAGE_WRITABLE: u64 = 1 << 1;
pub const PAGE_USER: u64     = 1 << 2;
pub const PAGE_NX: u64       = 1 << 63;

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

/// Zero one 4 KiB physical frame via the HHDM. Safe: `phys` must be a frame
/// this kernel allocated (or is otherwise HHDM-mapped).
pub fn zero_phys_frame(phys: usize) {
    let virt = phys_to_virt(phys) as *mut u8;
    // SAFETY: HHDM covers all RAM. Caller passes a 4 KiB frame base.
    unsafe { core::ptr::write_bytes(virt, 0, PAGE_SIZE) };
}

pub unsafe fn vmm_init(hhdm: usize) {
    *HHDM_OFFSET.lock() = hhdm;
}

pub struct VmSpace {
    pub pml4_phys: usize,
}

impl VmSpace {
    pub fn new() -> Option<Self> {
        let pml4_phys = alloc_frame()?;
        let pml4_virt = phys_to_virt(pml4_phys) as *mut PageTable;
        unsafe {
            core::ptr::write_bytes(pml4_virt, 0, 1);
            // Copy higher-half kernel mappings from current active PML4
            let active_pml4 = phys_to_virt(crate::ostd::arch::read_cr3() as usize & !0xFFF) as *const PageTable;
            for i in 256..512 {
                (*pml4_virt).entries[i] = (*active_pml4).entries[i];
            }
        }
        Some(Self { pml4_phys })
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

    pub fn map_page(
        &mut self,
        virt_addr: usize,
        phys_addr: usize,
        flags: u64,
    ) -> Result<(), &'static str> {
        let pml4_idx = (virt_addr >> 39) & 0x1FF;
        let pdpt_idx = (virt_addr >> 30) & 0x1FF;
        let pd_idx   = (virt_addr >> 21) & 0x1FF;
        let pt_idx   = (virt_addr >> 12) & 0x1FF;

        // SAFETY: all table pointers are HHDM views of frames we allocated
        // (or copied from the kernel PML4). Indices are masked to 0..512.
        unsafe {
            let pml4 = phys_to_virt(self.pml4_phys) as *mut PageTable;

            if (*pml4).entries[pml4_idx] & PAGE_PRESENT == 0 {
                let frame = alloc_frame().ok_or("Out of memory for PDPT")?;
                zero_phys_frame(frame);
                (*pml4).entries[pml4_idx] = (frame as u64) | PAGE_PRESENT | PAGE_WRITABLE | (flags & PAGE_USER);
            }
            let pdpt_phys = ((*pml4).entries[pml4_idx] & 0x000F_FFFF_FFFF_F000) as usize;
            let pdpt = phys_to_virt(pdpt_phys) as *mut PageTable;

            if (*pdpt).entries[pdpt_idx] & PAGE_PRESENT == 0 {
                let frame = alloc_frame().ok_or("Out of memory for PD")?;
                zero_phys_frame(frame);
                (*pdpt).entries[pdpt_idx] = (frame as u64) | PAGE_PRESENT | PAGE_WRITABLE | (flags & PAGE_USER);
            }
            let pd_phys = ((*pdpt).entries[pdpt_idx] & 0x000F_FFFF_FFFF_F000) as usize;
            let pd = phys_to_virt(pd_phys) as *mut PageTable;

            if (*pd).entries[pd_idx] & PAGE_PRESENT == 0 {
                let frame = alloc_frame().ok_or("Out of memory for PT")?;
                zero_phys_frame(frame);
                (*pd).entries[pd_idx] = (frame as u64) | PAGE_PRESENT | PAGE_WRITABLE | (flags & PAGE_USER);
            }
            let pt_phys = ((*pd).entries[pd_idx] & 0x000F_FFFF_FFFF_F000) as usize;
            let pt = phys_to_virt(pt_phys) as *mut PageTable;

            (*pt).entries[pt_idx] = (phys_addr as u64) | flags | PAGE_PRESENT;
        }
        Ok(())
    }

    pub fn unmap_page(&mut self, virt_addr: usize) {
        let pml4_idx = (virt_addr >> 39) & 0x1FF;
        let pdpt_idx = (virt_addr >> 30) & 0x1FF;
        let pd_idx   = (virt_addr >> 21) & 0x1FF;
        let pt_idx   = (virt_addr >> 12) & 0x1FF;

        // SAFETY: same contract as `map_page`.
        unsafe {
            let pml4 = phys_to_virt(self.pml4_phys) as *mut PageTable;
            if (*pml4).entries[pml4_idx] & PAGE_PRESENT == 0 { return; }
            let pdpt = phys_to_virt(((*pml4).entries[pml4_idx] & 0x000F_FFFF_FFFF_F000) as usize) as *mut PageTable;

            if (*pdpt).entries[pdpt_idx] & PAGE_PRESENT == 0 { return; }
            let pd = phys_to_virt(((*pdpt).entries[pdpt_idx] & 0x000F_FFFF_FFFF_F000) as usize) as *mut PageTable;

            if (*pd).entries[pd_idx] & PAGE_PRESENT == 0 { return; }
            let pt = phys_to_virt(((*pd).entries[pd_idx] & 0x000F_FFFF_FFFF_F000) as usize) as *mut PageTable;

            let entry = (*pt).entries[pt_idx];
            if entry & PAGE_PRESENT != 0 {
                let phys = (entry & 0x000F_FFFF_FFFF_F000) as usize;
                (*pt).entries[pt_idx] = 0;
                free_frame(phys);
            }
        }
    }

    pub fn translate(&self, virt_addr: usize) -> Option<usize> {
        let pml4_idx = (virt_addr >> 39) & 0x1FF;
        let pdpt_idx = (virt_addr >> 30) & 0x1FF;
        let pd_idx   = (virt_addr >> 21) & 0x1FF;
        let pt_idx   = (virt_addr >> 12) & 0x1FF;
        let offset   = virt_addr & 0xFFF;

        unsafe {
            let pml4 = phys_to_virt(self.pml4_phys) as *const PageTable;
            if (*pml4).entries[pml4_idx] & PAGE_PRESENT == 0 { return None; }
            let pdpt = phys_to_virt(((*pml4).entries[pml4_idx] & 0x000F_FFFF_FFFF_F000) as usize) as *const PageTable;

            if (*pdpt).entries[pdpt_idx] & PAGE_PRESENT == 0 { return None; }
            let pd = phys_to_virt(((*pdpt).entries[pdpt_idx] & 0x000F_FFFF_FFFF_F000) as usize) as *const PageTable;

            if (*pd).entries[pd_idx] & PAGE_PRESENT == 0 { return None; }
            let pt = phys_to_virt(((*pd).entries[pd_idx] & 0x000F_FFFF_FFFF_F000) as usize) as *const PageTable;

            let entry = (*pt).entries[pt_idx];
            if entry & PAGE_PRESENT == 0 { return None; }
            let phys_base = (entry & 0x000F_FFFF_FFFF_F000) as usize;
            Some(phys_base + offset)
        }
    }

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
        Ok(())
    }

    pub fn write_bytes_to_space(&mut self, target_virt: usize, data: &[u8]) -> Result<(), &'static str> {
        let mut written = 0;
        while written < data.len() {
            let curr_virt = target_virt + written;
            let offset_in_page = curr_virt & 0xFFF;
            let available_in_page = PAGE_SIZE - offset_in_page;
            let to_write = (data.len() - written).min(available_in_page);

            let phys = self.translate(curr_virt).ok_or("Unmapped virtual address during write")?;
            let virt = phys_to_virt(phys) as *mut u8;

            unsafe {
                core::ptr::copy_nonoverlapping(data[written..].as_ptr(), virt, to_write);
            }
            written += to_write;
        }
        Ok(())
    }
}
