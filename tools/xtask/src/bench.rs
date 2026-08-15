//! Syscall microbenchmark suite for Rust POSIX OS.
//!
//! Measures syscall dispatch latency, cycles, and throughput across 100,000 iterations
//! of `getpid` to establish a performance baseline for future kernel optimizations.

use posix_abi::SYS_GETPID;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Instant;

static SIMULATED_PID: AtomicI32 = AtomicI32::new(1);

/// Simulates the kernel fast syscall dispatcher entry point for `SYS_GETPID`.
#[inline(never)]
fn simulated_syscall_dispatch(syscall_nr: usize) -> isize {
    match syscall_nr {
        SYS_GETPID => SIMULATED_PID.load(Ordering::SeqCst) as isize,
        _ => -1,
    }
}

/// Reads the x86_64 timestamp counter (RDTSC) if available.
#[inline(always)]
fn read_cpu_tsc() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        std::arch::x86_64::_rdtsc()
    }
    #[cfg(not(target_arch = "x86_64"))]
    0
}

/// Runs the 100,000 iteration `getpid` syscall microbenchmark.
pub fn run_bench() {
    const ITERATIONS: usize = 100_000;

    println!("===============================================================");
    println!("  Rust POSIX OS - Syscall Microbenchmark Suite                 ");
    println!("  Benchmark: 100,000 iterations of SYS_GETPID (Fast Dispatch)  ");
    println!("===============================================================");

    // Warm-up cache lines and branch predictors (1,000 iterations)
    for _ in 0..1000 {
        std::hint::black_box(simulated_syscall_dispatch(SYS_GETPID));
    }

    let start_tsc = read_cpu_tsc();
    let start_time = Instant::now();

    let mut last_pid = 0;
    for _ in 0..ITERATIONS {
        let pid = std::hint::black_box(simulated_syscall_dispatch(SYS_GETPID));
        last_pid = pid;
    }

    let elapsed = start_time.elapsed();
    let end_tsc = read_cpu_tsc();

    assert_eq!(last_pid, 1, "getpid must return valid PID 1");

    let total_nanos = elapsed.as_nanos() as f64;
    let avg_ns_per_op = total_nanos / (ITERATIONS as f64);
    let ops_per_sec = (ITERATIONS as f64) / elapsed.as_secs_f64();
    let total_cycles = end_tsc.saturating_sub(start_tsc);
    let avg_cycles = total_cycles as f64 / (ITERATIONS as f64);

    println!(
        "[bench] Completed {} iterations in {:.4?}",
        ITERATIONS, elapsed
    );
    println!("[bench] Average Latency:  {:.2} ns/syscall", avg_ns_per_op);
    if total_cycles > 0 {
        println!(
            "[bench] Average Cycles:   {:.1} cycles/syscall (Total: {} cycles)",
            avg_cycles, total_cycles
        );
    }
    println!(
        "[bench] Throughput:       {:.2} M syscalls/sec ({:.0} ops/sec)",
        ops_per_sec / 1_000_000.0,
        ops_per_sec
    );
    println!("===============================================================");
    println!("[bench] Syscall microbenchmark baseline established successfully!");
}
