//! x86_64 4-Level Paging Implementation (PML4, PDPT, PD, PT).
//!
//! Encapsulates low-level x86_64 MMU paging structures, page table index decomposition,
//! physical address extraction, and TLB invalidation.

use crate::ostd::mm::pmm::{alloc_frame, free_frame};
use crate::ostd::mm::user::UserAccessError;
use crate::ostd::mm::{PageFlags, phys_to_virt, zero_phys_frame};
use core::arch::asm;

/// Page present attribute bit in x86_64 PTE (bit 0).
pub const PAGE_PRESENT: u64 = 1 << 0;
/// Read/Write attribute bit in x86_64 PTE (bit 1, 1 = read/write, 0 = read-only).
pub const PAGE_WRITABLE: u64 = 1 << 1;
/// User/Supervisor attribute bit in x86_64 PTE (bit 2, 1 = user/ring 3, 0 = supervisor/ring 0).
pub const PAGE_USER: u64 = 1 << 2;
/// Page Size (PS / Huge Page) attribute bit in x86_64 PDE/PDPTE (bit 7).
pub const PAGE_PS: u64 = 1 << 7;
/// No-Execute (NX / XD) attribute bit in x86_64 PTE (bit 63).
pub const PAGE_NX: u64 = 1 << 63;

/// Bitmask extracting the 4 KiB-aligned physical address from an x86_64 PTE (bits 12..51).
pub const PT_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
/// Bitmask extracting a 9-bit page table index (bits 0..8).
pub const PT_INDEX_MASK: usize = 0x1FF;

/// 4096-byte aligned x86_64 page table level (PML4, PDPT, PD, or PT) containing 512 64-bit PTEs.
#[repr(C, align(4096))]
pub struct PageTable {
    /// 512 64-bit page table entries.
    pub entries: [u64; 512],
}

impl PageTable {
    /// Creates an empty page table initialized with all entries set to 0 (not present).
    pub const fn empty() -> Self {
        Self { entries: [0; 512] }
    }
}

/// Decomposes a 48-bit canonical virtual address into (PML4, PDPT, PD, PT) 9-bit indices.
#[inline(always)]
pub fn pt_indices(virt_addr: usize) -> (usize, usize, usize, usize) {
    (
        (virt_addr >> 39) & PT_INDEX_MASK,
        (virt_addr >> 30) & PT_INDEX_MASK,
        (virt_addr >> 21) & PT_INDEX_MASK,
        (virt_addr >> 12) & PT_INDEX_MASK,
    )
}

/// Extracts the 4 KiB-aligned physical address from an x86_64 page table entry.
#[inline(always)]
pub fn pte_phys(entry: u64) -> usize {
    (entry & PT_ADDR_MASK) as usize
}

/// Converts architecture-neutral `PageFlags` into x86_64 hardware PTE attribute bits.
#[inline(always)]
pub fn page_flags_to_arch(flags: PageFlags) -> u64 {
    let mut bits = 0u64;
    if flags.present {
        bits |= PAGE_PRESENT;
    }
    if flags.writable {
        bits |= PAGE_WRITABLE;
    }
    if flags.user {
        bits |= PAGE_USER;
    }
    if flags.no_exec {
        bits |= PAGE_NX;
    }
    bits
}

/// Converts x86_64 hardware PTE attribute bits into architecture-neutral `PageFlags`.
#[inline(always)]
pub fn page_flags_from_arch(bits: u64) -> PageFlags {
    PageFlags {
        present: (bits & PAGE_PRESENT) != 0,
        writable: (bits & PAGE_WRITABLE) != 0,
        user: (bits & PAGE_USER) != 0,
        no_exec: (bits & PAGE_NX) != 0,
    }
}

/// Invalidates the local CPU Translation Lookaside Buffer (TLB) entry for `virt_addr`.
///
/// # Safety
///
/// Modifies CPU hardware TLB caching state.
#[inline(always)]
pub unsafe fn tlb_flush(virt_addr: usize) {
    // SAFETY: Executing invlpg instruction to invalidate TLB translation cache for virtual address.
    unsafe {
        asm!("invlpg [{}]", in(reg) virt_addr, options(nostack, preserves_flags));
    }
}

