//! POSIX Signals and IPC - De-privileged Safe Service.
//!
//! Lock hierarchy tier (ADR-0002):
//! PROCESS_TABLE -> SCHEDULER -> IPC (SignalManager) -> VFS Mount/Table -> Inode -> Devices

use crate::ostd::sync::SpinLock;
use alloc::collections::BTreeMap;
use posix_abi::*;

pub struct SignalManager {
    pub pending_signals: SpinLock<BTreeMap<i32, u64>>,
    pub blocked_masks: SpinLock<BTreeMap<i32, SigSet>>,
    pub signal_actions: SpinLock<BTreeMap<i32, [SigAction; 32]>>,
}

impl SignalManager {
    pub const fn new() -> Self {
        Self {
            pending_signals: SpinLock::new(BTreeMap::new()),
            blocked_masks: SpinLock::new(BTreeMap::new()),
            signal_actions: SpinLock::new(BTreeMap::new()),
        }
    }

    /// Sends a POSIX signal to a target process PID.
    ///
    /// Target PID must be positive (`pid > 0`), as process groups (pid <= 0)
    /// are not yet implemented. Signal numbers are strictly validated against
    /// `SIG_MIN..=SIG_MAX` (`1..=31`).
    ///
    /// If the target process is blocked, wakes it to enable EINTR interruption.
    pub fn send_signal(&self, pid: i32, sig: i32) -> Result<(), i32> {
        if pid <= 0 || !(SIG_MIN..=SIG_MAX).contains(&sig) {
            return Err(EINVAL);
        }

        // Verify target process exists in PROCESS_TABLE
        let target_exists = {
            let table = crate::services::process::PROCESS_TABLE.lock();
            table.contains_key(&pid)
        };

        if !target_exists {
            return Err(ESRCH);
        }

        {
            let mut pending = self.pending_signals.lock();
            let mask = pending.entry(pid).or_insert(0);
            *mask |= 1 << (sig - 1);
        }

        // Wake target task if it is blocked on a wait queue
        crate::services::scheduler::wake_tasks(&[pid]);

        Ok(())
    }

    /// Gets the pending signal bitmask for a process.
    pub fn get_pending(&self, pid: i32) -> u64 {
        self.pending_signals.lock().get(&pid).copied().unwrap_or(0)
    }

    /// Clears a specific pending signal bit for a process.
    pub fn clear_pending(&self, pid: i32, sig: i32) {
        if (SIG_MIN..=SIG_MAX).contains(&sig) {
            let mut pending = self.pending_signals.lock();
            if let Some(mask) = pending.get_mut(&pid) {
                *mask &= !(1 << (sig - 1));
            }
        }
    }

    /// Gets the blocked signal mask for a process.
    pub fn get_procmask(&self, pid: i32) -> SigSet {
        self.blocked_masks.lock().get(&pid).copied().unwrap_or(0)
    }

    /// Checks if a process has any pending signals that are not blocked.
    pub fn has_unblocked_signals(&self, pid: i32) -> bool {
        let pending = self.get_pending(pid);
        let blocked = self.get_procmask(pid);
        (pending & !blocked) != 0
    }

    /// Updates the blocked signal mask for a process (`how: SIG_BLOCK, SIG_UNBLOCK, SIG_SETMASK`).
    ///
    /// `SIGKILL` (9) and `SIGSTOP` (19) cannot be blocked per POSIX standard.
    pub fn set_procmask(&self, pid: i32, how: i32, new_set: SigSet) -> Result<SigSet, i32> {
        let unblockable = (1 << (SIGKILL - 1)) | (1 << (SIGSTOP - 1));
        let effective_set = new_set & !unblockable;

        let mut masks = self.blocked_masks.lock();
        let current = masks.entry(pid).or_insert(0);
        let old_mask = *current;

        match how {
            SIG_BLOCK => *current |= effective_set,
            SIG_UNBLOCK => *current &= !effective_set,
            SIG_SETMASK => *current = effective_set,
            _ => return Err(EINVAL),
        }

        Ok(old_mask)
    }

    /// Gets the registered `SigAction` disposition for a signal.
    pub fn get_action(&self, pid: i32, sig: i32) -> SigAction {
        if !(SIG_MIN..=SIG_MAX).contains(&sig) {
            return SigAction::default();
        }
        self.signal_actions
            .lock()
            .get(&pid)
            .map(|actions| actions[sig as usize])
            .unwrap_or_default()
    }

    /// Registers a `SigAction` disposition for a signal.
    ///
    /// `SIGKILL` (9) and `SIGSTOP` (19) cannot be caught or ignored per POSIX.
    pub fn set_action(&self, pid: i32, sig: i32, act: SigAction) -> Result<SigAction, i32> {
        if !(SIG_MIN..=SIG_MAX).contains(&sig) {
            return Err(EINVAL);
        }
        if sig == SIGKILL || sig == SIGSTOP {
            return Err(EINVAL);
        }

        let mut actions_map = self.signal_actions.lock();
        let actions = actions_map.entry(pid).or_insert([SigAction::default(); 32]);
        let old_act = actions[sig as usize];
        actions[sig as usize] = act;
        Ok(old_act)
    }

    /// Cleans up all signal state for an exited/reaped process.
    pub fn cleanup_process(&self, pid: i32) {
        self.pending_signals.lock().remove(&pid);
        self.blocked_masks.lock().remove(&pid);
        self.signal_actions.lock().remove(&pid);
    }
}

impl Default for SignalManager {
    fn default() -> Self {
        Self::new()
    }
}

pub static SIGNALS: SignalManager = SignalManager::new();
