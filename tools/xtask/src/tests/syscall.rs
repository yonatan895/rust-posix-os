//! Syscall dispatcher, user pointer validation, and EFAULT hammer test suite.

use super::harness::TestRunner;
use posix_abi::*;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Instant;

/// Registers syscall dispatcher and user pointer validation tests with the runner.
pub fn register_tests(runner: &mut TestRunner) {
    runner.run_test(
        "syscall",
        "User Pointer Validation and EFAULT Hammer",
        test_user_pointer_validation_efault_hammer,
    );
    runner.run_test(
        "syscall",
        "Syscall Dispatcher Simulation and Stateful Routing",
        test_syscall_microbench,
    );
}

/// Tests adversarial pointer boundaries: null pointers, unmapped pages, page boundaries, and kernel address leakage.
fn test_user_pointer_validation_efault_hammer() {
    const USER_SPACE_END: usize = 0x0000_8000_0000_0000;
    const PAGE_SIZE: usize = 4096;
    const PAGE_MASK: usize = 0xFFF;
    let kernel_addr = 0xFFFF_8000_0000_0000usize;

    struct SimAddressSpace {
        pages: BTreeMap<usize, (bool, [u8; PAGE_SIZE])>, // (writable, data)
    }

    impl SimAddressSpace {
        fn new() -> Self { Self { pages: BTreeMap::new() } }
        fn map(&mut self, addr: usize, w: bool) { self.pages.insert(addr & !PAGE_MASK, (w, [0u8; PAGE_SIZE])); }
        fn val_range(&self, addr: usize, len: usize, write: bool) -> Result<(), i32> {
            if len == 0 { return Ok(()); }
            if addr == 0 { return Err(EFAULT); }
            let end = addr.checked_add(len).ok_or(EFAULT)?;
            if end > USER_SPACE_END { return Err(EFAULT); }
            let mut p = addr & !PAGE_MASK;
            while p < end {
                let info = self.pages.get(&p).ok_or(EFAULT)?;
                if write && !info.0 { return Err(EFAULT); }
                p = p.checked_add(PAGE_SIZE).ok_or(EFAULT)?;
            }
            Ok(())
        }
        fn copy_cstr(&self, addr: usize, buf: &mut [u8]) -> Result<usize, i32> {
            if addr == 0 || addr >= USER_SPACE_END { return Err(EFAULT); }
            for (i, b) in buf.iter_mut().enumerate() {
                let cur = addr.checked_add(i).ok_or(EFAULT)?;
                if cur >= USER_SPACE_END { return Err(EFAULT); }
                let info = self.pages.get(&(cur & !PAGE_MASK)).ok_or(EFAULT)?;
                *b = info.1[cur & PAGE_MASK];
                if *b == 0 { return Ok(i); }
            }
            Err(ENAMETOOLONG)
        }
    }

    let mut aspace = SimAddressSpace::new();
    aspace.map(0x0040_0000, true);
    aspace.map(0x0040_1000, false);

    // 1. Pointer checks
    let val_ptr = |addr: usize, len: usize, w: bool| -> Result<(), i32> {
        if addr == 0 { return Err(EFAULT); }
        let end = addr.checked_add(len).ok_or(EFAULT)?;
        if end > USER_SPACE_END { return Err(EFAULT); }
        aspace.val_range(addr, len, w)
    };

    assert_eq!(val_ptr(0, 128, false), Err(EFAULT));
    assert!(val_ptr(0x0040_0000, 128, true).is_ok());
    assert_eq!(val_ptr(0x0040_2000, 128, false), Err(EFAULT));
    assert_eq!(val_ptr(kernel_addr, 128, false), Err(EFAULT));
    assert_eq!(val_ptr(usize::MAX - 4, 128, false), Err(EFAULT));

    // 2. Slice checks
    assert!(aspace.val_range(0x0040_0000, 100, true).is_ok());
    assert!(aspace.val_range(0x0040_1000, 100, false).is_ok());
    assert_eq!(aspace.val_range(0x0040_1000, 100, true), Err(EFAULT));
    assert!(aspace.val_range(0x0040_0FF0, 32, false).is_ok());
    assert_eq!(aspace.val_range(0x0040_1FF0, 32, false), Err(EFAULT));
    assert_eq!(aspace.val_range(0x0040_2000, 64, false), Err(EFAULT));

    // 3. String scanning checks
    let mut str_buf = [0u8; 64];
    aspace.pages.get_mut(&0x0040_0000).unwrap().1[0..5].copy_from_slice(b"test\0");
    assert_eq!(aspace.copy_cstr(0x0040_0000, &mut str_buf), Ok(4));
    assert_eq!(&str_buf[0..5], b"test\0");
    assert_eq!(aspace.copy_cstr(0, &mut str_buf), Err(EFAULT));
    assert_eq!(aspace.copy_cstr(kernel_addr, &mut str_buf), Err(EFAULT));
    assert_eq!(aspace.copy_cstr(0x0040_2000, &mut str_buf), Err(EFAULT));

    for b in &mut aspace.pages.get_mut(&0x0040_1000).unwrap().1[4090..4096] { *b = b'X'; }
    assert_eq!(aspace.copy_cstr(0x0040_1000 + 4090, &mut str_buf), Err(EFAULT));
    let mut tiny_buf = [0u8; 4];
    assert_eq!(aspace.copy_cstr(0x0040_1000 + 4090, &mut tiny_buf), Err(ENAMETOOLONG));
}

