//! Safe user-space memory access — the ONLY module in the kernel allowed to
//! dereference user pointers (ADR-0001, rule R2).
//!
//! Services never touch raw `*const` / `*mut` user pointers. The syscall
//! dispatcher converts raw register values into [`UserPtr`] / [`UserSlice`],
//! and every byte crossing the user/kernel boundary goes through this module.
//!
//! A pointer is accepted only if:
//!   1. the whole accessed range lies in the lower canonical half
//!      (< [`USER_SPACE_END`]),
//!   2. every page it touches is PRESENT in the *current* address space, and
//!   3. every page-table level on the walk has the USER bit set (x86-64
//!      requires U/S=1 at all levels for CPL3 access). Writes additionally
//!      require WRITABLE at every level.
//!
//! Documented limitations:
//!   * Validation walks the CURRENT CR3, so these APIs may only be used from
//!     syscall context, where "current" == the calling process.
//!   * Validate-then-copy is not atomic against a concurrent `munmap` from
//!     another thread of the same process. The kernel is single-CPU with no
//!     threads today; when threading lands, hold the address-space lock here.

use core::marker::PhantomData;

use super::{phys_to_virt, PAGE_PRESENT, PAGE_USER, PAGE_WRITABLE};

/// Exclusive upper bound of the user address space (lower canonical half).
pub const USER_SPACE_END: usize = 0x0000_8000_0000_0000;

/// Maximum length of a NUL-terminated string copied from user space.
/// Matches the PATH_MAX convention used by the rest of the kernel.
pub const USER_STR_MAX: usize = 4096;

const PAGE_MASK: usize = 0xFFF;
const PT_INDEX_MASK: usize = 0x1FF;
const PT_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
/// PS bit: leaf entry for a 1 GiB (PDPT) or 2 MiB (PD) huge page.
const PAGE_PS: u64 = 1 << 7;

/// Why a user-memory access was rejected.
///
/// Services map these to errno: everything here is `EFAULT`, except
/// [`UserAccessError::TooLong`], which is `ENAMETOOLONG`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserAccessError {
    NullPointer,
    OutOfUserRange,
    Overflow,
    NotMapped,
    NotWritable,
    TooLong,
}

/// Physical base of the current PML4.
fn current_root_table() -> usize {
    let cr3: usize;
    // SAFETY: reading CR3 is always valid in ring 0 and has no side effects.
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)) };
    cr3 & (PT_ADDR_MASK as usize)
}

/// Walk the current 4-level page tables for `vaddr`. Succeeds only if every
/// level is PRESENT and USER-accessible (and WRITABLE when `need_write`).
fn validate_user_page(vaddr: usize, need_write: bool) -> Result<(), UserAccessError> {
    let mut table_phys = current_root_table();
    for level in (1u32..=4).rev() {
        let shift = 12 + (level - 1) * 9;
        let index = (vaddr >> shift) & PT_INDEX_MASK;
        let entry = {
            // SAFETY: `table_phys` is the physical address of a live
            // page-table page reachable from CR3; `phys_to_virt` maps it
            // through the HHDM, which covers all physical RAM. `index` is
            // masked to 0..512, so the read stays inside the 4 KiB table.
            // Volatile so entries are not cached across context switches.
            let table = phys_to_virt(table_phys) as *const u64;
            unsafe { table.add(index).read_volatile() }
        };
        if entry & PAGE_PRESENT == 0 || entry & PAGE_USER == 0 {
            return Err(UserAccessError::NotMapped);
        }
        if need_write && entry & PAGE_WRITABLE == 0 {
            return Err(UserAccessError::NotWritable);
        }
        // Leaf: 4 KiB PTE, or a huge-page entry (PS is only meaningful at
        // PDPT/PD level; PML4 has no PS bit).
        if level == 1 || (level <= 3 && entry & PAGE_PS != 0) {
            return Ok(());
        }
        table_phys = (entry & PT_ADDR_MASK) as usize;
    }
    unreachable!("the walk always returns at level 1")
}

/// Validate that `[addr, addr + len)` is fully mapped user memory.
fn validate_user_range(addr: usize, len: usize, need_write: bool) -> Result<(), UserAccessError> {
    if len == 0 {
        return Ok(());
    }
    let end = addr.checked_add(len).ok_or(UserAccessError::Overflow)?;
    if end > USER_SPACE_END {
        return Err(UserAccessError::OutOfUserRange);
    }
    // Checking the first address of each page suffices: a mapped page covers
    // the whole page (huge pages re-validate per 4 KiB slice — redundant
    // walks, same result).
    let mut page = addr & !PAGE_MASK;
    while page < end {
        validate_user_page(page, need_write)?;
        page = page.checked_add(PAGE_MASK + 1).ok_or(UserAccessError::Overflow)?;
    }
    Ok(())
}

/// A validated pointer to a single `T` in the current process's address
/// space. Intended for `repr(C)` POD types from `posix-abi`
/// (`Stat`, `Utsname`, `EpollEvent`, ...).
///
/// Construction checks range only; mapping is (re-)validated on every
/// `read`/`write`, mirroring Linux's EFAULT-at-access semantics.
pub struct UserPtr<T> {
    addr: usize,
    _marker: PhantomData<T>,
}

