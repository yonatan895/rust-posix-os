//! Safe wrappers for boot-module payloads and the syscall register frame.
//!
//! Lives in the TCB so `services/` never dereferences Limine pointers or
//! the asm trampoline's register save area (ADR-0001 R3 / dispatcher).

use crate::ostd::limine::{LimineFile, LimineModuleResponse};
use crate::ostd::task::SyscallRegisters;

/// A bootloader-supplied file, already validated as a kernel slice.
pub struct BootBlob {
    /// Static byte slice of the payload memory mapped in higher-half direct map.
    pub bytes: &'static [u8],
}

/// Walk the bootloader-supplied modules and return each as a safe payload slice.
pub fn boot_modules() -> alloc::vec::Vec<BootBlob> {
    boot_module_blobs(crate::ostd::limine::module_response())
}

/// Walk the Limine module list and return each non-empty payload.
/// Null response / null pointers yield an empty vec, never a panic.
pub(crate) fn boot_module_blobs(resp: *mut LimineModuleResponse) -> alloc::vec::Vec<BootBlob> {
    let mut out = alloc::vec::Vec::new();
    if resp.is_null() {
        return out;
    }
    // SAFETY: `resp` is the Limine module response pointer passed from
    // `_start` after the bootloader filled the request. We only read the
    // count and the file list; we never write. Each `LimineFile` is copied
    // by value, then `address`/`size` become a `&'static [u8]` because
    // initrd memory is reserved for the life of the kernel.
    unsafe {
        let count = (*resp).module_count as usize;
        let modules = (*resp).modules;
        if modules.is_null() {
            return out;
        }
        for i in 0..count {
            let ptr: *mut LimineFile = *modules.add(i);
            if ptr.is_null() {
                continue;
            }
            let file = *ptr;
            if file.address.is_null() || file.size == 0 {
                continue;
            }
            out.push(BootBlob {
                bytes: core::slice::from_raw_parts(file.address, file.size as usize),
            });
        }
    }
    out
}

/// Borrow the trampoline's register save area for the duration of `f`.
///
/// # Safety
///
/// `regs` must either be null or point to a valid, live `SyscallRegisters` structure.
pub unsafe fn with_syscall_regs<F>(regs: *mut SyscallRegisters, f: F) -> usize
where
    F: FnOnce(&mut SyscallRegisters) -> usize,
{
    if regs.is_null() {
        return usize::MAX;
    }
    // SAFETY: Caller guarantees `regs` points to a valid, properly aligned, live `SyscallRegisters`
    // structure. In the single-CPU framekernel model, the register save area is uniquely borrowed for the duration
    // of `f` and not concurrently accessed.
    let r = unsafe { &mut *regs };
    f(r)
}
