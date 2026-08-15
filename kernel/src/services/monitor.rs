//! Background System Resource & Process Monitoring Service.

use crate::ostd::mm::{PAGE_SIZE, get_heap_stats, get_pmm_stats};
use crate::ostd::sync::SpinLock;
use crate::services::process::{PROCESS_TABLE, ProcessState};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct ProcessMetric {
    pub pid: i32,
    pub ppid: i32,
    pub state: String,
    pub open_fds: usize,
    pub cwd: String,
}

#[derive(Debug, Clone)]
pub struct SystemMetricsSnapshot {
    pub sample_tick: u64,
    pub total_memory_bytes: usize,
    pub free_memory_bytes: usize,
    pub used_memory_bytes: usize,
    pub total_heap_bytes: usize,
    pub used_heap_bytes: usize,
    pub total_processes: usize,
    pub running_processes: usize,
    pub processes: Vec<ProcessMetric>,
}

impl SystemMetricsSnapshot {
    pub const fn empty() -> Self {
        Self {
            sample_tick: 0,
            total_memory_bytes: 0,
            free_memory_bytes: 0,
            used_memory_bytes: 0,
            total_heap_bytes: 0,
            used_heap_bytes: 0,
            total_processes: 0,
            running_processes: 0,
            processes: Vec::new(),
        }
    }
}

pub static SYSTEM_MONITOR: SpinLock<SystemMetricsSnapshot> =
    SpinLock::new(SystemMetricsSnapshot::empty());

pub fn update_system_metrics() {
    let (total_frames, free_frames) = get_pmm_stats();
    let used_frames = total_frames.saturating_sub(free_frames);
    let total_mem = total_frames * PAGE_SIZE;
    let free_mem = free_frames * PAGE_SIZE;
    let used_mem = used_frames * PAGE_SIZE;

    let (heap_total, heap_used) = get_heap_stats();

    let mut proc_metrics = Vec::new();
    let mut running_count = 0;

    {
        let table = PROCESS_TABLE.lock();
        for (&pid, proc_arc) in table.iter() {
            let proc = proc_arc.lock();
            let state_str = match proc.state {
                ProcessState::Ready => "READY",
                ProcessState::Running => {
                    running_count += 1;
                    "RUNNING"
                }
                ProcessState::Blocked => "BLOCKED",
                ProcessState::Zombie => "ZOMBIE",
            };
            let open_fds = proc.fds.iter().filter(|f| f.is_some()).count();
            proc_metrics.push(ProcessMetric {
                pid,
                ppid: proc.ppid,
                state: alloc::string::ToString::to_string(state_str),
                open_fds,
                cwd: proc.cwd.clone(),
            });
        }
    }

    let mut monitor = SYSTEM_MONITOR.lock();
    monitor.sample_tick += 1;
    monitor.total_memory_bytes = total_mem;
    monitor.free_memory_bytes = free_mem;
    monitor.used_memory_bytes = used_mem;
    monitor.total_heap_bytes = heap_total;
    monitor.used_heap_bytes = heap_used;
    monitor.total_processes = proc_metrics.len();
    monitor.running_processes = running_count;
    monitor.processes = proc_metrics;
}
