//! System security audit trail and periodic/on-demand system state snapshots.

use crate::ostd::sync::SpinLock;
use crate::services::monitor::SYSTEM_MONITOR;
use crate::services::process::PROCESS_TABLE;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use posix_abi::*;

/// Maximum number of audit events retained in the circular ring journal.
pub const MAX_JOURNAL_ENTRIES: usize = 512;
/// Maximum number of system snapshots retained in memory.
pub const MAX_SNAPSHOTS: usize = 64;

/// Structured record representing an audited kernel or security event.
#[derive(Debug, Clone)]
pub struct AuditEvent {
    /// Monotonically increasing event sequence number.
    pub seq: u64,
    /// System tick counter when the event was logged.
    pub timestamp_ticks: u64,
    /// Process ID that triggered the event.
    pub pid: i32,
    /// Effective user ID of the calling process.
    pub uid: u32,
    /// Categorical audit event type identifier.
    pub event_type: u32,
    /// Outcome status code (0 for success or negative POSIX errno).
    pub status: i32,
    /// Object or target path of the operation.
    pub target: String,
    /// Detailed diagnostic message or argument description.
    pub details: String,
}

/// Circular in-memory audit log buffer.
pub struct AuditJournal {
    /// Ordered collection of recent audit events.
    pub entries: Vec<AuditEvent>,
    /// Cumulative count of all events appended since boot.
    pub total_logged: u64,
}

impl AuditJournal {
    /// Creates a new empty audit journal.
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            total_logged: 0,
        }
    }

    /// Appends an event to the journal, evicting the oldest event if capacity is exceeded.
    pub fn append(&mut self, event: AuditEvent) {
        if self.entries.len() >= MAX_JOURNAL_ENTRIES {
            self.entries.remove(0);
        }
        self.entries.push(event);
        self.total_logged += 1;
    }
}

impl Default for AuditJournal {
    fn default() -> Self {
        Self::new()
    }
}

/// Global spinlock-guarded audit event journal.
pub static AUDIT_JOURNAL: SpinLock<AuditJournal> = SpinLock::new(AuditJournal::new());
/// Sequence counter generator for audit events.
static NEXT_EVENT_SEQ: AtomicU64 = AtomicU64::new(1);
/// Unique ID generator for system snapshots.
static NEXT_SNAPSHOT_ID: AtomicU64 = AtomicU64::new(1);

/// Point-in-time process state captured within an audit snapshot.
#[derive(Debug, Clone)]
pub struct ProcessSnapshotInfo {
    /// Process identifier.
    pub pid: i32,
    /// Parent process identifier.
    pub ppid: i32,
    /// User identifier.
    pub uid: u32,
    /// Group identifier.
    pub gid: u32,
    /// Execution state string.
    pub state: String,
    /// Count of open file descriptors.
    pub open_fds: usize,
    /// Current working directory.
    pub cwd: String,
}

/// Complete system state snapshot capturing resource usage and process metadata.
#[derive(Debug, Clone)]
pub struct AuditSnapshot {
    /// Unique identifier for this snapshot.
    pub id: u64,
    /// Descriptive label or tag.
    pub label: String,
    /// System tick timestamp when created.
    pub timestamp_ticks: u64,
    /// Highest audit journal sequence number at creation time.
    pub journal_seq: u64,
    /// Total system RAM in KiB.
    pub total_memory_kb: u64,
    /// Used system RAM in KiB.
    pub used_memory_kb: u64,
    /// Kernel heap memory used in KiB.
    pub heap_used_kb: u64,
    /// Number of active processes at snapshot time.
    pub process_count: u32,
    /// Metadata for all active processes.
    pub processes: Vec<ProcessSnapshotInfo>,
}

/// Storage manager for system snapshots.
pub struct SnapshotManager {
    /// Collection of retained audit snapshots.
    pub snapshots: Vec<AuditSnapshot>,
}

impl SnapshotManager {
    /// Creates a new empty snapshot manager.
    pub const fn new() -> Self {
        Self {
            snapshots: Vec::new(),
        }
    }
}

impl Default for SnapshotManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Global spinlock-guarded snapshot manager.
pub static SNAPSHOT_MANAGER: SpinLock<SnapshotManager> = SpinLock::new(SnapshotManager::new());

/// Logs a new audit event to the global journal and returns its sequence number.
pub fn log_audit_event(
    pid: i32,
    uid: u32,
    event_type: u32,
    status: i32,
    target: &str,
    details: &str,
) -> u64 {
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

/// Retrieves a clone of all current audit events in the journal.
pub fn get_audit_events() -> Vec<AuditEvent> {
    let journal = AUDIT_JOURNAL.lock();
    journal.entries.clone()
}

/// Creates a new system state snapshot attributed to the current calling process.
pub fn create_audit_snapshot(label: &str) -> u64 {
    let (caller_pid, caller_uid) = match crate::services::process::get_current_process() {
        Some(p) => {
            let proc = p.lock();
            (proc.pid, proc.uid)
        }
        None => (0, 0),
    };
    create_audit_snapshot_with_creds(caller_pid, caller_uid, label)
}

/// Creates a new system state snapshot attributed to the specified PID and UID.
pub fn create_audit_snapshot_with_creds(pid: i32, uid: u32, label: &str) -> u64 {
    let id = NEXT_SNAPSHOT_ID.fetch_add(1, Ordering::Relaxed);
    let current_seq = NEXT_EVENT_SEQ.load(Ordering::Relaxed).saturating_sub(1);
    // Note: Memory/heap stats come from the monitor's last sample and may be slightly
    // stale if the monitor hasn't polled recently. This is acceptable for audit snapshots.
    let (total_ram_kb, used_ram_kb, heap_used_kb, ticks) = {
        let mon = SYSTEM_MONITOR.lock();
        (
            (mon.total_memory_bytes / 1024) as u64,
            (mon.used_memory_bytes / 1024) as u64,
            (mon.used_heap_bytes / 1024) as u64,
            mon.sample_tick,
        )
    };

    let mut procs = Vec::new();
    {
        let table = PROCESS_TABLE.lock();
        for (&p_id, proc_arc) in table.iter() {
            let proc = proc_arc.lock();
            let state_str = match proc.state {
                crate::services::process::ProcessState::Ready => "READY",
                crate::services::process::ProcessState::Running => "RUNNING",
                crate::services::process::ProcessState::Blocked => "BLOCKED",
                crate::services::process::ProcessState::Zombie => "ZOMBIE",
            };
            procs.push(ProcessSnapshotInfo {
                pid: p_id,
                ppid: proc.ppid,
                uid: proc.uid,
                gid: proc.gid,
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
        pid,
        uid,
        AUDIT_TYPE_SNAPSHOT_CREATED,
        0,
        label,
        &alloc::format!(
            "Snapshot #{} created (covered up to journal seq {})",
            id,
            current_seq
        ),
    );

    id
}

/// Returns a copy of all stored system state snapshots.
pub fn get_snapshots() -> Vec<AuditSnapshot> {
    let mgr = SNAPSHOT_MANAGER.lock();
    mgr.snapshots.clone()
}

/// Finds a snapshot by its unique ID.
pub fn get_snapshot_by_id(id: u64) -> Option<AuditSnapshot> {
    let mgr = SNAPSHOT_MANAGER.lock();
    mgr.snapshots.iter().find(|s| s.id == id).cloned()
}

/// Initializes the audit subsystem and records the initial boot baseline snapshot.
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
    log::info!(
        "[AUDIT] Subsystem initialized. Created baseline snapshot #{}.",
        boot_snap
    );
}
