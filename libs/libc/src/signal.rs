//! POSIX Signals (signal.h).

use crate::syscall::*;
use crate::unistd::getpid;
use posix_abi::*;

/// Examines and changes a signal action.
///
/// Sets the disposition of signal `signum` to `act` if non-null, and stores the previous
/// disposition into `oldact` if non-null.
///
/// # Safety
///
/// `act` and `oldact` must either be null or point to valid, aligned [`SigAction`] memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigaction(
    signum: i32,
    act: *const SigAction,
    oldact: *mut SigAction,
) -> i32 {
    unsafe {
        syscall4(
            SYS_RT_SIGACTION,
            signum as usize,
            act as usize,
            oldact as usize,
            core::mem::size_of::<SigSet>(),
        ) as i32
    }
}

/// Examines and changes the blocked signal mask.
///
/// `how` determines behavior: [`SIG_BLOCK`], [`SIG_UNBLOCK`], or [`SIG_SETMASK`].
///
/// # Safety
///
/// `set` and `oldset` must either be null or point to valid, aligned [`SigSet`] memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigprocmask(how: i32, set: *const SigSet, oldset: *mut SigSet) -> i32 {
    unsafe {
        syscall4(
            SYS_RT_SIGPROCMASK,
            how as usize,
            set as usize,
            oldset as usize,
            core::mem::size_of::<SigSet>(),
        ) as i32
    }
}

/// Sends a signal to a process or process group.
///
/// # Safety
///
/// Invokes the direct kernel `SYS_KILL` system call. The caller must ensure process permissions
/// and valid signal arguments.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kill(pid: i32, sig: i32) -> i32 {
    unsafe { syscall2(SYS_KILL, pid as usize, sig as usize) as i32 }
}

/// Sends a signal to the calling process.
///
/// Equivalent to `kill(getpid(), sig)`.
///
/// # Safety
///
/// Invokes kernel signal dispatch on the current process.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raise(sig: i32) -> i32 {
    unsafe { kill(getpid(), sig) }
}

/// Signal return trampoline calling `SYS_RT_SIGRETURN`.
///
/// Restores the process state from the stack frame after handling a signal.
///
/// # Safety
///
/// Must only be executed when the top of the user stack contains a valid [`SignalFrame`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __restore_rt() {
    unsafe {
        syscall0(SYS_RT_SIGRETURN);
    }
}
