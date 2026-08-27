//! Pseudo-Filesystem (/proc) Dynamic Inodes.

use crate::services::audit::{get_audit_events, get_snapshots};
use crate::services::monitor::{SYSTEM_MONITOR, update_system_metrics};
use crate::services::vfs::{FileType, Inode};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use posix_abi::*;

/// Dynamic `/proc` file content category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcKind {
    /// `/proc/meminfo` system memory and heap metrics.
    Meminfo,
    /// `/proc/stat` CPU ticks and process count statistics.
    Stat,
    /// `/proc/uptime` system uptime ticks.
    Uptime,
    /// `/proc/processes` process listing table.
    Processes,
    /// `/proc/audit_journal` formatted audit log event stream.
    AuditJournal,
    /// `/proc/snapshots` list of system state snapshots.
    AuditSnapshots,
}

/// Inode generating dynamic read-only text content on demand for `/proc` nodes.
pub struct ProcDynamicFile {
    /// Specific category of proc file to generate.
    pub kind: ProcKind,
}

impl ProcDynamicFile {
    /// Creates a new reference-counted dynamic procfs file inode.
    pub fn new(kind: ProcKind) -> Arc<Self> {
        Arc::new(Self { kind })
    }

    /// Renders current dynamic system statistics into a text buffer.
    fn generate_content(&self) -> String {
        update_system_metrics();
        let mon = SYSTEM_MONITOR.lock();

        match self.kind {
            ProcKind::Meminfo => {
                let total_kb = mon.total_memory_bytes / 1024;
                let free_kb = mon.free_memory_bytes / 1024;
                let used_kb = mon.used_memory_bytes / 1024;
                let heap_total_kb = mon.total_heap_bytes / 1024;
                let heap_used_kb = mon.used_heap_bytes / 1024;

                alloc::format!(
                    "MemTotal:       {:8} kB\nMemFree:        {:8} kB\nMemUsed:        {:8} kB\nHeapTotal:      {:8} kB\nHeapUsed:       {:8} kB\n",
                    total_kb,
                    free_kb,
                    used_kb,
                    heap_total_kb,
                    heap_used_kb
                )
            }
            ProcKind::Stat => {
                alloc::format!(
                    "cpu_ticks {}\nprocesses {}\nprocs_running {}\n",
                    mon.sample_tick,
                    mon.total_processes,
                    mon.running_processes
                )
            }
            ProcKind::Uptime => {
                let ticks = mon.sample_tick;
                alloc::format!("{}.00 {}.00\n", ticks, ticks)
            }
            ProcKind::Processes => {
                let mut out = String::from("PID\tPPID\tSTATE\tFDS\tCWD\n");
                for p in mon.processes.iter() {
                    let line = alloc::format!(
                        "{}\t{}\t{}\t{}\t{}\n",
                        p.pid,
                        p.ppid,
                        p.state,
                        p.open_fds,
                        p.cwd
                    );
                    out.push_str(&line);
                }
                out
            }
            ProcKind::AuditJournal => {
                let mut out = String::from("SEQ\tTIME\tPID\tTYPE\tSTATUS\tTARGET\tDETAILS\n");
                let events = get_audit_events();
                for ev in events.iter() {
                    let type_name = match ev.event_type {
                        AUDIT_TYPE_USER_ACTION => "USER_ACTION",
                        AUDIT_TYPE_PROCESS_SPAWN => "PROC_SPAWN",
                        AUDIT_TYPE_PROCESS_EXIT => "PROC_EXIT",
                        AUDIT_TYPE_FILE_CREATE => "FILE_CREATE",
                        AUDIT_TYPE_FILE_MODIFY => "FILE_MODIFY",
                        AUDIT_TYPE_FILE_UNLINK => "FILE_UNLINK",
                        AUDIT_TYPE_DIR_CREATE => "DIR_CREATE",
                        AUDIT_TYPE_DIR_CHANGE => "DIR_CHANGE",
                        AUDIT_TYPE_SNAPSHOT_CREATED => "SNAPSHOT",
                        AUDIT_TYPE_SECURITY_ALERT => "SEC_ALERT",
                        _ => "OTHER",
                    };
                    let line = alloc::format!(
                        "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                        ev.seq,
                        ev.timestamp_ticks,
                        ev.pid,
                        type_name,
                        ev.status,
                        ev.target,
                        ev.details
                    );
                    out.push_str(&line);
                }
                out
            }
            ProcKind::AuditSnapshots => {
                let mut out =
                    String::from("ID\tLABEL\tTIME\tJOURNAL_SEQ\tRAM_USED\tHEAP_USED\tPROCS\n");
                let snapshots = get_snapshots();
                for s in snapshots.iter() {
                    let line = alloc::format!(
                        "{}\t{}\t{}\t{}\t{} kB\t{} kB\t{}\n",
                        s.id,
                        s.label,
                        s.timestamp_ticks,
                        s.journal_seq,
                        s.used_memory_kb,
                        s.heap_used_kb,
                        s.process_count
                    );
                    out.push_str(&line);
                }
                out
            }
        }
    }
}

impl Inode for ProcDynamicFile {
    fn file_type(&self) -> FileType {
        FileType::Regular
    }

    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, i32> {
        let content = self.generate_content();
        let bytes = content.as_bytes();

        if offset >= bytes.len() {
            return Ok(0);
        }

        let to_copy = (bytes.len() - offset).min(buf.len());
        buf[..to_copy].copy_from_slice(&bytes[offset..offset + to_copy]);
        Ok(to_copy)
    }

    fn write(&self, _offset: usize, _buf: &[u8]) -> Result<usize, i32> {
        Err(posix_abi::EPERM)
    }

    fn lookup(&self, _name: &str) -> Result<Arc<dyn Inode>, i32> {
        Err(posix_abi::ENOTDIR)
    }

    fn readdir(&self) -> Result<Vec<Dirent64>, i32> {
        Err(posix_abi::ENOTDIR)
    }

    fn stat(&self) -> Result<Stat, i32> {
        let content = self.generate_content();
        Ok(Stat {
            st_mode: S_IFREG | 0o444,
            st_size: content.len() as i64,
            ..Default::default()
        })
    }
}