/// Traverses or allocates an intermediate page table level in the HHDM.
///
/// # Safety
///
/// `table` must point to an initialized `PageTable` in valid HHDM virtual memory.
unsafe fn get_or_create_table(
    table: *mut PageTable,
    idx: usize,
    flags: u64,
) -> Result<*mut PageTable, &'static str> {
    // SAFETY: Dereferencing page table pointer and mutating entry if unmapped.
    unsafe {
        if (*table).entries[idx] & PAGE_PRESENT == 0 {
            let frame = alloc_frame().ok_or("Out of physical frames for page table")?;
            zero_phys_frame(frame);
            (*table).entries[idx] =
                (frame as u64) | PAGE_PRESENT | PAGE_WRITABLE | (flags & PAGE_USER);
        }
        let next_phys = pte_phys((*table).entries[idx]);
        Ok(phys_to_virt(next_phys) as *mut PageTable)
    }
}

/// Reads an intermediate page table level if present.
///
/// # Safety
///
/// `table` must point to an initialized `PageTable` in valid HHDM virtual memory.
unsafe fn get_table(table: *const PageTable, idx: usize) -> Option<*const PageTable> {
    // SAFETY: Reading page table entry within bounds 0..512.
    unsafe {
        if (*table).entries[idx] & PAGE_PRESENT == 0 {
            None
        } else {
            let next_phys = pte_phys((*table).entries[idx]);
            Some(phys_to_virt(next_phys) as *const PageTable)
        }
    }
}

/// Maps a 4 KiB virtual page to a physical frame in the given root page table.
///
/// # Safety
///
/// `root_phys` must be a valid 4 KiB-aligned PML4 table frame.
pub unsafe fn map_page(
    root_phys: usize,
    virt_addr: usize,
    phys_addr: usize,
    flags: PageFlags,
) -> Result<(), &'static str> {
    let (pml4_idx, pdpt_idx, pd_idx, pt_idx) = pt_indices(virt_addr);
    let arch_flags = page_flags_to_arch(flags) | PAGE_PRESENT;

    // SAFETY: Walking/allocating PML4 -> PDPT -> PD -> PT and populating leaf PTE.
    unsafe {
        let pml4 = phys_to_virt(root_phys) as *mut PageTable;
        let pdpt = get_or_create_table(pml4, pml4_idx, arch_flags)?;
        let pd = get_or_create_table(pdpt, pdpt_idx, arch_flags)?;
        let pt = get_or_create_table(pd, pd_idx, arch_flags)?;

        (*pt).entries[pt_idx] = (phys_addr as u64) | arch_flags;
    }
    Ok(())
}

/// Unmaps a 4 KiB virtual page from the given root page table and returns the unmapped physical frame if any.
///
/// # Safety
///
/// `root_phys` must be a valid 4 KiB-aligned PML4 table frame.
pub unsafe fn unmap_page(root_phys: usize, virt_addr: usize) -> Option<usize> {
    let (pml4_idx, pdpt_idx, pd_idx, pt_idx) = pt_indices(virt_addr);

    // SAFETY: Walking page table hierarchy and clearing leaf PTE.
    unsafe {
        let pml4 = phys_to_virt(root_phys) as *const PageTable;
        let pdpt = get_table(pml4, pml4_idx)?;
        let pd = get_table(pdpt, pdpt_idx)?;
        let pt = get_table(pd, pd_idx)?;
        let pt_mut = pt as *mut PageTable;

        let entry = (*pt_mut).entries[pt_idx];
        if entry & PAGE_PRESENT != 0 {
            let phys = pte_phys(entry);
            (*pt_mut).entries[pt_idx] = 0;
            tlb_flush(virt_addr);
            Some(phys)
        } else {
            None
        }
    }
}

/// Modifies protection flags for a mapped virtual page and flushes the TLB.
///
/// # Safety
///
/// `root_phys` must be a valid 4 KiB-aligned PML4 table frame.
pub unsafe fn set_page_flags(root_phys: usize, virt_addr: usize, flags: PageFlags) {
    let (pml4_idx, pdpt_idx, pd_idx, pt_idx) = pt_indices(virt_addr);
    let arch_flags = page_flags_to_arch(flags) | PAGE_PRESENT;

    // SAFETY: Walking page table hierarchy and rewriting PTE attributes.
    unsafe {
        let pml4 = phys_to_virt(root_phys) as *const PageTable;
        let Some(pdpt) = get_table(pml4, pml4_idx) else {
            return;
        };
        let Some(pd) = get_table(pdpt, pdpt_idx) else {
            return;
        };
        let Some(pt) = get_table(pd, pd_idx) else {
            return;
        };
        let pt_mut = pt as *mut PageTable;

        let entry = (*pt_mut).entries[pt_idx];
        if entry & PAGE_PRESENT != 0 {
            let phys_base = pte_phys(entry) as u64;
            (*pt_mut).entries[pt_idx] = phys_base | arch_flags;
            tlb_flush(virt_addr);
        }
    }
}

