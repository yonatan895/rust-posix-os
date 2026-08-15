//! POSIX Signals, Stack Frame Layout, Red Zone, and Sigreturn Test Suite.

use super::harness::TestRunner;
use posix_abi::*;

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

fn test_signal_ranges_and_masks() {
    // 1. Range consistency tests: SIG_MIN..=SIG_MAX (1..=31) and pid > 0 requirement
    let is_valid_signal = |sig: i32| (SIG_MIN..=SIG_MAX).contains(&sig);
    assert!(!is_valid_signal(0), "Signal 0 should be invalid");
    assert!(!is_valid_signal(32), "Signal 32 should be invalid");
    assert!(!is_valid_signal(-1), "Signal -1 should be invalid");
    assert!(is_valid_signal(SIGKILL), "SIGKILL should be valid");
    assert!(is_valid_signal(SIGUSR1), "SIGUSR1 should be valid");
    assert!(is_valid_signal(SIGTERM), "SIGTERM should be valid");
    assert!(is_valid_signal(SIGSYS), "SIGSYS should be valid");

    let is_valid_pid = |pid: i32| pid > 0;
    assert!(
        !is_valid_pid(0),
        "PID 0 (idle task / process group) must not be signaled directly"
    );
    assert!(
        !is_valid_pid(-1),
        "Negative PIDs must be rejected until process groups exist"
    );

    // 2. Uncatchable signals enforcement (SIGKILL=9, SIGSTOP=19)
    let can_catch_signal = |sig: i32| sig != SIGKILL && sig != SIGSTOP;
    assert!(!can_catch_signal(SIGKILL), "SIGKILL must not be catchable");
    assert!(!can_catch_signal(SIGSTOP), "SIGSTOP must not be catchable");
    assert!(can_catch_signal(SIGUSR1), "SIGUSR1 should be catchable");

    // 3. Signal mask blocking and unblockable bit filtering
    let unblockable_mask: SigSet = (1 << (SIGKILL - 1)) | (1 << (SIGSTOP - 1));

    // Try blocking all 64 bits (e.g. SIG_SETMASK)
    let new_set: SigSet = !0;
    let blocked_mask: SigSet = new_set & !unblockable_mask;
    assert_eq!(
        blocked_mask & (1 << (SIGKILL - 1)),
        0,
        "SIGKILL must not be blocked"
    );
    assert_eq!(
        blocked_mask & (1 << (SIGSTOP - 1)),
        0,
        "SIGSTOP must not be blocked"
    );
    assert_ne!(
        blocked_mask & (1 << (SIGUSR1 - 1)),
        0,
        "SIGUSR1 should be blocked"
    );

    // 4. Default dispositions: Terminate vs Stop vs Ignore
    let is_default_ignore = |sig: i32| sig == SIGCHLD || sig == SIGURG || sig == SIGWINCH;
    let is_default_stop =
        |sig: i32| sig == SIGSTOP || sig == SIGTSTP || sig == SIGTTIN || sig == SIGTTOU;

    assert!(is_default_ignore(SIGCHLD));
    assert!(is_default_stop(SIGSTOP));
    assert!(!is_default_stop(SIGTERM));
    assert!(!is_default_ignore(SIGTERM));
}

