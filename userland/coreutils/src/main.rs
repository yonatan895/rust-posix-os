//! Multicall binary providing standard POSIX core utilities.
//!
//! Supports applets: `ls`, `cat`, `echo`, `uname`, `pwd`, `touch`, `mkdir`, `rm`, `cp`, `mv`, and `help`.

#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
// Userland crate uses C-style FFI patterns (nul-terminated byte-string literals,
// raw pointer arithmetic) that conflict with clippy's Rust-idiomatic expectations.
#![allow(clippy::all)]

use core::panic::PanicInfo;
use libc::*;
use posix_abi::*;

/// Raw entry point for the multicall coreutils executable.
///
/// Parses `argc` and `argv` from the user stack and delegates to [`coreutils_main`].
///
/// # Safety
///
/// Must be invoked as the initial ELF entry point with a valid stack containing `argc` and `argv`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    let sp: *const usize;
    // SAFETY: Reads initial user stack pointer (RSP) setup by kernel ELF loader to access argc and argv array,
    // then runs main dispatcher and terminates process via exit syscall.
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) sp, options(nomem, nostack));
        let raw_argc = *sp;
        let argc = if raw_argc > 256 { 256 } else { raw_argc };
        let argv = sp.add(1) as *const *const u8;

        let code = coreutils_main(argc, argv);
        exit(code);
    }
}

/// Extracts the basename component from a null-terminated POSIX file path.
///
/// # Safety
///
/// `path` must be a valid pointer to a null-terminated byte string or null.
pub unsafe fn get_basename(path: *const u8) -> *const u8 {
    if path.is_null() {
        return b"\0".as_ptr();
    }
    let mut last = path;
    let mut ptr = path;
    // SAFETY: Caller guarantees `path` is a valid null-terminated C string.
    unsafe {
        while *ptr != 0 {
            if *ptr == b'/' && *ptr.add(1) != 0 {
                last = ptr.add(1);
            }
            ptr = ptr.add(1);
        }
    }
    last
}

/// Prints usage instructions and the list of available coreutils applets to standard output.
///
/// # Safety
///
/// Requires standard output file descriptor (stdout) to be valid and writable.
pub unsafe fn print_usage() {
    // SAFETY: Standard output is open and valid for writing the static usage string.
    unsafe {
        puts(b"POSIX Coreutils v1.0.0 (x86_64 Rust)\nUsage: coreutils <applet> [arguments...]\n\nAvailable Applets:\n  ls, cat, echo, uname, pwd, touch, mkdir, rm, cp, mv, help\0".as_ptr());
    }
}

