//! Workspace Build Automation & Binary Stripping for x86_64 Bare-Metal.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn build_all() {
    println!("[xtask] Compiling workspace crates for bare-metal target x86_64-unknown-none...");
    let status = Command::new("cargo")
        .args([
            "build",
            "--workspace",
            "--exclude",
            "xtask",
            "--target",
            "x86_64-unknown-none",
            "-Zbuild-std=core,compiler_builtins,alloc",
            "-Zbuild-std-features=compiler-builtins-mem",
        ])
        .env(
            "RUSTFLAGS",
            "-C relocation-model=static -C code-model=kernel",
        )
        .status()
        .expect("Failed to execute cargo build");
    if !status.success() {
        eprintln!("[xtask] Compilation failed.");
        std::process::exit(1);
    }
    println!("[xtask] Workspace crates compiled successfully!");
}

pub fn rustc_sysroot() -> Option<PathBuf> {
    let out = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let p = PathBuf::from(s.trim());
    if p.exists() { Some(p) } else { None }
}

pub fn find_llvm_strip() -> Option<PathBuf> {
    for name in [
        "llvm-strip",
        "rust-llvm-strip",
        "strip",
        "llvm-strip.exe",
        "strip.exe",
    ] {
        if Command::new(name)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(PathBuf::from(name));
        }
    }
    let sysroot = rustc_sysroot()?;
    let host = env::var("HOST").ok().or_else(|| {
        Command::new("rustc")
            .args(["-vV"])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8(o.stdout).ok().and_then(|s| {
                    s.lines()
                        .find_map(|l| l.strip_prefix("host: ").map(|h| h.trim().to_string()))
                })
            })
    })?;
    let candidates = [
        sysroot
            .join("lib/rustlib")
            .join(&host)
            .join("bin/llvm-strip.exe"),
        sysroot
            .join("lib/rustlib")
            .join(&host)
            .join("bin/llvm-strip"),
        sysroot
            .join("lib/rustlib")
            .join(&host)
            .join("bin/llvm-objcopy.exe"),
        sysroot
            .join("lib/rustlib")
            .join(&host)
            .join("bin/llvm-objcopy"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

pub fn strip_binary(path: &Path) {
    let Some(tool) = find_llvm_strip() else {
        eprintln!(
            "[xtask] warning: llvm-strip not found (install rustup component llvm-tools-preview)"
        );
        return;
    };
    let ok = Command::new(&tool)
        .arg("--strip-all")
        .arg(path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!(
            "[xtask] warning: {} --strip-all {} failed",
            tool.display(),
            path.display()
        );
    }
}
