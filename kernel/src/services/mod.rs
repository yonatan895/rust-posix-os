//! De-Privileged OS Services - 100% Safe Rust (#![deny(unsafe_code)]).
//!
//! All high-level operating system functionality is implemented in this module
//! in safe Rust, using only the safe abstractions exposed by the OSTD framework.

pub mod vfs;
pub mod tty;
pub mod process;
pub mod scheduler;
pub mod ipc;
pub mod posix;
pub mod monitor;
pub mod audit;

use alloc::sync::Arc;
use alloc::string::ToString;
use posix_abi::O_RDWR;
use crate::ostd::limine::LimineModuleResponse;
use crate::services::vfs::ramfs::{RamFsDir, RamFsFile};
use crate::services::vfs::devfs::{DevNull, DevZero, DevConsole};
use crate::services::vfs::procfs::{ProcDynamicFile, ProcKind};
use crate::services::vfs::tar::unpack_tar_archive;
use crate::services::vfs::{vfs_init, FileHandle};
use crate::services::process::{Process, PROCESS_TABLE};
use crate::services::audit::audit_init;

pub fn services_init(module_resp: *mut LimineModuleResponse) {
    log::info!("[SERVICES] Starting de-privileged OS services initialization...");

    // 1. Build Virtual File System Root hierarchy
    let root_dir = RamFsDir::new();
    let dev_dir = RamFsDir::new();
    let proc_dir = RamFsDir::new();
    let tmp_dir = RamFsDir::new();
    let bin_dir = RamFsDir::new();
    let etc_dir = RamFsDir::new();

    // Populate /dev
    dev_dir.add_child("null", Arc::new(DevNull));
    dev_dir.add_child("zero", Arc::new(DevZero));
    let console = Arc::new(DevConsole);
    dev_dir.add_child("console", console.clone());
    dev_dir.add_child("tty", console.clone());

    // Populate /proc
    proc_dir.add_child("meminfo", ProcDynamicFile::new(ProcKind::Meminfo));
    proc_dir.add_child("stat", ProcDynamicFile::new(ProcKind::Stat));
    proc_dir.add_child("uptime", ProcDynamicFile::new(ProcKind::Uptime));
    proc_dir.add_child("processes", ProcDynamicFile::new(ProcKind::Processes));
    proc_dir.add_child("audit_journal", ProcDynamicFile::new(ProcKind::AuditJournal));
    proc_dir.add_child("snapshots", ProcDynamicFile::new(ProcKind::AuditSnapshots));

    // Populate /etc
    etc_dir.add_child("os-release", RamFsFile::new(b"NAME=\"RustPOSIX\"\nVERSION=\"1.0.0\"\nID=rustposix\nPRETTY_NAME=\"Rust POSIX OS\"\n".to_vec()));
    etc_dir.add_child("motd", RamFsFile::new(b"Welcome to Rust POSIX OS (Framekernel Model)\nType 'help' for available commands.\n\n".to_vec()));

    // Attach subdirectories to root
    root_dir.add_child("dev", dev_dir);
    root_dir.add_child("proc", proc_dir);
    root_dir.add_child("tmp", tmp_dir);
    root_dir.add_child("bin", bin_dir);
    root_dir.add_child("etc", etc_dir);
    log::info!("[SERVICES] Root filesystem directory hierarchy created.");

    // 2. Unpack Initramfs if supplied by Limine bootloader
    if !module_resp.is_null() {
        let count = unsafe { (*module_resp).module_count as usize };
        let modules = unsafe { (*module_resp).modules };
        log::info!("[SERVICES] Limine modules response present, count = {}", count);
        for i in 0..count {
            let file = unsafe { **modules.add(i) };
            log::info!("[SERVICES] Boot module {}: addr {:p}, size {} bytes", i, file.address, file.size);
            if !file.address.is_null() && file.size > 0 {
                let slice = unsafe { core::slice::from_raw_parts(file.address, file.size as usize) };
                match unpack_tar_archive(slice, &root_dir) {
                    Ok(unpacked) => log::info!("[VFS] Unpacked {} files from boot module initramfs.", unpacked),
                    Err(e) => log::error!("[VFS] Failed to unpack initramfs: {}", e),
                }
            }
        }
    } else {
        log::warn!("[SERVICES] No boot module response from Limine.");
    }

    vfs_init(root_dir);
    log::info!("[SERVICES] VFS initialized.");

    // 3. Spawn Init Process (PID 1)
    let mut init_proc = Process::new(1, 0, "/".to_string());
    // Attach stdin, stdout, stderr (FD 0, 1, 2) to /dev/console
    let stdin_h = Arc::new(FileHandle::new(console.clone(), O_RDWR));
    let stdout_h = Arc::new(FileHandle::new(console.clone(), O_RDWR));
    let stderr_h = Arc::new(FileHandle::new(console, O_RDWR));

    init_proc.fds.push(Some(stdin_h));
    init_proc.fds.push(Some(stdout_h));
    init_proc.fds.push(Some(stderr_h));

    // Try loading /bin/init if present in initramfs
    match init_proc.exec("/bin/init") {
        Ok(()) => log::info!("[SERVICES] Successfully loaded /bin/init into PID 1."),
        Err(e) => log::warn!("[SERVICES] /bin/init not loaded (errno: {}).", e),
    }

    PROCESS_TABLE.lock().insert(1, Arc::new(crate::ostd::sync::SpinLock::new(init_proc)));

    // 4. Initialize Audit & Snapshot Subsystem
    audit_init();

    log::info!("De-privileged services initialized (VFS, DevFS, Init Process PID 1, Audit ready).");
}
