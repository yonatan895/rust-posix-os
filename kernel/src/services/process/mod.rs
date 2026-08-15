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

/// Execution states of a kernel process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Runnable and awaiting scheduler dispatch.
    Ready,
    /// Currently running on a CPU core.
    Running,
    /// Blocked waiting on I/O, pipe, child exit, or sleep.
    Blocked,
    /// Terminated and awaiting parent waitpid reaping.
    Zombie,
}

/// Process Control Block (PCB) representing a POSIX task.
pub struct Process {
    /// Process identifier.
    pub pid: i32,
    /// Parent process identifier.
    pub ppid: i32,
    /// Real user identifier.
    pub uid: u32,
    /// Real group identifier.
    pub gid: u32,
    /// Effective user identifier.
    pub euid: u32,
    /// Effective group identifier.
    pub egid: u32,
    /// File mode creation mask.
    pub umask: u32,
    /// Current scheduling/execution state.
    pub state: ProcessState,
    /// Current working directory path.
    pub cwd: String,
    /// File descriptor table mapping fd indices to open file handles.
    pub fds: Vec<Option<Arc<FileHandle>>>,
    /// Exit status code returned on termination.
    pub exit_code: i32,
    /// Fatal signal number if terminated by signal.
    pub killed_by_sig: Option<i32>,
    /// Virtual memory address space and page tables.
    pub vm_space: Option<VmSpace>,
    /// User-mode ELF entry point virtual address.
    pub entry_point: usize,
    /// User-mode initial stack pointer (RSP).
    pub user_stack_top: usize,
    /// Next available virtual address for anonymous mmap allocations.
    pub mmap_next_vaddr: usize,
    /// Dedicated per-process kernel execution stack.
    pub kernel_stack: Vec<u8>,
    /// Saved kernel stack pointer (RSP) when context-switched out.
    pub saved_kernel_rsp: AtomicUsize,
    /// Whether the process has completed initial setup and started execution.
    pub has_started: bool,
}

/// Default starting virtual address for user anonymous memory mappings (1.5 GiB boundary).
pub const DEFAULT_MMAP_BASE: usize = 0x6000_0000;

impl Process {
    /// Creates a new unmapped process with the given PID, PPID, and working directory.
    pub fn new(pid: i32, ppid: i32, cwd: String) -> Self {
        let kernel_stack = alloc::vec![0u8; crate::ostd::task::KERNEL_STACK_SIZE];
        let saved_kernel_rsp = kernel_stack.as_ptr() as usize + kernel_stack.len();
        Self {
            pid,
            ppid,
            uid: 0,
            gid: 0,
            euid: 0,
            egid: 0,
            umask: 0o022,
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

    /// Computes the top (highest memory address) of the process kernel stack.
    pub fn kernel_stack_top(&self) -> u64 {
        self.kernel_stack.as_ptr() as u64 + self.kernel_stack.len() as u64
    }

    /// Allocates the lowest available file descriptor slot for `handle`.
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

    /// Retrieves an `Arc` reference to the file handle at descriptor `fd`.
    pub fn get_fd(&self, fd: i32) -> Option<Arc<FileHandle>> {
        if fd < 0 {
            return None;
        }
        self.fds.get(fd as usize).and_then(|opt| opt.clone())
    }

    /// Closes and releases the file descriptor at `fd`.
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

    /// Loads and executes a new ELF binary in this process's address space.
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

/// Global PID generator counter.
static NEXT_PID: AtomicI32 = AtomicI32::new(1);
/// Global process table mapping PID to process control block.
pub static PROCESS_TABLE: SpinLock<BTreeMap<i32, Arc<SpinLock<Process>>>> =
    SpinLock::new(BTreeMap::new());
/// PID of the currently executing process on the local CPU core.
pub static CURRENT_PID: AtomicI32 = AtomicI32::new(1);

/// Allocates a new unique positive process identifier.
pub fn alloc_pid() -> i32 {
    let pid = NEXT_PID.fetch_add(1, Ordering::SeqCst);
    if pid <= 0 {
        NEXT_PID.store(2, Ordering::SeqCst);
        2
    } else {
        pid
    }
}

/// Looks up the currently active process from the global process table.
pub fn get_current_process() -> Option<Arc<SpinLock<Process>>> {
    let pid = CURRENT_PID.load(Ordering::SeqCst);
    PROCESS_TABLE.lock().get(&pid).cloned()
}
