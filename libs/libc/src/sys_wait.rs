//! POSIX Process Wait Operations (sys/wait.h).

use crate::syscall::*;
use posix_abi::*;

#[no_mangle]
pub unsafe extern "C" fn wait(wstatus: *mut i32) -> i32 {
    waitpid(-1, wstatus, 0)
}

#[no_mangle]
pub unsafe extern "C" fn waitpid(pid: i32, wstatus: *mut i32, options: i32) -> i32 {
    syscall4(
        SYS_WAIT4,
        pid as usize,
        wstatus as usize,
        options as usize,
        0,
    ) as i32
}
