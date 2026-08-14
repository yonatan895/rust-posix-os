//! Coreutils Multi-Call Binary in Rust.
//!
//! Note: Full argv/argc dispatching will be enabled after execve implements
//! standard System V user stack frame setup (argc, argv, envp, auxv).

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use libc::*;
use posix_abi::*;

#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    puts(b"POSIX Coreutils v1.0.0 (x86_64 Rust)\0".as_ptr());
    exit(0);
}

pub unsafe fn cmd_ls(path: *const u8) -> i32 {
    let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        printf(b"ls: cannot open '%s'\n\0".as_ptr(), path);
        return 1;
    }

    let mut buf = [0u8; 1024];
    let n = syscall::syscall3(SYS_GETDENTS64, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as isize;
    if n > 0 {
        let mut offset = 0;
        while offset < n as usize {
            let dirent = &*(buf.as_ptr().add(offset) as *const Dirent64);
            let name_ptr = dirent.d_name.as_ptr();
            let suffix = match dirent.d_type {
                DT_DIR => b"/\0".as_ptr(),
                _ => b"\0".as_ptr(),
            };
            printf(b"  %s%s\n\0".as_ptr(), name_ptr, suffix);
            offset += core::mem::size_of::<Dirent64>();
        }
    }

    close(fd);
    0
}

pub unsafe fn cmd_cat(path: *const u8) -> i32 {
    let fd = open(path, O_RDONLY, 0);
    if fd < 0 {
        printf(b"cat: '%s': No such file or directory\n\0".as_ptr(), path);
        return 1;
    }

    let mut buf = [0u8; 512];
    loop {
        let n = read(fd, buf.as_mut_ptr(), buf.len());
        if n <= 0 {
            break;
        }
        write(STDOUT_FILENO, buf.as_ptr(), n as usize);
    }
    close(fd);
    0
}

pub unsafe fn cmd_echo(args: *const *const u8, count: usize) -> i32 {
    for i in 0..count {
        if i > 0 {
            putchar(b' ' as i32);
        }
        let arg = *args.add(i);
        if !arg.is_null() {
            let len = strlen(arg);
            write(STDOUT_FILENO, arg, len);
        }
    }
    putchar(b'\n' as i32);
    0
}

pub unsafe fn cmd_uname() -> i32 {
    let mut uts = Utsname::default();
    syscall::syscall1(SYS_UNAME, &mut uts as *mut _ as usize);
    printf(
        b"%s %s %s %s\n\0".as_ptr(),
        uts.sysname.as_ptr(),
        uts.release.as_ptr(),
        uts.version.as_ptr(),
        uts.machine.as_ptr(),
    );
    0
}

pub unsafe fn cmd_pwd() -> i32 {
    let mut buf = [0u8; 256];
    getcwd(buf.as_mut_ptr(), buf.len());
    puts(buf.as_ptr());
    0
}

pub unsafe fn cmd_touch(path: *const u8) -> i32 {
    let fd = open(path, O_RDWR | O_CREAT, 0o644);
    if fd < 0 {
        printf(b"touch: cannot touch '%s'\n\0".as_ptr(), path);
        return 1;
    }
    close(fd);
    0
}

pub unsafe fn cmd_mkdir(path: *const u8) -> i32 {
    let res = mkdir(path, 0o755);
    if res < 0 {
        printf(b"mkdir: cannot create directory '%s'\n\0".as_ptr(), path);
        return 1;
    }
    0
}

pub unsafe fn cmd_rm(path: *const u8) -> i32 {
    let res = unlink(path);
    if res < 0 {
        printf(b"rm: cannot remove '%s'\n\0".as_ptr(), path);
        return 1;
    }
    0
}

pub unsafe fn cmd_cp(src: *const u8, dest: *const u8) -> i32 {
    let in_fd = open(src, O_RDONLY, 0);
    if in_fd < 0 {
        printf(b"cp: cannot open '%s'\n\0".as_ptr(), src);
        return 1;
    }
    let out_fd = open(dest, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if out_fd < 0 {
        close(in_fd);
        printf(b"cp: cannot create '%s'\n\0".as_ptr(), dest);
        return 1;
    }
    let mut buf = [0u8; 1024];
    loop {
        let n = read(in_fd, buf.as_mut_ptr(), buf.len());
        if n <= 0 { break; }
        write(out_fd, buf.as_ptr(), n as usize);
    }
    close(in_fd);
    close(out_fd);
    0
}

pub unsafe fn cmd_mv(src: *const u8, dest: *const u8) -> i32 {
    let res = rename(src, dest);
    if res < 0 {
        let cp_res = cmd_cp(src, dest);
        if cp_res == 0 {
            unlink(src);
            0
        } else {
            printf(b"mv: cannot move '%s' to '%s'\n\0".as_ptr(), src, dest);
            1
        }
    } else {
        0
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe { exit(1) };
}
