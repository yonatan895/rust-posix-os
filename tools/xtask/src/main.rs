//! Cargo xtask automation for Rust POSIX OS.

mod build;
mod initramfs;
mod qemu;
mod test;

use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(|s| s.as_str()).unwrap_or("run");

    match command {
        "build" => {
            build::build_all();
            initramfs::create_initramfs();
            qemu::setup_iso_root();
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
            test::run_tests();
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("Usage: cargo xtask [build|initramfs|run|test]");
            std::process::exit(1);
        }
    }
}
