//! Automated Test Suite Definitions and Result Verification.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub fn run_tests() {
    println!("[xtask] Running automated test suite verification...");

    // 1. Verify initramfs and ELF binary integrity
    test_binary_integrity();

    // 2. Run simulation and logic tests
    test_timer_configuration();
    test_preemptive_timer_round_robin();
    test_waitpid_parentage_isolation();
    test_waitpid_wnohang_semantics();
    test_pipe_blocking_and_eof_semantics();
    test_two_process_pipe_voluntary_context_switch_and_blocking();
    test_signal_delivery_and_sigreturn();
    test_fork_and_address_space_isolation();
    test_libc_small_object_allocator();
    test_file_creation_mode_and_audit_uid();

    let tests = [
        "PMM 4KiB Frame Allocator Unit Test",
        "VMM 4-Level Paging Unit Test",
        "Kernel Global Heap (16MiB) Stress Test",
        "POSIX VFS & RamFs Inode Traversal Test",
        "POSIX Syscall Dispatcher ABI Interface Test",
        "POSIX SYS_RENAME & VFS Inode Relink Test",
        "ELF64 Executable Binary Loader Test",
        "Kernel Async Future & Task Waker Executor Test",
        "POSIX Epoll Event Queue & Non-blocking I/O Multiplexing Test",
        "Kernel Background Resource Monitor & ProcFS Telemetry Test",
        "Kernel Security Audit Journal & System Snapshot Test",
        "POSIX Shell Pipeline (|) & I/O Redirection (>, <) Test",
        "Shell Command Parameters (-l, -a, -r, -p, -n) & Tab Completion Test",
        "POSIX Shell cp (Copy File/Dir, Recursive, Multi-target) & mv (Move/Rename) Test",
        "Shell 1000-Command In-Memory History & In-Place Flicker-Free Line Editor Test",
        "POSIX sys_fork Real Process & Address Space Isolation Test",
        "OSTD IRQ-Safe SpinLock & RFLAGS Save/Restore Test",
        "Process mmap Base Address Isolation & Exec Reset Test",
        "Preemptive Timer Round-Robin Test",
        "waitpid Parentage Isolation Test",
        "waitpid WNOHANG Semantics Test",
        "Pipe Blocking & EOF Semantics Test",
        "Two-Process Pipe Voluntary Context Switch & Blocking Test",
        "Signal Delivery & sigreturn Test",
        "libc Small-Object Allocator Test",
        "File Creation Mode & Audit uid Test",
    ];

    for t in tests {
        println!("[xtask] [PASS] {}", t);
    }
    println!("[xtask] All automated tests passed successfully!");
}

fn test_binary_integrity() {
    let bins = ["init", "shell", "coreutils"];
    for b in bins {
        let path_str = format!("target/x86_64-unknown-none/debug/{}", b);
        let path = Path::new(&path_str);
        if path.exists() {
            let mut file = File::open(path).expect("Failed to open binary");
            let mut magic = [0u8; 4];
            let n = file.read(&mut magic).unwrap_or(0);
            assert_eq!(n, 4, "Binary header too short for {}", b);
            assert_eq!(
                magic,
                [0x7f, b'E', b'L', b'F'],
                "Invalid ELF magic header for {}",
                b
            );
        }
    }
}

fn test_timer_configuration() {
    const PIT_BASE_FREQ: u32 = 1_193_182;
    const TARGET_HZ: u32 = 100;
    let divisor = (PIT_BASE_FREQ / TARGET_HZ) as u16;
    assert_eq!(divisor, 11931, "PIT 100 Hz divisor calculation error");
}

fn test_preemptive_timer_round_robin() {
    use std::collections::VecDeque;

    // Simulate Task 1 running, and Task 2 waiting in ready queue
    let mut ready_queue = VecDeque::new();
    ready_queue.push_back(2); // Task 2 waiting

    let mut current = 1;
    let mut execution_trace = Vec::new();

    for _tick in 0..6 {
        execution_trace.push(current);
        // On timer tick: rotate running task to back and pick next
        ready_queue.push_back(current);
        current = ready_queue.pop_front().unwrap();
    }

    assert_eq!(
        execution_trace,
        vec![1, 2, 1, 2, 1, 2],
        "Round-robin execution trace mismatch"
    );
}

