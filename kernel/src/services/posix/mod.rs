//! POSIX.1-2024 System Call Dispatcher - De-privileged Safe Service.

pub mod fs;
pub mod process;
pub mod mem;
pub mod system;
pub mod epoll;
pub mod audit;

pub use fs::*;
pub use process::*;
pub use mem::*;
pub use system::*;
pub use epoll::*;
pub use audit::*;

use posix_abi::*;
use crate::ostd::arch::syscall::SyscallRegisters;
use crate::services::process::get_current_process;

#[no_mangle]
pub extern "C" fn rust_syscall_dispatcher(regs: *mut SyscallRegisters) -> usize {
    let r = unsafe { &mut *regs };
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
        SYS_MUNMAP => sys_munmap(a1, a2),
        SYS_PIPE => sys_pipe(a1 as *mut [i32; 2]),
        SYS_DUP => sys_dup(a1 as i32),
        SYS_DUP2 => sys_dup2(a1 as i32, a2 as i32),
        SYS_FORK => sys_fork(),
        SYS_EXECVE => {
            let res = sys_execve(a1 as *const u8);
            if res == 0 {
                if let Some(p) = get_current_process() {
                    let proc = p.lock();
                    r.rcx = proc.entry_point;
                    r.rsp = proc.user_stack_top;
                    if let Some(ref vm) = proc.vm_space {
                        unsafe { core::arch::asm!("mov cr3, {}", in(reg) vm.pml4_phys) };
                    }
                }
            }
            res
        }
        SYS_WAIT4 => sys_wait4(a1 as i32, a2 as *mut i32, a3 as i32),
        SYS_GETPID => sys_getpid(),
        SYS_GETPPID => sys_getppid(),
        SYS_EXIT => sys_exit(a1 as i32),
        SYS_KILL => sys_kill(a1 as i32, a2 as i32),
        SYS_UNAME => sys_uname(a1 as *mut Utsname),
        SYS_SYSINFO => sys_sysinfo(a1 as *mut Sysinfo),
        SYS_GETCWD => sys_getcwd(a1 as *mut u8, a2),
        SYS_CHDIR => sys_chdir(a1 as *const u8),
        SYS_RENAME => sys_rename(a1 as *const u8, a2 as *const u8),
        SYS_MKDIR => sys_mkdir(a1 as *const u8, a2 as u32),
        SYS_RMDIR => sys_unlink(a1 as *const u8),
        SYS_UNLINK => sys_unlink(a1 as *const u8),
        SYS_GETDENTS64 => sys_getdents64(a1 as i32, a2 as *mut u8, a3),
        SYS_EPOLL_CREATE | SYS_EPOLL_CREATE1 => sys_epoll_create1(a1 as i32),
        SYS_EPOLL_CTL => sys_epoll_ctl(a1 as i32, a2 as i32, a3 as i32, a4 as *const EpollEvent),
        SYS_EPOLL_WAIT => sys_epoll_wait(a1 as i32, a2 as *mut EpollEvent, a3 as i32, a4 as i32),
        SYS_AUDIT_LOG => sys_audit_log(a1 as u32, a2 as *const u8, a3 as *const u8),
        SYS_AUDIT_SNAPSHOT => sys_audit_snapshot(a1 as *const u8, a2 as u32),
        _ => -(ENOSYS as isize),
    };

    r.rax = ret as usize;
    ret as usize
}
