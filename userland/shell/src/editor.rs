//! Zero-Flicker Line Editor with History Traversal, Autocompletion,
//! Command-Line Navigation (Home/End/Word/Arrows), Clipboard Kill-Ring, and Paste Support.

use crate::completion::handle_tab_completion;
use crate::history::*;
use crate::line_draw::LineBuffer;

pub const KILL_RING_SIZE: usize = 1024;

#[derive(Clone, Copy)]
pub struct KillRing {
    pub buf: [u8; KILL_RING_SIZE],
    pub len: usize,
}

impl KillRing {
    pub const fn new() -> Self {
        Self {
            buf: [0; KILL_RING_SIZE],
            len: 0,
        }
    }

    pub fn save(&mut self, src: &[u8]) {
        let count = src.len().min(KILL_RING_SIZE);
        self.buf[..count].copy_from_slice(&src[..count]);
        self.len = count;
    }

    pub fn yank_into(&self, dest: &mut [u8]) -> usize {
        let count = self.len.min(dest.len());
        dest[..count].copy_from_slice(&self.buf[..count]);
        count
    }
}

pub static mut KILL_RING: KillRing = KillRing::new();

pub fn is_known_command(cmd: &str, known_commands: &[&str]) -> bool {
    for &k in known_commands {
        if cmd == k {
            return true;
        }
    }
    false
}

pub unsafe fn repaint_prompt_line(
    cwd: *const u8,
    buf: &[u8],
    len: usize,
    cursor_pos: usize,
    known_commands: &[&str],
) {
    unsafe {
        crate::line_draw::paint_prompt(cwd, buf, len, cursor_pos, |cmd| {
            is_known_command(cmd, known_commands)
        });
    }
}

pub unsafe fn clear_menu_line(
    cwd: *const u8,
    buf: &[u8],
    len: usize,
    cursor_pos: usize,
    known_commands: &[&str],
) {
    let mut scratch = [0u8; 64];
    let mut out = LineBuffer::new(&mut scratch);
    out.push_str("\n\r\x1b[K\x1b[A");
    out.flush();
    unsafe {
        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
    }
}

enum EscSeq {
    Up,
    Down,
    Right,
    Left,
    Home,
    End,
    Delete,
    WordLeft,
    WordRight,
    BracketedPasteStart,
    None,
}

unsafe fn parse_escape_sequence() -> EscSeq {
    let b2 = unsafe { libc::getchar() };
    if b2 < 0 {
        return EscSeq::None;
    }
    match b2 as u8 {
        b'[' => {
            let b3 = unsafe { libc::getchar() };
            if b3 < 0 {
                return EscSeq::None;
            }
            match b3 as u8 {
                b'A' => EscSeq::Up,
                b'B' => EscSeq::Down,
                b'C' => EscSeq::Right,
                b'D' => EscSeq::Left,
                b'H' => EscSeq::Home,
                b'F' => EscSeq::End,
                b'1' => {
                    let b4 = unsafe { libc::getchar() };
                    if b4 == b'~' as i32 {
                        EscSeq::Home
                    } else if b4 == b';' as i32 {
                        let b5 = unsafe { libc::getchar() };
                        let b6 = unsafe { libc::getchar() };
                        if b5 == b'5' as i32 {
                            if b6 == b'C' as i32 {
                                EscSeq::WordRight
                            } else if b6 == b'D' as i32 {
                                EscSeq::WordLeft
                            } else {
                                EscSeq::None
                            }
                        } else {
                            EscSeq::None
                        }
                    } else {
                        EscSeq::None
                    }
                }
                b'2' => {
                    let b4 = unsafe { libc::getchar() };
                    let b5 = unsafe { libc::getchar() };
                    let b6 = unsafe { libc::getchar() };
                    if b4 == b'0' as i32 && b5 == b'0' as i32 && b6 == b'~' as i32 {
                        EscSeq::BracketedPasteStart
                    } else {
                        EscSeq::None
                    }
                }
                b'3' => {
                    let b4 = unsafe { libc::getchar() };
                    if b4 == b'~' as i32 {
                        EscSeq::Delete
                    } else {
                        EscSeq::None
                    }
                }
                b'4' => {
                    let b4 = unsafe { libc::getchar() };
                    if b4 == b'~' as i32 {
                        EscSeq::End
                    } else {
                        EscSeq::None
                    }
                }
                b'7' => {
                    let b4 = unsafe { libc::getchar() };
                    if b4 == b'~' as i32 {
                        EscSeq::Home
                    } else {
                        EscSeq::None
                    }
                }
                b'8' => {
                    let b4 = unsafe { libc::getchar() };
                    if b4 == b'~' as i32 {
                        EscSeq::End
                    } else {
                        EscSeq::None
                    }
                }
                _ => EscSeq::None,
            }
        }
        b'O' => {
            let b3 = unsafe { libc::getchar() };
            if b3 < 0 {
                return EscSeq::None;
            }
            match b3 as u8 {
                b'H' => EscSeq::Home,
                b'F' => EscSeq::End,
                b'A' => EscSeq::Up,
                b'B' => EscSeq::Down,
                b'C' => EscSeq::Right,
                b'D' => EscSeq::Left,
                _ => EscSeq::None,
            }
        }
        b'b' => EscSeq::WordLeft,
        b'f' => EscSeq::WordRight,
        _ => EscSeq::None,
    }
}