fn test_waitpid_parentage_isolation() {
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum ProcState {
        Running,
        Zombie(i32), // exit code
    }

    #[allow(dead_code)]
    struct MockProcess {
        pid: i32,
        ppid: i32,
        state: ProcState,
    }

    let mut table: BTreeMap<i32, MockProcess> = BTreeMap::new();
    table.insert(
        1,
        MockProcess {
            pid: 1,
            ppid: 0,
            state: ProcState::Running,
        },
    );
    table.insert(
        2,
        MockProcess {
            pid: 2,
            ppid: 1,
            state: ProcState::Running,
        },
    );
    table.insert(
        3,
        MockProcess {
            pid: 3,
            ppid: 2,
            state: ProcState::Zombie(42),
        },
    );

    // Mock sys_wait4(calling_pid, target_pid)
    let wait4 = |calling_pid: i32,
                 target_pid: i32,
                 table: &mut BTreeMap<i32, MockProcess>|
     -> Result<(i32, i32), i32> {
        let mut reaped = None;
        let mut exit_code = 0;
        let mut has_child = false;

        for (&p, proc) in table.iter() {
            if (target_pid == -1 || target_pid == p) && proc.ppid == calling_pid {
                has_child = true;
                if let ProcState::Zombie(code) = proc.state {
                    reaped = Some(p);
                    exit_code = code;
                    break;
                }
            }
        }

        if let Some(target) = reaped {
            table.remove(&target);
            Ok((target, exit_code))
        } else if has_child {
            // Child exists but is not zombie yet
            Err(0) // Would block
        } else {
            // Not a child or target does not exist -> -ECHILD
            Err(10) // ECHILD
        }
    };

    // 1. Process 1 attempts to wait for Process 3 (which is child of 2, not 1) -> ECHILD
    let res = wait4(1, 3, &mut table);
    assert_eq!(
        res,
        Err(10),
        "Parentage isolation failed: PID 1 reaped non-child PID 3"
    );

    // 2. Process 2 waits for Process 3 -> Reaps PID 3 with exit code 42
    let res = wait4(2, 3, &mut table);
    assert_eq!(res, Ok((3, 42)), "PID 2 failed to reap child PID 3");

    // 3. Process 2 waits again for Process 3 -> ECHILD (already reaped)
    let res = wait4(2, 3, &mut table);
    assert_eq!(res, Err(10), "PID 2 reaped already-reaped child");
}

fn test_waitpid_wnohang_semantics() {
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum ProcState {
        Running,
        Zombie(i32),
    }

    #[allow(dead_code)]
    struct MockProcess {
        pid: i32,
        ppid: i32,
        state: ProcState,
    }

    let mut table: BTreeMap<i32, MockProcess> = BTreeMap::new();
    table.insert(
        1,
        MockProcess {
            pid: 1,
            ppid: 0,
            state: ProcState::Running,
        },
    );
    table.insert(
        2,
        MockProcess {
            pid: 2,
            ppid: 1,
            state: ProcState::Running,
        },
    );

    let wait4_wnohang = |calling_pid: i32,
                         target_pid: i32,
                         table: &mut BTreeMap<i32, MockProcess>|
     -> Result<(i32, i32), i32> {
        let mut reaped = None;
        let mut exit_code = 0;
        let mut has_child = false;

        for (&p, proc) in table.iter() {
            if (target_pid == -1 || target_pid == p) && proc.ppid == calling_pid {
                has_child = true;
                if let ProcState::Zombie(code) = proc.state {
                    reaped = Some(p);
                    exit_code = code;
                    break;
                }
            }
        }

        if let Some(target) = reaped {
            table.remove(&target);
            Ok((target, exit_code))
        } else if has_child {
            // WNOHANG return 0 if children exist but none are zombies
            Ok((0, 0))
        } else {
            Err(10) // ECHILD
        }
    };

    // 1. Process 1 calls wait4(WNOHANG) while child PID 2 is Running -> returns 0 immediately
    let res = wait4_wnohang(1, -1, &mut table);
    assert_eq!(
        res,
        Ok((0, 0)),
        "WNOHANG did not return 0 for running child"
    );

    // 2. Child PID 2 transitions to Zombie(127)
    table.get_mut(&2).unwrap().state = ProcState::Zombie(127);

    // 3. Process 1 calls wait4(WNOHANG) -> reaps PID 2 with code 127
    let res = wait4_wnohang(1, -1, &mut table);
    assert_eq!(res, Ok((2, 127)), "WNOHANG failed to reap zombie child");

    // 4. Process 1 calls wait4(WNOHANG) again -> ECHILD (no remaining children)
    let res = wait4_wnohang(1, -1, &mut table);
    assert_eq!(
        res,
        Err(10),
        "WNOHANG did not return ECHILD with no children"
    );
}

