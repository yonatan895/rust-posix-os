//! Automated documentation coverage verification command (`cargo xtask doc`).

use std::process::Command;

/// Runs documentation builds across all workspace crates ensuring 100% doc coverage without warnings.
pub fn run_doc() {
    println!("[xtask] Verifying documentation coverage (-D missing-docs -D warnings)...");

    // 1. Host Crates: posix-abi, xtask
    println!("[xtask] Documenting host crates (posix-abi, xtask)...");
    let status = Command::new("cargo")
        .args(["doc", "--no-deps", "-p", "posix-abi", "-p", "xtask"])
        .env("RUSTDOCFLAGS", "-D missing-docs -D warnings")
        .status()
        .expect("Failed to execute cargo doc on host crates");
    if !status.success() {
        eprintln!("[xtask] Documentation build failed for host crates.");
        std::process::exit(1);
    }

    // 2. Kernel Crate: kernel
    println!("[xtask] Documenting kernel crate...");
    let status = Command::new("cargo")
        .args([
            "doc",
            "--no-deps",
            "-p",
            "kernel",
            "--target",
            "x86_64-unknown-none",
            "-Zbuild-std=core,compiler_builtins,alloc",
            "-Zbuild-std-features=compiler-builtins-mem",
        ])
        .env("RUSTDOCFLAGS", "-D missing-docs -D warnings")
        .env(
            "RUSTFLAGS",
            "-C relocation-model=static -C code-model=kernel",
        )
        .status()
        .expect("Failed to execute cargo doc on kernel");
    if !status.success() {
        eprintln!("[xtask] Documentation build failed for kernel.");
        std::process::exit(1);
    }

    // 3. Userland Crates: libc, shell, init, coreutils
    println!("[xtask] Documenting userland crates (libc, shell, init, coreutils)...");
    let status = Command::new("cargo")
        .args([
            "doc",
            "--no-deps",
            "-p",
            "libc",
            "-p",
            "shell",
            "-p",
            "init",
            "-p",
            "coreutils",
            "--target",
            "x86_64-unknown-none",
            "-Zbuild-std=core,compiler_builtins,alloc",
            "-Zbuild-std-features=compiler-builtins-mem",
        ])
        .env("RUSTDOCFLAGS", "-D missing-docs -D warnings")
        .env("RUSTFLAGS", "-C relocation-model=static")
        .status()
        .expect("Failed to execute cargo doc on userland crates");
    if !status.success() {
        eprintln!("[xtask] Documentation build failed for userland crates.");
        std::process::exit(1);
    }

    println!("[xtask] 100% Documentation coverage verified successfully across all crates!");
}
