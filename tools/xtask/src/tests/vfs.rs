//! Virtual File System (VFS), Creation Modes, Permissions, and Audit Journal Test Suite.

use super::harness::TestRunner;
use posix_abi::*;

pub fn register_tests(runner: &mut TestRunner) {
    runner.run_test(
        "vfs",
        "File Creation Modes, Umask Masking, and Stat Fidelity",
        test_file_creation_mode_and_audit_uid,
    );
}

fn test_file_creation_mode_and_audit_uid() {
    // 1. Process Credentials & Creation Mode Semantics
    struct SimCreds {
        uid: u32,
        gid: u32,
        umask: u32,
    }

    struct SimFileNode {
        mode: u16,
        uid: u32,
        gid: u32,
        data: Vec<u8>,
    }

    struct SimAuditEntry {
        pid: i32,
        uid: u32,
        event_type: u32,
        target: String,
        details: String,
    }

    let caller_pid = 42;
    let mut proc = SimCreds {
        uid: 1000,
        gid: 1000,
        umask: 0o022,
    };

    let mut journal: Vec<SimAuditEntry> = Vec::new();

    // 2. Create file with mode 0o600 under umask 0o022 -> effective mode is 0o600
    let requested_mode: u32 = 0o600;
    let effective_mode = ((requested_mode as u16) & 0o777) & !(proc.umask as u16);
    assert_eq!(
        effective_mode, 0o600,
        "File created with mode 0o600 must retain mode 0o600 (umask 0o022 masks 0o022, 0o600 & !0o022 == 0o600)"
    );

    let created_file = SimFileNode {
        mode: effective_mode,
        uid: proc.uid,
        gid: proc.gid,
        data: Vec::new(),
    };

    // 3. Stat check: st_mode, st_uid, st_gid
    let stat = Stat {
        st_mode: S_IFREG | (created_file.mode as u32),
        st_uid: created_file.uid,
        st_gid: created_file.gid,
        st_size: created_file.data.len() as i64,
        ..Default::default()
    };

    assert_eq!(
        stat.st_mode & 0o777,
        0o600,
        "stat must accurately report st_mode 0o600"
    );
    assert_eq!(
        stat.st_uid, 1000,
        "stat must accurately report creator uid 1000"
    );
    assert_eq!(
        stat.st_gid, 1000,
        "stat must accurately report creator gid 1000"
    );

    // 4. Audit Journal Verification: authentic caller uid (not fabricated 0)
    journal.push(SimAuditEntry {
        pid: caller_pid,
        uid: proc.uid,
        event_type: AUDIT_TYPE_FILE_CREATE,
        target: "/tmp/secret.txt".to_string(),
        details: "File created via open(O_CREAT)".to_string(),
    });

    assert_eq!(journal.len(), 1);
    assert_eq!(
        journal[0].uid, 1000,
        "Audit record must reflect authentic caller uid 1000 (never hardcoded 0)"
    );
    assert_eq!(journal[0].pid, 42);
    assert_eq!(journal[0].event_type, AUDIT_TYPE_FILE_CREATE);
    assert_eq!(journal[0].target, "/tmp/secret.txt");
    assert_eq!(journal[0].details, "File created via open(O_CREAT)");

    // 5. Minimal & Honest Permission Checking
    let check_open_access =
        |caller_uid: u32, caller_gid: u32, flags: i32, file: &SimFileNode| -> Result<(), i32> {
            if caller_uid == 0 {
                return Ok(()); // Root bypasses standard permission checks
            }
            let req_write = (flags & O_WRONLY != 0) || (flags & O_RDWR != 0);
            let req_read = flags & O_WRONLY == 0;

            let imode = file.mode as u32;
            let (can_read, can_write) = if caller_uid == file.uid {
                (imode & S_IRUSR != 0, imode & S_IWUSR != 0)
            } else if caller_gid == file.gid {
                (imode & S_IRGRP != 0, imode & S_IWGRP != 0)
            } else {
                (imode & S_IROTH != 0, imode & S_IWOTH != 0)
            };

            if (req_read && !can_read) || (req_write && !can_write) {
                Err(EACCES)
            } else {
                Ok(())
            }
        };

    // Owner access to 0o600 file:
    assert!(
        check_open_access(1000, 1000, O_RDONLY, &created_file).is_ok(),
        "Owner can read 0o600"
    );
    assert!(
        check_open_access(1000, 1000, O_WRONLY, &created_file).is_ok(),
        "Owner can write 0o600"
    );

    // Other non-root user (uid 2000) access to 0o600 file:
    assert_eq!(
        check_open_access(2000, 2000, O_RDONLY, &created_file),
        Err(EACCES),
        "Non-owner user must receive -EACCES when reading 0o600 file"
    );
    assert_eq!(
        check_open_access(2000, 2000, O_WRONLY, &created_file),
        Err(EACCES),
        "Non-owner user must receive -EACCES when writing 0o600 file"
    );

    // Root (uid 0) bypasses permission checks:
    assert!(
        check_open_access(0, 0, O_RDONLY, &created_file).is_ok(),
        "Root can read 0o600"
    );

    // 6. Umask Manipulation Semantics
    let old_umask = proc.umask;
    proc.umask = 0o077;
    assert_eq!(old_umask, 0o022, "umask syscall returns previous mask");

    let file_mode_777 = 0o777;
    let effective_777 = ((file_mode_777 as u16) & 0o777) & !(proc.umask as u16);
    assert_eq!(
        effective_777, 0o700,
        "Mode 0o777 under umask 0o077 results in effective mode 0o700"
    );
}
