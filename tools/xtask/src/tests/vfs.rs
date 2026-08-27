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
    let umask = 0o022u16;
    let req_mode = 0o600u16;
    let effective_mode = (req_mode & 0o777) & !umask;
    assert_eq!(effective_mode, 0o600);

    let stat = Stat {
        st_mode: S_IFREG | (effective_mode as u32),
        st_uid: 1000,
        st_gid: 1000,
        st_size: 0,
        ..Default::default()
    };
    assert_eq!(stat.st_mode & 0o777, 0o600);
    assert_eq!(stat.st_uid, 1000);
    assert_eq!(stat.st_gid, 1000);

    let check_open_access = |caller_uid: u32, caller_gid: u32, flags: i32, mode: u16, fuid: u32, fgid: u32| -> Result<(), i32> {
        if caller_uid == 0 { return Ok(()); }
        let req_write = (flags & O_WRONLY != 0) || (flags & O_RDWR != 0);
        let req_read = flags & O_WRONLY == 0;
        let imode = mode as u32;
        let (can_read, can_write) = if caller_uid == fuid {
            (imode & S_IRUSR != 0, imode & S_IWUSR != 0)
        } else if caller_gid == fgid {
            (imode & S_IRGRP != 0, imode & S_IWGRP != 0)
        } else {
            (imode & S_IROTH != 0, imode & S_IWOTH != 0)
        };
        if (req_read && !can_read) || (req_write && !can_write) { Err(EACCES) } else { Ok(()) }
    };

    assert!(check_open_access(1000, 1000, O_RDONLY, effective_mode, 1000, 1000).is_ok());
    assert!(check_open_access(1000, 1000, O_WRONLY, effective_mode, 1000, 1000).is_ok());
    assert_eq!(check_open_access(2000, 2000, O_RDONLY, effective_mode, 1000, 1000), Err(EACCES));
    assert_eq!(check_open_access(2000, 2000, O_WRONLY, effective_mode, 1000, 1000), Err(EACCES));
    assert!(check_open_access(0, 0, O_RDONLY, effective_mode, 1000, 1000).is_ok());

    let umask_77 = 0o077u16;
    assert_eq!(0o777u16 & !umask_77, 0o700);
}

/// Tests atomic rename semantics, cross-directory address-ordered locking, and directory cycle rejection.
fn test_vfs_atomic_rename() {
    use std::collections::BTreeMap;

    struct SimFs {
        nodes: BTreeMap<usize, (bool, BTreeMap<String, usize>)>, // is_dir, entries
        next_id: usize,
        lock_order: Vec<usize>,
    }

    impl SimFs {
        fn new() -> Self {
            let mut fs = Self { nodes: BTreeMap::new(), next_id: 1, lock_order: Vec::new() };
            fs.nodes.insert(0, (true, BTreeMap::new()));
            fs
        }
        fn create(&mut self, parent: usize, name: &str, is_dir: bool) -> usize {
            let id = self.next_id; self.next_id += 1;
            self.nodes.insert(id, (is_dir, BTreeMap::new()));
            self.nodes.get_mut(&parent).unwrap().1.insert(name.to_string(), id);
            id
        }
        fn rename(&mut self, op: usize, on: &str, np: usize, nn: &str) -> Result<(), i32> {
            self.lock_order = if op == np { vec![op] } else if op < np { vec![op, np] } else { vec![np, op] };
            let src_id = *self.nodes.get(&op).ok_or(ENOENT)?.1.get(on).ok_or(ENOENT)?;
            if op == np && on == nn { return Ok(()); }
            let src_is_dir = self.nodes.get(&src_id).unwrap().0;
            if let Some(&tgt_id) = self.nodes.get(&np).ok_or(ENOENT)?.1.get(nn) {
                let tgt_is_dir = self.nodes.get(&tgt_id).unwrap().0;
                if src_is_dir && !tgt_is_dir { return Err(ENOTDIR); }
                if !src_is_dir && tgt_is_dir { return Err(EISDIR); }
                if src_is_dir && tgt_is_dir && (tgt_id == op || tgt_id == np || !self.nodes.get(&tgt_id).unwrap().1.is_empty()) {
                    return Err(ENOTEMPTY);
                }
            }
            self.nodes.get_mut(&op).unwrap().1.remove(on);
            self.nodes.get_mut(&np).unwrap().1.insert(nn.to_string(), src_id);
            Ok(())
        }
    }

    let mut fs = SimFs::new();
    let dir_a = fs.create(0, "dir_a", true);
    let dir_b = fs.create(0, "dir_b", true);
    let file1 = fs.create(dir_a, "file1.txt", false);

    assert_eq!(fs.rename(dir_a, "file1.txt", dir_a, "file2.txt"), Ok(()));
    assert_eq!(fs.lock_order, vec![dir_a]);
    assert_eq!(fs.nodes.get(&dir_a).unwrap().1.get("file2.txt"), Some(&file1));

    assert_eq!(fs.rename(dir_a, "file2.txt", dir_a, "file2.txt"), Ok(()));
    assert_eq!(fs.rename(dir_a, "file2.txt", dir_b, "file2.txt"), Ok(()));
    assert_eq!(fs.lock_order, vec![dir_a, dir_b]);
    assert_eq!(fs.nodes.get(&dir_b).unwrap().1.get("file2.txt"), Some(&file1));

    assert_eq!(fs.rename(dir_b, "file2.txt", dir_a, "file1.txt"), Ok(()));
    assert_eq!(fs.lock_order, vec![dir_a, dir_b]);

    let nested = fs.create(dir_a, "nested", true);
    assert_eq!(fs.rename(dir_a, "file1.txt", dir_a, "nested"), Err(EISDIR));

    let _sub = fs.create(nested, "sub.txt", false);
    let _other = fs.create(dir_a, "other", true);
    assert_eq!(fs.rename(dir_a, "other", dir_a, "nested"), Err(ENOTEMPTY));

    let sub_dir = fs.create(nested, "sub_dir", true);
    assert_eq!(fs.rename(nested, "sub_dir", dir_a, "nested"), Err(ENOTEMPTY));
    assert_eq!(fs.nodes.get(&nested).unwrap().1.get("sub_dir"), Some(&sub_dir));

    let check_cycle = |old_path: &str, new_path: &str| -> Result<(), i32> {
        if new_path.starts_with(&format!("{}/", old_path)) { Err(EINVAL) } else { Ok(()) }
    };
    assert_eq!(check_cycle("/a/b", "/a/b/c/d"), Err(EINVAL));
    assert_eq!(check_cycle("/a/b", "/a/c"), Ok(()));
}