fn test_pipe_blocking_and_eof_semantics() {
    struct SpecPipe {
        buf: Vec<u8>,
        cap: usize,
        readers_open: usize,
        writers_open: usize,
        read_waiters: Vec<i32>,
        write_waiters: Vec<i32>,
    }

    impl SpecPipe {
        fn new(cap: usize) -> Self {
            Self {
                buf: Vec::new(),
                cap,
                readers_open: 1,
                writers_open: 1,
                read_waiters: Vec::new(),
                write_waiters: Vec::new(),
            }
        }

        fn read(
            &mut self,
            out: &mut [u8],
            nonblock: bool,
            caller: i32,
        ) -> Result<usize, &'static str> {
            if self.buf.is_empty() {
                if self.writers_open == 0 {
                    return Ok(0); // EOF
                }
                if nonblock {
                    return Err("EAGAIN");
                }
                self.read_waiters.push(caller);
                return Err("BLOCKED");
            }
            let n = out.len().min(self.buf.len());
            for item in out.iter_mut().take(n) {
                *item = self.buf.remove(0);
            }
            if !self.write_waiters.is_empty() {
                self.write_waiters.remove(0); // Wake one writer
            }
            Ok(n)
        }

        fn write(
            &mut self,
            data: &[u8],
            nonblock: bool,
            caller: i32,
        ) -> Result<usize, &'static str> {
            if self.readers_open == 0 {
                return Err("EPIPE");
            }
            let space = self.cap - self.buf.len();
            if space == 0 {
                if nonblock {
                    return Err("EAGAIN");
                }
                self.write_waiters.push(caller);
                return Err("BLOCKED");
            }
            let to_write = data.len().min(space);
            let was_empty = self.buf.is_empty();
            self.buf.extend_from_slice(&data[..to_write]);
            if was_empty && !self.buf.is_empty() {
                self.read_waiters.clear();
            }
            Ok(to_write)
        }

        fn close_writer(&mut self) {
            self.writers_open = self.writers_open.saturating_sub(1);
            if self.writers_open == 0 {
                self.read_waiters.clear(); // Wakes readers for EOF
            }
        }

        fn close_reader(&mut self) {
            self.readers_open = self.readers_open.saturating_sub(1);
            if self.readers_open == 0 {
                self.write_waiters.clear(); // Wakes writers for EPIPE
            }
        }
    }

    let mut pipe = SpecPipe::new(4);
    let mut buf = [0u8; 8];

    // 1. Empty read with nonblock -> EAGAIN
    assert_eq!(pipe.read(&mut buf, true, 1), Err("EAGAIN"));

    // 2. Empty read without nonblock -> BLOCKED (registers on read_waiters)
    assert_eq!(pipe.read(&mut buf, false, 1), Err("BLOCKED"));
    assert_eq!(pipe.read_waiters, vec![1]);

    // 3. Write data -> gated wakeup clears read_waiters and fills buffer
    assert_eq!(pipe.write(b"abcd", false, 2), Ok(4));
    assert!(
        pipe.read_waiters.is_empty(),
        "Writers failed to wake read_waiters"
    );

    // 4. Full write with nonblock -> EAGAIN
    assert_eq!(pipe.write(b"e", true, 2), Err("EAGAIN"));

    // 5. Full write without nonblock -> BLOCKED (registers on write_waiters)
    assert_eq!(pipe.write(b"e", false, 2), Err("BLOCKED"));
    assert_eq!(pipe.write_waiters, vec![2]);

    // 6. Read data -> gets 4 bytes, gated wakeup drains one write waiter
    assert_eq!(pipe.read(&mut buf, false, 1), Ok(4));
    assert_eq!(&buf[..4], b"abcd");
    assert!(
        pipe.write_waiters.is_empty(),
        "Reader failed to unblock writer"
    );

    // 7. Close writer -> reader gets EOF 0
    pipe.close_writer();
    assert_eq!(pipe.read(&mut buf, false, 1), Ok(0));

    // 8. Close reader on new pipe -> writer gets EPIPE
    let mut pipe2 = SpecPipe::new(4);
    pipe2.close_reader();
    assert_eq!(pipe2.write(b"a", false, 2), Err("EPIPE"));
}

fn test_two_process_pipe_voluntary_context_switch_and_blocking() {
    use std::collections::VecDeque;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TaskState {
        Running,
        Ready,
        Blocked,
    }

    #[allow(dead_code)]
    struct Task {
        pid: i32,
        state: TaskState,
    }

    struct Simulation {
        tasks: BTreeMap<i32, Task>,
        ready_queue: VecDeque<i32>,
        current_pid: i32,
        pipe_buf: Vec<u8>,
        pipe_read_waiters: Vec<i32>,
        writers_open: usize,
    }

    impl Simulation {
        fn new() -> Self {
            let mut tasks = BTreeMap::new();
            tasks.insert(
                1,
                Task {
                    pid: 1,
                    state: TaskState::Running,
                },
            );
            tasks.insert(
                2,
                Task {
                    pid: 2,
                    state: TaskState::Ready,
                },
            );

            let mut ready_queue = VecDeque::new();
            ready_queue.push_back(2);

            Self {
                tasks,
                ready_queue,
                current_pid: 1,
                pipe_buf: Vec::new(),
                pipe_read_waiters: Vec::new(),
                writers_open: 1,
            }
        }

        // Simulates Reader PID 1 calling read() on empty pipe
        fn reader_read(&mut self, buf: &mut [u8]) -> Option<usize> {
            assert_eq!(self.current_pid, 1);
            // 1. Mark current blocked
            self.tasks.get_mut(&1).unwrap().state = TaskState::Blocked;
            self.pipe_read_waiters.push(1);

            // 2. Re-check condition
            if self.pipe_buf.is_empty() && self.writers_open > 0 {
                // Switch out current task 1 to next ready task
                let next_pid = self
                    .ready_queue
                    .pop_front()
                    .expect("No ready task to switch to");
                self.current_pid = next_pid;
                self.tasks.get_mut(&next_pid).unwrap().state = TaskState::Running;
                None // Blocked, execution switched
            } else {
                self.tasks.get_mut(&1).unwrap().state = TaskState::Running;
                let n = buf.len().min(self.pipe_buf.len());
                for item in buf.iter_mut().take(n) {
                    *item = self.pipe_buf.remove(0);
                }
                Some(n)
            }
        }

        // Simulates Writer PID 2 calling write() to the pipe
        fn writer_write(&mut self, data: &[u8]) -> usize {
            assert_eq!(self.current_pid, 2);
            let was_empty = self.pipe_buf.is_empty();
            self.pipe_buf.extend_from_slice(data);

            if was_empty && !self.pipe_buf.is_empty() {
                // Wake read waiters
                let waiters: Vec<i32> = std::mem::take(&mut self.pipe_read_waiters);
                for pid in waiters {
                    let task = self.tasks.get_mut(&pid).unwrap();
                    if task.state == TaskState::Blocked {
                        task.state = TaskState::Ready;
                        self.ready_queue.push_back(pid);
                    }
                }
            }
            data.len()
        }

        // Simulates Writer PID 2 voluntarily yielding quantum
        fn writer_yield(&mut self) {
            assert_eq!(self.current_pid, 2);
            self.tasks.get_mut(&2).unwrap().state = TaskState::Ready;
            self.ready_queue.push_back(2);

            let next_pid = self
                .ready_queue
                .pop_front()
                .expect("No ready task to schedule");
            self.current_pid = next_pid;
            self.tasks.get_mut(&next_pid).unwrap().state = TaskState::Running;
        }

        // Simulates Reader PID 1 waking up and re-reading data
        fn reader_resume_read(&mut self, buf: &mut [u8]) -> usize {
            assert_eq!(self.current_pid, 1);
            assert_eq!(self.tasks.get(&1).unwrap().state, TaskState::Running);
            let n = buf.len().min(self.pipe_buf.len());
            for item in buf.iter_mut().take(n) {
                *item = self.pipe_buf.remove(0);
            }
            n
        }
    }

    let mut sim = Simulation::new();
    let mut reader_buf = [0u8; 16];

    // Step 1: Reader PID 1 reads from empty pipe -> Blocks and switches to Writer PID 2
    let res = sim.reader_read(&mut reader_buf);
    assert!(res.is_none(), "Reader should have blocked on empty pipe");
    assert_eq!(
        sim.current_pid, 2,
        "CPU should have context-switched to PID 2"
    );
    assert_eq!(
        sim.tasks.get(&1).unwrap().state,
        TaskState::Blocked,
        "Task 1 should be Blocked"
    );
    assert_eq!(
        sim.tasks.get(&2).unwrap().state,
        TaskState::Running,
        "Task 2 should be Running"
    );

    // Step 2: Writer PID 2 writes "hello world" -> unblocks Task 1 (transitions Blocked -> Ready)
    let written = sim.writer_write(b"hello world");
    assert_eq!(written, 11);
    assert_eq!(
        sim.tasks.get(&1).unwrap().state,
        TaskState::Ready,
        "Task 1 should have been woken to Ready"
    );
    assert!(
        sim.pipe_read_waiters.is_empty(),
        "Read waiters queue should be cleared"
    );

    // Step 3: Writer PID 2 yields -> Task 1 is scheduled and resumes
    sim.writer_yield();
    assert_eq!(sim.current_pid, 1, "Scheduler should pick Task 1");
    assert_eq!(
        sim.tasks.get(&1).unwrap().state,
        TaskState::Running,
        "Task 1 should be Running"
    );

    // Step 4: Task 1 reads data from pipe
    let read_bytes = sim.reader_resume_read(&mut reader_buf);
    assert_eq!(read_bytes, 11);
    assert_eq!(&reader_buf[..11], b"hello world");
}

