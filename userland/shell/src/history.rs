//! In-Memory Shell Command History Ring Buffer (1000 Commands).

pub const MAX_HISTORY: usize = 1000;
pub const MAX_CMD_LEN: usize = 128;

#[derive(Clone, Copy)]
pub struct HistoryEntry {
    pub buf: [u8; MAX_CMD_LEN],
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

pub static mut HISTORY: [HistoryEntry; MAX_HISTORY] = [HistoryEntry {
    buf: [0; MAX_CMD_LEN],
    len: 0,
}; MAX_HISTORY];
pub static mut HISTORY_COUNT: usize = 0;

pub unsafe fn history_add(cmd: &[u8], len: usize) {
    if len == 0 {
        return;
    }
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

pub unsafe fn history_prev(cursor: &mut usize, buf: &mut [u8], len: &mut usize) {
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

pub unsafe fn history_next(
    cursor: &mut usize,
    buf: &mut [u8],
    len: &mut usize,
    draft: &[u8],
    draft_len: usize,
) {
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
