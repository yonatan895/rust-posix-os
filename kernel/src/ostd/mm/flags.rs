//! Architecture-Neutral Page Flags.
//!
//! Provides a portable description of memory page protection and mapping attributes
//! across all supported CPU architectures (x86_64, aarch64, riscv64).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PageFlags {
    pub present: bool,
    pub writable: bool,
    pub user: bool,
    pub no_exec: bool,
}

impl PageFlags {
    /// Creates an empty (unmapped / not present) page flag set.
    pub const fn empty() -> Self {
        Self {
            present: false,
            writable: false,
            user: false,
            no_exec: false,
        }
    }

    /// User-mode executable code page (read-only, executable, user-accessible).
    pub const fn user_code() -> Self {
        Self {
            present: true,
            writable: false,
            user: true,
            no_exec: false,
        }
    }

    /// User-mode data/stack/heap page (read/write, non-executable, user-accessible).
    pub const fn user_data() -> Self {
        Self {
            present: true,
            writable: true,
            user: true,
            no_exec: true,
        }
    }

    /// User-mode read-only data page (read-only, non-executable, user-accessible).
    pub const fn user_rodata() -> Self {
        Self {
            present: true,
            writable: false,
            user: true,
            no_exec: true,
        }
    }

    /// Kernel-mode data page (read/write, non-executable, privileged-only).
    pub const fn kernel_data() -> Self {
        Self {
            present: true,
            writable: true,
            user: false,
            no_exec: true,
        }
    }

    /// Kernel-mode code page (read-only, executable, privileged-only).
    pub const fn kernel_code() -> Self {
        Self {
            present: true,
            writable: false,
            user: false,
            no_exec: false,
        }
    }

    /// Derives user-space page flags from POSIX `PROT_*` bits (`PROT_READ`, `PROT_WRITE`, `PROT_EXEC`).
    pub fn from_prot(prot: u32) -> Self {
        Self {
            present: true,
            writable: prot & (posix_abi::PROT_WRITE as u32) != 0,
            user: true,
            no_exec: prot & (posix_abi::PROT_EXEC as u32) == 0,
        }
    }
}