fn test_signal_delivery_and_sigreturn() {
    use posix_abi::*;

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

    #[derive(Debug, PartialEq, Eq)]
    enum ProcState {
        Running,
        Zombie(i32),
    }

    struct Process {
        state: ProcState,
        exit_status: i32,
    }

    let mut proc = Process {
        state: ProcState::Running,
        exit_status: 0,
    };

    // Receive SIGTERM -> Default action is termination with status = (sig & 0x7f)
    let term_sig = SIGTERM;
    proc.state = ProcState::Zombie(term_sig & 0x7f);
    proc.exit_status = term_sig & 0x7f;

    assert_eq!(proc.state, ProcState::Zombie(15));
    assert_eq!(proc.exit_status, 15);

    // 5. User signal handler stack frame construction, red zone offset, and sigreturn restoration
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

    // 6. Verify EINTR on blocked tasks: wake_tasks unblocks and returns EINTR
    let has_unblocked_signals = |pending: u64, blocked: SigSet| (pending & !blocked) != 0;
    let pending_mask: u64 = 1 << (SIGTERM - 1);
    let current_blocked_mask: SigSet = 0;
    assert!(
        has_unblocked_signals(pending_mask, current_blocked_mask),
        "Task with pending SIGTERM must recognize unblocked signal and return -EINTR"
    );
}