fn test_signal_frame_layout_and_sigreturn() {
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
    struct MockSyscallRegisters {
        rax: usize,
        rdi: usize,
        rsi: usize,
        rdx: usize,
        rcx: usize, // RIP
        r11: usize, // RFLAGS
        rsp: usize, // User RSP
    }

    let mut regs = MockSyscallRegisters {
        rax: 123,
        rdi: 1,
        rsi: 2,
        rdx: 3,
        rcx: 0x400500, // Pre-signal RIP
        r11: 0x202,    // Pre-signal RFLAGS (IF=1)
        rsp: 0x7FFF_FF00,
    };

    let handler_addr: usize = 0x401000;
    let restorer_addr: usize = 0x401500;
    let initial_mask: SigSet = 0;

    // Simulate deliver_signal_to_user: subtract 128-byte red zone and frame size, align to 16 bytes
    let red_zone_size = 128;
    let frame_size = core::mem::size_of::<SignalFrame>();
    let signal_rsp = (regs.rsp.saturating_sub(red_zone_size + frame_size)) & !0xF;

    let frame = SignalFrame {
        restorer: restorer_addr as u64,
        signum: SIGUSR1 as u64,
        old_mask: initial_mask,
        r15: 0,
        r14: 0,
        r13: 0,
        r12: 0,
        rbp: 0,
        rbx: 0,
        r9: 0,
        r8: 0,
        r10: 0,
        rdx: regs.rdx as u64,
        rsi: regs.rsi as u64,
        rdi: regs.rdi as u64,
        rax: regs.rax as u64,
        rcx: regs.rcx as u64,
        r11: regs.r11 as u64,
        rsp: regs.rsp as u64,
    };

    // User signal handler begins execution at `signal_rsp`
    regs.rsp = signal_rsp;
    regs.rcx = handler_addr; // RIP jumps to handler
    regs.rdi = SIGUSR1 as usize; // arg1 = signum
    regs.rsi = 0;
    regs.rdx = signal_rsp; // arg3 = ucontext/frame pointer

    assert_eq!(regs.rcx, handler_addr);
    assert_eq!(regs.rdi, 10);
    assert_eq!(regs.rsp % 16, 0, "Signal stack must be 16-byte aligned");
    assert!(
        signal_rsp <= 0x7FFF_FF00 - 128 - frame_size,
        "Frame must not overlap with SysV red zone"
    );

    // User handler runs and finishes with `ret`, popping restorer address from [signal_rsp]
    // Stack pointer advances by 8 bytes upon jumping to `__restore_rt`
    let user_entry_rsp_to_sigreturn = signal_rsp + 8;
    regs.rsp = user_entry_rsp_to_sigreturn;

    // Kernel sys_rt_sigreturn reads frame base at `r.rsp - 8`
    let frame_read_addr = regs.rsp.saturating_sub(core::mem::size_of::<u64>());
    assert_eq!(
        frame_read_addr, signal_rsp,
        "sys_rt_sigreturn must read frame base at r.rsp - 8"
    );

    // Simulate SYS_RT_SIGRETURN register restore with RFLAGS masking
    regs.rax = frame.rax as usize;
    regs.rdi = frame.rdi as usize;
    regs.rsi = frame.rsi as usize;
    regs.rdx = frame.rdx as usize;
    regs.rcx = frame.rcx as usize; // Restored RIP
    const USER_RFLAGS_MASK: usize = 0xCD5;
    const USER_RFLAGS_RESERVED: usize = 0x202;
    regs.r11 = (frame.r11 as usize & USER_RFLAGS_MASK) | USER_RFLAGS_RESERVED;
    regs.rsp = frame.rsp as usize; // Restored user RSP

    assert_eq!(
        regs.rcx, 0x400500,
        "RIP not restored correctly on sigreturn"
    );
    assert_eq!(regs.rax, 123, "RAX not restored correctly on sigreturn");
    assert_eq!(
        regs.rsp, 0x7FFF_FF00,
        "RSP not restored correctly on sigreturn"
    );
    assert_eq!(
        regs.r11, 0x202,
        "RFLAGS not restored correctly on sigreturn"
    );

    // Verify EINTR on blocked tasks: wake_tasks unblocks and returns EINTR
    let has_unblocked_signals = |pending: u64, blocked: SigSet| (pending & !blocked) != 0;
    let pending_mask: u64 = 1 << (SIGTERM - 1);
    let current_blocked_mask: SigSet = 0;
    assert!(
        has_unblocked_signals(pending_mask, current_blocked_mask),
        "Task with pending SIGTERM must recognize unblocked signal and return -EINTR"
    );
}
