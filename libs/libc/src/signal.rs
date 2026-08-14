//! POSIX Signals (signal.h).

use crate::syscall::*;
use crate::unistd::getpid;
use posix_abi::*;

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kill(pid: i32, sig: i32) -> i32 {
    unsafe { syscall2(SYS_KILL, pid as usize, sig as usize) as i32 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn raise(sig: i32) -> i32 {
    unsafe { kill(getpid(), sig) }
}

/// Signal return trampoline calling SYS_RT_SIGRETURN.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __restore_rt() {
    unsafe {
        syscall0(SYS_RT_SIGRETURN);
    }
}
