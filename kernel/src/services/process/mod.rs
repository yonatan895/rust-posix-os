//! Process & Thread Management - De-privileged Safe Service.

pub mod elf;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicI32, Ordering};
use crate::ostd::mm::VmSpace;
use crate::ostd::sync::SpinLock;
use crate::services::vfs::FileHandle;
use self::elf::load_elf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Ready,
    Running,
    Blocked,
    Zombie,
}

pub struct Process {
    pub pid: i32,
    pub ppid: i32,
    pub state: ProcessState,
    pub cwd: String,
    pub fds: Vec<Option<Arc<FileHandle>>>,
    pub exit_code: i32,
    pub vm_space: Option<VmSpace>,
    pub entry_point: usize,
    pub user_stack_top: usize,
    pub mmap_next_vaddr: usize,
}

impl Process {
    pub fn new(pid: i32, ppid: i32, cwd: String) -> Self {
        Self {
            pid,
            ppid,
            state: ProcessState::Ready,
            cwd,
            fds: Vec::new(),
            exit_code: 0,
            vm_space: None,
            entry_point: 0,
            user_stack_top: 0,
            mmap_next_vaddr: 0x6000_0000,
        }
    }

    pub fn alloc_fd(&mut self, handle: Arc<FileHandle>) -> Result<i32, i32> {
        for (i, slot) in self.fds.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(handle);
                return Ok(i as i32);
            }
        }
        self.fds.push(Some(handle));
        Ok((self.fds.len() - 1) as i32)
    }

    pub fn get_fd(&self, fd: i32) -> Option<Arc<FileHandle>> {
        if fd < 0 {
            return None;
        }
        self.fds.get(fd as usize).and_then(|opt| opt.clone())
    }

    pub fn close_fd(&mut self, fd: i32) -> Result<(), i32> {
        if fd < 0 || fd as usize >= self.fds.len() {
            return Err(posix_abi::EBADF);
        }
        if self.fds[fd as usize].take().is_some() {
            Ok(())
        } else {
            Err(posix_abi::EBADF)
        }
    }

    pub fn exec(&mut self, path: &str) -> Result<(), i32> {
        let inode = crate::services::vfs::resolve_path_with_cwd(&self.cwd, path)?;
        let stat = inode.stat()?;
        let mut elf_data = alloc::vec![0u8; stat.st_size as usize];
        inode.read(0, &mut elf_data)?;

        let mut new_vm = VmSpace::new().ok_or(posix_abi::ENOMEM)?;
        let loaded = load_elf(&elf_data, &mut new_vm).map_err(|_| posix_abi::ENOEXEC)?;

        self.vm_space = Some(new_vm);
        self.entry_point = loaded.entry_point;
        self.user_stack_top = loaded.user_stack_top;

        Ok(())
    }
}

static NEXT_PID: AtomicI32 = AtomicI32::new(1);
pub static PROCESS_TABLE: SpinLock<BTreeMap<i32, Arc<SpinLock<Process>>>> = SpinLock::new(BTreeMap::new());
pub static CURRENT_PID: AtomicI32 = AtomicI32::new(1);

pub fn alloc_pid() -> i32 {
    NEXT_PID.fetch_add(1, Ordering::SeqCst)
}

pub fn get_current_process() -> Option<Arc<SpinLock<Process>>> {
    let pid = CURRENT_PID.load(Ordering::SeqCst);
    PROCESS_TABLE.lock().get(&pid).cloned()
}