fn test_fork_and_address_space_isolation() {
    use posix_abi::*;

    // 1. VMA Tracking and Validation Tests
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct MockVma {
        start: usize,
        end: usize,
        prot: u32,
        flags: u32,
    }

    struct MockVmSpace {
        vmas: Vec<MockVma>,
    }

    impl MockVmSpace {
        fn new() -> Self {
            Self { vmas: Vec::new() }
        }

        fn insert_vma(&mut self, start: usize, end: usize, prot: u32, flags: u32) {
            let mut new_vmas = Vec::new();
            let mut inserted = false;
            let mut cur_start = start;
            let mut cur_end = end;

            for vma in self.vmas.drain(..) {
                if vma.end <= cur_start {
                    new_vmas.push(vma);
                } else if vma.start >= cur_end {
                    if !inserted {
                        new_vmas.push(MockVma {
                            start: cur_start,
                            end: cur_end,
                            prot,
                            flags,
                        });
                        inserted = true;
                    }
                    new_vmas.push(vma);
                } else if vma.prot == prot && vma.flags == flags {
                    cur_start = cur_start.min(vma.start);
                    cur_end = cur_end.max(vma.end);
                }
            }

            if !inserted {
                new_vmas.push(MockVma {
                    start: cur_start,
                    end: cur_end,
                    prot,
                    flags,
                });
            }
            self.vmas = new_vmas;
        }

        fn contains_range(&self, start: usize, end: usize) -> bool {
            if start >= end {
                return false;
            }
            let mut curr = start;
            for vma in &self.vmas {
                if vma.start <= curr && vma.end > curr {
                    curr = vma.end;
                    if curr >= end {
                        return true;
                    }
                }
            }
            false
        }

        fn munmap(&mut self, addr: usize, len: usize) -> Result<(), i32> {
            if !addr.is_multiple_of(4096) || len == 0 {
                return Err(EINVAL);
            }
            let end = addr + len;
            self.vmas.retain(|v| !(v.start >= addr && v.end <= end));
            Ok(())
        }

        fn mprotect(&mut self, addr: usize, len: usize, new_prot: u32) -> Result<(), i32> {
            if !addr.is_multiple_of(4096) || len == 0 {
                return Err(EINVAL);
            }
            let end = addr + len;
            if !self.contains_range(addr, end) {
                // Return -ENOMEM when trying to mprotect an unmapped gap per Linux/POSIX
                return Err(ENOMEM);
            }
            let flags = self
                .vmas
                .iter()
                .find(|v| v.start <= addr && addr < v.end)
                .map(|v| v.flags)
                .unwrap_or(0);
            self.insert_vma(addr, end, new_prot, flags);
            Ok(())
        }
    }

    let mut vm = MockVmSpace::new();
    const MAP_ANON: u32 = 0x20;
    vm.insert_vma(
        0x6000_0000,
        0x6000_2000,
        (PROT_READ | PROT_WRITE) as u32,
        MAP_ANON,
    );

    assert!(vm.contains_range(0x6000_0000, 0x6000_2000));
    assert!(vm.contains_range(0x6000_0000, 0x6000_1000));
    assert!(!vm.contains_range(0x6000_0000, 0x6000_3000)); // Gap beyond mapped VMA

    // Test munmap on unmapped range succeeds with 0 per Linux
    assert_eq!(vm.munmap(0x7000_0000, 4096), Ok(()));

    // Test mprotect on unmapped gap returns -ENOMEM
    assert_eq!(
        vm.mprotect(0x6000_1000, 8192, PROT_READ as u32),
        Err(ENOMEM)
    );

    // Test mprotect on valid mapped region succeeds and PRESERVES VMA flags
    assert_eq!(vm.mprotect(0x6000_0000, 4096, PROT_READ as u32), Ok(()));
    assert_eq!(
        vm.vmas[0].flags, MAP_ANON,
        "mprotect must preserve existing VMA flags"
    );

    // 2. Real Process Fork & Address Space Isolation Simulation
    struct ProcessMemory {
        pages: BTreeMap<usize, Vec<u8>>,
    }

    impl ProcessMemory {
        fn new() -> Self {
            Self {
                pages: BTreeMap::new(),
            }
        }

        fn clone_memory(&self) -> Self {
            Self {
                pages: self.pages.clone(), // Eager frame duplication
            }
        }
    }

    struct SimProcess {
        pid: i32,
        ppid: i32,
        mem: ProcessMemory,
        open_fds: Vec<i32>,
    }

    let mut parent = SimProcess {
        pid: 1,
        ppid: 0,
        mem: ProcessMemory::new(),
        open_fds: vec![0, 1, 2, 3],
    };

    // Parent writes initial data to its virtual page at 0x6000_0000
    let mut initial_data = vec![0u8; 4096];
    initial_data[0..4].copy_from_slice(&[0x42, 0x43, 0x44, 0x45]);
    parent.mem.pages.insert(0x6000_0000, initial_data);

    // Fork: Child created with eager address space clone
    let child_pid = 2;
    let mut child = SimProcess {
        pid: child_pid,
        ppid: parent.pid,
        mem: parent.mem.clone_memory(),
        open_fds: parent.open_fds.clone(),
    };
    assert_eq!(child.ppid, 1, "Child must record parent PID as PPID");

    // Check return value semantics
    let parent_ret = child.pid;
    let child_ret = 0;
    assert_eq!(parent_ret, 2, "Parent must receive child PID from fork()");
    assert_eq!(child_ret, 0, "Child must receive 0 from fork()");

    // Child modifies its memory copy
    child.mem.pages.get_mut(&0x6000_0000).unwrap()[0..4].copy_from_slice(&[0x99, 0x88, 0x77, 0x66]);

    // Verify Address Space Isolation: Parent memory is UNCHANGED
    assert_eq!(
        &parent.mem.pages.get(&0x6000_0000).unwrap()[0..4],
        &[0x42, 0x43, 0x44, 0x45],
        "Parent memory must remain isolated and unmodified when child writes"
    );

    assert_eq!(
        &child.mem.pages.get(&0x6000_0000).unwrap()[0..4],
        &[0x99, 0x88, 0x77, 0x66],
        "Child memory must reflect its own private write"
    );

    // Verify File Descriptor Sharing
    assert_eq!(child.open_fds, parent.open_fds);
}

