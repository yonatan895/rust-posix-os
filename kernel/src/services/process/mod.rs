//! Process Management Service - De-privileged Safe Service.

use crate::ostd::mm::VmSpace;
use crate::ostd::sync::SpinLock;
use crate::services::process::elf::load_elf;
use crate::services::vfs::FileHandle;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

pub mod elf;

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
    pub killed_by_sig: Option<i32>,
    pub vm_space: Option<VmSpace>,
    pub entry_point: usize,
    pub user_stack_top: usize,
    pub mmap_next_vaddr: usize,
    pub kernel_stack: Vec<u8>,
    pub saved_kernel_rsp: AtomicUsize,
    pub has_started: bool,
}

pub const DEFAULT_MMAP_BASE: usize = 0x6000_0000;

impl Process {
    pub fn new(pid: i32, ppid: i32, cwd: String) -> Self {
        let kernel_stack = alloc::vec![0u8; crate::ostd::task::KERNEL_STACK_SIZE];
        let saved_kernel_rsp = kernel_stack.as_ptr() as usize + kernel_stack.len();
        Self {
            pid,
            ppid,
            state: ProcessState::Ready,
            cwd,
            fds: Vec::new(),
            exit_code: 0,
            killed_by_sig: None,
            vm_space: None,
            entry_point: 0,
            user_stack_top: 0,
            mmap_next_vaddr: DEFAULT_MMAP_BASE,
            kernel_stack,
            saved_kernel_rsp: AtomicUsize::new(saved_kernel_rsp),
            has_started: false,
        }
    }

    pub fn kernel_stack_top(&self) -> u64 {
        self.kernel_stack.as_ptr() as u64 + self.kernel_stack.len() as u64
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

    pub fn exec(&mut self, path: &str, argv: &[&str], envp: &[&str]) -> Result<(), i32> {
        let inode = crate::services::vfs::resolve_path_with_cwd(&self.cwd, path)?;
        let stat = inode.stat()?;
        let mut elf_data = alloc::vec![0u8; stat.st_size as usize];
        inode.read(0, &mut elf_data)?;

        let mut new_vm = VmSpace::new().ok_or(posix_abi::ENOMEM)?;
        let loaded =
            load_elf(&elf_data, &mut new_vm, argv, envp).map_err(|_| posix_abi::ENOEXEC)?;

        let old_vm = self.vm_space.replace(new_vm);
        self.entry_point = loaded.entry_point;
        self.user_stack_top = loaded.user_stack_top;
        self.mmap_next_vaddr = DEFAULT_MMAP_BASE;

        // Activate the new address space in CR3 before dropping the old address space
        if let Some(ref vm) = self.vm_space {
            vm.activate();
        }
        drop(old_vm);

        let initial_rsp = crate::ostd::task::init_user_kernel_stack(
            &mut self.kernel_stack,
            self.entry_point,
            self.user_stack_top,
        );
        self.saved_kernel_rsp.store(initial_rsp, Ordering::Release);
        self.has_started = true;

        Ok(())
    }
}

static NEXT_PID: AtomicI32 = AtomicI32::new(1);
pub static PROCESS_TABLE: SpinLock<BTreeMap<i32, Arc<SpinLock<Process>>>> =
    SpinLock::new(BTreeMap::new());
pub static CURRENT_PID: AtomicI32 = AtomicI32::new(1);

pub fn alloc_pid() -> i32 {
    let pid = NEXT_PID.fetch_add(1, Ordering::SeqCst);
    if pid <= 0 {
        NEXT_PID.store(2, Ordering::SeqCst);
        2
    } else {
        pid
    }
}

pub fn get_current_process() -> Option<Arc<SpinLock<Process>>> {
    let pid = CURRENT_PID.load(Ordering::SeqCst);
    PROCESS_TABLE.lock().get(&pid).cloned()
}