/// Translates a virtual address to its mapped physical address.
///
/// # Safety
///
/// `root_phys` must be a valid 4 KiB-aligned PML4 table frame.
pub unsafe fn translate(root_phys: usize, virt_addr: usize) -> Option<usize> {
    let (pml4_idx, pdpt_idx, pd_idx, pt_idx) = pt_indices(virt_addr);
    let offset = virt_addr & 0xFFF;

    // SAFETY: Walking page table hierarchy through HHDM.
    unsafe {
        let pml4 = phys_to_virt(root_phys) as *const PageTable;
        let pdpt = get_table(pml4, pml4_idx)?;
        let pd = get_table(pdpt, pdpt_idx)?;
        let pt = get_table(pd, pd_idx)?;

        let entry = (*pt).entries[pt_idx];
        if entry & PAGE_PRESENT == 0 {
            return None;
        }
        Some(pte_phys(entry) + offset)
    }
}

/// Walks the page tables starting from `root_phys` and validates user-space access for `vaddr`.
///
/// Succeeds only if every level on the walk is PRESENT and USER-accessible (and WRITABLE if `need_write`).
///
/// # Safety
///
/// `root_phys` must be a valid 4 KiB-aligned PML4 table frame.
pub unsafe fn validate_user_page(
    root_phys: usize,
    vaddr: usize,
    need_write: bool,
) -> Result<(), UserAccessError> {
    let mut table_phys = root_phys;
    for level in (1u32..=4).rev() {
        let shift = 12 + (level - 1) * 9;
        let index = (vaddr >> shift) & PT_INDEX_MASK;
        let entry = {
            // SAFETY: Reading live page-table page entry in HHDM.
            let table = phys_to_virt(table_phys) as *const u64;
            unsafe { table.add(index).read_volatile() }
        };
        if entry & PAGE_PRESENT == 0 || entry & PAGE_USER == 0 {
            return Err(UserAccessError::NotMapped);
        }
        if need_write && entry & PAGE_WRITABLE == 0 {
            return Err(UserAccessError::NotWritable);
        }
        // Leaf: 4 KiB PTE, or a huge-page entry (PS is valid at PDPT/PD level)
        if level == 1 || (level <= 3 && entry & PAGE_PS != 0) {
            return Ok(());
        }
        table_phys = (entry & PT_ADDR_MASK) as usize;
    }
    unreachable!("the walk always returns at level 1")
}

/// Copies higher-half kernel PML4 entries (256..512) into a newly allocated user PML4.
///
/// # Safety
///
/// Both `dest_root_phys` and `src_root_phys` must be valid 4 KiB-aligned PML4 table frames.
pub unsafe fn copy_kernel_mappings(dest_root_phys: usize, src_root_phys: usize) {
    let dest_virt = phys_to_virt(dest_root_phys) as *mut PageTable;
    let src_virt = phys_to_virt(src_root_phys) as *const PageTable;

    // SAFETY: Copying entries 256..512 for higher-half kernel space isolation.
    unsafe {
        for i in 256..512 {
            (*dest_virt).entries[i] = (*src_virt).entries[i];
        }
    }
}

/// Recursively traverses and frees all lower-half page table intermediate frames and data pages.
///
/// # Safety
///
/// `root_phys` must not be actively loaded in the MMU / CR3 register.
pub unsafe fn free_page_table_hierarchy(root_phys: usize) {
    let pml4 = phys_to_virt(root_phys) as *mut PageTable;

    // SAFETY: Freeing lower-half page table frames (entries 0..256).
    unsafe {
        for i in 0..256 {
            if (*pml4).entries[i] & PAGE_PRESENT != 0 {
                let pdpt_phys = pte_phys((*pml4).entries[i]);
                let pdpt = phys_to_virt(pdpt_phys) as *mut PageTable;
                for j in 0..512 {
                    if (*pdpt).entries[j] & PAGE_PRESENT != 0 {
                        let pd_phys = pte_phys((*pdpt).entries[j]);
                        let pd = phys_to_virt(pd_phys) as *mut PageTable;
                        for k in 0..512 {
                            if (*pd).entries[k] & PAGE_PRESENT != 0 {
                                let pt_phys = pte_phys((*pd).entries[k]);
                                let pt = phys_to_virt(pt_phys) as *mut PageTable;
                                for l in 0..512 {
                                    if (*pt).entries[l] & PAGE_PRESENT != 0 {
                                        let leaf_phys = pte_phys((*pt).entries[l]);
                                        free_frame(leaf_phys);
                                    }
                                }
                                free_frame(pt_phys);
                            }
                        }
                        free_frame(pd_phys);
                    }
                }
                free_frame(pdpt_phys);
            }
        }
        free_frame(root_phys);
    }
}