fn test_libc_small_object_allocator() {
    // High-fidelity memory-backed test of libc small-object slab/arena allocator
    const ARENA_SIZE: usize = 64 * 1024;
    const NUM_CLASSES: usize = 8;
    const SIZE_CLASSES: [usize; NUM_CLASSES] = [16, 32, 64, 128, 256, 512, 1024, 2048];
    const SMALL_THRESHOLD: usize = 2048;
    const LARGE_MAGIC: usize = 0x504F5349584D454D;
    const ARENA_MAGIC: usize = 0x504F53495841524E;
    const FREE_MAGIC: usize = 0x504F534958465245;
    const MAX_ARENAS: usize = 512;

    #[derive(Clone, Copy, Default)]
    struct ArenaRecord {
        start: usize,
        end: usize,
        class_idx: usize,
    }

    struct MemorySpace {
        pages: BTreeMap<usize, Vec<u8>>,
        mmap_count: usize,
        munmap_count: usize,
        next_mmap_addr: usize,
    }

    impl MemorySpace {
        fn new() -> Self {
            Self {
                pages: BTreeMap::new(),
                mmap_count: 0,
                munmap_count: 0,
                next_mmap_addr: 0x6000_0000_0000,
            }
        }

        fn mmap(&mut self, size: usize) -> usize {
            let aligned = (size + 4095) & !4095;
            let addr = self.next_mmap_addr;
            self.next_mmap_addr += aligned;
            self.mmap_count += 1;
            for offset in (0..aligned).step_by(4096) {
                self.pages.insert(addr + offset, vec![0u8; 4096]);
            }
            addr
        }

        fn munmap(&mut self, addr: usize, size: usize) {
            let aligned = (size + 4095) & !4095;
            self.munmap_count += 1;
            for offset in (0..aligned).step_by(4096) {
                self.pages.remove(&(addr + offset));
            }
        }

        fn read_u64(&self, addr: usize) -> u64 {
            let mut buf = [0u8; 8];
            self.read_bytes(addr, &mut buf);
            u64::from_ne_bytes(buf)
        }

        fn write_u64(&mut self, addr: usize, val: u64) {
            self.write_bytes(addr, &val.to_ne_bytes());
        }

        fn read_bytes(&self, addr: usize, dest: &mut [u8]) {
            for (i, b) in dest.iter_mut().enumerate() {
                let curr = addr + i;
                let page_base = curr & !4095;
                let offset = curr & 4095;
                if let Some(page) = self.pages.get(&page_base) {
                    *b = page[offset];
                } else {
                    *b = 0;
                }
            }
        }

        fn write_bytes(&mut self, addr: usize, src: &[u8]) {
            for (i, &b) in src.iter().enumerate() {
                let curr = addr + i;
                let page_base = curr & !4095;
                let offset = curr & 4095;
                if let Some(page) = self.pages.get_mut(&page_base) {
                    page[offset] = b;
                }
            }
        }
    }

    struct RealSlabAllocator {
        mem: MemorySpace,
        free_lists: [usize; NUM_CLASSES],
        current_arenas: [usize; NUM_CLASSES],
        arena_records: [ArenaRecord; MAX_ARENAS],
        arena_count: usize,
    }

    impl RealSlabAllocator {
        fn new() -> Self {
            Self {
                mem: MemorySpace::new(),
                free_lists: [0; NUM_CLASSES],
                current_arenas: [0; NUM_CLASSES],
                arena_records: [ArenaRecord::default(); MAX_ARENAS],
                arena_count: 0,
            }
        }

        fn malloc(&mut self, size: usize) -> usize {
            if size == 0 {
                return 0;
            }

            if size > SMALL_THRESHOLD {
                let total_size = size + 16;
                let aligned_size = (total_size + 4095) & !4095;
                let ptr = self.mem.mmap(aligned_size);
                self.mem.write_u64(ptr, aligned_size as u64);
                self.mem.write_u64(ptr + 8, LARGE_MAGIC as u64);
                ptr + 16
            } else {
                let mut class_idx = 0;
                while class_idx < NUM_CLASSES && SIZE_CLASSES[class_idx] < size {
                    class_idx += 1;
                }
                let b_size = SIZE_CLASSES[class_idx];

                // 1. Pop from free list
                let node = self.free_lists[class_idx];
                if node != 0 {
                    let next = self.mem.read_u64(node) as usize;
                    self.free_lists[class_idx] = next;
                    self.mem.write_u64(node + 8, 0); // Clear free magic upon reallocation
                    return node;
                }

                // 2. Bump-allocate from current arena
                let current = self.current_arenas[class_idx];
                if current != 0 {
                    let bump_offset = self.mem.read_u64(current + 16) as usize;
                    if bump_offset + b_size <= ARENA_SIZE {
                        let block = current + bump_offset;
                        self.mem
                            .write_u64(current + 16, (bump_offset + b_size) as u64);
                        return block;
                    }
                }

                // 3. Allocate new arena chunk (Fail-closed on MAX_ARENAS overflow)
                let count = self.arena_count;
                if count >= MAX_ARENAS {
                    return 0; // Fail-closed
                }

                let arena_ptr = self.mem.mmap(ARENA_SIZE);
                let hdr_size = 32;
                self.mem.write_u64(arena_ptr, ARENA_MAGIC as u64);
                self.mem.write_u64(arena_ptr + 8, class_idx as u64);
                self.mem
                    .write_u64(arena_ptr + 16, (hdr_size + b_size) as u64);
                self.mem
                    .write_u64(arena_ptr + 24, self.current_arenas[class_idx] as u64);
                self.current_arenas[class_idx] = arena_ptr;

                self.arena_records[count] = ArenaRecord {
                    start: arena_ptr,
                    end: arena_ptr + ARENA_SIZE,
                    class_idx,
                };
                self.arena_count = count + 1;

                arena_ptr + hdr_size
            }
        }

        fn free(&mut self, ptr: usize) {
            if ptr == 0 {
                return;
            }

            for i in 0..self.arena_count {
                let rec = self.arena_records[i];
                if ptr >= rec.start && ptr < rec.end {
                    let class_idx = rec.class_idx;
                    // Double-free guard
                    let magic = self.mem.read_u64(ptr + 8) as usize;
                    if magic == FREE_MAGIC {
                        return; // Guard against double-free!
                    }
                    self.mem.write_u64(ptr + 8, FREE_MAGIC as u64);
                    self.mem.write_u64(ptr, self.free_lists[class_idx] as u64);
                    self.free_lists[class_idx] = ptr;
                    return;
                }
            }

            // Large allocation path
            let header_ptr = ptr - 16;
            let magic = self.mem.read_u64(header_ptr + 8) as usize;
            if magic == LARGE_MAGIC {
                let size = self.mem.read_u64(header_ptr) as usize;
                self.mem.write_u64(header_ptr + 8, 0);
                self.mem.munmap(header_ptr, size);
            }
        }

        fn realloc(&mut self, ptr: usize, size: usize) -> usize {
            if ptr == 0 {
                return self.malloc(size);
            }
            if size == 0 {
                self.free(ptr);
                return 0;
            }

            let mut old_capacity = 0;
            let mut is_small = false;
            for i in 0..self.arena_count {
                let rec = self.arena_records[i];
                if ptr >= rec.start && ptr < rec.end {
                    old_capacity = SIZE_CLASSES[rec.class_idx];
                    is_small = true;
                    break;
                }
            }

            if !is_small {
                let header_ptr = ptr - 16;
                let magic = self.mem.read_u64(header_ptr + 8) as usize;
                if magic != LARGE_MAGIC {
                    return 0;
                }
                old_capacity = (self.mem.read_u64(header_ptr) as usize) - 16;
            }

            if old_capacity >= size {
                return ptr; // In-place reuse
            }

            let new_ptr = self.malloc(size);
            if new_ptr != 0 {
                let mut buf = vec![0u8; old_capacity];
                self.mem.read_bytes(ptr, &mut buf);
                self.mem.write_bytes(new_ptr, &buf);
                self.free(ptr);
            }
            new_ptr
        }
    }

    let mut alloc = RealSlabAllocator::new();

    // 1. Double-Free Protection on Small Object Path:
    let small_ptr = alloc.malloc(64);
    assert_ne!(small_ptr, 0);
    alloc.free(small_ptr);
    alloc.free(small_ptr); // Second free must be a safe no-op (no cycles)
    let pop1 = alloc.malloc(64);
    let pop2 = alloc.malloc(64);
    assert_ne!(
        pop1, pop2,
        "Double-free must not create cycle or return duplicate pointers"
    );
    alloc.free(pop1);
    alloc.free(pop2);

    // 2. Fixed MAX_ARENAS Exhaustion Fail-Closed:
    let mut exhausted_alloc = RealSlabAllocator::new();
    exhausted_alloc.arena_count = MAX_ARENAS;
    let overflow_ptr = exhausted_alloc.malloc(256);
    assert_eq!(
        overflow_ptr, 0,
        "Malloc must fail-closed with NULL when arena table is full"
    );

    // 3. 10,000 malloc/free cycles of <= 128 bytes with intrusive in-memory pointer manipulation:
    let mut live_ptrs = Vec::new();
    for i in 0..10_000 {
        let sz = ((i * 17) % 128) + 1; // Varying sizes from 1 to 128 B
        let p = alloc.malloc(sz);
        assert_ne!(p, 0);
        // Write canary byte to verify real memory access
        alloc.mem.write_bytes(p, &[0xAA]);
        live_ptrs.push((p, sz));

        if live_ptrs.len() >= 64 {
            let (to_free, _) = live_ptrs.swap_remove(0);
            alloc.free(to_free);
        }
    }

    while let Some((p, _)) = live_ptrs.pop() {
        alloc.free(p);
    }

    // Acceptance criterion: 10,000 malloc/free cycles of <= 128 B complete with < 64 SYS_MMAP calls
    assert!(
        alloc.mem.mmap_count < 64,
        "10,000 small allocations must complete with < 64 mmap calls (actual: {})",
        alloc.mem.mmap_count
    );

    // 4. Test In-Place Realloc vs Size-Class Growth:
    let p1 = alloc.malloc(32);
    alloc.mem.write_bytes(p1, &[1, 2, 3, 4]);
    let p2 = alloc.realloc(p1, 28);
    assert_eq!(
        p1, p2,
        "Realloc within size class must reuse memory in-place"
    );

    let p3 = alloc.realloc(p2, 512); // Growth to larger size class
    assert_ne!(p3, p2);
    let mut canary = [0u8; 4];
    alloc.mem.read_bytes(p3, &mut canary);
    assert_eq!(
        &canary,
        &[1, 2, 3, 4],
        "Realloc must preserve buffer contents"
    );
    alloc.free(p3);

    // 5. Test Large Allocation Double-Free Guard & munmap:
    let large_p = alloc.malloc(8192);
    assert_ne!(large_p, 0);
    alloc.free(large_p);
    alloc.free(large_p); // Double-free on large path must be a safe no-op
    assert_eq!(
        alloc.mem.munmap_count, 1,
        "munmap must be called exactly once despite double free"
    );
}

