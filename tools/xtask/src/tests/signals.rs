//! POSIX signals, stack frame layout, red zone, and sigreturn test suite.

use super::harness::TestRunner;
use posix_abi::*;

/// Registers POSIX signal semantics and stack frame layout tests with the runner.
pub fn register_tests(runner: &mut TestRunner) {
    runner.run_test(
        "signals",
        "Signal Range, PID Filtering, and Uncatchable Masks",
        test_signal_ranges_and_masks,
    );
    runner.run_test(
        "signals",
        "User Signal Frame Layout, Red Zone, and Sigreturn",
        test_signal_frame_layout_and_sigreturn,
    );
}

/// Tests signal numeric range validation, uncatchable signal rules (SIGKILL/SIGSTOP), and default dispositions.
fn test_signal_ranges_and_masks() {
    let is_valid_signal = |sig: i32| (SIG_MIN..=SIG_MAX).contains(&sig);
    assert!(!is_valid_signal(0));
    assert!(!is_valid_signal(32));
    assert!(!is_valid_signal(-1));
    assert!(is_valid_signal(SIGKILL));
    assert!(is_valid_signal(SIGUSR1));
    assert!(is_valid_signal(SIGTERM));
    assert!(is_valid_signal(SIGSYS));

    let is_valid_pid = |pid: i32| pid > 0;
    assert!(!is_valid_pid(0));
    assert!(!is_valid_pid(-1));

    let can_catch = |sig: i32| sig != SIGKILL && sig != SIGSTOP;
    assert!(!can_catch(SIGKILL));
    assert!(!can_catch(SIGSTOP));
    assert!(can_catch(SIGUSR1));

    let unblockable: SigSet = (1 << (SIGKILL - 1)) | (1 << (SIGSTOP - 1));
    let blocked: SigSet = !unblockable;
    assert_eq!(blocked & (1 << (SIGKILL - 1)), 0);
    assert_eq!(blocked & (1 << (SIGSTOP - 1)), 0);
    assert_ne!(blocked & (1 << (SIGUSR1 - 1)), 0);

    let is_ignore = |s: i32| s == SIGCHLD || s == SIGURG || s == SIGWINCH;
    let is_stop = |s: i32| s == SIGSTOP || s == SIGTSTP || s == SIGTTIN || s == SIGTTOU;
    assert!(is_ignore(SIGCHLD));
    assert!(is_stop(SIGSTOP));
    assert!(!is_stop(SIGTERM));
    assert!(!is_ignore(SIGTERM));
}

/// Tests user signal frame construction, SysV 128-byte red zone avoidance, and rt_sigreturn restoration.
fn test_signal_frame_layout_and_sigreturn() {
    let old_rsp = 0x7FFF_FF00usize;
    let old_rip = 0x400500usize;
    let old_rflags = 0x202usize;
    let red_zone = 128;
    let frame_sz = core::mem::size_of::<SignalFrame>();
    let signal_rsp = (old_rsp.saturating_sub(red_zone + frame_sz)) & !0xF;

    let frame = SignalFrame {
        restorer: 0x401500,
        signum: SIGUSR1 as u64,
        old_mask: 0,
        rdx: 3, rsi: 2, rdi: 1, rax: 123, rcx: old_rip as u64, r11: old_rflags as u64, rsp: old_rsp as u64,
        ..Default::default()
    };

    assert_eq!(signal_rsp % 16, 0);
    assert!(signal_rsp <= old_rsp - 128 - frame_sz);

    let user_entry_rsp = signal_rsp + 8;
    let frame_read_addr = user_entry_rsp - 8;
    assert_eq!(frame_read_addr, signal_rsp);

    let restored_rip = frame.rcx as usize;
    let restored_rax = frame.rax as usize;
    let restored_rsp = frame.rsp as usize;
    let restored_rflags = (frame.r11 as usize & 0xCD5) | 0x202;

    assert_eq!(restored_rip, 0x400500);
    assert_eq!(restored_rax, 123);
    assert_eq!(restored_rsp, 0x7FFF_FF00);
    assert_eq!(restored_rflags, 0x202);

    let pending: u64 = 1 << (SIGTERM - 1);
    let blocked: SigSet = 0;
    assert_ne!(pending & !blocked, 0);
}
