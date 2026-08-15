//! Automated Test Suite Definitions and Result Verification.
//!
//! Decomposed into domain-specific test suites following Rust best practices
//! and OS testing methodologies.

pub mod binary;
pub mod harness;
pub mod ipc;
pub mod mm;
pub mod process;
pub mod signals;
pub mod syscall;
pub mod userland;
pub mod vfs;

use harness::TestRunner;

/// Runs all automated test suites, applying an optional pattern filter if provided.
pub fn run_tests(args: &[String]) {
    let filter = parse_filter(args);

    if let Some(ref f) = filter {
        println!(
            "[xtask] Running automated test suite (filter: \"{}\")...",
            f
        );
    } else {
        println!("[xtask] Running automated test suite verification...");
    }

    let mut runner = TestRunner::new(filter);

    // 1. Binary Integrity and ELF64 Headers
    binary::register_tests(&mut runner);

    // 2. Memory Management (MM) and Address Space Isolation
    mm::register_tests(&mut runner);

    // 3. Process Lifecycle, Round-Robin Scheduling, and Waitpid
    process::register_tests(&mut runner);

    // 4. Inter-Process Communication (IPC) and Pipe State Machine
    ipc::register_tests(&mut runner);

    // 5. Signals, Frame Layout, Red Zone, and Sigreturn
    signals::register_tests(&mut runner);

    // 6. Virtual File System (VFS), Creation Modes, Permissions, and Audit
    vfs::register_tests(&mut runner);

    // 7. Syscall Dispatcher, User Pointer Validation, and EFAULT Hammer
    syscall::register_tests(&mut runner);

    // 8. Userland Libraries, Allocator Stress, Panic Formatting, and Line Editor
    userland::register_tests(&mut runner);

    // Print summary and enforce exit status
    runner.summary();
}

/// Parses the optional substring filter argument (`--filter <pattern>` or `-f <pattern>`).
fn parse_filter(args: &[String]) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--filter" || arg == "-f" {
            if let Some(val) = iter.next() {
                return Some(val.clone());
            }
        } else if let Some(stripped) = arg.strip_prefix("--filter=") {
            return Some(stripped.to_string());
        }
    }
    None
}
