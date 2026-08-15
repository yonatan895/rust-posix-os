//! POSIX Security Audit Journal & Snapshot System Calls.

use super::{copy_optional_user_str, copy_user_path};
use crate::ostd::mm::USER_STR_MAX;
use crate::services::audit::{create_audit_snapshot, log_audit_event};
use crate::services::process::get_current_process;

/// Appends a security audit event to the kernel audit journal.
pub fn sys_audit_log(event_type: u32, target_ptr: *const u8, details_ptr: *const u8) -> isize {
    let (pid, uid) = match get_current_process() {
        Some(p) => {
            let proc = p.lock();
            (proc.pid, proc.uid)
        }
        None => (0, 0),
    };
    let mut tbuf = [0u8; USER_STR_MAX];
    let target = match copy_optional_user_str(target_ptr, &mut tbuf) {
        Ok(s) => s,
        Err(e) => return -(e as isize),
    };
    let mut dbuf = [0u8; USER_STR_MAX];
    let details = match copy_optional_user_str(details_ptr, &mut dbuf) {
        Ok(s) => s,
        Err(e) => return -(e as isize),
    };
    let seq = log_audit_event(pid, uid, event_type, 0, target, details);
    seq as isize
}

/// Takes a snapshot of current system memory and process metrics.
pub fn sys_audit_snapshot(label_ptr: *const u8, _flags: u32) -> isize {
    let mut lbuf = [0u8; USER_STR_MAX];
    let label = if label_ptr.is_null() {
        "snapshot"
    } else {
        match copy_user_path(label_ptr, &mut lbuf) {
            Ok(s) => s,
            Err(e) => return -(e as isize),
        }
    };
    let snap_id = create_audit_snapshot(label);
    snap_id as isize
}
