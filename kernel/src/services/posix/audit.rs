//! POSIX Security Audit Journal & Snapshot System Calls.

use crate::services::process::get_current_process;
use crate::services::audit::{log_audit_event, create_audit_snapshot};
use crate::ostd::mm::USER_STR_MAX;
use super::{copy_optional_user_str, copy_user_path};

pub fn sys_audit_log(event_type: u32, target_ptr: *const u8, details_ptr: *const u8) -> isize {
    let pid = match get_current_process() {
        Some(p) => p.lock().pid,
        None => 0,
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
    let seq = log_audit_event(pid, 0, event_type, 0, target, details);
    seq as isize
}

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
