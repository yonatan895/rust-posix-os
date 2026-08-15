//! UEFI image staging and cross-platform QEMU boot launcher.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Stages the UEFI boot directory tree containing kernel, initramfs, Limine bootloader, and configuration.
pub fn setup_iso_root() {
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
                .args([
                    "-Command",
                    &format!("Invoke-WebRequest -Uri '{}' -OutFile 'BOOTX64.EFI'", url),
                ])
                .status();
        }
    }
    let _ = fs::copy("BOOTX64.EFI", efi_boot.join("BOOTX64.EFI"));
    let _ = fs::copy(
        "target/x86_64-unknown-none/debug/kernel",
        boot_dir.join("kernel"),
    );
    let _ = fs::copy(
        "target/x86_64-unknown-none/debug/initramfs.tar",
        boot_dir.join("initramfs.tar"),
    );
    let limine_cfg = "timeout: 0\nserial: yes\nverbose: yes\n\n/Rust POSIX OS\n    protocol: limine\n    kernel_path: boot():/boot/kernel\n    module_path: boot():/boot/initramfs.tar\n";
    let _ = fs::write(iso_root.join("limine.conf"), limine_cfg);
    let _ = fs::write(boot_dir.join("limine.conf"), limine_cfg);
    let _ = fs::write(efi_boot.join("limine.conf"), limine_cfg);
    println!("[xtask] UEFI boot drive staging complete.");
}

/// Discovers the path or binary name for `qemu-system-x86_64`.
pub fn find_qemu() -> String {
    if let Ok(q) = env::var("QEMU") {
        return q;
    }
    for q in ["qemu-system-x86_64", "qemu-system-x86_64.exe"] {
        if Command::new(q).arg("--version").output().is_ok() {
            return q.to_string();
        }
    }
    if let Ok(home) = env::var("USERPROFILE").or_else(|_| env::var("HOME")) {
        let qemu_dir = PathBuf::from(&home).join("scoop/apps/qemu");
        if let Ok(entries) = fs::read_dir(qemu_dir) {
            for entry in entries.flatten() {
                let exe = entry.path().join("qemu-system-x86_64.exe");
                if exe.exists() {
                    return exe.to_string_lossy().to_string();
                }
            }
        }
    }
    eprintln!("[xtask] qemu-system-x86_64 not found. Install QEMU or set QEMU=...");
    std::process::exit(1);
}

/// Locates the OVMF UEFI firmware image on the host system.
pub fn find_ovmf() -> PathBuf {
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
        candidates
            .push(PathBuf::from(&home).join("scoop/apps/qemu/current/share/edk2-x86_64-code.fd"));
        candidates.push(
            PathBuf::from(&home).join("scoop/apps/qemu/current/share/edk2-x86_64-secure-code.fd"),
        );
        let qemu_dir = PathBuf::from(&home).join("scoop/apps/qemu");
        if let Ok(entries) = fs::read_dir(qemu_dir) {
            for entry in entries.flatten() {
                candidates.push(entry.path().join("share/edk2-x86_64-code.fd"));
            }
        }
    }
    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    eprintln!("[xtask] OVMF firmware not found. Set OVMF_PATH to edk2-x86_64-code.fd");
    std::process::exit(1);
}

/// Spawns QEMU configured with UEFI firmware, SMP, FAT boot drive, and stdio serial console.
pub fn run_qemu() {
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
