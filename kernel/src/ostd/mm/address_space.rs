//! Architecture-Neutral Address Space Handle.
//!
//! Encapsulates the hardware root page table (CR3 on x86_64, TTBR0_EL1 on aarch64, satp on riscv64).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressSpace(pub usize);

impl AddressSpace {
    /// Returns the raw physical base address of the root page table.
    #[inline(always)]
    pub fn as_phys(&self) -> usize {
        self.0
    }

    /// Activates this address space in the CPU memory management unit.
    #[inline(always)]
    pub fn activate(&self) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            crate::ostd::arch::x86_64::write_cr3(self.0 as u64);
        }
        #[cfg(not(target_arch = "x86_64"))]
        unimplemented!("AddressSpace::activate() not implemented for this architecture");
    }

    /// Reads the currently active address space root from the CPU registers.
    #[inline(always)]
    pub fn current() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            let cr3 = unsafe { crate::ostd::arch::x86_64::read_cr3() };
            Self((cr3 as usize) & !0xFFF)
        }
        #[cfg(not(target_arch = "x86_64"))]
        unimplemented!("AddressSpace::current() not implemented for this architecture")
    }
}
