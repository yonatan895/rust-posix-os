//! Cargo xtask automation for Rust POSIX OS.

mod bench;
mod build;
mod doc;
mod initramfs;
mod qemu;
mod tests;

use std::env;

/// CLI entry point for cargo xtask task runner.
fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(|s| s.as_str()).unwrap_or("run");

    match command {
        "bench" => {
            build::build_all();
            initramfs::create_initramfs();
            bench::run_bench();
        }
        "build" => {
            build::build_all();
            initramfs::create_initramfs();
            qemu::setup_iso_root();
        }
        "doc" => {
            let open = args.iter().any(|arg| arg == "--open");
            doc::run_doc(open);
        }
        "initramfs" => initramfs::create_initramfs(),
        "run" => {
            build::build_all();
            initramfs::create_initramfs();
            qemu::setup_iso_root();
            qemu::run_qemu();
        }
        "test" => {
            build::build_all();
            initramfs::create_initramfs();
            let test_args = if args.len() > 1 { &args[1..] } else { &[] };
            tests::run_tests(test_args);
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!(
                "Usage: cargo xtask [bench|build|doc|initramfs|run|test] [--filter <pattern>]"
            );
            std::process::exit(1);
        }
    }
}
