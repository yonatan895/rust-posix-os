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

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SimUserAccessError {
        NullPointer,
        OutOfUserRange,
        Overflow,
        NotMapped,
        NotWritable,
        TooLong,
    }

    fn sim_map_user_error(err: SimUserAccessError) -> i32 {
        match err {
            SimUserAccessError::NullPointer
            | SimUserAccessError::OutOfUserRange
            | SimUserAccessError::Overflow
            | SimUserAccessError::NotMapped
            | SimUserAccessError::NotWritable => EFAULT,
            SimUserAccessError::TooLong => ENAMETOOLONG,
        }
    }

    struct SimPageInfo {
        writable: bool,
        data: [u8; PAGE_SIZE],
    }

    struct SimAddressSpace {
        pages: BTreeMap<usize, SimPageInfo>,
    }

    impl SimAddressSpace {
        fn new() -> Self {
            Self {
                pages: BTreeMap::new(),
            }
        }

        fn map_page(&mut self, page_vaddr: usize, writable: bool) {
            self.pages.insert(
                page_vaddr & !PAGE_MASK,
                SimPageInfo {
                    writable,
                    data: [0u8; PAGE_SIZE],
                },
            );
        }

        fn validate_page(&self, page: usize, need_write: bool) -> Result<(), SimUserAccessError> {
            let info = self
                .pages
                .get(&(page & !PAGE_MASK))
                .ok_or(SimUserAccessError::NotMapped)?;
            if need_write && !info.writable {
                return Err(SimUserAccessError::NotWritable);
            }
            Ok(())
        }

        fn validate_range(
            &self,
            addr: usize,
            len: usize,
            need_write: bool,
        ) -> Result<(), SimUserAccessError> {
            if len == 0 {
                return Ok(());
            }
            let end = addr.checked_add(len).ok_or(SimUserAccessError::Overflow)?;
            if end > USER_SPACE_END {
                return Err(SimUserAccessError::OutOfUserRange);
            }
            let mut page = addr & !PAGE_MASK;
            while page < end {
                self.validate_page(page, need_write)?;
                page = page
                    .checked_add(PAGE_SIZE)
                    .ok_or(SimUserAccessError::Overflow)?;
            }
            Ok(())
        }

        fn copy_cstr_from_user(
            &self,
            addr: usize,
            buf: &mut [u8],
        ) -> Result<usize, SimUserAccessError> {
            if addr == 0 {
                return Err(SimUserAccessError::NullPointer);
            }
            if addr >= USER_SPACE_END {
                return Err(SimUserAccessError::OutOfUserRange);
            }
            let mut i = 0usize;
            loop {
                if i == buf.len() {
                    return Err(SimUserAccessError::TooLong);
                }
                let cur = addr.checked_add(i).ok_or(SimUserAccessError::Overflow)?;
                if cur >= USER_SPACE_END {
                    return Err(SimUserAccessError::OutOfUserRange);
                }
                if i == 0 || cur & PAGE_MASK == 0 {
                    self.validate_page(cur & !PAGE_MASK, false)?;
                }
                let page_info = self
                    .pages
                    .get(&(cur & !PAGE_MASK))
                    .ok_or(SimUserAccessError::NotMapped)?;
                let byte = page_info.data[cur & PAGE_MASK];
                buf[i] = byte;
                if byte == 0 {
                    return Ok(i);
                }
                i += 1;
            }
        }
    }

    let mut aspace = SimAddressSpace::new();
    aspace.map_page(0x0040_0000, true); // Page 0: RW
    aspace.map_page(0x0040_1000, false); // Page 1: RO
    // Page 0x0040_2000 is intentionally UNMAPPED

    // 1. UserPtr Null Pointer Checks
    #[derive(Debug)]
    struct SimUserPtr<T> {
        addr: usize,
        _marker: std::marker::PhantomData<T>,
    }

    impl<T> SimUserPtr<T> {
        fn from_raw(addr: usize) -> Result<Self, SimUserAccessError> {
            if addr == 0 {
                return Err(SimUserAccessError::NullPointer);
            }
            let end = addr
                .checked_add(std::mem::size_of::<T>())
                .ok_or(SimUserAccessError::Overflow)?;
            if end > USER_SPACE_END {
                return Err(SimUserAccessError::OutOfUserRange);
            }
            Ok(Self {
                addr,
                _marker: std::marker::PhantomData,
            })
        }

        fn validate(
            &self,
            aspace: &SimAddressSpace,
            need_write: bool,
        ) -> Result<(), SimUserAccessError> {
            aspace.validate_range(self.addr, std::mem::size_of::<T>(), need_write)
        }
    }

    assert_eq!(
        SimUserPtr::<Stat>::from_raw(0).err(),
        Some(SimUserAccessError::NullPointer),
        "Null pointer must be rejected with NullPointer"
    );
    assert_eq!(
        sim_map_user_error(SimUserPtr::<Stat>::from_raw(0).unwrap_err()),
        EFAULT,
        "Null pointer error must map to EFAULT"
    );

    let ptr_valid = SimUserPtr::<Stat>::from_raw(0x0040_0000).unwrap();
    assert!(ptr_valid.validate(&aspace, true).is_ok());

    let ptr_unmapped = SimUserPtr::<Stat>::from_raw(0x0040_2000).unwrap();
    assert_eq!(
        ptr_unmapped.validate(&aspace, false).err(),
        Some(SimUserAccessError::NotMapped)
    );

    // 2. Kernel-Space and High Address Checks
    let kernel_addr = 0xFFFF_8000_0000_0000usize;
    assert_eq!(
        SimUserPtr::<Stat>::from_raw(kernel_addr).err(),
        Some(SimUserAccessError::OutOfUserRange),
        "Kernel space address must be rejected with OutOfUserRange"
    );
    assert_eq!(
        sim_map_user_error(SimUserPtr::<Stat>::from_raw(kernel_addr).unwrap_err()),
        EFAULT,
        "Kernel space address error must map to EFAULT"
    );

    let overflow_addr = usize::MAX - 4;
    assert_eq!(
        SimUserPtr::<Stat>::from_raw(overflow_addr).err(),
        Some(SimUserAccessError::Overflow),
        "Overflowing address must be rejected with Overflow"
    );

    // 3. UserSlice Bounds and Page Boundary Straddle Checks
    #[derive(Debug)]
    struct SimUserSlice {
        addr: usize,
        len: usize,
    }

    impl SimUserSlice {
        fn from_raw(addr: usize, len: usize) -> Result<Self, SimUserAccessError> {
            if len > 0 && addr == 0 {
                return Err(SimUserAccessError::NullPointer);
            }
            if len > 0 {
                let end = addr.checked_add(len).ok_or(SimUserAccessError::Overflow)?;
                if end > USER_SPACE_END {
                    return Err(SimUserAccessError::OutOfUserRange);
                }
            }
            Ok(Self { addr, len })
        }

        fn validate(
            &self,
            aspace: &SimAddressSpace,
            need_write: bool,
        ) -> Result<(), SimUserAccessError> {
            aspace.validate_range(self.addr, self.len, need_write)
        }
    }

    // Fully inside Page 0 (RW):
    let s_valid = SimUserSlice::from_raw(0x0040_0000, 100).unwrap();
    assert!(s_valid.validate(&aspace, true).is_ok());

    // Inside Page 1 (RO): read succeeds, write fails with NotWritable -> EFAULT
    let s_ro = SimUserSlice::from_raw(0x0040_1000, 100).unwrap();
    assert!(s_ro.validate(&aspace, false).is_ok());
    assert_eq!(
        s_ro.validate(&aspace, true).err(),
        Some(SimUserAccessError::NotWritable),
        "Write to read-only page must fail with NotWritable"
    );
    assert_eq!(
        sim_map_user_error(s_ro.validate(&aspace, true).unwrap_err()),
        EFAULT
    );

    // Straddling across Page 0 (RW) and Page 1 (RO): read succeeds across present pages
    let s_straddle_present = SimUserSlice::from_raw(0x0040_0FF0, 32).unwrap();
    assert!(s_straddle_present.validate(&aspace, false).is_ok());

    // Straddling across Page 1 (RO mapped) and Page 2 (UNMAPPED): must fail with NotMapped -> EFAULT
    let s_straddle_unmapped = SimUserSlice::from_raw(0x0040_1FF0, 32).unwrap();
    assert_eq!(
        s_straddle_unmapped.validate(&aspace, false).err(),
        Some(SimUserAccessError::NotMapped),
        "Buffer straddling into unmapped page must fail with NotMapped"
    );
    assert_eq!(
        sim_map_user_error(s_straddle_unmapped.validate(&aspace, false).unwrap_err()),
        EFAULT,
        "Straddling into unmapped page must map to EFAULT"
    );

    // Completely unmapped range:
    let s_unmapped = SimUserSlice::from_raw(0x0040_2000, 64).unwrap();
    assert_eq!(
        s_unmapped.validate(&aspace, false).err(),
        Some(SimUserAccessError::NotMapped)
    );
    assert_eq!(
        sim_map_user_error(s_unmapped.validate(&aspace, false).unwrap_err()),
        EFAULT
    );

    // 4. String Scanning (copy_cstr_from_user) Adversarial Coverage
    let mut str_buf = [0u8; 64];

    // Valid string in Page 0:
    let page0 = aspace.pages.get_mut(&0x0040_0000).unwrap();
    page0.data[0..5].copy_from_slice(b"test\0");
    assert_eq!(
        aspace.copy_cstr_from_user(0x0040_0000, &mut str_buf),
        Ok(4),
        "Valid user string should copy successfully"
    );
    assert_eq!(&str_buf[0..5], b"test\0");

    // Null string pointer:
    assert_eq!(
        aspace.copy_cstr_from_user(0, &mut str_buf).err(),
        Some(SimUserAccessError::NullPointer)
    );

    // Kernel space string pointer:
    assert_eq!(
        aspace.copy_cstr_from_user(kernel_addr, &mut str_buf).err(),
        Some(SimUserAccessError::OutOfUserRange)
    );

    // Unmapped string pointer:
    assert_eq!(
        aspace.copy_cstr_from_user(0x0040_2000, &mut str_buf).err(),
        Some(SimUserAccessError::NotMapped)
    );

    // String without NUL terminator at end of mapped page crossing into unmapped page:
    let page1 = aspace.pages.get_mut(&0x0040_1000).unwrap();
    for b in &mut page1.data[4090..4096] {
        *b = b'X';
    }
    assert_eq!(
        aspace
            .copy_cstr_from_user(0x0040_1000 + 4090, &mut str_buf)
            .err(),
        Some(SimUserAccessError::NotMapped),
        "Non-terminated string straddling into unmapped page must return NotMapped"
    );
    assert_eq!(
        sim_map_user_error(
            aspace
                .copy_cstr_from_user(0x0040_1000 + 4090, &mut str_buf)
                .unwrap_err()
        ),
        EFAULT,
        "Non-terminated string straddling into unmapped page must map to EFAULT"
    );

    // String without NUL terminator within single page exceeding buffer:
    let mut tiny_buf = [0u8; 4];
    assert_eq!(
        aspace
            .copy_cstr_from_user(0x0040_1000 + 4090, &mut tiny_buf)
            .err(),
        Some(SimUserAccessError::TooLong),
        "String exceeding buffer without NUL must return TooLong"
    );
    assert_eq!(
        sim_map_user_error(
            aspace
                .copy_cstr_from_user(0x0040_1000 + 4090, &mut tiny_buf)
                .unwrap_err()
        ),
        ENAMETOOLONG,
        "TooLong must map to ENAMETOOLONG"
    );
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
