//! POSIX Coreutils Multi-Call Binary in Rust (x86_64 Userland).

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use libc::*;
use posix_abi::*;

#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    let sp: *const usize;
    core::arch::asm!("mov {}, rsp", out(reg) sp, options(nomem, nostack));
    let argc = if !sp.is_null() { *sp } else { 0 };
    let argv = if !sp.is_null() { sp.add(1) as *const *const u8 } else { core::ptr::null() };

    let code = coreutils_main(argc, argv);
    exit(code);
}

pub unsafe fn get_basename(path: *const u8) -> *const u8 {
    if path.is_null() {
        return b"\0".as_ptr();
    }
    let mut last = path;
    let mut ptr = path;
    while *ptr != 0 {
        if *ptr == b'/' && *ptr.add(1) != 0 {
            last = ptr.add(1);
        }
        ptr = ptr.add(1);
    }
    last
}

pub unsafe fn print_usage() {
    puts(b"POSIX Coreutils v1.0.0 (x86_64 Rust)\nUsage: coreutils <applet> [arguments...]\n\nAvailable Applets:\n  ls, cat, echo, uname, pwd, touch, mkdir, rm, cp, mv, help\0".as_ptr());
}

pub unsafe fn coreutils_main(argc: usize, argv: *const *const u8) -> i32 {
    if argc == 0 || argv.is_null() || (*argv).is_null() {
        print_usage();
        return 0;
    }

    let prog_name = get_basename(*argv);
    let (applet, sub_argv, sub_argc) = if strcmp(prog_name, b"coreutils\0".as_ptr()) == 0 {
        if argc <= 1 || (*argv.add(1)).is_null() {
            print_usage();
            return 0;
        }
        let app = get_basename(*argv.add(1));
        (app, argv.add(1), argc - 1)
    } else {
        (prog_name, argv, argc)
    };

    if strcmp(applet, b"help\0".as_ptr()) == 0 || strcmp(applet, b"--help\0".as_ptr()) == 0 {
        print_usage();
        0
    } else if strcmp(applet, b"ls\0".as_ptr()) == 0 {
        cmd_ls(sub_argc, sub_argv)
    } else if strcmp(applet, b"cat\0".as_ptr()) == 0 {
        cmd_cat(sub_argc, sub_argv)
    } else if strcmp(applet, b"echo\0".as_ptr()) == 0 {
        cmd_echo(sub_argc, sub_argv)
    } else if strcmp(applet, b"uname\0".as_ptr()) == 0 {
        cmd_uname()
    } else if strcmp(applet, b"pwd\0".as_ptr()) == 0 {
        cmd_pwd()
    } else if strcmp(applet, b"touch\0".as_ptr()) == 0 {
        cmd_touch(sub_argc, sub_argv)
    } else if strcmp(applet, b"mkdir\0".as_ptr()) == 0 {
        cmd_mkdir(sub_argc, sub_argv)
    } else if strcmp(applet, b"rm\0".as_ptr()) == 0 {
        cmd_rm(sub_argc, sub_argv)
    } else if strcmp(applet, b"cp\0".as_ptr()) == 0 {
        cmd_cp(sub_argc, sub_argv)
    } else if strcmp(applet, b"mv\0".as_ptr()) == 0 {
        cmd_mv(sub_argc, sub_argv)
    } else {
        printf(b"coreutils: '%s': applet not found\n\0".as_ptr(), applet);
        127
    }
}

pub unsafe fn cmd_ls(argc: usize, argv: *const *const u8) -> i32 {
    let path = if argc > 1 && !(*argv.add(1)).is_null() { *argv.add(1) } else { b".\0".as_ptr() };
    let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        let mut st = Stat::default();
        if stat(path, &mut st) == 0 {
            printf(b"  %s\n\0".as_ptr(), path);
            return 0;
        }
        printf(b"ls: cannot access '%s'\n\0".as_ptr(), path);
        return 1;
    }

    let mut buf = [0u8; 1024];
    loop {
        let n = syscall::syscall3(SYS_GETDENTS64, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as isize;
        if n <= 0 { break; }
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

pub unsafe fn cmd_cat(argc: usize, argv: *const *const u8) -> i32 {
    if argc <= 1 {
        let mut buf = [0u8; 512];
        loop {
            let n = read(STDIN_FILENO, buf.as_mut_ptr(), buf.len());
            if n <= 0 { break; }
            write(STDOUT_FILENO, buf.as_ptr(), n as usize);
        }
        return 0;
    }

    let mut ret = 0;
    for i in 1..argc {
        let path = *argv.add(i);
        if path.is_null() { continue; }
        let fd = open(path, O_RDONLY, 0);
        if fd < 0 {
            printf(b"cat: '%s': No such file or directory\n\0".as_ptr(), path);
            ret = 1;
            continue;
        }
        let mut buf = [0u8; 512];
        loop {
            let n = read(fd, buf.as_mut_ptr(), buf.len());
            if n <= 0 { break; }
            write(STDOUT_FILENO, buf.as_ptr(), n as usize);
        }
        close(fd);
    }
    ret
}

pub unsafe fn cmd_echo(argc: usize, argv: *const *const u8) -> i32 {
    for i in 1..argc {
        if i > 1 {
            putchar(b' ' as i32);
        }
        let arg = *argv.add(i);
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

pub unsafe fn cmd_touch(argc: usize, argv: *const *const u8) -> i32 {
    if argc <= 1 {
        puts(b"touch: missing file operand\0".as_ptr());
        return 1;
    }
    let mut ret = 0;
    for i in 1..argc {
        let path = *argv.add(i);
        if path.is_null() { continue; }
        let fd = open(path, O_RDWR | O_CREAT, 0o644);
        if fd < 0 {
            printf(b"touch: cannot touch '%s'\n\0".as_ptr(), path);
            ret = 1;
        } else {
            close(fd);
        }
    }
    ret
}

pub unsafe fn cmd_mkdir(argc: usize, argv: *const *const u8) -> i32 {
    if argc <= 1 {
        puts(b"mkdir: missing operand\0".as_ptr());
        return 1;
    }
    let mut ret = 0;
    for i in 1..argc {
        let path = *argv.add(i);
        if path.is_null() { continue; }
        let res = mkdir(path, 0o755);
        if res < 0 {
            printf(b"mkdir: cannot create directory '%s'\n\0".as_ptr(), path);
            ret = 1;
        }
    }
    ret
}

pub unsafe fn cmd_rm(argc: usize, argv: *const *const u8) -> i32 {
    if argc <= 1 {
        puts(b"rm: missing operand\0".as_ptr());
        return 1;
    }
    let mut ret = 0;
    for i in 1..argc {
        let path = *argv.add(i);
        if path.is_null() { continue; }
        let res = unlink(path);
        if res < 0 {
            printf(b"rm: cannot remove '%s'\n\0".as_ptr(), path);
            ret = 1;
        }
    }
    ret
}

pub unsafe fn cmd_cp(argc: usize, argv: *const *const u8) -> i32 {
    if argc < 3 {
        puts(b"cp: missing destination file operand\0".as_ptr());
        return 1;
    }
    let src = *argv.add(1);
    let dest = *argv.add(2);
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

pub unsafe fn cmd_mv(argc: usize, argv: *const *const u8) -> i32 {
    if argc < 3 {
        puts(b"mv: missing destination file operand\0".as_ptr());
        return 1;
    }
    let src = *argv.add(1);
    let dest = *argv.add(2);
    let res = rename(src, dest);
    if res < 0 {
        let cp_res = cmd_cp(argc, argv);
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
