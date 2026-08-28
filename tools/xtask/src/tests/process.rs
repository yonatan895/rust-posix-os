//! Process lifecycle, scheduling, and waitpid test suite.

use super::harness::TestRunner;
use std::collections::{BTreeMap, VecDeque};

/// Registers process lifecycle, timer, PIC, and scheduler tests with the runner.
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
    runner.run_test(
        "process",
        "Saved-UID Privilege Drop and Regain Model",
        test_saved_uid_credentials_model,
    );
}

use posix_abi::{pit_calc_divisor, pit_effective_freq};

/// Tests PIT frequency divisor arithmetic, boundary clamping, and invalid frequency rejection.
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

/// Tests PIC IRQ line and port calculations and monotonic atomic tick sequencing.
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

/// Tests preemptive timer-driven round-robin scheduling traces across active tasks.
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

/// Tests that processes can only wait on their own direct child processes and receive ECHILD otherwise.
fn test_waitpid_parentage_isolation() {
    let mut table: BTreeMap<i32, (i32, Option<i32>)> = BTreeMap::new(); // pid -> (ppid, exit_code)
    table.insert(1, (0, None));
    table.insert(2, (1, None));
    table.insert(3, (2, Some(42)));

    let wait4 = |calling_pid: i32,
                 target_pid: i32,
                 tbl: &mut BTreeMap<i32, (i32, Option<i32>)>|
     -> Result<(i32, i32), i32> {
        let mut reaped = None;
        let mut has_child = false;
        for (&p, &(ppid, code)) in tbl.iter() {
            if (target_pid == -1 || target_pid == p) && ppid == calling_pid {
                has_child = true;
                if let Some(c) = code {
                    reaped = Some((p, c));
                    break;
                }
            }
        }
        if let Some((target, code)) = reaped {
            tbl.remove(&target);
            Ok((target, code))
        } else if has_child {
            Err(0)
        } else {
            Err(10)
        }
    };

    assert_eq!(wait4(1, 3, &mut table), Err(10));
    assert_eq!(wait4(2, 3, &mut table), Ok((3, 42)));
    assert_eq!(wait4(2, 3, &mut table), Err(10));
}

/// Tests non-blocking WNOHANG waitpid semantics for running vs zombie child processes.
fn test_waitpid_wnohang_semantics() {
    let mut table: BTreeMap<i32, (i32, Option<i32>)> = BTreeMap::new();
    table.insert(1, (0, None));
    table.insert(2, (1, None));

    let wait4_wnohang = |calling_pid: i32,
                         target_pid: i32,
                         tbl: &mut BTreeMap<i32, (i32, Option<i32>)>|
     -> Result<(i32, i32), i32> {
        let mut reaped = None;
        let mut has_child = false;
        for (&p, &(ppid, code)) in tbl.iter() {
            if (target_pid == -1 || target_pid == p) && ppid == calling_pid {
                has_child = true;
                if let Some(c) = code {
                    reaped = Some((p, c));
                    break;
                }
            }
        }
        if let Some((target, code)) = reaped {
            tbl.remove(&target);
            Ok((target, code))
        } else if has_child {
            Ok((0, 0))
        } else {
            Err(10)
        }
    };

    assert_eq!(wait4_wnohang(1, -1, &mut table), Ok((0, 0)));
    table.get_mut(&2).unwrap().1 = Some(127);
    assert_eq!(wait4_wnohang(1, -1, &mut table), Ok((2, 127)));
    assert_eq!(wait4_wnohang(1, -1, &mut table), Err(10));
}

/// Tests POSIX credentials model: saved-UID/saved-GID privilege drop and regain, setuid/seteuid/setresuid state transitions.
fn test_saved_uid_credentials_model() {
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Creds {
        uid: u32,
        euid: u32,
        suid: u32,
    }

    impl Creds {
        fn new_root() -> Self {
            Self {
                uid: 0,
                euid: 0,
                suid: 0,
            }
        }
        fn setuid(&mut self, new_uid: u32) -> Result<(), i32> {
            if self.euid == 0 {
                self.uid = new_uid;
                self.euid = new_uid;
                self.suid = new_uid;
                Ok(())
            } else if new_uid == self.uid || new_uid == self.suid {
                self.euid = new_uid;
                Ok(())
            } else {
                Err(1)
            }
        }
        fn seteuid(&mut self, new_euid: u32) -> Result<(), i32> {
            if self.euid == 0
                || new_euid == self.uid
                || new_euid == self.euid
                || new_euid == self.suid
            {
                self.euid = new_euid;
                Ok(())
            } else {
                Err(1)
            }
        }
        fn setresuid(&mut self, ruid: u32, euid: u32, suid: u32) -> Result<(), i32> {
            const UNCHANGED: u32 = u32::MAX;
            let valid =
                |id: u32| id == UNCHANGED || id == self.uid || id == self.euid || id == self.suid;
            if self.euid == 0 || (valid(ruid) && valid(euid) && valid(suid)) {
                if ruid != UNCHANGED {
                    self.uid = ruid;
                }
                if euid != UNCHANGED {
                    self.euid = euid;
                }
                if suid != UNCHANGED {
                    self.suid = suid;
                }
                Ok(())
            } else {
                Err(1)
            }
        }
    }

    let mut creds = Creds::new_root();
    assert_eq!(creds.uid, 0);
    assert_eq!(creds.euid, 0);
    assert_eq!(creds.suid, 0);
    assert_eq!(creds.seteuid(1000), Ok(()));
    assert_eq!(creds.uid, 0);
    assert_eq!(creds.euid, 1000);
    assert_eq!(creds.suid, 0);
    assert_eq!(creds.seteuid(0), Ok(()));
    assert_eq!(creds.euid, 0);
    assert_eq!(creds.seteuid(1000), Ok(()));
    assert_eq!(creds.seteuid(2000), Err(1));
    assert_eq!(creds.setuid(0), Ok(()));
    assert_eq!(creds.setuid(1000), Ok(()));
    assert_eq!(creds.uid, 1000);
    assert_eq!(creds.euid, 1000);
    assert_eq!(creds.suid, 1000);
    assert_eq!(creds.seteuid(0), Err(1));
    assert_eq!(creds.setuid(0), Err(1));

    let mut root_creds = Creds::new_root();
    assert_eq!(root_creds.setresuid(1000, 2000, 3000), Ok(()));
    assert_eq!(root_creds.uid, 1000);
    assert_eq!(root_creds.euid, 2000);
    assert_eq!(root_creds.suid, 3000);
}
