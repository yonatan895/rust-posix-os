//! POSIX Security Audit Journal & Snapshot System Calls.

use crate::services::process::get_current_process;
use crate::services::audit::{log_audit_event, create_audit_snapshot};

pub fn sys_audit_log(event_type: u32, target_ptr: *const u8, details_ptr: *const u8) -> isize {
    let pid = match get_current_process() {
        Some(p) => p.lock().pid,
        None => 0,
    };
    let target = unsafe {
        if target_ptr.is_null() {
            ""
        } else {
            let mut len = 0;
            while *target_ptr.add(len) != 0 {
                len += 1;
            }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(target_ptr, len))
        }
    };
    let details = unsafe {
        if details_ptr.is_null() {
            ""
        } else {
            let mut len = 0;
            while *details_ptr.add(len) != 0 {
                len += 1;
            }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(details_ptr, len))
        }
    };
    let seq = log_audit_event(pid, 0, event_type, 0, target, details);
    seq as isize
}

pub fn sys_audit_snapshot(label_ptr: *const u8, _flags: u32) -> isize {
    let label = unsafe {
        if label_ptr.is_null() {
            "snapshot"
        } else {
            let mut len = 0;
            while *label_ptr.add(len) != 0 {
                len += 1;
            }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(label_ptr, len))
        }
    };
    let snap_id = create_audit_snapshot(label);
    snap_id as isize
}
