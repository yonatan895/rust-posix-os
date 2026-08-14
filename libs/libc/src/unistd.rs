//! POSIX Standard Symbolic Constants & Types (unistd).

use crate::syscall::*;
use posix_abi::*;

#[no_mangle]
pub unsafe extern "C" fn read(fd: i32, buf: *mut u8, count: usize) -> isize {
    syscall3(SYS_READ, fd as usize, buf as usize, count) as isize
}

#[no_mangle]
pub unsafe extern "C" fn write(fd: i32, buf: *const u8, count: usize) -> isize {
    syscall3(SYS_WRITE, fd as usize, buf as usize, count) as isize
}

#[no_mangle]
pub unsafe extern "C" fn open(path: *const u8, flags: i32, mode: u32) -> i32 {
    syscall3(SYS_OPEN, path as usize, flags as usize, mode as usize) as i32
}

#[no_mangle]
pub unsafe extern "C" fn close(fd: i32) -> i32 {
    syscall1(SYS_CLOSE, fd as usize) as i32
}

#[no_mangle]
pub unsafe extern "C" fn lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    syscall3(SYS_LSEEK, fd as usize, offset as usize, whence as usize) as i64
}

#[no_mangle]
pub unsafe extern "C" fn dup(oldfd: i32) -> i32 {
    syscall1(SYS_DUP, oldfd as usize) as i32
}

#[no_mangle]
pub unsafe extern "C" fn dup2(oldfd: i32, newfd: i32) -> i32 {
    syscall2(SYS_DUP2, oldfd as usize, newfd as usize) as i32
}

#[no_mangle]
pub unsafe extern "C" fn pipe(pipefd: *mut [i32; 2]) -> i32 {
    syscall1(SYS_PIPE, pipefd as usize) as i32
}

#[no_mangle]
pub unsafe extern "C" fn fork() -> i32 {
    syscall0(SYS_FORK) as i32
}

#[no_mangle]
pub unsafe extern "C" fn execve(
    path: *const u8,
    argv: *const *const u8,
    envp: *const *const u8,
) -> i32 {
    syscall3(SYS_EXECVE, path as usize, argv as usize, envp as usize) as i32
}

#[no_mangle]
pub unsafe extern "C" fn getpid() -> i32 {
    syscall0(SYS_GETPID) as i32
}

#[no_mangle]
pub unsafe extern "C" fn getppid() -> i32 {
    syscall0(SYS_GETPPID) as i32
}

#[no_mangle]
pub unsafe extern "C" fn isatty(fd: i32) -> i32 {
    let mut term = Termios::default();
    let res = syscall3(
        SYS_IOCTL,
        fd as usize,
        0x5401, /* TCGETS */
        &mut term as *mut _ as usize,
    );
    if res == 0 { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn chdir(path: *const u8) -> i32 {
    syscall1(SYS_CHDIR, path as usize) as i32
}

#[no_mangle]
pub unsafe extern "C" fn getcwd(buf: *mut u8, size: usize) -> *mut u8 {
    let res = syscall2(SYS_GETCWD, buf as usize, size) as isize;
    if res >= 0 { buf } else { core::ptr::null_mut() }
}

#[no_mangle]
pub unsafe extern "C" fn unlink(path: *const u8) -> i32 {
    syscall1(SYS_UNLINK, path as usize) as i32
}

#[no_mangle]
pub unsafe extern "C" fn mkdir(path: *const u8, mode: u32) -> i32 {
    syscall2(SYS_MKDIR, path as usize, mode as usize) as i32
}

#[no_mangle]
pub unsafe extern "C" fn rmdir(path: *const u8) -> i32 {
    syscall1(SYS_RMDIR, path as usize) as i32
}

#[no_mangle]
pub unsafe extern "C" fn sleep(seconds: u32) -> u32 {
    let req = Timespec {
        tv_sec: seconds as i64,
        tv_nsec: 0,
    };
    let mut rem = Timespec::default();
    syscall2(
        SYS_NANOSLEEP,
        &req as *const _ as usize,
        &mut rem as *mut _ as usize,
    );
    rem.tv_sec as u32
}

#[no_mangle]
pub unsafe extern "C" fn usleep(usec: u64) -> i32 {
    let req = Timespec {
        tv_sec: (usec / 1_000_000) as i64,
        tv_nsec: ((usec % 1_000_000) * 1000) as i64,
    };
    syscall2(SYS_NANOSLEEP, &req as *const _ as usize, 0) as i32
}

#[no_mangle]
pub unsafe extern "C" fn sysinfo(info: *mut Sysinfo) -> i32 {
    syscall1(SYS_SYSINFO, info as usize) as i32
}

#[no_mangle]
pub unsafe extern "C" fn audit_log(event_type: u32, target: *const u8, details: *const u8) -> i32 {
    syscall3(
        SYS_AUDIT_LOG,
        event_type as usize,
        target as usize,
        details as usize,
    ) as i32
}

#[no_mangle]
pub unsafe extern "C" fn audit_snapshot(label: *const u8, flags: u32) -> i64 {
    syscall2(SYS_AUDIT_SNAPSHOT, label as usize, flags as usize) as i64
}
