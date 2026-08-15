//! Rust POSIX Shell - Interactive Ring 3 Terminal & Command Interpreter.

#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
// Userland crate uses C-style FFI patterns (nul-terminated byte-string literals,
// raw pointer arithmetic) that conflict with clippy's Rust-idiomatic expectations.
#![allow(clippy::all)]

mod builtins;
mod completion;
mod editor;
mod history;
mod line_draw;
mod pipeline;

use builtins::*;
use core::panic::PanicInfo;
use editor::read_line_with_history;
use libc::*;
use pipeline::execute_pipeline_line;
use posix_abi::*;

/// List of built-in shell commands recognized for syntax highlighting and tab completion.
pub static KNOWN_COMMANDS: [&str; 22] = [
    "help",
    "uname",
    "pwd",
    "id",
    "cd",
    "ls",
    "cp",
    "mv",
    "cat",
    "touch",
    "mkdir",
    "rm",
    "ps",
    "top",
    "monitor",
    "journal",
    "snapshot",
    "echo",
    "clip",
    "async-demo",
    "clear",
    "exit",
];

/// Entry point for the interactive shell process.
///
/// Prints greeting banner and executes the interactive read-eval-print loop (REPL).
///
/// # Safety
///
/// Must be invoked as the initial ELF entry point with a valid stack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    // SAFETY: Outputs welcome banner strings to stdout.
    unsafe {
        puts(b"\x1b[1;32m=====================================================\x1b[0m\0".as_ptr());
        puts(b"\x1b[1;32m   Rust POSIX Shell (POSIX.1-2024 / x86_64 Userland)  \x1b[0m\0".as_ptr());
        puts(b"\x1b[1;32m=====================================================\x1b[0m\0".as_ptr());
        puts(b"Type 'help' for built-in utilities.\n\0".as_ptr());
    }

    let mut line_buf = [0u8; 256];
    let mut cwd_buf = [0u8; 128];

    loop {
        // SAFETY: Queries working directory, reads line from terminal with history, and executes pipeline.
        unsafe {
            getcwd(cwd_buf.as_mut_ptr(), cwd_buf.len());
            let len = read_line_with_history(cwd_buf.as_ptr(), &mut line_buf, &KNOWN_COMMANDS);
            if len > 0 {
                execute_pipeline_line(line_buf.as_mut_ptr(), |argc, argv| {
                    execute_command(argc, argv);
                });
            }
        }
    }
}

/// Dispatches an individual command to its built-in handler or reports an unknown command error.
///
/// # Safety
///
/// `argv` pointers up to `argc` must be valid null-terminated C-strings or null.
pub unsafe fn execute_command(argc: usize, argv: &[*const u8; 16]) {
    let cmd = argv[0];
    // SAFETY: Matches command string against built-ins and invokes appropriate handler.
    unsafe {
        if strcmp(cmd, b"help\0".as_ptr()) == 0 {
            puts(b"Available POSIX Shell Commands:\n  help, uname, pwd, id, cd, ls, cp, mv, cat, touch, mkdir, rm,\n  ps, top, monitor, journal, snapshot, echo, clip, async-demo, clear, exit\n\nPipeline: cmd1 | cmd2    Redirect: >, >>, <\0".as_ptr());
        } else if strcmp(cmd, b"uname\0".as_ptr()) == 0 {
            let mut uts = Utsname::default();
            syscall::syscall1(SYS_UNAME, &mut uts as *mut _ as usize);
            printf(
                b"%s %s %s %s\n\0".as_ptr(),
                uts.sysname.as_ptr(),
                uts.release.as_ptr(),
                uts.version.as_ptr(),
                uts.machine.as_ptr(),
            );
        } else if strcmp(cmd, b"pwd\0".as_ptr()) == 0 {
            let mut buf = [0u8; 128];
            getcwd(buf.as_mut_ptr(), buf.len());
            puts(buf.as_ptr());
        } else if strcmp(cmd, b"id\0".as_ptr()) == 0 {
            handle_id();
        } else if strcmp(cmd, b"ps\0".as_ptr()) == 0 {
            display_file(b"/proc/processes\0".as_ptr(), false);
        } else if strcmp(cmd, b"top\0".as_ptr()) == 0 || strcmp(cmd, b"monitor\0".as_ptr()) == 0 {
            display_system_monitor();
        } else if strcmp(cmd, b"journal\0".as_ptr()) == 0 {
            display_file(b"/proc/audit_journal\0".as_ptr(), false);
        } else if strcmp(cmd, b"snapshot\0".as_ptr()) == 0 {
            handle_snapshot_command(argc, argv);
        } else if strcmp(cmd, b"cd\0".as_ptr()) == 0 {
            handle_cd(argc, argv);
        } else if strcmp(cmd, b"ls\0".as_ptr()) == 0 {
            handle_ls(argc, argv);
        } else if strcmp(cmd, b"cp\0".as_ptr()) == 0 {
            handle_cp(argc, argv);
        } else if strcmp(cmd, b"mv\0".as_ptr()) == 0 {
            handle_mv(argc, argv);
        } else if strcmp(cmd, b"cat\0".as_ptr()) == 0 {
            handle_cat(argc, argv);
        } else if strcmp(cmd, b"touch\0".as_ptr()) == 0 {
            handle_touch(argc, argv);
        } else if strcmp(cmd, b"mkdir\0".as_ptr()) == 0 {
            handle_mkdir(argc, argv);
        } else if strcmp(cmd, b"rm\0".as_ptr()) == 0 {
            handle_rm(argc, argv);
        } else if strcmp(cmd, b"echo\0".as_ptr()) == 0 {
            handle_echo(argc, argv);
        } else if strcmp(cmd, b"clip\0".as_ptr()) == 0 {
            handle_clip(argc, argv);
        } else if strcmp(cmd, b"async-demo\0".as_ptr()) == 0 {
            run_async_demo();
        } else if strcmp(cmd, b"clear\0".as_ptr()) == 0 {
            puts(b"\x1b[2J\x1b[H\0".as_ptr());
        } else if strcmp(cmd, b"exit\0".as_ptr()) == 0 {
            puts(b"Exiting shell...\0".as_ptr());
            exit(0);
        } else {
            printf(b"shell: %s: command not found\n\0".as_ptr(), cmd);
        }
    }
}

/// Shell panic handler.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    write_panic_info(STDERR_FILENO, "shell panic", info);
    // SAFETY: Exiting shell process on panic.
    unsafe {
        exit(1);
    }
}
