//! Kernel Security Audit Journal and System Snapshot Subsystem.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use posix_abi::*;
use crate::ostd::sync::SpinLock;
use crate::ostd::mm::{get_pmm_stats, get_heap_stats};
use crate::services::process::PROCESS_TABLE;
use crate::services::monitor::SYSTEM_MONITOR;

pub const MAX_JOURNAL_ENTRIES: usize = 512;
pub const MAX_SNAPSHOTS: usize = 64;

#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub seq: u64,
    pub timestamp_ticks: u64,
    pub pid: i32,
    pub uid: u32,
    pub event_type: u32,
    pub status: i32,
    pub target: String,
    pub details: String,
}

pub struct AuditJournal {
    pub entries: Vec<AuditEvent>,
    pub total_logged: u64,
}

impl AuditJournal {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            total_logged: 0,
        }
    }

    pub fn append(&mut self, event: AuditEvent) {
        if self.entries.len() >= MAX_JOURNAL_ENTRIES {
            self.entries.remove(0);
        }
        self.entries.push(event);
        self.total_logged += 1;
    }
}

pub static AUDIT_JOURNAL: SpinLock<AuditJournal> = SpinLock::new(AuditJournal::new());
static NEXT_EVENT_SEQ: AtomicU64 = AtomicU64::new(1);
static NEXT_SNAPSHOT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct ProcessSnapshotInfo {
    pub pid: i32,
    pub ppid: i32,
    pub state: String,
    pub open_fds: usize,
    pub cwd: String,
}

#[derive(Debug, Clone)]
pub struct AuditSnapshot {
    pub id: u64,
    pub label: String,
    pub timestamp_ticks: u64,
    pub journal_seq: u64,
    pub total_memory_kb: u64,
    pub used_memory_kb: u64,
    pub heap_used_kb: u64,
    pub process_count: u32,
    pub processes: Vec<ProcessSnapshotInfo>,
}

pub struct SnapshotManager {
    pub snapshots: Vec<AuditSnapshot>,
}

impl SnapshotManager {
    pub const fn new() -> Self {
        Self {
            snapshots: Vec::new(),
        }
    }
}

pub static SNAPSHOT_MANAGER: SpinLock<SnapshotManager> = SpinLock::new(SnapshotManager::new());

pub fn log_audit_event(pid: i32, uid: u32, event_type: u32, status: i32, target: &str, details: &str) -> u64 {
    let seq = NEXT_EVENT_SEQ.fetch_add(1, Ordering::Relaxed);
    let ticks = {
        let mon = SYSTEM_MONITOR.lock();
        mon.sample_tick
    };

    let event = AuditEvent {
        seq,
        timestamp_ticks: ticks,
        pid,
        uid,
        event_type,
        status,
        target: String::from(target),
        details: String::from(details),
    };

    let mut journal = AUDIT_JOURNAL.lock();
    journal.append(event);
    seq
}

pub fn get_audit_events() -> Vec<AuditEvent> {
    let journal = AUDIT_JOURNAL.lock();
    journal.entries.clone()
}

pub fn create_audit_snapshot(label: &str) -> u64 {
    let id = NEXT_SNAPSHOT_ID.fetch_add(1, Ordering::Relaxed);
    let current_seq = NEXT_EVENT_SEQ.load(Ordering::Relaxed).saturating_sub(1);

    let (total_frames, free_frames) = get_pmm_stats();
    let total_ram_kb = (total_frames * 4096 / 1024) as u64;
    let free_ram_kb = (free_frames * 4096 / 1024) as u64;
    let used_ram_kb = total_ram_kb.saturating_sub(free_ram_kb);

    let (_heap_total, heap_used) = get_heap_stats();
    let heap_used_kb = (heap_used / 1024) as u64;

    let ticks = {
        let mon = SYSTEM_MONITOR.lock();
        mon.sample_tick
    };

    let mut procs = Vec::new();
    {
        let table = PROCESS_TABLE.lock();
        for (&pid, proc_arc) in table.iter() {
            let proc = proc_arc.lock();
            let state_str = match proc.state {
                crate::services::process::ProcessState::Ready => "READY",
                crate::services::process::ProcessState::Running => "RUNNING",
                crate::services::process::ProcessState::Blocked => "BLOCKED",
                crate::services::process::ProcessState::Zombie => "ZOMBIE",
            };
            procs.push(ProcessSnapshotInfo {
                pid,
                ppid: proc.ppid,
                state: String::from(state_str),
                open_fds: proc.fds.iter().filter(|f| f.is_some()).count(),
                cwd: String::from(&proc.cwd),
            });
        }
    }

    let snapshot = AuditSnapshot {
        id,
        label: String::from(label),
        timestamp_ticks: ticks,
        journal_seq: current_seq,
        total_memory_kb: total_ram_kb,
        used_memory_kb: used_ram_kb,
        heap_used_kb,
        process_count: procs.len() as u32,
        processes: procs,
    };

    {
        let mut mgr = SNAPSHOT_MANAGER.lock();
        if mgr.snapshots.len() >= MAX_SNAPSHOTS {
            mgr.snapshots.remove(0);
        }
        mgr.snapshots.push(snapshot);
    }

    log_audit_event(
        1,
        0,
        AUDIT_TYPE_SNAPSHOT_CREATED,
        0,
        label,
        &alloc::format!("Snapshot #{} created (covered up to journal seq {})", id, current_seq),
    );

    id
}

pub fn get_snapshots() -> Vec<AuditSnapshot> {
    let mgr = SNAPSHOT_MANAGER.lock();
    mgr.snapshots.clone()
}

pub fn get_snapshot_by_id(id: u64) -> Option<AuditSnapshot> {
    let mgr = SNAPSHOT_MANAGER.lock();
    mgr.snapshots.iter().find(|s| s.id == id).cloned()
}

pub fn audit_init() {
    log_audit_event(
        0,
        0,
        AUDIT_TYPE_USER_ACTION,
        0,
        "kernel",
        "Kernel audit logging initialized",
    );
    let boot_snap = create_audit_snapshot("boot_baseline");
    log::info!("[AUDIT] Subsystem initialized. Created baseline snapshot #{}.", boot_snap);
}
