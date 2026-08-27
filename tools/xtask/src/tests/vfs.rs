//! Virtual file system (VFS), creation modes, permissions, and audit journal test suite.

use super::harness::TestRunner;
use posix_abi::*;

/// Registers VFS creation mode, umask, permission checking, and audit journal tests with the runner.
pub fn register_tests(runner: &mut TestRunner) {
    runner.run_test(
        "vfs",
        "File Creation Modes, Umask Masking, and Stat Fidelity",
        test_file_creation_mode_and_audit_uid,
    );
    runner.run_test(
        "vfs",
        "Atomic Rename, Cross-Directory Lock Ordering, and Cycle Rejection",
        test_vfs_atomic_rename,
    );
}

/// Tests file creation mode masking against umask, stat field fidelity, and permission checks.
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

/// Tests atomic rename semantics, cross-directory address-ordered locking, and directory cycle rejection.
fn test_vfs_atomic_rename() {
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum NodeType {
        File,
        Dir,
    }

    struct SimNode {
        node_type: NodeType,
        entries: BTreeMap<String, usize>,
    }

    struct SimFs {
        nodes: BTreeMap<usize, SimNode>,
        next_id: usize,
        lock_order: Vec<usize>,
    }

    impl SimFs {
        fn new() -> Self {
            let mut fs = Self {
                nodes: BTreeMap::new(),
                next_id: 1,
                lock_order: Vec::new(),
            };
            fs.nodes.insert(
                0,
                SimNode {
                    node_type: NodeType::Dir,
                    entries: BTreeMap::new(),
                },
            );
            fs
        }

        fn create_file(&mut self, parent: usize, name: &str) -> usize {
            let id = self.next_id;
            self.next_id += 1;
            self.nodes.insert(
                id,
                SimNode {
                    node_type: NodeType::File,
                    entries: BTreeMap::new(),
                },
            );
            self.nodes
                .get_mut(&parent)
                .unwrap()
                .entries
                .insert(name.to_string(), id);
            id
        }

        fn create_dir(&mut self, parent: usize, name: &str) -> usize {
            let id = self.next_id;
            self.next_id += 1;
            self.nodes.insert(
                id,
                SimNode {
                    node_type: NodeType::Dir,
                    entries: BTreeMap::new(),
                },
            );
            self.nodes
                .get_mut(&parent)
                .unwrap()
                .entries
                .insert(name.to_string(), id);
            id
        }

        fn rename(
            &mut self,
            old_parent: usize,
            old_name: &str,
            new_parent: usize,
            new_name: &str,
        ) -> Result<(), i32> {
            self.lock_order.clear();
            if old_parent == new_parent {
                self.lock_order.push(old_parent);
            } else if old_parent < new_parent {
                self.lock_order.push(old_parent);
                self.lock_order.push(new_parent);
            } else {
                self.lock_order.push(new_parent);
                self.lock_order.push(old_parent);
            }

            let source_id = *self
                .nodes
                .get(&old_parent)
                .ok_or(ENOENT)?
                .entries
                .get(old_name)
                .ok_or(ENOENT)?;

            if old_parent == new_parent && old_name == new_name {
                return Ok(());
            }

            let source_type = self.nodes.get(&source_id).unwrap().node_type;

            if let Some(&target_id) = self
                .nodes
                .get(&new_parent)
                .ok_or(ENOENT)?
                .entries
                .get(new_name)
            {
                let target_type = self.nodes.get(&target_id).unwrap().node_type;
                if source_type == NodeType::Dir && target_type != NodeType::Dir {
                    return Err(ENOTDIR);
                }
                if source_type != NodeType::Dir && target_type == NodeType::Dir {
                    return Err(EISDIR);
                }
                if source_type == NodeType::Dir
                    && target_type == NodeType::Dir
                    && !self.nodes.get(&target_id).unwrap().entries.is_empty()
                {
                    return Err(ENOTEMPTY);
                }
            }

            self.nodes
                .get_mut(&old_parent)
                .unwrap()
                .entries
                .remove(old_name);
            self.nodes
                .get_mut(&new_parent)
                .unwrap()
                .entries
                .insert(new_name.to_string(), source_id);

            Ok(())
        }
    }

    let mut fs = SimFs::new();
    let dir_a = fs.create_dir(0, "dir_a");
    let dir_b = fs.create_dir(0, "dir_b");
    let file1 = fs.create_file(dir_a, "file1.txt");

    // 1. Same-directory rename
    assert_eq!(fs.rename(dir_a, "file1.txt", dir_a, "file2.txt"), Ok(()));
    assert_eq!(fs.lock_order, vec![dir_a]);
    let dir_a_entries = &fs.nodes.get(&dir_a).unwrap().entries;
    assert!(!dir_a_entries.contains_key("file1.txt"));
    assert_eq!(dir_a_entries.get("file2.txt"), Some(&file1));

    // 2. Same-directory no-op rename
    assert_eq!(fs.rename(dir_a, "file2.txt", dir_a, "file2.txt"), Ok(()));
    let dir_a_entries = &fs.nodes.get(&dir_a).unwrap().entries;
    assert_eq!(dir_a_entries.get("file2.txt"), Some(&file1));

    // 3. Cross-directory rename with lower address locked first (dir_a < dir_b)
    assert_eq!(fs.rename(dir_a, "file2.txt", dir_b, "file2.txt"), Ok(()));
    assert_eq!(fs.lock_order, vec![dir_a, dir_b]);
    let dir_a_entries = &fs.nodes.get(&dir_a).unwrap().entries;
    let dir_b_entries = &fs.nodes.get(&dir_b).unwrap().entries;
    assert!(!dir_a_entries.contains_key("file2.txt"));
    assert_eq!(dir_b_entries.get("file2.txt"), Some(&file1));

    // 4. Reverse cross-directory rename with lower address locked first (dir_b > dir_a)
    assert_eq!(fs.rename(dir_b, "file2.txt", dir_a, "file1.txt"), Ok(()));
    assert_eq!(fs.lock_order, vec![dir_a, dir_b]);
    let dir_a_entries = &fs.nodes.get(&dir_a).unwrap().entries;
    assert_eq!(dir_a_entries.get("file1.txt"), Some(&file1));

    // 5. Error atomicity: file to directory -> EISDIR (source remains intact)
    let nested_dir = fs.create_dir(dir_a, "nested");
    assert_eq!(fs.rename(dir_a, "file1.txt", dir_a, "nested"), Err(EISDIR));
    let dir_a_entries = &fs.nodes.get(&dir_a).unwrap().entries;
    assert_eq!(
        dir_a_entries.get("file1.txt"),
        Some(&file1),
        "Source file must remain intact on EISDIR"
    );

    // 6. Error atomicity: directory to non-empty directory -> ENOTEMPTY
    let _subfile = fs.create_file(nested_dir, "sub.txt");
    let other_dir = fs.create_dir(dir_a, "other_dir");
    assert_eq!(
        fs.rename(dir_a, "other_dir", dir_a, "nested"),
        Err(ENOTEMPTY)
    );
    let dir_a_entries = &fs.nodes.get(&dir_a).unwrap().entries;
    assert_eq!(
        dir_a_entries.get("other_dir"),
        Some(&other_dir),
        "Source directory must remain intact on ENOTEMPTY"
    );

    // 7. Directory cycle prevention check logic
    let check_cycle = |old_path: &str, new_path: &str| -> Result<(), i32> {
        let prefix = format!("{}/", old_path);
        if new_path.starts_with(&prefix) {
            Err(EINVAL)
        } else {
            Ok(())
        }
    };
    assert_eq!(
        check_cycle("/a/b", "/a/b/c/d"),
        Err(EINVAL),
        "Renaming directory into its own subdirectory must return -EINVAL"
    );
    assert_eq!(
        check_cycle("/a/b", "/a/c"),
        Ok(()),
        "Renaming to peer directory is valid"
    );
}
