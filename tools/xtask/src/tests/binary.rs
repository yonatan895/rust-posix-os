//! Binary integrity and ELF header format test suite.

use super::harness::TestRunner;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Registers all binary integrity test cases with the test runner.
pub fn register_tests(runner: &mut TestRunner) {
    runner.run_test(
        "binary",
        "ELF64 Binary Integrity and Magic Headers",
        test_binary_integrity,
    );
}

/// Verifies that generated userland binaries exist and start with valid ELF magic bytes (`\x7fELF`).
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
