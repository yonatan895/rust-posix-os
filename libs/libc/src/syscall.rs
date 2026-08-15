//! Direct x86_64 POSIX System Call Invocation Layer.

use core::arch::asm;

/// Issues an x86_64 system call with 0 arguments.
///
/// # Safety
///
/// Executes raw `syscall` instruction with syscall number `n`.
#[inline(always)]
pub unsafe fn syscall0(n: usize) -> usize {
    let ret: usize;
    // SAFETY: Issues a direct x86_64 `syscall` instruction with syscall number `n` in `rax`.
    // The CPU saves RIP to RCX and RFLAGS to R11, which are marked as clobbered.
    // The caller ensures that `n` is a valid syscall number.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") n => ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Issues an x86_64 system call with 1 argument in register `rdi`.
///
/// # Safety
///
/// Executes raw `syscall` instruction with syscall number `n` and argument `a1`.
#[inline(always)]
pub unsafe fn syscall1(n: usize, a1: usize) -> usize {
    let ret: usize;
    // SAFETY: Issues a direct x86_64 `syscall` instruction with syscall number `n` in `rax`
    // and argument `a1` in `rdi` per x86_64 Linux/POSIX syscall ABI.
    // RCX and R11 are clobbered by CPU architecture. Caller guarantees validity of syscall and argument.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") n => ret,
            in("rdi") a1,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Issues an x86_64 system call with 2 arguments in registers `rdi`, `rsi`.
///
/// # Safety
///
/// Executes raw `syscall` instruction with syscall number `n` and arguments `a1`, `a2`.
#[inline(always)]
pub unsafe fn syscall2(n: usize, a1: usize, a2: usize) -> usize {
    let ret: usize;
    // SAFETY: Issues a direct x86_64 `syscall` instruction with syscall number `n` in `rax`
    // and arguments (`rdi`, `rsi`) per x86_64 ABI. RCX and R11 are clobbered by hardware.
    // Caller ensures validity of syscall number and arguments.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") n => ret,
            in("rdi") a1,
            in("rsi") a2,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Issues an x86_64 system call with 3 arguments in registers `rdi`, `rsi`, `rdx`.
///
/// # Safety
///
/// Executes raw `syscall` instruction with syscall number `n` and arguments `a1`, `a2`, `a3`.
#[inline(always)]
pub unsafe fn syscall3(n: usize, a1: usize, a2: usize, a3: usize) -> usize {
    let ret: usize;
    // SAFETY: Issues a direct x86_64 `syscall` instruction with syscall number `n` in `rax`
    // and arguments (`rdi`, `rsi`, `rdx`) per x86_64 ABI. RCX and R11 are clobbered by hardware.
    // Caller ensures validity of syscall number and memory/descriptor arguments.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") n => ret,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Issues an x86_64 system call with 4 arguments in registers `rdi`, `rsi`, `rdx`, `r10`.
///
/// # Safety
///
/// Executes raw `syscall` instruction with syscall number `n` and arguments `a1`..`a4`.
#[inline(always)]
pub unsafe fn syscall4(n: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> usize {
    let ret: usize;
    // SAFETY: Issues a direct x86_64 `syscall` instruction with syscall number `n` in `rax`
    // and arguments (`rdi`, `rsi`, `rdx`, `r10`) per x86_64 ABI. Note that `r10` is used instead
    // of `rcx` as `rcx` is destroyed by `syscall`. RCX and R11 clobbers are declared.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") n => ret,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            in("r10") a4,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Issues an x86_64 system call with 5 arguments in registers `rdi`, `rsi`, `rdx`, `r10`, `r8`.
///
/// # Safety
///
/// Executes raw `syscall` instruction with syscall number `n` and arguments `a1`..`a5`.
#[inline(always)]
pub unsafe fn syscall5(n: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> usize {
    let ret: usize;
    // SAFETY: Issues a direct x86_64 `syscall` instruction with syscall number `n` in `rax`
    // and arguments (`rdi`, `rsi`, `rdx`, `r10`, `r8`) per x86_64 ABI. RCX/R11 clobbers declared.
    // Caller ensures validity of syscall number and arguments.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") n => ret,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            in("r10") a4,
            in("r8") a5,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Issues an x86_64 system call with 6 arguments in registers `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`.
///
/// # Safety
///
/// Executes raw `syscall` instruction with syscall number `n` and arguments `a1`..`a6`.
#[inline(always)]
pub unsafe fn syscall6(
    n: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
) -> usize {
    let ret: usize;
    // SAFETY: Issues a direct x86_64 `syscall` instruction with syscall number `n` in `rax`
    // and arguments (`rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`) per x86_64 ABI. RCX/R11 clobbers declared.
    // Caller ensures validity of syscall number and arguments.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") n => ret,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            in("r10") a4,
            in("r8") a5,
            in("r9") a6,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}
