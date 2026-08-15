//! Test Harness Infrastructure for Rust POSIX OS Automated Testing.
//!
//! Provides zero-boilerplate test execution, panic safety, execution timing,
//! pattern filtering, and structured diagnostic reporting.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

/// Result of an individual test case execution.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TestResult {
    /// Name of the executed test case.
    pub name: String,
    /// Test suite classification grouping.
    pub suite: String,
    /// Whether the test completed without panic.
    pub passed: bool,
    /// Execution duration of the test.
    pub duration: Duration,
    /// Panic or failure error diagnostics if failed.
    pub error_message: Option<String>,
}

/// Test runner orchestrator that executes test cases with timing and panic protection.
pub struct TestRunner {
    /// Optional pattern filter restricting execution to matching test names.
    filter: Option<String>,
    /// Accumulated records of executed test cases.
    results: Vec<TestResult>,
}

impl TestRunner {
    /// Creates a new test runner with an optional substring filter.
    pub fn new(filter: Option<String>) -> Self {
        Self {
            filter,
            results: Vec::new(),
        }
    }

    /// Executes a single test case within the specified suite name.
    pub fn run_test<F>(&mut self, suite: &str, name: &str, test_fn: F)
    where
        F: FnOnce() + std::panic::UnwindSafe,
    {
        if let Some(ref f) = self.filter {
            let full_name = format!("{}::{}", suite, name);
            if !full_name.to_lowercase().contains(&f.to_lowercase()) {
                return;
            }
        }

        let start = Instant::now();
        let result = catch_unwind(AssertUnwindSafe(test_fn));
        let duration = start.elapsed();

        match result {
            Ok(_) => {
                println!(
                    "  [PASS] {:<58} ({:>7.2}ms)",
                    format!("{}: {}", suite, name),
                    duration.as_secs_f64() * 1000.0
                );
                self.results.push(TestResult {
                    name: name.to_string(),
                    suite: suite.to_string(),
                    passed: true,
                    duration,
                    error_message: None,
                });
            }
            Err(e) => {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "assertion failed or panic occurred".to_string()
                };
                eprintln!(
                    "  [FAIL] {:<58} ({:>7.2}ms)\n         Error: {}",
                    format!("{}: {}", suite, name),
                    duration.as_secs_f64() * 1000.0,
                    msg
                );
                self.results.push(TestResult {
                    name: name.to_string(),
                    suite: suite.to_string(),
                    passed: false,
                    duration,
                    error_message: Some(msg),
                });
            }
        }
    }

    /// Prints a structured summary of test execution and exits with an error code if any test failed.
    pub fn summary(&self) {
        let total = self.results.len();
        let passed = self.results.iter().filter(|r| r.passed).count();
        let failed = total - passed;
        let total_time: Duration = self.results.iter().map(|r| r.duration).sum();

        println!("\n===============================================================");
        println!("  Rust POSIX OS - Automated Test Suite Results Summary        ");
        println!("===============================================================");
        println!("  Total Executed Tests: {}", total);
        println!("  Passed:               {}", passed);
        println!("  Failed:               {}", failed);
        println!("  Total Duration:       {:.2}s", total_time.as_secs_f64());
        println!("===============================================================");

        if failed > 0 {
            eprintln!("\nFailures:");
            for r in self.results.iter().filter(|r| !r.passed) {
                eprintln!(
                    "  - {}: {}\n    Error: {}",
                    r.suite,
                    r.name,
                    r.error_message.as_deref().unwrap_or("unknown error")
                );
            }
            eprintln!("\n[xtask] {} test(s) failed!", failed);
            std::process::exit(1);
        } else if total == 0 {
            println!("\n[xtask] No tests matched the specified filter.");
        } else {
            println!("[xtask] All automated tests passed successfully!");
        }
    }
}