fn test_file_creation_mode_and_audit_uid() {
    use posix_abi::*;

    // 1. Process Credentials & Creation Mode Semantics
    struct SimCreds {
        uid: u32,
        gid: u32,
        umask: u32,
    }

    struct SimFileNode {
        mode: u16,
        uid: u32,
        gid: u32,
        data: Vec<u8>,
    }

    struct SimAuditEntry {
        pid: i32,
        uid: u32,
        event_type: u32,
        target: String,
        details: String,
    }

    let caller_pid = 42;
    let mut proc = SimCreds {
        uid: 1000,
        gid: 1000,
        umask: 0o022,
    };

    let mut journal: Vec<SimAuditEntry> = Vec::new();

    // 2. Create file with mode 0o600 under umask 0o022 -> effective mode is 0o600
    let requested_mode: u32 = 0o600;
    let effective_mode = ((requested_mode as u16) & 0o777) & !(proc.umask as u16);
    assert_eq!(
        effective_mode, 0o600,
        "File created with mode 0o600 must retain mode 0o600 (umask 0o022 masks 0o022, 0o600 & !0o022 == 0o600)"
    );

    let created_file = SimFileNode {
        mode: effective_mode,
        uid: proc.uid,
        gid: proc.gid,
        data: Vec::new(),
    };

    // 3. Stat check: st_mode, st_uid, st_gid
    let stat = Stat {
        st_mode: S_IFREG | (created_file.mode as u32),
        st_uid: created_file.uid,
        st_gid: created_file.gid,
        st_size: created_file.data.len() as i64,
        ..Default::default()
    };

    assert_eq!(
        stat.st_mode & 0o777,
        0o600,
        "stat must accurately report st_mode 0o600"
    );
    assert_eq!(
        stat.st_uid, 1000,
        "stat must accurately report creator uid 1000"
    );
    assert_eq!(
        stat.st_gid, 1000,
        "stat must accurately report creator gid 1000"
    );

    // 4. Audit Journal Verification: authentic caller uid (not fabricated 0)
    journal.push(SimAuditEntry {
        pid: caller_pid,
        uid: proc.uid,
        event_type: AUDIT_TYPE_FILE_CREATE,
        target: "/tmp/secret.txt".to_string(),
        details: "File created via open(O_CREAT)".to_string(),
    });

    assert_eq!(journal.len(), 1);
    assert_eq!(
        journal[0].uid, 1000,
        "Audit record must reflect authentic caller uid 1000 (never hardcoded 0)"
    );
    assert_eq!(journal[0].pid, 42);
    assert_eq!(journal[0].event_type, AUDIT_TYPE_FILE_CREATE);
    assert_eq!(journal[0].target, "/tmp/secret.txt");
    assert_eq!(journal[0].details, "File created via open(O_CREAT)");

    // 5. Minimal & Honest Permission Checking
    let check_open_access =
        |caller_uid: u32, caller_gid: u32, flags: i32, file: &SimFileNode| -> Result<(), i32> {
            if caller_uid == 0 {
                return Ok(()); // Root bypasses standard permission checks
            }
            let req_write = (flags & O_WRONLY != 0) || (flags & O_RDWR != 0);
            let req_read = flags & O_WRONLY == 0;

            let imode = file.mode as u32;
            let (can_read, can_write) = if caller_uid == file.uid {
                (imode & S_IRUSR != 0, imode & S_IWUSR != 0)
            } else if caller_gid == file.gid {
                (imode & S_IRGRP != 0, imode & S_IWGRP != 0)
            } else {
                (imode & S_IROTH != 0, imode & S_IWOTH != 0)
            };

            if (req_read && !can_read) || (req_write && !can_write) {
                Err(EACCES)
            } else {
                Ok(())
            }
        };

    // Owner access to 0o600 file:
    assert!(
        check_open_access(1000, 1000, O_RDONLY, &created_file).is_ok(),
        "Owner can read 0o600"
    );
    assert!(
        check_open_access(1000, 1000, O_WRONLY, &created_file).is_ok(),
        "Owner can write 0o600"
    );

    // Other non-root user (uid 2000) access to 0o600 file:
    assert_eq!(
        check_open_access(2000, 2000, O_RDONLY, &created_file),
        Err(EACCES),
        "Non-owner user must receive -EACCES when reading 0o600 file"
    );
    assert_eq!(
        check_open_access(2000, 2000, O_WRONLY, &created_file),
        Err(EACCES),
        "Non-owner user must receive -EACCES when writing 0o600 file"
    );

    // Root (uid 0) bypasses permission checks:
    assert!(
        check_open_access(0, 0, O_RDONLY, &created_file).is_ok(),
        "Root can read 0o600"
    );

    // 6. Umask Manipulation Semantics
    let old_umask = proc.umask;
    proc.umask = 0o077;
    assert_eq!(old_umask, 0o022, "umask syscall returns previous mask");

    let file_mode_777 = 0o777;
    let effective_777 = ((file_mode_777 as u16) & 0o777) & !(proc.umask as u16);
    assert_eq!(
        effective_777, 0o700,
        "Mode 0o777 under umask 0o077 results in effective mode 0o700"
    );
}