impl<T> UserPtr<T> {
    pub fn from_raw(addr: usize) -> Result<Self, UserAccessError> {
        if addr == 0 {
            return Err(UserAccessError::NullPointer);
        }
        let end = addr
            .checked_add(core::mem::size_of::<T>())
            .ok_or(UserAccessError::Overflow)?;
        if end > USER_SPACE_END {
            return Err(UserAccessError::OutOfUserRange);
        }
        Ok(Self { addr, _marker: PhantomData })
    }

    pub fn addr(&self) -> usize {
        self.addr
    }

    /// Pre-validate the target range without performing the access.
    /// Useful when a later failure would leak kernel resources (e.g. pipe
    /// fds already allocated): validate early, then `read`/`write`.
    pub fn validate(&self, need_write: bool) -> Result<(), UserAccessError> {
        validate_user_range(self.addr, core::mem::size_of::<T>(), need_write)
    }

    pub fn read(&self) -> Result<T, UserAccessError> {
        validate_user_range(self.addr, core::mem::size_of::<T>(), false)?;
        // SAFETY: range just validated as in-user-range, present, and
        // user-accessible in the current address space. Unaligned read
        // because user pointers carry no alignment guarantee. Single-CPU:
        // no concurrent unmap between validation and read (module docs).
        Ok(unsafe { (self.addr as *const T).read_unaligned() })
    }

    pub fn write(&self, value: T) -> Result<(), UserAccessError> {
        validate_user_range(self.addr, core::mem::size_of::<T>(), true)?;
        // SAFETY: as `read`, plus every page checked WRITABLE. Callers only
        // use POD types, so writing bytes violates no user-side invariant.
        unsafe { (self.addr as *mut T).write_unaligned(value) };
        Ok(())
    }
}

/// A validated byte buffer in the current process's address space.
pub struct UserSlice {
    addr: usize,
    len: usize,
}

impl UserSlice {
    pub fn from_raw(addr: usize, len: usize) -> Result<Self, UserAccessError> {
        if len > 0 && addr == 0 {
            return Err(UserAccessError::NullPointer);
        }
        Ok(Self { addr, len })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Pre-validate the whole buffer without copying.
    pub fn validate(&self, need_write: bool) -> Result<(), UserAccessError> {
        validate_user_range(self.addr, self.len, need_write)
    }

    /// copy_from_user: read `self.len` bytes from the user buffer into `dst`.
    pub fn copy_from_user(&self, dst: &mut [u8]) -> Result<usize, UserAccessError> {
        if dst.len() < self.len {
            return Err(UserAccessError::Overflow);
        }
        validate_user_range(self.addr, self.len, false)?;
        // SAFETY: user range validated (in-range, present, user-accessible);
        // `dst` is a valid kernel buffer per its slice contract. The regions
        // cannot alias: one side is below USER_SPACE_END, the other is a
        // kernel address.
        unsafe {
            core::ptr::copy_nonoverlapping(self.addr as *const u8, dst.as_mut_ptr(), self.len);
        }
        Ok(self.len)
    }

    /// copy_to_user: write `self.len` bytes from `src` into the user buffer.
    pub fn copy_to_user(&self, src: &[u8]) -> Result<usize, UserAccessError> {
        if src.len() < self.len {
            return Err(UserAccessError::Overflow);
        }
        validate_user_range(self.addr, self.len, true)?;
        // SAFETY: as `copy_from_user`, plus every page checked WRITABLE.
        unsafe {
            core::ptr::copy_nonoverlapping(src.as_ptr(), self.addr as *mut u8, self.len);
        }
        Ok(self.len)
    }
}

/// Copy a NUL-terminated string from user space into `buf` (bounded scan).
/// Returns the length excluding the NUL. `TooLong` (→ ENAMETOOLONG) if no
/// NUL appears within `buf.len()` bytes.
pub fn copy_cstr_from_user(addr: usize, buf: &mut [u8]) -> Result<usize, UserAccessError> {
    if addr == 0 {
        return Err(UserAccessError::NullPointer);
    }
    if addr >= USER_SPACE_END {
        return Err(UserAccessError::OutOfUserRange);
    }
    let mut i = 0usize;
    loop {
        if i == buf.len() {
            return Err(UserAccessError::TooLong);
        }
        let cur = addr.checked_add(i).ok_or(UserAccessError::Overflow)?;
        if cur >= USER_SPACE_END {
            return Err(UserAccessError::OutOfUserRange);
        }
        // Validate on entry and at each page crossing.
        if i == 0 || cur & PAGE_MASK == 0 {
            validate_user_page(cur, false)?;
        }
        // SAFETY: `cur` is in user range and its page was just validated as
        // present + user-accessible in the current address space.
        let byte = unsafe { (cur as *const u8).read_volatile() };
        buf[i] = byte;
        if byte == 0 {
            return Ok(i);
        }
        i += 1;
    }
}