/// Tests syscall dispatcher fast-path routing and multi-process state retrieval.
fn test_syscall_microbench() {
    struct SimProcess {
        pid: i32,
        ppid: i32,
    }

    let procs = [
        SimProcess { pid: 1, ppid: 0 },
        SimProcess { pid: 42, ppid: 1 },
        SimProcess { pid: 100, ppid: 42 },
        SimProcess {
            pid: 1337,
            ppid: 100,
        },
    ];

    static CURRENT_PID_SLOT: AtomicI32 = AtomicI32::new(1);

    let sim_dispatch = |syscall_nr: usize, current_proc: &SimProcess| -> isize {
        match syscall_nr {
            posix_abi::SYS_GETPID => current_proc.pid as isize,
            posix_abi::SYS_GETPPID => current_proc.ppid as isize,
            399 => -38, // -ENOSYS
            _ => -1,
        }
    };

    // 1. Verify ENOSYS routing
    assert_eq!(
        sim_dispatch(399, &procs[0]),
        -38,
        "Unimplemented syscall must return -ENOSYS"
    );

    // 2. Verify accurate PID/PPID retrieval across distinct process contexts
    for p in &procs {
        CURRENT_PID_SLOT.store(p.pid, Ordering::Relaxed);
        assert_eq!(sim_dispatch(posix_abi::SYS_GETPID, p), p.pid as isize);
        assert_eq!(sim_dispatch(posix_abi::SYS_GETPPID, p), p.ppid as isize);
    }

    // 3. Execute 100,000 iterations of simulated fast-dispatch across round-robin processes
    const BENCH_ROUNDS: usize = 100_000;
    let start = Instant::now();
    let mut checksum: isize = 0;
    for i in 0..BENCH_ROUNDS {
        let proc = &procs[i % procs.len()];
        let pid = std::hint::black_box(sim_dispatch(posix_abi::SYS_GETPID, proc));
        checksum += pid;
    }
    let duration = start.elapsed();

    let expected_checksum: isize = (0..BENCH_ROUNDS)
        .map(|i| procs[i % procs.len()].pid as isize)
        .sum();
    assert_eq!(
        checksum, expected_checksum,
        "100k iteration checksum must match expected multi-PID sum"
    );

    let total_nanos = duration.as_nanos() as f64;
    let avg_ns = total_nanos / (BENCH_ROUNDS as f64);
    assert!(avg_ns > 0.0, "Measured elapsed time must be non-zero");
}
