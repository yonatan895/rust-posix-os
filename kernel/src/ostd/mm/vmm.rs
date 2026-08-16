//! Virtual Memory Manager (VMM) and Architecture-Neutral Address Space Model.
//!
//! Provides the portable [`VmSpace`] and [`Vma`] abstraction layer. Low-level MMU
//! page table manipulation is delegated to architecture-specific backends (`ostd::arch`).

use super::address_space::AddressSpace;
use super::cow::cow_dec_ref;
use super::flags::PageFlags;
use super::pmm::{PAGE_SIZE, alloc_frame, free_frame};
use crate::ostd::sync::SpinLock;
use alloc::vec::Vec;

/// Global spinlock-protected higher-half direct map (HHDM) virtual address offset.
pub static HHDM_OFFSET: SpinLock<usize> = SpinLock::new(0);

/// Converts a physical RAM address into its higher-half direct map (HHDM) virtual address.
#[inline(always)]
pub fn phys_to_virt(phys: usize) -> usize {
    phys + *HHDM_OFFSET.lock()
}

/// Converts a higher-half direct map (HHDM) virtual address into its physical RAM address.
#[inline(always)]
pub fn virt_to_phys(virt: usize) -> usize {
    virt.saturating_sub(*HHDM_OFFSET.lock())
}

/// Zeroes one 4 KiB physical frame via the HHDM.
pub fn zero_phys_frame(phys: usize) {
    let virt = phys_to_virt(phys) as *mut u8;
    // SAFETY: `virt` is a valid HHDM virtual address pointing to an allocated 4 KiB physical page frame of size `PAGE_SIZE`.
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

/// Represents a contiguous range of user virtual memory with uniform protection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vma {
    /// Starting virtual address of the region (page-aligned).
    pub start: usize,
    /// Ending virtual address of the region (exclusive upper bound, page-aligned).
    pub end: usize,
    /// POSIX memory protection flags (`PROT_READ`, `PROT_WRITE`, `PROT_EXEC`).
    pub prot: u32,
    /// POSIX memory mapping flags (`MAP_ANONYMOUS`, `MAP_PRIVATE`, `MAP_SHARED`).
    pub flags: u32,
}

/// Process Virtual Memory Address Space.
///
/// Encapsulates the hardware MMU root table handle ([`AddressSpace`]) and the list of
/// active user-space virtual memory areas ([`Vma`]).
pub struct VmSpace {
    /// Hardware root page table handle for the CPU MMU.
    pub address_space: AddressSpace,
    /// Disjoint, sorted list of user virtual memory areas.
    pub vmas: Vec<Vma>,
}

impl VmSpace {
    /// Backward-compatibility accessor for the root table physical address.
    #[inline(always)]
    pub fn pml4_phys(&self) -> usize {
        self.address_space.as_phys()
    }

    /// Allocates and initializes a new user virtual address space with higher-half kernel isolation.
    pub fn new() -> Option<Self> {
        let root_phys = alloc_frame()?;
        zero_phys_frame(root_phys);

        let active_root = AddressSpace::current().as_phys();
        #[cfg(target_arch = "x86_64")]
        // SAFETY: `root_phys` is a valid, freshly allocated 4 KiB root table physical frame. `active_root` is
        // the valid active PML4 root table. `copy_kernel_mappings` clones higher-half kernel entries
        // into the new root table without exposing or modifying lower-half user entries.
        unsafe {
            crate::ostd::arch::x86_64::paging::copy_kernel_mappings(root_phys, active_root);
        }
        #[cfg(not(target_arch = "x86_64"))]
        unimplemented!("VmSpace::new() kernel mapping copy not implemented for this architecture");

        Some(Self {
            address_space: AddressSpace(root_phys),
            vmas: Vec::new(),
        })
    }

    /// Activates this address space in the CPU memory management unit.
    #[inline(always)]
    pub fn activate(&self) {
        self.address_space.activate();
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
    /// Preserves existing VMA flags (e.g. MAP_ANONYMOUS, MAP_SHARED) across the range.
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

        let flags = PageFlags::from_prot(new_prot);

        while page_vaddr < aligned_end {
            self.set_page_flags(page_vaddr, flags);
            page_vaddr += PAGE_SIZE;
        }

        // Collect existing VMA flags for the segments in [start, end) to preserve them
        let mut segments: Vec<(usize, usize, u32)> = Vec::new();
        for vma in &self.vmas {
            let seg_start = vma.start.max(start);
            let seg_end = vma.end.min(end);
            if seg_start < seg_end {
                segments.push((seg_start, seg_end, vma.flags));
            }
        }

        for (seg_start, seg_end, vma_flags) in segments {
            self.insert_vma(seg_start, seg_end, new_prot, vma_flags);
        }

        Ok(())
    }

