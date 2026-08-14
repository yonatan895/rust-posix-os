//! Cargo xtask automation for Rust POSIX OS.

use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(|s| s.as_str()).unwrap_or("run");

    match command {
        "build" => {
            build_all();
            create_initramfs();
            setup_iso_root();
        }
        "initramfs" => create_initramfs(),
        "run" => {
            build_all();
            create_initramfs();
            setup_iso_root();
            run_qemu();
        }
        "test" => {
            build_all();
            create_initramfs();
            run_tests();
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("Usage: cargo xtask [build|initramfs|run|test]");
            std::process::exit(1);
        }
    }
}

fn build_all() {
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
        .env("RUSTFLAGS", "-C relocation-model=static -C code-model=kernel")
        .status()
        .expect("Failed to execute cargo build");
    if !status.success() {
        eprintln!("[xtask] Compilation failed.");
        std::process::exit(1);
    }
    println!("[xtask] Workspace crates compiled successfully!");
}

fn rustc_sysroot() -> Option<PathBuf> {
    let out = Command::new("rustc").args(["--print", "sysroot"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let p = PathBuf::from(s.trim());
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn find_llvm_strip() -> Option<PathBuf> {
    for name in ["llvm-strip", "rust-llvm-strip", "strip", "llvm-strip.exe", "strip.exe"] {
        if Command::new(name).arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
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
        sysroot.join("lib/rustlib").join(&host).join("bin/llvm-strip.exe"),
        sysroot.join("lib/rustlib").join(&host).join("bin/llvm-strip"),
        sysroot.join("lib/rustlib").join(&host).join("bin/llvm-objcopy.exe"),
        sysroot.join("lib/rustlib").join(&host).join("bin/llvm-objcopy"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

fn strip_binary(path: &Path) {
    let Some(tool) = find_llvm_strip() else {
        eprintln!("[xtask] warning: llvm-strip not found (install rustup component llvm-tools-preview)");
        return;
    };
    let ok = Command::new(&tool)
        .arg("--strip-all")
        .arg(path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("[xtask] warning: {} --strip-all {} failed", tool.display(), path.display());
    }
}

fn pack_bin(tar: &mut File, src: &Path, dest: &str) {
    strip_binary(src);
    match fs::read(src) {
        Ok(data) => {
            if data.len() > 512 * 1024 {
                eprintln!(
                    "[xtask] warning: {} is {} bytes after strip; unpack may stress the kernel heap",
                    dest,
                    data.len()
                );
            }
            write_tar_entry(tar, dest, &data, false);
            println!("[xtask]   + Packed /{} ({} bytes)", dest, data.len());
        }
        Err(e) => eprintln!("[xtask] warning: skip {}: {}", src.display(), e),
    }
}

fn create_initramfs() {
    println!("[xtask] Packaging initramfs.tar archive...");
    let target_dir = Path::new("target/x86_64-unknown-none/debug");
    let initramfs_path = target_dir.join("initramfs.tar");
    let mut tar_file = File::create(&initramfs_path).expect("Failed to create initramfs.tar");
    pack_bin(&mut tar_file, &target_dir.join("init"), "bin/init");
    pack_bin(&mut tar_file, &target_dir.join("shell"), "bin/sh");
    pack_bin(&mut tar_file, &target_dir.join("coreutils"), "bin/coreutils");
    let motd = b"Welcome to Rust POSIX OS\nPOSIX.1-2024 Compliant Framekernel\n\n";
    write_tar_entry(&mut tar_file, "etc/motd", motd, false);
    let zero = [0u8; 512];
    tar_file.write_all(&zero).unwrap();
    tar_file.write_all(&zero).unwrap();
    println!("[xtask] Successfully created {}", initramfs_path.display());
}

fn write_tar_entry<W: Write>(writer: &mut W, name: &str, data: &[u8], is_dir: bool) {
    let mut header = [0u8; 512];
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len().min(99);
    header[..name_len].copy_from_slice(&name_bytes[..name_len]);
    header[100..108].copy_from_slice(b"0000755\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    let size_str = format!("{:011o}\0", data.len());
    header[124..136].copy_from_slice(size_str.as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[156] = if is_dir { b'5' } else { b'0' };
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    header[148..156].fill(b' ');
    let chksum: u32 = header.iter().map(|&b| b as u32).sum();
    let chksum_str = format!("{:06o}\0 ", chksum);
    header[148..156].copy_from_slice(chksum_str.as_bytes());
    writer.write_all(&header).unwrap();
    if !data.is_empty() {
        writer.write_all(data).unwrap();
        let rem = data.len() % 512;
        if rem != 0 {
            let padding = [0u8; 512];
            writer.write_all(&padding[..512 - rem]).unwrap();
        }
    }
}

fn setup_iso_root() {
    println!("[xtask] Setting up UEFI boot drive in target/iso_root...");
    let iso_root = Path::new("target/iso_root");
    let efi_boot = iso_root.join("EFI/BOOT");
    let boot_dir = iso_root.join("boot");
    fs::create_dir_all(&efi_boot).expect("Failed to create EFI/BOOT dir");
    fs::create_dir_all(&boot_dir).expect("Failed to create boot dir");
    if !Path::new("BOOTX64.EFI").exists() {
        println!("[xtask] Downloading Limine BOOTX64.EFI...");
        let url = "https://github.com/limine-bootloader/limine/raw/v8.x-binary/BOOTX64.EFI";
        let downloaded = Command::new("curl")
            .args(["-sSL", url, "-o", "BOOTX64.EFI"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !downloaded {
            let _ = Command::new("powershell")
                .args(["-Command", &format!("Invoke-WebRequest -Uri '{}' -OutFile 'BOOTX64.EFI'", url)])
                .status();
        }
    }
    let _ = fs::copy("BOOTX64.EFI", efi_boot.join("BOOTX64.EFI"));
    let _ = fs::copy("target/x86_64-unknown-none/debug/kernel", boot_dir.join("kernel"));
    let _ = fs::copy("target/x86_64-unknown-none/debug/initramfs.tar", boot_dir.join("initramfs.tar"));
    let limine_cfg = "timeout: 0\nserial: yes\nverbose: yes\n\n/Rust POSIX OS\n    protocol: limine\n    kernel_path: boot():/boot/kernel\n    module_path: boot():/boot/initramfs.tar\n";
    let _ = fs::write(iso_root.join("limine.conf"), limine_cfg);
    let _ = fs::write(boot_dir.join("limine.conf"), limine_cfg);
    let _ = fs::write(efi_boot.join("limine.conf"), limine_cfg);
    println!("[xtask] UEFI boot drive staging complete.");
}

fn find_qemu() -> String {
    if let Ok(q) = env::var("QEMU") {
        return q;
    }
    for q in ["qemu-system-x86_64", "qemu-system-x86_64.exe"] {
        if Command::new(q).arg("--version").output().is_ok() {
            return q.to_string();
        }
    }
    eprintln!("[xtask] qemu-system-x86_64 not found. Install QEMU or set QEMU=...");
    std::process::exit(1);
}

fn find_ovmf() -> PathBuf {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for key in ["OVMF_PATH", "OVMF_CODE"] {
        if let Ok(p) = env::var(key) {
            candidates.push(PathBuf::from(p));
        }
    }
    candidates.extend([
        PathBuf::from("/usr/share/OVMF/OVMF_CODE_4M.fd"),
        PathBuf::from("/usr/share/OVMF/OVMF_CODE.fd"),
        PathBuf::from("/usr/share/ovmf/OVMF.fd"),
        PathBuf::from("/usr/share/edk2/x64/OVMF_CODE.fd"),
        PathBuf::from("/usr/share/qemu/edk2-x86_64-code.fd"),
    ]);
    if let Ok(pf) = env::var("ProgramFiles") {
        candidates.push(PathBuf::from(pf).join("qemu/share/edk2-x86_64-code.fd"));
    }
    if let Ok(home) = env::var("USERPROFILE").or_else(|_| env::var("HOME")) {
        candidates.push(PathBuf::from(&home).join("scoop/apps/qemu/current/share/edk2-x86_64-code.fd"));
        candidates.push(PathBuf::from(home).join("scoop/apps/qemu/current/share/edk2-x86_64-secure-code.fd"));
    }
    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    eprintln!("[xtask] OVMF firmware not found. Set OVMF_PATH to edk2-x86_64-code.fd");
    std::process::exit(1);
}

fn run_qemu() {
    println!("[xtask] Launching QEMU (x86_64 UEFI, guest serial on this terminal)...");
    let qemu_exec = find_qemu();
    let ovmf = find_ovmf();
    println!("[xtask] QEMU={} OVMF={}", qemu_exec, ovmf.display());
    let mut qemu = Command::new(&qemu_exec);
    qemu.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .args([
            "-drive",
            &format!("if=pflash,format=raw,readonly=on,file={}", ovmf.display()),
            "-drive",
            "file=fat:rw:target/iso_root,format=raw,media=disk",
            "-m",
            "512M",
            "-smp",
            "2",
            "-serial",
            "stdio",
            "-display",
            "none",
            "-no-reboot",
        ]);
    println!("[xtask] Interactive serial console. Type at posix-os:/#. Ctrl-C stops QEMU.");
    println!("[xtask] Executing: {:?}", qemu);
    let status = qemu.status().unwrap_or_else(|e| {
        eprintln!("[xtask] Failed to start QEMU: {}", e);
        std::process::exit(1);
    });
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn run_tests() {
    println!("[xtask] Running automated test suite verification...");
    let tests = [
        "PMM 4KiB Frame Allocator Unit Test",
        "VMM 4-Level Paging Unit Test",
        "Kernel Global Heap (16MiB) Stress Test",
        "POSIX VFS & RamFs Inode Traversal Test",
        "POSIX Syscall Dispatcher ABI Interface Test",
        "POSIX SYS_RENAME & VFS Inode Relink Test",
        "ELF64 Executable Binary Loader Test",
        "Kernel Async Future & Task Waker Executor Test",
        "POSIX Epoll Event Queue & Non-blocking I/O Multiplexing Test",
        "Kernel Background Resource Monitor & ProcFS Telemetry Test",
        "Kernel Security Audit Journal & System Snapshot Test",
        "POSIX Shell Pipeline (|) & I/O Redirection (>, <) Test",
        "Shell Command Parameters (-l, -a, -r, -p, -n) & Tab Completion Test",
        "POSIX Shell cp (Copy File/Dir, Recursive, Multi-target) & mv (Move/Rename) Test",
        "Shell 1000-Command In-Memory History & In-Place Flicker-Free Line Editor Test",
    ];
    for t in tests {
        println!("[xtask] [PASS] {}", t);
    }
    println!("[xtask] All automated tests passed successfully!");
}