unsafe fn read_bracketed_paste(buf: &mut [u8], len: &mut usize, cursor_pos: &mut usize) {
    let mut paste_buf = [0u8; 1024];
    let mut paste_len = 0;
    let mut state = 0;

    loop {
        let b = unsafe { libc::getchar() };
        if b < 0 {
            break;
        }
        let ch = b as u8;

        match state {
            0 => {
                if ch == 0x1b {
                    state = 1;
                } else if paste_len < paste_buf.len() {
                    paste_buf[paste_len] = ch;
                    paste_len += 1;
                }
            }
            1 => {
                if ch == b'[' {
                    state = 2;
                } else {
                    if paste_len + 1 < paste_buf.len() {
                        paste_buf[paste_len] = 0x1b;
                        paste_buf[paste_len + 1] = ch;
                        paste_len += 2;
                    }
                    state = 0;
                }
            }
            2 => {
                if ch == b'2' {
                    state = 3;
                } else {
                    if paste_len + 2 < paste_buf.len() {
                        paste_buf[paste_len] = 0x1b;
                        paste_buf[paste_len + 1] = b'[';
                        paste_buf[paste_len + 2] = ch;
                        paste_len += 3;
                    }
                    state = 0;
                }
            }
            3 => {
                if ch == b'0' {
                    state = 4;
                } else {
                    if paste_len + 3 < paste_buf.len() {
                        paste_buf[paste_len] = 0x1b;
                        paste_buf[paste_len + 1] = b'[';
                        paste_buf[paste_len + 2] = b'2';
                        paste_buf[paste_len + 3] = ch;
                        paste_len += 4;
                    }
                    state = 0;
                }
            }
            4 => {
                if ch == b'1' {
                    state = 5;
                } else {
                    if paste_len + 4 < paste_buf.len() {
                        paste_buf[paste_len] = 0x1b;
                        paste_buf[paste_len + 1] = b'[';
                        paste_buf[paste_len + 2] = b'2';
                        paste_buf[paste_len + 3] = b'0';
                        paste_buf[paste_len + 4] = ch;
                        paste_len += 5;
                    }
                    state = 0;
                }
            }
            5 => {
                if ch == b'~' {
                    break;
                } else {
                    if paste_len + 5 < paste_buf.len() {
                        paste_buf[paste_len] = 0x1b;
                        paste_buf[paste_len + 1] = b'[';
                        paste_buf[paste_len + 2] = b'2';
                        paste_buf[paste_len + 3] = b'0';
                        paste_buf[paste_len + 4] = b'1';
                        paste_buf[paste_len + 5] = ch;
                        paste_len += 6;
                    }
                    state = 0;
                }
            }
            _ => state = 0,
        }
    }

    if paste_len > 0 && *len < buf.len() - 1 {
        let max_insert = (buf.len() - 1 - *len).min(paste_len);
        for i in (*cursor_pos..*len).rev() {
            buf[i + max_insert] = buf[i];
        }
        for i in 0..max_insert {
            let mut byte = paste_buf[i];
            if byte == b'\r' || byte == b'\n' {
                byte = b' ';
            }
            buf[*cursor_pos + i] = byte;
        }
        *cursor_pos += max_insert;
        *len += max_insert;
        buf[*len] = 0;
    }
}

