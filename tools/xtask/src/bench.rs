//! Syscall microbenchmark suite for Rust POSIX OS.
//!
//! Provides accurate performance benchmarking:
//! 1. In-guest hardware fast-syscall benchmark: Executed in PID 1 user space under QEMU
//!    timing 100,000 real `syscall` instruction ring transitions with `rdtsc`.
//! 2. Host fast-syscall dispatcher simulation: Measures dispatcher routing logic overhead.

use posix_abi::SYS_GETPID;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Instant;

/// Simulated process ID for host-side syscall routing benchmarks.
static SIMULATED_PID: AtomicI32 = AtomicI32::new(1);

/// Simulates host-side dispatcher routing logic for `SYS_GETPID`.
#[inline(never)]
fn simulated_dispatcher_routing(syscall_nr: usize) -> isize {
    match syscall_nr {
        SYS_GETPID => SIMULATED_PID.load(Ordering::Relaxed) as isize,
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

/// Runs the 100,000 iteration host dispatcher simulation benchmark.
pub fn run_bench() {
    const ITERATIONS: usize = 100_000;

    println!("===============================================================");
    println!("  Rust POSIX OS - Syscall Microbenchmark Suite                 ");
    println!("  Mode: Host Syscall Dispatcher Logic Simulation               ");
    println!("  Note: In-guest hardware syscalls (rdtsc) run during boot.    ");
    println!("===============================================================");

    // Warm-up cache lines and branch predictors (1,000 iterations)
    for _ in 0..1000 {
        std::hint::black_box(simulated_dispatcher_routing(SYS_GETPID));
    }

    let start_tsc = read_cpu_tsc();
    let start_time = Instant::now();

    let mut last_pid = 0;
    for _ in 0..ITERATIONS {
        let pid = std::hint::black_box(simulated_dispatcher_routing(SYS_GETPID));
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
        "[bench] Completed {} dispatcher iterations in {:.4?}",
        ITERATIONS, elapsed
    );
    println!(
        "[bench] Dispatcher Routing Latency:  {:.2} ns/dispatch",
        avg_ns_per_op
    );
    if total_cycles > 0 {
        println!(
            "[bench] Dispatcher Cycles:           {:.1} cycles/dispatch (Total: {} cycles)",
            avg_cycles, total_cycles
        );
    }
    println!(
        "[bench] Dispatcher Throughput:       {:.2} M dispatches/sec ({:.0} ops/sec)",
        ops_per_sec / 1_000_000.0,
        ops_per_sec
    );
    println!("===============================================================");
    println!("[bench] Syscall dispatcher baseline established successfully!");
}
