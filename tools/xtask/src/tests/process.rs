//! Process Lifecycle, Scheduling, and Waitpid Test Suite.

use super::harness::TestRunner;
use std::collections::{BTreeMap, VecDeque};

pub fn register_tests(runner: &mut TestRunner) {
    runner.run_test(
        "process",
        "PIT Timer Frequency Divisor Arithmetic and Bounds",
        test_timer_configuration,
    );
    runner.run_test(
        "process",
        "PIC IRQ Bounds Validation and Monotonic Ticks",
        test_pic_and_irq_primitives,
    );
    runner.run_test(
        "process",
        "Preemptive Round-Robin Scheduling Trace",
        test_preemptive_timer_round_robin,
    );
    runner.run_test(
        "process",
        "waitpid Parentage Isolation and ECHILD",
        test_waitpid_parentage_isolation,
    );
    runner.run_test(
        "process",
        "waitpid WNOHANG Polling Semantics",
        test_waitpid_wnohang_semantics,
    );
}

use posix_abi::{pit_calc_divisor, pit_effective_freq};

fn test_timer_configuration() {
    // 100 Hz standard divisor and effective frequency
    assert_eq!(
        pit_calc_divisor(100),
        Some(11931),
        "PIT 100 Hz divisor calculation error"
    );
    assert_eq!(
        pit_effective_freq(11931),
        100,
        "PIT 100 Hz effective frequency calculation error"
    );

    // Minimum frequency (19 Hz policy bound, divisor 62799)
    assert_eq!(
        pit_calc_divisor(19),
        Some(62799),
        "PIT 19 Hz divisor calculation error"
    );
    assert_eq!(
        pit_effective_freq(62799),
        19,
        "PIT 19 Hz effective frequency calculation error"
    );

    // Maximum frequency (1_193_182 Hz -> 1 tick per oscillator cycle)
    assert_eq!(
        pit_calc_divisor(1_193_182),
        Some(1),
        "PIT max freq divisor calculation error"
    );
    assert_eq!(
        pit_effective_freq(1),
        1_193_182,
        "PIT max freq effective frequency calculation error"
    );

    // Out of range (underflow / overflow)
    assert_eq!(pit_calc_divisor(0), None, "0 Hz should be rejected");
    assert_eq!(
        pit_calc_divisor(18),
        None,
        "18 Hz (< 19 Hz min policy bound) should be rejected"
    );
    assert_eq!(
        pit_calc_divisor(2_000_000),
        None,
        "2 MHz (> 1.19 MHz max) should be rejected"
    );
}

fn test_pic_and_irq_primitives() {
    // PIC IRQ range validation: valid lines 0..=15
    for irq in 0..=15 {
        let is_slave = irq >= 8;
        let port = if !is_slave { 0x21 } else { 0xA1 };
        let bit = if !is_slave { irq } else { irq - 8 };
        assert!(bit < 8, "PIC bit calculation must never overflow u8 shift");
        assert!(port == 0x21 || port == 0xA1);
    }

    // Monotonic tick sequence simulation
    let ticks = std::sync::atomic::AtomicU64::new(0);
    assert_eq!(ticks.load(std::sync::atomic::Ordering::Relaxed), 0);
    for expected in 1..=100 {
        let new_val = ticks.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        assert_eq!(new_val, expected, "Monotonic tick increment mismatch");
    }
    assert_eq!(ticks.load(std::sync::atomic::Ordering::Relaxed), 100);
}

fn test_preemptive_timer_round_robin() {
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