/// Main dispatcher for the multicall coreutils binary.
///
/// Determines whether the program was invoked as `coreutils <applet>` or via an applet symlink/alias,
/// and delegates execution to the corresponding command handler.
///
/// # Safety
///
/// `argv` must be an array of at least `argc` valid null-terminated C-string pointers, or null if `argc == 0`.
pub unsafe fn coreutils_main(argc: usize, argv: *const *const u8) -> i32 {
    // SAFETY: Checking whether first element of argv is null.
    let is_null_first = unsafe { (*argv).is_null() };
    if argc == 0 || argv.is_null() || is_null_first {
        // SAFETY: Standard output is valid for writing usage info.
        unsafe { print_usage() };
        return 1;
    }

    // SAFETY: Caller guarantees *argv is a valid null-terminated C string.
    let prog_name = unsafe { get_basename(*argv) };
    // SAFETY: Comparing prog_name with "coreutils".
    let is_coreutils = unsafe { strcmp(prog_name, b"coreutils\0".as_ptr()) } == 0;
    let (applet, sub_argv, sub_argc) = if is_coreutils {
        let is_null_second = if argc > 1 {
            // SAFETY: Checking if argv[1] is null.
            unsafe { (*argv.add(1)).is_null() }
        } else {
            true
        };
        if argc <= 1 || is_null_second {
            // SAFETY: Standard output is valid for writing usage info.
            unsafe { print_usage() };
            return 1;
        }
        // SAFETY: argv[1] is a valid C-string pointer.
        let app = unsafe { get_basename(*argv.add(1)) };
        // SAFETY: Pointer arithmetic within argv array bounds.
        let next_argv = unsafe { argv.add(1) };
        (app, next_argv, argc - 1)
    } else {
        (prog_name, argv, argc)
    };

    // SAFETY: Comparing applet name against supported utilities and dispatching execution with valid sub_argv.
    unsafe {
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
}

/// Executes the `ls` applet to list directory contents or file status.
///
/// # Safety
///
/// `argv` must point to an array of valid null-terminated C-string pointers with length at least `argc`.
pub unsafe fn cmd_ls(argc: usize, argv: *const *const u8) -> i32 {
    let path = if argc > 1 && !unsafe { (*argv.add(1)).is_null() } {
        // SAFETY: argv[1] is verified non-null and valid.
        unsafe { *argv.add(1) }
    } else {
        b".\0".as_ptr()
    };
    // SAFETY: Opens target directory with O_RDONLY | O_DIRECTORY.
    let fd = unsafe { open(path, O_RDONLY | O_DIRECTORY, 0) };
    if fd < 0 {
        let mut st = Stat::default();
        // SAFETY: Invokes stat on path.
        if unsafe { stat(path, &mut st) } == 0 {
            // SAFETY: Prints file info to stdout.
            unsafe { printf(b"  %s\n\0".as_ptr(), path) };
            return 0;
        }
        // SAFETY: Prints error message to stdout.
        unsafe { printf(b"ls: cannot access '%s'\n\0".as_ptr(), path) };
        return 1;
    }

    let mut buf = [0u8; 1024];
    loop {
        // SAFETY: Invokes SYS_GETDENTS64 syscall to read directory entries into stack buffer.
        let n = unsafe {
            syscall::syscall3(
                SYS_GETDENTS64,
                fd as usize,
                buf.as_mut_ptr() as usize,
                buf.len(),
            ) as isize
        };
        if n <= 0 {
            break;
        }
        let mut offset = 0;
        while offset < n as usize {
            // SAFETY: buf contains valid Dirent64 structs returned by kernel up to n bytes.
            let dirent = unsafe { &*(buf.as_ptr().add(offset) as *const Dirent64) };
            let name_ptr = dirent.d_name.as_ptr();
            let suffix = match dirent.d_type {
                DT_DIR => b"/\0".as_ptr(),
                _ => b"\0".as_ptr(),
            };
            // SAFETY: Prints directory entry name and suffix to stdout.
            unsafe { printf(b"  %s%s\n\0".as_ptr(), name_ptr, suffix) };
            offset += core::mem::size_of::<Dirent64>();
        }
    }
    // SAFETY: Closes open directory file descriptor.
    unsafe { close(fd) };
    0
}

/// Executes the `cat` applet to concatenate and display file contents or standard input.
///
/// # Safety
///
/// `argv` must point to an array of valid null-terminated C-string pointers with length at least `argc`.
pub unsafe fn cmd_cat(argc: usize, argv: *const *const u8) -> i32 {
    if argc <= 1 {
        let mut buf = [0u8; 512];
        loop {
            // SAFETY: Reads from STDIN_FILENO into local buffer.
            let n = unsafe { read(STDIN_FILENO, buf.as_mut_ptr(), buf.len()) };
            if n <= 0 {
                break;
            }
            // SAFETY: Writes read bytes to STDOUT_FILENO.
            unsafe { write(STDOUT_FILENO, buf.as_ptr(), n as usize) };
        }
        return 0;
    }

    let mut ret = 0;
    for i in 1..argc {
        // SAFETY: Accesses argv[i] within argv capacity.
        let path = unsafe { *argv.add(i) };
        if path.is_null() {
            continue;
        }
        // SAFETY: Opens file path for reading.
        let fd = unsafe { open(path, O_RDONLY, 0) };
        if fd < 0 {
            // SAFETY: Prints error message to stdout.
            unsafe { printf(b"cat: '%s': No such file or directory\n\0".as_ptr(), path) };
            ret = 1;
            continue;
        }
        let mut buf = [0u8; 512];
        loop {
            // SAFETY: Reads from opened file descriptor.
            let n = unsafe { read(fd, buf.as_mut_ptr(), buf.len()) };
            if n <= 0 {
                break;
            }
            // SAFETY: Writes data to stdout.
            unsafe { write(STDOUT_FILENO, buf.as_ptr(), n as usize) };
        }
        // SAFETY: Closes opened file descriptor.
        unsafe { close(fd) };
    }
    ret
}

/// Executes the `echo` applet to print arguments separated by spaces followed by a newline.
///
/// # Safety
///
/// `argv` must point to an array of valid null-terminated C-string pointers with length at least `argc`.
pub unsafe fn cmd_echo(argc: usize, argv: *const *const u8) -> i32 {
    for i in 1..argc {
        if i > 1 {
            // SAFETY: Writes separating space to stdout.
            unsafe { putchar(b' ' as i32) };
        }
        // SAFETY: Accesses argv[i] within array bounds.
        let arg = unsafe { *argv.add(i) };
        if !arg.is_null() {
            // SAFETY: Computes length and writes argument string to stdout.
            unsafe {
                let len = strlen(arg);
                write(STDOUT_FILENO, arg, len);
            }
        }
    }
    // SAFETY: Writes trailing newline to stdout.
    unsafe { putchar(b'\n' as i32) };
    0
}

/// Executes the `uname` applet to print system and kernel identification information.
///
/// # Safety
///
/// Standard output file descriptor must be open and writable.
pub unsafe fn cmd_uname() -> i32 {
    let mut uts = Utsname::default();
    // SAFETY: Issues SYS_UNAME syscall with pointer to local Utsname struct and writes results to stdout.
    unsafe {
        syscall::syscall1(SYS_UNAME, &mut uts as *mut _ as usize);
        printf(
            b"%s %s %s %s\n\0".as_ptr(),
            uts.sysname.as_ptr(),
            uts.release.as_ptr(),
            uts.version.as_ptr(),
            uts.machine.as_ptr(),
        );
    }
    0
}

/// Executes the `pwd` applet to print the current working directory.
///
/// # Safety
///
/// Standard output file descriptor must be open and writable.
pub unsafe fn cmd_pwd() -> i32 {
    let mut buf = [0u8; 256];
    // SAFETY: Queries current working directory into stack buffer and prints it to stdout.
    unsafe {
        getcwd(buf.as_mut_ptr(), buf.len());
        puts(buf.as_ptr());
    }
    0
}

/// Executes the `touch` applet to create empty files or update file timestamps.
///
/// # Safety
///
/// `argv` must point to an array of valid null-terminated C-string pointers with length at least `argc`.
pub unsafe fn cmd_touch(argc: usize, argv: *const *const u8) -> i32 {
    if argc <= 1 {
        // SAFETY: Writes error message to stdout.
        unsafe { puts(b"touch: missing file operand\0".as_ptr()) };
        return 1;
    }
    let mut ret = 0;
    for i in 1..argc {
        // SAFETY: Accesses argv[i] within array bounds.
        let path = unsafe { *argv.add(i) };
        if path.is_null() {
            continue;
        }
        // SAFETY: Opens or creates file with O_RDWR | O_CREAT.
        let fd = unsafe { open(path, O_RDWR | O_CREAT, 0o644) };
        if fd < 0 {
            // SAFETY: Prints error message to stdout.
            unsafe { printf(b"touch: cannot touch '%s'\n\0".as_ptr(), path) };
            ret = 1;
        } else {
            // SAFETY: Closes opened file descriptor.
            unsafe { close(fd) };
        }
    }
    ret
}

/// Executes the `mkdir` applet to create directories with default permissions (0755).
///
/// # Safety
///
/// `argv` must point to an array of valid null-terminated C-string pointers with length at least `argc`.
pub unsafe fn cmd_mkdir(argc: usize, argv: *const *const u8) -> i32 {
    if argc <= 1 {
        // SAFETY: Writes error message to stdout.
        unsafe { puts(b"mkdir: missing operand\0".as_ptr()) };
        return 1;
    }
    let mut ret = 0;
    for i in 1..argc {
        // SAFETY: Accesses argv[i] within array bounds.
        let path = unsafe { *argv.add(i) };
        if path.is_null() {
            continue;
        }
        // SAFETY: Invokes mkdir syscall with permissions 0755.
        let res = unsafe { mkdir(path, 0o755) };
        if res < 0 {
            // SAFETY: Prints error message to stdout.
            unsafe { printf(b"mkdir: cannot create directory '%s'\n\0".as_ptr(), path) };
            ret = 1;
        }
    }
    ret
}

/// Executes the `rm` applet to remove filesystem entries via `unlink`.
///
/// # Safety
///
/// `argv` must point to an array of valid null-terminated C-string pointers with length at least `argc`.
pub unsafe fn cmd_rm(argc: usize, argv: *const *const u8) -> i32 {
    if argc <= 1 {
        // SAFETY: Writes error message to stdout.
        unsafe { puts(b"rm: missing operand\0".as_ptr()) };
        return 1;
    }
    let mut ret = 0;
    for i in 1..argc {
        // SAFETY: Accesses argv[i] within array bounds.
        let path = unsafe { *argv.add(i) };
        if path.is_null() {
            continue;
        }
        // SAFETY: Invokes unlink syscall to remove file.
        let res = unsafe { unlink(path) };
        if res < 0 {
            // SAFETY: Prints error message to stdout.
            unsafe { printf(b"rm: cannot remove '%s'\n\0".as_ptr(), path) };
            ret = 1;
        }
    }
    ret
}

/// Executes the `cp` applet to copy the contents of a source file to a destination file.
///
/// # Safety
///
/// `argv` must point to an array of valid null-terminated C-string pointers with length at least `argc`.
pub unsafe fn cmd_cp(argc: usize, argv: *const *const u8) -> i32 {
    if argc < 3 {
        // SAFETY: Writes error message to stdout.
        unsafe { puts(b"cp: missing destination file operand\0".as_ptr()) };
        return 1;
    }
    // SAFETY: Accesses src and dest argument pointers.
    let (src, dest) = unsafe { (*argv.add(1), *argv.add(2)) };
    // SAFETY: Opens source file for reading.
    let in_fd = unsafe { open(src, O_RDONLY, 0) };
    if in_fd < 0 {
        // SAFETY: Prints error message to stdout.
        unsafe { printf(b"cp: cannot open '%s'\n\0".as_ptr(), src) };
        return 1;
    }
    // SAFETY: Opens/creates destination file for writing.
    let out_fd = unsafe { open(dest, O_WRONLY | O_CREAT | O_TRUNC, 0o644) };
    if out_fd < 0 {
        // SAFETY: Closes in_fd and prints error message to stdout.
        unsafe {
            close(in_fd);
            printf(b"cp: cannot create '%s'\n\0".as_ptr(), dest);
        }
        return 1;
    }
    let mut buf = [0u8; 1024];
    loop {
        // SAFETY: Reads from source file descriptor.
        let n = unsafe { read(in_fd, buf.as_mut_ptr(), buf.len()) };
        if n <= 0 {
            break;
        }
        // SAFETY: Writes bytes to destination file descriptor.
        unsafe { write(out_fd, buf.as_ptr(), n as usize) };
    }
    // SAFETY: Closes both source and destination file descriptors.
    unsafe {
        close(in_fd);
        close(out_fd);
    }
    0
}

/// Executes the `mv` applet to rename or move a file to a new destination.
///
/// # Safety
///
/// `argv` must point to an array of valid null-terminated C-string pointers with length at least `argc`.
pub unsafe fn cmd_mv(argc: usize, argv: *const *const u8) -> i32 {
    if argc < 3 {
        // SAFETY: Writes error message to stdout.
        unsafe { puts(b"mv: missing destination file operand\0".as_ptr()) };
        return 1;
    }
    // SAFETY: Accesses src and dest argument pointers.
    let (src, dest) = unsafe { (*argv.add(1), *argv.add(2)) };
    // SAFETY: Attempts fast rename syscall.
    let res = unsafe { rename(src, dest) };
    if res < 0 {
        // SAFETY: Falls back to cmd_cp and unlink on rename failure.
        let cp_res = unsafe { cmd_cp(argc, argv) };
        if cp_res == 0 {
            // SAFETY: Unlinks source file upon successful copy.
            unsafe { unlink(src) };
            0
        } else {
            // SAFETY: Prints error message to stdout.
            unsafe { printf(b"mv: cannot move '%s' to '%s'\n\0".as_ptr(), src, dest) };
            1
        }
    } else {
        0
    }
}

/// Panic handler for the coreutils binary.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    write_panic_info(STDERR_FILENO, "coreutils panic", info);
    // SAFETY: Exiting coreutils process on panic.
    unsafe { exit(1) };
}