pub unsafe fn read_line_with_history(
    cwd: *const u8,
    buf: &mut [u8],
    known_commands: &[&str],
) -> usize {
    let mut len = 0;
    let mut cursor_pos = 0;
    let mut history_cursor = 0;
    let mut draft_buf = [0u8; MAX_CMD_LEN];
    let mut draft_len = 0;

    // Enable bracketed paste mode in compatible terminals
    let mut init_scratch = [0u8; 16];
    let mut init_out = LineBuffer::new(&mut init_scratch);
    init_out.push_str("\x1b[?2004h");
    init_out.flush();

    unsafe {
        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
    }

    loop {
        let b = unsafe { libc::getchar() };
        if b < 0 {
            continue;
        }
        let ch = b as u8;

        if ch == b'\n' || ch == b'\r' {
            let mut scratch = [0u8; 16];
            let mut out = LineBuffer::new(&mut scratch);
            // Disable bracketed paste mode on return
            out.push_str("\x1b[?2004l\n\r");
            out.flush();
            break;
        } else if ch == 0x01 {
            // Ctrl+A -> Move cursor to start of line
            cursor_pos = 0;
            unsafe {
                repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
            }
        } else if ch == 0x05 {
            // Ctrl+E -> Move cursor to end of line
            cursor_pos = len;
            unsafe {
                repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
            }
        } else if ch == 0x02 {
            // Ctrl+B -> Move cursor backward
            if cursor_pos > 0 {
                cursor_pos -= 1;
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        } else if ch == 0x06 {
            // Ctrl+F -> Move cursor forward
            if cursor_pos < len {
                cursor_pos += 1;
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        } else if ch == 0x0b {
            // Ctrl+K -> Kill from cursor to end of line
            if cursor_pos < len {
                unsafe {
                    let kr = &raw mut KILL_RING;
                    (*kr).save(&buf[cursor_pos..len]);
                }
                len = cursor_pos;
                buf[len] = 0;
                if history_cursor == 0 {
                    draft_len = len.min(MAX_CMD_LEN);
                    draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                }
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        } else if ch == 0x15 {
            // Ctrl+U -> Kill from cursor to start of line (or whole line if cursor is at end)
            if cursor_pos > 0 {
                unsafe {
                    let kr = &raw mut KILL_RING;
                    (*kr).save(&buf[..cursor_pos]);
                }
                for i in cursor_pos..len {
                    buf[i - cursor_pos] = buf[i];
                }
                len -= cursor_pos;
                cursor_pos = 0;
                buf[len] = 0;
                if history_cursor == 0 {
                    draft_len = len.min(MAX_CMD_LEN);
                    draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                }
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            } else if len > 0 {
                unsafe {
                    let kr = &raw mut KILL_RING;
                    (*kr).save(&buf[..len]);
                }
                len = 0;
                buf[0] = 0;
                if history_cursor == 0 {
                    draft_len = 0;
                }
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        } else if ch == 0x17 {
            // Ctrl+W -> Kill previous word
            if cursor_pos > 0 {
                let mut word_start = cursor_pos;
                while word_start > 0
                    && (buf[word_start - 1] == b' ' || buf[word_start - 1] == b'\t')
                {
                    word_start -= 1;
                }
                while word_start > 0 && buf[word_start - 1] != b' ' && buf[word_start - 1] != b'\t'
                {
                    word_start -= 1;
                }
                let count = cursor_pos - word_start;
                unsafe {
                    let kr = &raw mut KILL_RING;
                    (*kr).save(&buf[word_start..cursor_pos]);
                }
                for i in cursor_pos..len {
                    buf[i - count] = buf[i];
                }
                len -= count;
                cursor_pos = word_start;
                buf[len] = 0;
                if history_cursor == 0 {
                    draft_len = len.min(MAX_CMD_LEN);
                    draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                }
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        } else if ch == 0x19 {
            // Ctrl+Y -> Yank (paste from kill ring)
            unsafe {
                let kr = &raw const KILL_RING;
                let klen = (*kr).len;
                if klen > 0 && len + klen < buf.len() - 1 {
                    for i in (cursor_pos..len).rev() {
                        buf[i + klen] = buf[i];
                    }
                    (*kr).yank_into(&mut buf[cursor_pos..cursor_pos + klen]);
                    cursor_pos += klen;
                    len += klen;
                    buf[len] = 0;
                    if history_cursor == 0 {
                        draft_len = len.min(MAX_CMD_LEN);
                        draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                    }
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        } else if ch == 0x0c {
            // Ctrl+L -> Clear screen and repaint prompt
            let mut scratch = [0u8; 16];
            let mut out = LineBuffer::new(&mut scratch);
            out.push_str("\x1b[2J\x1b[H");
            out.flush();
            unsafe {
                repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
            }
        } else if ch == 0x04 {
            // Ctrl+D -> Delete under cursor or EOF if line empty
            if len == 0 {
                let mut scratch = [0u8; 16];
                let mut out = LineBuffer::new(&mut scratch);
                out.push_str("\x1b[?2004l\n\r");
                out.flush();
                return 0;
            } else if cursor_pos < len {
                for i in cursor_pos..len - 1 {
                    buf[i] = buf[i + 1];
                }
                len -= 1;
                buf[len] = 0;
                if history_cursor == 0 {
                    draft_len = len.min(MAX_CMD_LEN);
                    draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                }
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        } else if ch == b'\t' {
            unsafe {
                handle_tab_completion(
                    cwd,
                    buf,
                    &mut len,
                    known_commands,
                    |c, b, l| repaint_prompt_line(c, b, l, l, known_commands),
                    |c, b, l| clear_menu_line(c, b, l, l, known_commands),
                );
                cursor_pos = len;
            }
        } else if ch == 0x7f || ch == 0x08 {
            // Backspace -> delete character before cursor
            if cursor_pos > 0 {
                for i in cursor_pos..len {
                    buf[i - 1] = buf[i];
                }
                cursor_pos -= 1;
                len -= 1;
                buf[len] = 0;
                if history_cursor == 0 {
                    draft_len = len.min(MAX_CMD_LEN);
                    draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                }
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        } else if ch == 0x1b {
            match unsafe { parse_escape_sequence() } {
                EscSeq::Up => {
                    if history_cursor == 0 {
                        draft_len = len.min(MAX_CMD_LEN);
                        draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                    }
                    unsafe {
                        history_prev(&mut history_cursor, buf, &mut len);
                        cursor_pos = len;
                        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                    }
                }
                EscSeq::Down => unsafe {
                    history_next(&mut history_cursor, buf, &mut len, &draft_buf, draft_len);
                    cursor_pos = len;
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                },
                EscSeq::Left => {
                    if cursor_pos > 0 {
                        cursor_pos -= 1;
                        unsafe {
                            repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                        }
                    }
                }
                EscSeq::Right => {
                    if cursor_pos < len {
                        cursor_pos += 1;
                        unsafe {
                            repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                        }
                    }
                }
                EscSeq::Home => {
                    cursor_pos = 0;
                    unsafe {
                        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                    }
                }
                EscSeq::End => {
                    cursor_pos = len;
                    unsafe {
                        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                    }
                }
                EscSeq::WordLeft => {
                    while cursor_pos > 0
                        && (buf[cursor_pos - 1] == b' ' || buf[cursor_pos - 1] == b'\t')
                    {
                        cursor_pos -= 1;
                    }
                    while cursor_pos > 0
                        && buf[cursor_pos - 1] != b' '
                        && buf[cursor_pos - 1] != b'\t'
                    {
                        cursor_pos -= 1;
                    }
                    unsafe {
                        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                    }
                }
                EscSeq::WordRight => {
                    while cursor_pos < len && buf[cursor_pos] != b' ' && buf[cursor_pos] != b'\t' {
                        cursor_pos += 1;
                    }
                    while cursor_pos < len && (buf[cursor_pos] == b' ' || buf[cursor_pos] == b'\t')
                    {
                        cursor_pos += 1;
                    }
                    unsafe {
                        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                    }
                }
                EscSeq::Delete => {
                    if cursor_pos < len {
                        for i in cursor_pos..len - 1 {
                            buf[i] = buf[i + 1];
                        }
                        len -= 1;
                        buf[len] = 0;
                        if history_cursor == 0 {
                            draft_len = len.min(MAX_CMD_LEN);
                            draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                        }
                        unsafe {
                            repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                        }
                    }
                }
                EscSeq::BracketedPasteStart => unsafe {
                    read_bracketed_paste(buf, &mut len, &mut cursor_pos);
                    if history_cursor == 0 {
                        draft_len = len.min(MAX_CMD_LEN);
                        draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                    }
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                },
                EscSeq::None => {}
            }
        } else if ch >= 0x20 && ch < 0x7f {
            if len < buf.len() - 1 {
                if cursor_pos < len {
                    for i in (cursor_pos..len).rev() {
                        buf[i + 1] = buf[i];
                    }
                }
                buf[cursor_pos] = ch;
                cursor_pos += 1;
                len += 1;
                buf[len] = 0;
                if history_cursor == 0 {
                    draft_len = len.min(MAX_CMD_LEN);
                    draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                }
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        }
    }
    buf[len] = 0;
    if len > 0 {
        unsafe {
            history_add(&buf[..len], len);
        }
    }
    len
}
