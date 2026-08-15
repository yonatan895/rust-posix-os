//! In-memory shell command history ring buffer supporting up to 1000 commands.

/// Maximum number of command entries retained in the history ring buffer.
pub const MAX_HISTORY: usize = 1000;
/// Maximum length in bytes stored for an individual command entry.
pub const MAX_CMD_LEN: usize = 128;

/// Fixed-size storage entry for a recorded shell history command line.
#[derive(Clone, Copy)]
pub struct HistoryEntry {
    /// Byte buffer containing the command string.
    pub buf: [u8; MAX_CMD_LEN],
    /// Length of the command string in bytes.
    pub len: usize,
}

impl Default for HistoryEntry {
    fn default() -> Self {
        Self {
            buf: [0; MAX_CMD_LEN],
            len: 0,
        }
    }
}

/// Global circular history buffer storage.
pub static mut HISTORY: [HistoryEntry; MAX_HISTORY] = [HistoryEntry {
    buf: [0; MAX_CMD_LEN],
    len: 0,
}; MAX_HISTORY];
/// Total count of command entries added to history over time.
pub static mut HISTORY_COUNT: usize = 0;

/// Appends a new command to the circular history buffer, skipping consecutive duplicates.
///
/// # Safety
///
/// Must only be called from single-threaded shell execution context.
pub unsafe fn history_add(cmd: &[u8], len: usize) {
    if len == 0 {
        return;
    }
    // SAFETY: Caller guarantees single-threaded shell execution context. Accesses and updates global history static ring buffer.
    unsafe {
        let count = HISTORY_COUNT;
        if count > 0 {
            let last_idx = (count - 1) % MAX_HISTORY;
            let last = &HISTORY[last_idx];
            if last.len == len && &last.buf[..len] == cmd {
                return;
            }
        }
        let idx = count % MAX_HISTORY;
        let store_len = len.min(MAX_CMD_LEN);
        HISTORY[idx].buf[..store_len].copy_from_slice(&cmd[..store_len]);
        HISTORY[idx].len = store_len;
        HISTORY_COUNT += 1;
    }
}

/// Retrieves the previous (older) history entry, copying it into `buf`.
///
/// # Safety
///
/// Must only be called from single-threaded shell execution context.
pub unsafe fn history_prev(cursor: &mut usize, buf: &mut [u8], len: &mut usize) {
    // SAFETY: Caller guarantees single-threaded shell execution context. Reads entry from global circular history static buffer.
    unsafe {
        let total = HISTORY_COUNT;
        if total == 0 {
            return;
        }
        let max_avail = total.min(MAX_HISTORY);
        if *cursor < max_avail {
            *cursor += 1;
            let idx = if total >= MAX_HISTORY {
                (total - *cursor) % MAX_HISTORY
            } else {
                total - *cursor
            };
            let entry = &HISTORY[idx];
            let copy_len = entry.len.min(buf.len() - 1);
            buf[..copy_len].copy_from_slice(&entry.buf[..copy_len]);
            buf[copy_len] = 0;
            *len = copy_len;
        }
    }
}

/// Retrieves the next (newer) history entry or restores the active `draft` buffer if at index 0.
///
/// # Safety
///
/// Must only be called from single-threaded shell execution context.
pub unsafe fn history_next(
    cursor: &mut usize,
    buf: &mut [u8],
    len: &mut usize,
    draft: &[u8],
    draft_len: usize,
) {
    // SAFETY: Caller guarantees single-threaded shell execution context. Reads entry from global circular history static buffer.
    unsafe {
        let total = HISTORY_COUNT;
        if *cursor > 1 {
            *cursor -= 1;
            let idx = if total >= MAX_HISTORY {
                (total - *cursor) % MAX_HISTORY
            } else {
                total - *cursor
            };
            let entry = &HISTORY[idx];
            let copy_len = entry.len.min(buf.len() - 1);
            buf[..copy_len].copy_from_slice(&entry.buf[..copy_len]);
            buf[copy_len] = 0;
            *len = copy_len;
        } else if *cursor == 1 {
            *cursor = 0;
            let copy_len = draft_len.min(buf.len() - 1);
            buf[..copy_len].copy_from_slice(&draft[..copy_len]);
            buf[copy_len] = 0;
            *len = copy_len;
        }
    }
}
