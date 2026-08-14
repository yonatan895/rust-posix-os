//! De-Privileged OS Services - 100% Safe Rust.
//!
//! All high-level operating system functionality is implemented in this module
//! in safe Rust, using only the safe abstractions exposed by the OSTD framework.

pub mod audit;
pub mod ipc;
pub mod monitor;
pub mod posix;
pub mod process;
pub mod scheduler;
pub mod tty;
pub mod vfs;

use crate::ostd::mm::BootBlob;
use crate::services::audit::audit_init;
use crate::services::process::{PROCESS_TABLE, Process};
use crate::services::vfs::devfs::{DevConsole, DevNull, DevZero};
use crate::services::vfs::procfs::{ProcDynamicFile, ProcKind};
use crate::services::vfs::ramfs::{RamFsDir, RamFsFile};
use crate::services::vfs::tar::unpack_tar_archive;
use crate::services::vfs::{FileHandle, vfs_init};
use alloc::string::ToString;
use alloc::sync::Arc;
use posix_abi::O_RDWR;

pub fn services_init(blobs: alloc::vec::Vec<BootBlob>) {
    log::info!("[SERVICES] Starting de-privileged OS services initialization...");

    let root_dir = RamFsDir::new();
    let dev_dir = RamFsDir::new();
    let proc_dir = RamFsDir::new();
    let tmp_dir = RamFsDir::new();
    let bin_dir = RamFsDir::new();
    let etc_dir = RamFsDir::new();

    dev_dir.add_child("null", Arc::new(DevNull));
    dev_dir.add_child("zero", Arc::new(DevZero));
    let console = Arc::new(DevConsole);
    dev_dir.add_child("console", console.clone());
    dev_dir.add_child("tty", console.clone());

    proc_dir.add_child("meminfo", ProcDynamicFile::new(ProcKind::Meminfo));
    proc_dir.add_child("stat", ProcDynamicFile::new(ProcKind::Stat));
    proc_dir.add_child("uptime", ProcDynamicFile::new(ProcKind::Uptime));
    proc_dir.add_child("processes", ProcDynamicFile::new(ProcKind::Processes));
    proc_dir.add_child(
        "audit_journal",
        ProcDynamicFile::new(ProcKind::AuditJournal),
    );
    proc_dir.add_child("snapshots", ProcDynamicFile::new(ProcKind::AuditSnapshots));

    etc_dir.add_child(
        "os-release",
        RamFsFile::new(
            b"NAME=\"RustPOSIX\"\nVERSION=\"1.0.0\"\nID=rustposix\nPRETTY_NAME=\"Rust POSIX OS\"\n"
                .to_vec(),
        ),
    );
    etc_dir.add_child("motd", RamFsFile::new(b"Welcome to Rust POSIX OS (Framekernel Model)\nType 'help' for available commands.\n\n".to_vec()));

    root_dir.add_child("dev", dev_dir);
    root_dir.add_child("proc", proc_dir);
    root_dir.add_child("tmp", tmp_dir);
    root_dir.add_child("bin", bin_dir);
    root_dir.add_child("etc", etc_dir);
    log::info!("[SERVICES] Root filesystem directory hierarchy created.");

    if blobs.is_empty() {
        log::warn!("[SERVICES] No boot module payloads from Limine.");
    }
    for (i, blob) in blobs.iter().enumerate() {
        log::info!("[SERVICES] Boot module {}: {} bytes", i, blob.bytes.len());
        match unpack_tar_archive(blob.bytes, &root_dir) {
            Ok(unpacked) => log::info!(
                "[VFS] Unpacked {} files from boot module initramfs.",
                unpacked
            ),
            Err(e) => log::error!("[VFS] Failed to unpack initramfs: {}", e),
        }
    }

    vfs_init(root_dir);
    log::info!("[SERVICES] VFS initialized.");

    let mut init_proc = Process::new(1, 0, "/".to_string());
    let stdin_h = Arc::new(FileHandle::new(console.clone(), O_RDWR));
    let stdout_h = Arc::new(FileHandle::new(console.clone(), O_RDWR));
    let stderr_h = Arc::new(FileHandle::new(console, O_RDWR));

    init_proc.fds.push(Some(stdin_h));
    init_proc.fds.push(Some(stdout_h));
    init_proc.fds.push(Some(stderr_h));

    match init_proc.exec("/bin/init", &["/bin/init"], &[]) {
        Ok(()) => log::info!("[SERVICES] Successfully loaded /bin/init into PID 1."),
        Err(e) => log::warn!("[SERVICES] /bin/init not loaded (errno: {}).", e),
    }

    let init_arc = Arc::new(crate::ostd::sync::SpinLock::new(init_proc));
    PROCESS_TABLE.lock().insert(1, init_arc.clone());
    crate::services::scheduler::set_current_process(init_arc);

    let mut idle_proc = Process::new(0, 0, "/".to_string());
    idle_proc.saved_kernel_rsp = crate::ostd::task::init_kernel_task_stack(
        &mut idle_proc.kernel_stack,
        crate::ostd::task::kernel_idle_loop as *const () as usize,
    );
    let idle_arc = Arc::new(crate::ostd::sync::SpinLock::new(idle_proc));
    PROCESS_TABLE.lock().insert(0, idle_arc.clone());
    crate::services::scheduler::set_idle_task(idle_arc);

    audit_init();

    log::info!(
        "De-privileged services initialized (VFS, DevFS, Init Process PID 1, Idle Task PID 0, Audit ready)."
    );
}
