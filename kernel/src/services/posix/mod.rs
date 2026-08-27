//! POSIX syscall surface.
//!
//! Submodules own one POSIX concern each. `user_access` is the only
//! place that translates ostd user-memory errors into errno (ADR-0001 R2).
//! The dispatcher itself is safe: register-frame deref and CR3 switch live in ostd.

pub mod audit;
pub mod epoll;
pub mod fs;
pub mod mem;
pub mod process;
pub mod system;
mod user_access;

pub use audit::*;
pub use epoll::*;
pub use fs::*;
pub use mem::*;
pub use process::*;
pub use system::*;
pub(crate) use user_access::{
    copy_optional_user_str, copy_user_path, copy_user_str_array, map_user_error,
};

use crate::ostd::task::SyscallRegisters;
use crate::services::process::get_current_process;
use posix_abi::*;

/// Dispatches a POSIX system call according to register values in `r`.
///
/// Decodes the system call number in `rax`, passes arguments `rdi`, `rsi`, `rdx`, `r10`,
/// routes to the appropriate handler, processes any pending signals, and stores
/// the return value into `r.rax`.
pub fn dispatch_syscall(r: &mut SyscallRegisters) -> usize {
    let syscall_nr = r.rax;
    let a1 = r.rdi;
    let a2 = r.rsi;
    let a3 = r.rdx;
    let a4 = r.r10;

    let ret: isize = match syscall_nr {
        SYS_READ => sys_read(a1 as i32, a2 as *mut u8, a3),
        SYS_WRITE => sys_write(a1 as i32, a2 as *const u8, a3),
        SYS_OPEN => sys_open(a1 as *const u8, a2 as i32, a3 as u32),
        SYS_CLOSE => sys_close(a1 as i32),
        SYS_STAT => sys_stat(a1 as *const u8, a2 as *mut Stat),
        SYS_FSTAT => sys_fstat(a1 as i32, a2 as *mut Stat),
        SYS_LSEEK => sys_lseek(a1 as i32, a2 as i64, a3 as i32),
        SYS_MMAP => sys_mmap(a1, a2, a3 as i32, a4 as i32),
        SYS_MPROTECT => sys_mprotect(a1, a2, a3 as i32),
        SYS_MUNMAP => sys_munmap(a1, a2),
        SYS_RT_SIGACTION => {
            sys_rt_sigaction(a1 as i32, a2 as *const SigAction, a3 as *mut SigAction, a4)
        }
        SYS_RT_SIGPROCMASK => {
            sys_rt_sigprocmask(a1 as i32, a2 as *const SigSet, a3 as *mut SigSet, a4)
        }
        SYS_RT_SIGRETURN => sys_rt_sigreturn(r),
        SYS_PIPE => sys_pipe(a1 as *mut [i32; 2]),
        SYS_DUP => sys_dup(a1 as i32),
        SYS_DUP2 => sys_dup2(a1 as i32, a2 as i32),
        SYS_FORK => sys_fork(r),
        SYS_EXECVE => {
            let res = sys_execve(
                a1 as *const u8,
                a2 as *const *const u8,
                a3 as *const *const u8,
            );
            if res == 0
                && let Some(p) = get_current_process()
            {
                let proc = p.lock();
                r.rcx = proc.entry_point;
                r.rsp = proc.user_stack_top;
                if let Some(ref vm) = proc.vm_space {
                    vm.activate();
                }
            }
            res
        }
        SYS_EXIT => sys_exit(a1 as i32),
        SYS_WAIT4 => sys_wait4(a1 as i32, a2 as *mut i32, a3 as i32),
        SYS_KILL => sys_kill(a1 as i32, a2 as i32),
        SYS_GETPID => sys_getpid(),
        SYS_GETPPID => sys_getppid(),
        SYS_GETUID => sys_getuid(),
        SYS_GETGID => sys_getgid(),
        SYS_GETEUID => sys_geteuid(),
        SYS_GETEGID => sys_getegid(),
        SYS_SETUID => sys_setuid(a1 as u32),
        SYS_SETGID => sys_setgid(a1 as u32),
        SYS_SETRESUID => sys_setresuid(a1 as u32, a2 as u32, a3 as u32),
        SYS_GETRESUID => sys_getresuid(a1 as *mut u32, a2 as *mut u32, a3 as *mut u32),
        SYS_SETRESGID => sys_setresgid(a1 as u32, a2 as u32, a3 as u32),
        SYS_GETRESGID => sys_getresgid(a1 as *mut u32, a2 as *mut u32, a3 as *mut u32),
        SYS_UMASK => sys_umask(a1 as u32),
        SYS_UNAME => sys_uname(a1 as *mut Utsname),
        SYS_GETCWD => sys_getcwd(a1 as *mut u8, a2),
        SYS_CHDIR => sys_chdir(a1 as *const u8),
        SYS_MKDIR => sys_mkdir(a1 as *const u8, a2 as u32),
        SYS_UNLINK => sys_unlink(a1 as *const u8),
        SYS_RENAME => sys_rename(a1 as *const u8, a2 as *const u8),
        SYS_GETDENTS64 => sys_getdents64(a1 as i32, a2 as *mut u8, a3),
        SYS_NANOSLEEP => -(ENOSYS as isize),
        SYS_SYSINFO => sys_sysinfo(a1 as *mut Sysinfo),
        SYS_EPOLL_CREATE1 => sys_epoll_create1(a1 as i32),
        SYS_EPOLL_CTL => sys_epoll_ctl(a1 as i32, a2 as i32, a3 as i32, a4 as *const EpollEvent),
        SYS_EPOLL_WAIT => sys_epoll_wait(a1 as i32, a2 as *mut EpollEvent, a3 as i32, a4 as i32),
        SYS_AUDIT_LOG => sys_audit_log(a1 as u32, a2 as *const u8, a3 as *const u8),
        SYS_AUDIT_SNAPSHOT => sys_audit_snapshot(a1 as *const u8, a2 as u32),
        _ => -(ENOSYS as isize),
    };

    if syscall_nr != SYS_RT_SIGRETURN {
        // Set return value in rax
        r.rax = ret as usize;
        // Check and deliver any pending unblocked signals before sysretq
        check_and_deliver_signals(r);
    }

    r.rax
}
