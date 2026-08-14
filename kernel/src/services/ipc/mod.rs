//! POSIX Signals and IPC - De-privileged Safe Service.

use alloc::collections::BTreeMap;
use posix_abi::*;
use crate::ostd::sync::SpinLock;

pub struct SignalManager {
    pub pending_signals: SpinLock<BTreeMap<i32, u64>>,
    pub signal_actions: SpinLock<BTreeMap<i32, [SigAction; 64]>>,
}

impl SignalManager {
    pub const fn new() -> Self {
        Self {
            pending_signals: SpinLock::new(BTreeMap::new()),
            signal_actions: SpinLock::new(BTreeMap::new()),
        }
    }

    pub fn send_signal(&self, pid: i32, sig: i32) -> Result<(), i32> {
        if sig < 1 || sig >= 64 {
            return Err(EINVAL);
        }
        let mut pending = self.pending_signals.lock();
        let mask = pending.entry(pid).or_insert(0);
        *mask |= 1 << (sig - 1);
        Ok(())
    }
}

pub static SIGNALS: SignalManager = SignalManager::new();