    /// Updates page table flags for a mapped virtual page.
    pub fn set_page_flags(&mut self, virt_addr: usize, flags: PageFlags) {
        #[cfg(target_arch = "x86_64")]
        // SAFETY: `self.address_space.as_phys()` is a valid root table. `virt_addr` is a canonical
        // virtual address whose page table entries are updated with architecture-appropriate attributes.
        unsafe {
            crate::ostd::arch::x86_64::paging::set_page_flags(
                self.address_space.as_phys(),
                virt_addr,
                flags,
            );
        }
        #[cfg(not(target_arch = "x86_64"))]
        unimplemented!("VmSpace::set_page_flags() not implemented for this architecture");
    }

    /// Maps a 4 KiB virtual page to a physical frame in this address space.
    pub fn map_page(
        &mut self,
        virt_addr: usize,
        phys_addr: usize,
        flags: PageFlags,
    ) -> Result<(), &'static str> {
        #[cfg(target_arch = "x86_64")]
        // SAFETY: `self.address_space.as_phys()` is a valid root table. `virt_addr` and `phys_addr`
        // are 4 KiB aligned addresses. Intermediate page tables are safely allocated from PMM as required.
        unsafe {
            crate::ostd::arch::x86_64::paging::map_page(
                self.address_space.as_phys(),
                virt_addr,
                phys_addr,
                flags,
            )
        }
        #[cfg(not(target_arch = "x86_64"))]
        unimplemented!("VmSpace::map_page() not implemented for this architecture")
    }

    /// Unmaps a 4 KiB virtual page from this address space.
    ///
    /// The underlying physical frame is released via [`cow_dec_ref`], which only
    /// calls [`free_frame`] when no other address space still holds a reference
    /// to it (important for CoW-shared pages after `clone_cow`).
    pub fn unmap_page(&mut self, virt_addr: usize) {
        #[cfg(target_arch = "x86_64")]
        // SAFETY: `self.address_space.as_phys()` is a valid root table. `virt_addr` is page-aligned
        // and cleared from the page tables. The returned physical address is decremented via cow_dec_ref.
        let freed_frame = unsafe {
            crate::ostd::arch::x86_64::paging::unmap_page(self.address_space.as_phys(), virt_addr)
        };
        #[cfg(not(target_arch = "x86_64"))]
        let freed_frame: Option<usize> =
            unimplemented!("VmSpace::unmap_page() not implemented for this architecture");

        if let Some(phys) = freed_frame {
            cow_dec_ref(phys);
        }
    }

    /// Translates a virtual address to its mapped physical address.
    pub fn translate(&self, virt_addr: usize) -> Option<usize> {
        #[cfg(target_arch = "x86_64")]
        // SAFETY: `self.address_space.as_phys()` is a valid root table. Page table traversal performs
        // read-only lookups via HHDM virtual addresses.
        unsafe {
            crate::ostd::arch::x86_64::paging::translate(self.address_space.as_phys(), virt_addr)
        }
        #[cfg(not(target_arch = "x86_64"))]
        unimplemented!("VmSpace::translate() not implemented for this architecture")
    }

    /// Allocates physical frames, maps them into `[start_virt, start_virt + size)`, and registers a VMA.
    pub fn alloc_and_map_range(
        &mut self,
        start_virt: usize,
        size: usize,
        flags: PageFlags,
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
        if flags.writable {
            prot |= posix_abi::PROT_WRITE as u32;
        }
        if !flags.no_exec {
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

            // SAFETY: `virt` is a valid HHDM virtual address pointing to the mapped physical frame.
            // `to_write` is bounded by `PAGE_SIZE - offset_in_page`, preventing buffer overflow past the frame boundary.
            // Kernel source data and destination physical page do not overlap.
            unsafe {
                core::ptr::copy_nonoverlapping(data[written..].as_ptr(), virt, to_write);
            }
            written += to_write;
        }
        Ok(())
    }

    /// Creates a Copy-on-Write clone of this address space.
    ///
    /// All currently mapped writable pages are marked read-only in **both** the parent
    /// and the new child, and their [`cow_ref_count`] is incremented. A write fault in
    /// either space will be caught by `rust_page_fault_handler`, which allocates a fresh
    /// private frame, copies the content, and restores write permission for that process.
    ///
    /// Read-only and execute-only pages (code, rodata) are shared without a refcount bump
    /// because they can never trigger a CoW break.
    pub fn clone_cow(&self) -> Option<Self> {
        use super::cow::cow_inc_ref;

        let mut child = Self::new()?;
        child.vmas = self.vmas.clone();

        for vma in &self.vmas {
            let mut vaddr = vma.start & !0xFFF;
            let end_vaddr = (vma.end + PAGE_SIZE - 1) & !0xFFF;
            while vaddr < end_vaddr {
                if let Some(phys) = self.translate(vaddr) {
                    let phys_aligned = phys & !0xFFF;

                    // Derive flags: writable pages become read-only in both spaces
                    // to arm the CoW write-fault mechanism.
                    let base_flags = PageFlags::from_prot(vma.prot);
                    let shared_flags = PageFlags {
                        writable: false,
                        ..base_flags
                    };

                    let is_writable = (vma.prot & posix_abi::PROT_WRITE as u32) != 0;

                    if is_writable {
                        // Demote parent PTE to read-only (CoW protection).
                        #[cfg(target_arch = "x86_64")]
                        // SAFETY: `self.address_space.as_phys()` is the parent's valid root table.
                        // `vaddr` is a canonical user virtual address within a mapped VMA.
                        // `set_page_flags` only updates the permission bits; the mapping is preserved.
                        unsafe {
                            crate::ostd::arch::x86_64::paging::set_page_flags(
                                self.address_space.as_phys(),
                                vaddr,
                                shared_flags,
                            );
                        }
                        cow_inc_ref(phys_aligned);
                        let _ = child.map_page(vaddr, phys_aligned, shared_flags);
                    } else {
                        // Read-only pages can be shared without CoW plumbing.
                        let _ = child.map_page(vaddr, phys_aligned, base_flags);
                    }
                }
                vaddr += PAGE_SIZE;
            }
        }

        Some(child)
    }

    /// Creates an independent eager clone of this address space (legacy path, pre-CoW).
    ///
    /// Prefer [`clone_cow`] for `fork()`; this method is retained for tests and
    /// any path that genuinely needs an immediately diverged copy.
    pub fn clone_from(&self) -> Option<Self> {
        let mut new_vm = Self::new()?;
        new_vm.vmas = self.vmas.clone();

        for vma in &self.vmas {
            let mut vaddr = vma.start & !0xFFF;
            let end_vaddr = (vma.end + PAGE_SIZE - 1) & !0xFFF;
            while vaddr < end_vaddr {
                if let Some(parent_phys) = self.translate(vaddr) {
                    let child_phys = alloc_frame()?;
                    let parent_src = phys_to_virt(parent_phys & !0xFFF) as *const u8;
                    let child_dst = phys_to_virt(child_phys) as *mut u8;
                    // SAFETY: `parent_src` and `child_dst` are valid HHDM pointers to distinct, non-overlapping
                    // 4 KiB physical frames. Copying `PAGE_SIZE` bytes duplicates the memory frame content.
                    unsafe {
                        core::ptr::copy_nonoverlapping(parent_src, child_dst, PAGE_SIZE);
                    }

                    let flags = PageFlags::from_prot(vma.prot);
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
        // INVARIANT: An address space must not be torn down while it is actively loaded in MMU.
        // The calling task must switch to a different address space (e.g. kernel/idle PML4) before dropping.
        debug_assert_ne!(
            AddressSpace::current(),
            self.address_space,
            "Attempted to drop VmSpace while it is still actively loaded in MMU"
        );

        #[cfg(target_arch = "x86_64")]
        // SAFETY: `self.address_space.as_phys()` is a valid root table and the debug assertion verifies
        // it is not active on the CPU. `free_page_table_hierarchy` recursively unmaps and frees all lower-half
        // user page tables and user physical frames.
        unsafe {
            crate::ostd::arch::x86_64::paging::free_page_table_hierarchy(
                self.address_space.as_phys(),
            );
        }
        #[cfg(not(target_arch = "x86_64"))]
        unimplemented!("VmSpace drop not implemented for this architecture");

        self.vmas.clear();
    }
}
